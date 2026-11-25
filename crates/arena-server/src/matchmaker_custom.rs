//! Private custom match lobbies, owned by [`Matchmaker`].
//!
//! A lobby code only admits players into the lobby (up to `2 * team_size`
//! players plus [`MAX_LOBBY_SPECTATORS`] spectators). It is never a seat
//! credential: when the host starts a full lobby, [`Out::StartCustom`] asks
//! the front door to sign fresh per-seat tickets carrying the lobby's
//! versioned [`MatchSettings`], while keeping the lobby open for rematches.

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::{Matchmaker, Mode, Out, QueueConn, Refused, State};
use crate::ticket::MatchSettings;

/// Spectator slots reserved in every private custom lobby.
pub const MAX_LOBBY_SPECTATORS: usize = 4;

/// Where a member sits inside a custom lobby.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LobbySlot {
    TeamA,
    TeamB,
    Spectator,
}

/// Why a connection is no longer in a custom lobby.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbyLeft {
    Left,
    Kicked,
}
impl LobbyLeft {
    pub fn code(self) -> &'static str {
        match self {
            LobbyLeft::Left => "left",
            LobbyLeft::Kicked => "kicked",
        }
    }
}

/// One member of a custom lobby, safe to share with everyone in that lobby.
/// Uses a per-lobby numeric id so neither guest tokens nor global connection
/// counts leak across players.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyMemberView {
    pub id: u32,
    pub slot: LobbySlot,
    pub host: bool,
    pub you: bool,
}

/// A custom lobby as one member sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyView {
    pub code: String,
    pub mode: Mode,
    pub settings: MatchSettings,
    pub spectator_slots: usize,
    pub you: u32,
    pub host: bool,
    pub can_start: bool,
    pub members: Vec<LobbyMemberView>,
}

#[derive(Clone, Copy, Debug)]
struct LobbyMember {
    id: u32,
    conn: QueueConn,
    slot: LobbySlot,
}

#[derive(Debug)]
pub(super) struct CustomLobby {
    code: String,
    mode: Mode,
    settings: MatchSettings,
    host: u32,
    next_member_id: u32,
    members: Vec<LobbyMember>,
}

impl CustomLobby {
    fn count(&self, slot: LobbySlot) -> usize {
        self.members.iter().filter(|m| m.slot == slot).count()
    }
    fn can_start(&self) -> bool {
        let size = self.mode.team_size();
        self.count(LobbySlot::TeamA) == size && self.count(LobbySlot::TeamB) == size
    }
    fn spectator_capacity(&self) -> usize {
        if self.settings.allow_spectators {
            MAX_LOBBY_SPECTATORS
        } else {
            0
        }
    }
    fn has_space(&self, slot: LobbySlot) -> bool {
        match slot {
            LobbySlot::TeamA | LobbySlot::TeamB => self.count(slot) < self.mode.team_size(),
            LobbySlot::Spectator => self.count(LobbySlot::Spectator) < self.spectator_capacity(),
        }
    }
    fn next_open_slot(&self) -> Option<LobbySlot> {
        let team_a = self.count(LobbySlot::TeamA);
        let team_b = self.count(LobbySlot::TeamB);
        let size = self.mode.team_size();
        if team_a < size && (team_a <= team_b || team_b >= size) {
            Some(LobbySlot::TeamA)
        } else if team_b < size {
            Some(LobbySlot::TeamB)
        } else if self.count(LobbySlot::Spectator) < self.spectator_capacity() {
            Some(LobbySlot::Spectator)
        } else {
            None
        }
    }
    fn member_by_conn(&self, conn: QueueConn) -> Option<&LobbyMember> {
        self.members.iter().find(|m| m.conn == conn)
    }
    fn is_host(&self, conn: QueueConn) -> bool {
        self.member_by_conn(conn)
            .is_some_and(|member| member.id == self.host)
    }
    fn view(&self, conn: QueueConn) -> LobbyView {
        let you = self.member_by_conn(conn).map_or(0, |member| member.id);
        LobbyView {
            code: self.code.clone(),
            mode: self.mode,
            settings: self.settings,
            spectator_slots: self.spectator_capacity(),
            you,
            host: you == self.host && you != 0,
            can_start: self.can_start(),
            members: self
                .members
                .iter()
                .map(|member| LobbyMemberView {
                    id: member.id,
                    slot: member.slot,
                    host: member.id == self.host,
                    you: member.conn == conn,
                })
                .collect(),
        }
    }
}

impl Matchmaker {
    /// Whether `conn` is currently in a private custom lobby.
    pub fn in_custom_lobby(&self, conn: QueueConn) -> bool {
        self.in_lobby.contains_key(&conn)
    }

    /// The invite code of `conn`'s custom lobby, if it is in one.
    pub fn lobby_code(&self, conn: QueueConn) -> Option<String> {
        let id = self.in_lobby.get(&conn)?;
        self.lobbies.get(id).map(|lobby| lobby.code.clone())
    }

    fn announce_lobby(&self, id: u64, out: &mut Vec<Out>) {
        if let Some(lobby) = self.lobbies.get(&id) {
            for member in &lobby.members {
                out.push(Out::Lobby {
                    conn: member.conn,
                    view: lobby.view(member.conn),
                });
            }
        }
    }

    /// Removes `conn` from its custom lobby, passing host to the next member
    /// or deleting the lobby when empty. Emits `LobbyLeft` when `reason` is given.
    pub(super) fn remove_from_lobby(
        &mut self,
        conn: QueueConn,
        reason: Option<LobbyLeft>,
    ) -> Vec<Out> {
        let mut out = Vec::new();
        let Some(id) = self.in_lobby.remove(&conn) else {
            return out;
        };
        if let Some(reason) = reason {
            out.push(Out::LobbyLeft { conn, reason });
        }
        let Some(lobby) = self.lobbies.get_mut(&id) else {
            return out;
        };
        if let Some(at) = lobby.members.iter().position(|member| member.conn == conn) {
            let removed = lobby.members.remove(at);
            if lobby.members.is_empty() {
                let code = lobby.code.clone();
                self.lobbies.remove(&id);
                self.lobby_codes.remove(&code);
                return out;
            }
            if lobby.host == removed.id {
                lobby.host = lobby.members[0].id;
            }
        }
        self.announce_lobby(id, &mut out);
        out
    }

    /// Ensures `conn` is idle in matchmaking (leaving any group party first)
    /// before entering a custom lobby.
    fn leave_party_for_lobby(&mut self, conn: QueueConn, now: Instant, out: &mut Vec<Out>) -> bool {
        let Some(&party_id) = self.of.get(&conn) else {
            return false;
        };
        if self.parties[&party_id].state != State::Idle {
            out.push(Out::Refused {
                conn,
                reason: Refused::PartyBusy,
            });
            return false;
        }
        if self.parties[&party_id].members.len() > 1 || self.parties[&party_id].code.is_some() {
            out.extend(self.leave_party(conn, now));
        }
        true
    }

    /// Creates a new private custom lobby with `conn` as host on Team A.
    pub fn create_lobby(&mut self, conn: QueueConn, mode: Mode, now: Instant) -> Vec<Out> {
        if !self.of.contains_key(&conn) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if !self.leave_party_for_lobby(conn, now, &mut out) {
            return out;
        }
        out.extend(self.remove_from_lobby(conn, None));
        let id = self.id();
        let code = self.new_code();
        self.lobby_codes.insert(code.clone(), id);
        self.lobbies.insert(
            id,
            CustomLobby {
                code,
                mode,
                settings: MatchSettings::default(),
                host: 1,
                next_member_id: 2,
                members: vec![LobbyMember {
                    id: 1,
                    conn,
                    slot: LobbySlot::TeamA,
                }],
            },
        );
        self.in_lobby.insert(conn, id);
        self.announce_lobby(id, &mut out);
        out
    }

    /// Joins an existing custom lobby by its invite code.
    pub fn join_lobby(&mut self, conn: QueueConn, code: &str, now: Instant) -> Vec<Out> {
        if !self.of.contains_key(&conn) {
            return Vec::new();
        }
        let normalized = code.trim().to_ascii_uppercase();
        let Some(&target) = self.lobby_codes.get(&normalized) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchLobby,
            }];
        };
        if self.in_lobby.get(&conn) == Some(&target) {
            return vec![Out::Lobby {
                conn,
                view: self.lobbies[&target].view(conn),
            }];
        }
        let Some(slot) = self.lobbies[&target].next_open_slot() else {
            return vec![Out::Refused {
                conn,
                reason: Refused::LobbyFull,
            }];
        };
        let mut out = Vec::new();
        if !self.leave_party_for_lobby(conn, now, &mut out) {
            return out;
        }
        out.extend(self.remove_from_lobby(conn, None));
        let lobby = self.lobbies.get_mut(&target).expect("target lobby exists");
        let member_id = lobby.next_member_id;
        lobby.next_member_id += 1;
        lobby.members.push(LobbyMember {
            id: member_id,
            conn,
            slot,
        });
        self.in_lobby.insert(conn, target);
        self.announce_lobby(target, &mut out);
        out
    }

    /// Leaves the current custom lobby.
    pub fn leave_lobby(&mut self, conn: QueueConn) -> Vec<Out> {
        self.remove_from_lobby(conn, Some(LobbyLeft::Left))
    }

    /// Moves `conn` into `slot` (or swaps teams when `conn` is the host and
    /// both teams are full).
    pub fn set_lobby_slot(&mut self, conn: QueueConn, slot: LobbySlot) -> Vec<Out> {
        let Some(&id) = self.in_lobby.get(&conn) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchLobby,
            }];
        };
        let lobby = self.lobbies.get_mut(&id).expect("lobby exists");
        let Some(index) = lobby.members.iter().position(|m| m.conn == conn) else {
            return Vec::new();
        };
        let current = lobby.members[index].slot;
        if current == slot {
            return Vec::new();
        }
        if slot == LobbySlot::Spectator && !lobby.settings.allow_spectators {
            return vec![Out::Refused {
                conn,
                reason: Refused::SpectatorsDisabled,
            }];
        }
        if lobby.has_space(slot) {
            lobby.members[index].slot = slot;
        } else if lobby.is_host(conn)
            && matches!(
                (current, slot),
                (LobbySlot::TeamA, LobbySlot::TeamB) | (LobbySlot::TeamB, LobbySlot::TeamA)
            )
            && let Some(other) = lobby.members.iter().rposition(|m| m.slot == slot)
        {
            lobby.members[index].slot = slot;
            lobby.members[other].slot = current;
        } else {
            return vec![Out::Refused {
                conn,
                reason: Refused::SlotFull,
            }];
        }
        let mut out = Vec::new();
        self.announce_lobby(id, &mut out);
        out
    }

    /// Host-only: moves `member_id` to `slot`, swapping across teams when the
    /// target team is full.
    pub fn move_lobby_member(
        &mut self,
        conn: QueueConn,
        member_id: u32,
        slot: LobbySlot,
    ) -> Vec<Out> {
        let Some(&id) = self.in_lobby.get(&conn) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchLobby,
            }];
        };
        let lobby = self.lobbies.get_mut(&id).expect("lobby exists");
        if !lobby.is_host(conn) {
            return vec![Out::Refused {
                conn,
                reason: Refused::NotHost,
            }];
        }
        let Some(index) = lobby.members.iter().position(|m| m.id == member_id) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchMember,
            }];
        };
        let current = lobby.members[index].slot;
        if current == slot {
            return Vec::new();
        }
        if slot == LobbySlot::Spectator && !lobby.settings.allow_spectators {
            return vec![Out::Refused {
                conn,
                reason: Refused::SpectatorsDisabled,
            }];
        }
        if lobby.has_space(slot) {
            lobby.members[index].slot = slot;
        } else if matches!(
            (current, slot),
            (LobbySlot::TeamA, LobbySlot::TeamB) | (LobbySlot::TeamB, LobbySlot::TeamA)
        ) && let Some(other) = lobby.members.iter().rposition(|m| m.slot == slot)
        {
            lobby.members[index].slot = slot;
            lobby.members[other].slot = current;
        } else {
            return vec![Out::Refused {
                conn,
                reason: Refused::SlotFull,
            }];
        }
        let mut out = Vec::new();
        self.announce_lobby(id, &mut out);
        out
    }

    /// Host-only: removes another member from the custom lobby.
    pub fn kick_lobby_member(&mut self, conn: QueueConn, member_id: u32) -> Vec<Out> {
        let Some(&id) = self.in_lobby.get(&conn) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchLobby,
            }];
        };
        let lobby = &self.lobbies[&id];
        if !lobby.is_host(conn) {
            return vec![Out::Refused {
                conn,
                reason: Refused::NotHost,
            }];
        }
        let Some(target) = lobby
            .members
            .iter()
            .find(|m| m.id == member_id && m.conn != conn)
        else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchMember,
            }];
        };
        let target_conn = target.conn;
        self.remove_from_lobby(target_conn, Some(LobbyLeft::Kicked))
    }

    /// Host-only: transfers the host role to another member in the lobby.
    pub fn transfer_lobby_host(&mut self, conn: QueueConn, member_id: u32) -> Vec<Out> {
        let Some(&id) = self.in_lobby.get(&conn) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchLobby,
            }];
        };
        let lobby = self.lobbies.get_mut(&id).expect("lobby exists");
        if !lobby.is_host(conn) {
            return vec![Out::Refused {
                conn,
                reason: Refused::NotHost,
            }];
        }
        if !lobby
            .members
            .iter()
            .any(|m| m.id == member_id && m.conn != conn)
        {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchMember,
            }];
        }
        lobby.host = member_id;
        let mut out = Vec::new();
        self.announce_lobby(id, &mut out);
        out
    }

    /// Host-only: changes team size before starting. Overflow players move to
    /// open spectator slots; refused if spectator slots cannot hold them.
    pub fn set_lobby_mode(&mut self, conn: QueueConn, mode: Mode) -> Vec<Out> {
        let Some(&id) = self.in_lobby.get(&conn) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchLobby,
            }];
        };
        let lobby = self.lobbies.get_mut(&id).expect("lobby exists");
        if !lobby.is_host(conn) {
            return vec![Out::Refused {
                conn,
                reason: Refused::NotHost,
            }];
        }
        if lobby.mode == mode {
            return Vec::new();
        }
        let new_size = mode.team_size();
        let excess_a = lobby.count(LobbySlot::TeamA).saturating_sub(new_size);
        let excess_b = lobby.count(LobbySlot::TeamB).saturating_sub(new_size);
        if lobby.count(LobbySlot::Spectator) + excess_a + excess_b > lobby.spectator_capacity() {
            return vec![Out::Refused {
                conn,
                reason: Refused::LobbyTooBig,
            }];
        }
        lobby.mode = mode;
        for (team, mut excess) in [(LobbySlot::TeamA, excess_a), (LobbySlot::TeamB, excess_b)] {
            for member in lobby.members.iter_mut().rev() {
                if excess == 0 {
                    break;
                }
                if member.slot == team {
                    member.slot = LobbySlot::Spectator;
                    excess -= 1;
                }
            }
        }
        let mut out = Vec::new();
        self.announce_lobby(id, &mut out);
        out
    }

    /// Host-only: updates the versioned match settings enforced by the server.
    pub fn set_lobby_settings(&mut self, conn: QueueConn, settings: MatchSettings) -> Vec<Out> {
        let Some(&id) = self.in_lobby.get(&conn) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchLobby,
            }];
        };
        let lobby = self.lobbies.get_mut(&id).expect("lobby exists");
        if !lobby.is_host(conn) {
            return vec![Out::Refused {
                conn,
                reason: Refused::NotHost,
            }];
        }
        if !settings.is_valid() {
            return vec![Out::Refused {
                conn,
                reason: Refused::InvalidSettings,
            }];
        }
        if !settings.allow_spectators && lobby.count(LobbySlot::Spectator) > 0 {
            return vec![Out::Refused {
                conn,
                reason: Refused::SpectatorsOccupied,
            }];
        }
        lobby.settings = settings;
        let mut out = Vec::new();
        self.announce_lobby(id, &mut out);
        out
    }

    /// Host-only: starts the match when both teams are full. The lobby stays
    /// open so players return to it for rematches without new invites.
    pub fn start_lobby(&mut self, conn: QueueConn) -> Vec<Out> {
        let Some(&id) = self.in_lobby.get(&conn) else {
            return vec![Out::Refused {
                conn,
                reason: Refused::NoSuchLobby,
            }];
        };
        let lobby = &self.lobbies[&id];
        if !lobby.is_host(conn) {
            return vec![Out::Refused {
                conn,
                reason: Refused::NotHost,
            }];
        }
        if !lobby.can_start() {
            return vec![Out::Refused {
                conn,
                reason: Refused::LobbyNotFull,
            }];
        }
        let team_a: Vec<QueueConn> = lobby
            .members
            .iter()
            .filter(|m| m.slot == LobbySlot::TeamA)
            .map(|m| m.conn)
            .collect();
        let team_b: Vec<QueueConn> = lobby
            .members
            .iter()
            .filter(|m| m.slot == LobbySlot::TeamB)
            .map(|m| m.conn)
            .collect();
        let spectators: Vec<QueueConn> = lobby
            .members
            .iter()
            .filter(|m| m.slot == LobbySlot::Spectator)
            .map(|m| m.conn)
            .collect();
        let size = lobby.mode.team_size();
        let mut seats = Vec::with_capacity(2 * size);
        for index in 0..size {
            seats.push(team_a[index]);
            seats.push(team_b[index]);
        }
        vec![Out::StartCustom {
            host: conn,
            mode: lobby.mode,
            settings: lobby.settings,
            seats,
            spectators,
        }]
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::*;
    use crate::matchmaker::CODE_LENGTH;
    use crate::ticket::{MATCH_SETTINGS_VERSION, MapId};

    fn seeded_random(bound: usize) -> usize {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        NEXT.fetch_add(7, Ordering::Relaxed) % bound
    }

    fn lobby_of(out: &[Out], conn: QueueConn) -> LobbyView {
        out.iter()
            .rev()
            .find_map(|item| match item {
                Out::Lobby { conn: to, view } if *to == conn => Some(view.clone()),
                _ => None,
            })
            .expect("lobby view for connection")
    }

    #[test]
    fn custom_lobby_lifecycle_moves_host_controls_settings_and_rematch() {
        let now = Instant::now();
        let mut mm = Matchmaker::new(Duration::from_secs(15), seeded_random);
        for id in 1u64..=8 {
            mm.connect(id, u128::from(id), now);
        }

        // Create a 2v2 lobby; host lands on Team A with a 6-char code.
        let out = mm.create_lobby(1, Mode::Team2v2, now);
        let view = lobby_of(&out, 1);
        assert_eq!(view.code.len(), CODE_LENGTH);
        assert_eq!(view.mode, Mode::Team2v2);
        assert!(view.host);
        assert!(!view.can_start);
        assert_eq!(view.spectator_slots, MAX_LOBBY_SPECTATORS);
        assert_eq!(view.members.len(), 1);
        assert_eq!(view.members[0].slot, LobbySlot::TeamA);

        // Cannot queue publicly or join a party while inside a custom lobby.
        assert_eq!(
            mm.queue(1, Mode::Duel, now),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::InLobby
            }]
        );
        assert_eq!(
            mm.create_party(1, now),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::InLobby
            }]
        );

        // Invalid code is refused cleanly.
        assert_eq!(
            mm.join_lobby(2, "ZZZZZZ", now),
            vec![Out::Refused {
                conn: 2,
                reason: Refused::NoSuchLobby
            }]
        );

        // Joining balances teams (A, B, A, B), then fills spectator slots.
        let code = view.code.clone();
        let out = mm.join_lobby(2, &code.to_ascii_lowercase(), now);
        assert_eq!(lobby_of(&out, 2).members[1].slot, LobbySlot::TeamB);
        let out = mm.join_lobby(3, &code, now);
        assert_eq!(lobby_of(&out, 3).members[2].slot, LobbySlot::TeamA);
        // Not full yet (2 on A, 1 on B) -> host cannot start.
        assert_eq!(
            mm.start_lobby(1),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::LobbyNotFull
            }]
        );
        // Non-host cannot start either.
        assert_eq!(
            mm.start_lobby(2),
            vec![Out::Refused {
                conn: 2,
                reason: Refused::NotHost
            }]
        );

        let out = mm.join_lobby(4, &code, now);
        let full = lobby_of(&out, 1);
        assert!(full.can_start);
        assert_eq!(full.members[3].slot, LobbySlot::TeamB);

        // A 5th player joins as spectator.
        let out = mm.join_lobby(5, &code, now);
        assert_eq!(lobby_of(&out, 5).members[4].slot, LobbySlot::Spectator);

        // Non-host cannot displace someone on a full team.
        assert_eq!(
            mm.set_lobby_slot(2, LobbySlot::TeamA),
            vec![Out::Refused {
                conn: 2,
                reason: Refused::SlotFull
            }]
        );
        // Host CAN swap two players across full teams.
        let out = mm.move_lobby_member(1, 2, LobbySlot::TeamA);
        let swapped = lobby_of(&out, 1);
        assert_eq!(
            swapped.members.iter().find(|m| m.id == 2).unwrap().slot,
            LobbySlot::TeamA
        );
        assert_eq!(
            swapped.members.iter().find(|m| m.id == 3).unwrap().slot,
            LobbySlot::TeamB
        );

        // Non-host can move themselves to spectator, then back to open team slot.
        let out = mm.set_lobby_slot(4, LobbySlot::Spectator);
        assert!(!lobby_of(&out, 1).can_start);
        let out = mm.set_lobby_slot(4, LobbySlot::TeamB);
        assert!(lobby_of(&out, 1).can_start);

        // Versioned settings validation: invalid rejected, valid applied.
        let bad = MatchSettings {
            version: MATCH_SETTINGS_VERSION,
            map: MapId::Colosseum,
            round_seconds: 45,
            allow_spectators: true,
        };
        assert_eq!(
            mm.set_lobby_settings(1, bad),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::InvalidSettings
            }]
        );
        // Disabling spectators while spectator 5 is present is refused.
        assert_eq!(
            mm.set_lobby_settings(
                1,
                MatchSettings {
                    allow_spectators: false,
                    ..MatchSettings::default()
                }
            ),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::SpectatorsOccupied
            }]
        );
        let custom = MatchSettings {
            version: MATCH_SETTINGS_VERSION,
            map: MapId::Colosseum,
            round_seconds: 300,
            allow_spectators: true,
        };
        let out = mm.set_lobby_settings(1, custom);
        assert_eq!(lobby_of(&out, 1).settings, custom);

        // Starting issues seats in interleaved team order (A0, B0, A1, B1) + spectators.
        let out = mm.start_lobby(1);
        assert_eq!(
            out,
            vec![Out::StartCustom {
                host: 1,
                mode: Mode::Team2v2,
                settings: custom,
                seats: vec![1, 3, 2, 4],
                spectators: vec![5],
            }]
        );

        // Rematch: lobby is still intact, host can start again immediately!
        let rematch = mm.start_lobby(1);
        assert_eq!(rematch, out);

        // Host transfers host role to member 2, then original host is no longer host.
        let out = mm.transfer_lobby_host(1, 2);
        assert!(!lobby_of(&out, 1).host);
        assert!(lobby_of(&out, 2).host);
        assert_eq!(
            mm.kick_lobby_member(1, 5),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::NotHost
            }]
        );

        // New host (2) kicks spectator 5.
        let out = mm.kick_lobby_member(2, 5);
        assert!(out.contains(&Out::LobbyLeft {
            conn: 5,
            reason: LobbyLeft::Kicked
        }));
        assert!(!mm.in_custom_lobby(5));

        // Shrinking 2v2 -> 1v1 moves overflow team members to spectators.
        let out = mm.set_lobby_mode(2, Mode::Duel);
        let shrunk = lobby_of(&out, 2);
        assert_eq!(shrunk.mode, Mode::Duel);
        assert!(shrunk.can_start);
        assert_eq!(
            shrunk
                .members
                .iter()
                .filter(|m| m.slot == LobbySlot::Spectator)
                .count(),
            2
        );

        // Host disconnecting passes host to the next remaining member.
        let out = mm.disconnect(2, now);
        assert!(lobby_of(&out, 1).host);
    }

    #[test]
    fn custom_lobby_refuses_overflow_join_and_overflow_shrink() {
        let now = Instant::now();
        let mut mm = Matchmaker::new(Duration::from_secs(15), seeded_random);
        for id in 1u64..=12 {
            mm.connect(id, u128::from(id), now);
        }
        let code = lobby_of(&mm.create_lobby(1, Mode::Duel, now), 1).code;
        // 1v1 holds 2 players + 4 spectators = 6 total.
        for id in 2u64..=6 {
            let out = mm.join_lobby(id, &code, now);
            assert_eq!(lobby_of(&out, id).members.len(), id as usize);
        }
        // 7th player is refused with LobbyFull.
        assert_eq!(
            mm.join_lobby(7, &code, now),
            vec![Out::Refused {
                conn: 7,
                reason: Refused::LobbyFull
            }]
        );

        // Grow to 2v2, fill 2 more seats (now 4 players + 4 spectators = 8).
        mm.set_lobby_mode(1, Mode::Team2v2);
        assert_eq!(mm.join_lobby(7, &code, now).len(), 7);
        assert_eq!(mm.join_lobby(8, &code, now).len(), 8);
        // Shrinking back to 1v1 would need 2 extra spectator slots (4 + 2 > 4) -> refused!
        assert_eq!(
            mm.set_lobby_mode(1, Mode::Duel),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::LobbyTooBig
            }]
        );
    }

    #[test]
    fn custom_lobby_enforces_host_authority_party_exclusion_and_leave_cleanup() {
        let now = Instant::now();
        let mut mm = Matchmaker::new(Duration::from_secs(15), seeded_random);
        for id in 1u64..=5 {
            mm.connect(id, u128::from(id), now);
        }

        // Forming a party and queueing blocks entering a custom lobby (PartyBusy).
        let party_code = mm
            .create_party(1, now)
            .into_iter()
            .find_map(|item| match item {
                Out::Party { view, .. } => view.code,
                _ => None,
            })
            .unwrap();
        mm.join_party(2, &party_code, now);
        mm.queue(1, Mode::Team2v2, now);
        assert_eq!(
            mm.create_lobby(1, Mode::Duel, now),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::PartyBusy
            }]
        );
        mm.cancel(1);

        // Creating a lobby from an idle multi-member party cleanly leaves that party.
        let lobby_code = lobby_of(&mm.create_lobby(1, Mode::Duel, now), 1).code;
        // Cross-namespace codes never match each other.
        assert_eq!(
            mm.join_lobby(3, &party_code, now),
            vec![Out::Refused {
                conn: 3,
                reason: Refused::NoSuchLobby
            }]
        );
        assert_eq!(
            mm.join_party(3, &lobby_code, now),
            vec![Out::Refused {
                conn: 3,
                reason: Refused::NoSuchParty
            }]
        );
        // While in a lobby, joining a party is refused with InLobby.
        assert_eq!(
            mm.join_party(1, &party_code, now),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::InLobby
            }]
        );

        mm.join_lobby(3, &lobby_code, now);
        // Re-joining the same lobby only updates the caller, not other members.
        let rejoin = mm.join_lobby(3, &lobby_code, now);
        assert_eq!(rejoin.len(), 1);
        assert!(matches!(rejoin[0], Out::Lobby { conn: 3, .. }));

        // Non-host is refused every host-only action.
        assert_eq!(
            mm.move_lobby_member(3, 1, LobbySlot::Spectator),
            vec![Out::Refused {
                conn: 3,
                reason: Refused::NotHost
            }]
        );
        assert_eq!(
            mm.transfer_lobby_host(3, 1),
            vec![Out::Refused {
                conn: 3,
                reason: Refused::NotHost
            }]
        );
        assert_eq!(
            mm.set_lobby_mode(3, Mode::Team2v2),
            vec![Out::Refused {
                conn: 3,
                reason: Refused::NotHost
            }]
        );
        assert_eq!(
            mm.set_lobby_settings(3, MatchSettings::default()),
            vec![Out::Refused {
                conn: 3,
                reason: Refused::NotHost
            }]
        );

        // Self-kick and self-transfer are refused cleanly.
        assert_eq!(
            mm.kick_lobby_member(1, 1),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::NoSuchMember
            }]
        );
        assert_eq!(
            mm.transfer_lobby_host(1, 1),
            vec![Out::Refused {
                conn: 1,
                reason: Refused::NoSuchMember
            }]
        );

        // Host leaving voluntarily passes host to the next member.
        let out = mm.leave_lobby(1);
        assert!(out.contains(&Out::LobbyLeft {
            conn: 1,
            reason: LobbyLeft::Left
        }));
        assert!(lobby_of(&out, 3).host);

        // Last member leaving destroys the lobby and invalidates its invite code.
        let out = mm.leave_lobby(3);
        assert_eq!(
            out,
            vec![Out::LobbyLeft {
                conn: 3,
                reason: LobbyLeft::Left
            }]
        );
        assert_eq!(
            mm.join_lobby(4, &lobby_code, now),
            vec![Out::Refused {
                conn: 4,
                reason: Refused::NoSuchLobby
            }]
        );
    }
}
