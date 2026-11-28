//! 1v1 challenges between players in a hub world (ADR 0060), owned by
//! [`Matchmaker`].
//!
//! A player challenges another by name from the unit menu; the door has
//! already found the target in the challenger's world and checked blocks.
//! Each player has at most one open challenge of their own (a new one
//! withdraws the old) and holds at most [`MAX_INCOMING`] from others. A
//! challenge lasts [`CHALLENGE_TTL`]. Accepting asks for a 1v1 match
//! ([`Out::StartChallenge`]) when both players are free: not searching, in a
//! ready check or in a custom lobby. Parties are untouched: the two play
//! alone and stay in their parties.
//!
//! The matchmaker knows no names, so a challenge keeps both players'
//! handles as the door gave them, for the messages it sends.

use std::time::Duration;

use tokio::time::Instant;

use super::{Matchmaker, Out, QueueConn, Refused, State};

/// How long a challenge waits for an answer.
pub const CHALLENGE_TTL: Duration = Duration::from_secs(30);
/// Challenges from others a player holds at once.
pub const MAX_INCOMING: usize = 3;
/// A player sends at most one challenge this often.
pub const CHALLENGE_GAP: Duration = Duration::from_secs(2);

/// Why a challenge closed without a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChallengeEnded {
    /// The challenged player declined.
    Declined,
    /// Nobody answered in time.
    Expired,
    /// The challenger challenged someone else, or left.
    Withdrawn,
    /// The challenged player left the hub or the game.
    Left,
    /// Accepted, but the challenger was searching or in a custom lobby.
    Busy,
}
impl ChallengeEnded {
    pub fn code(self) -> &'static str {
        match self {
            ChallengeEnded::Declined => "declined",
            ChallengeEnded::Expired => "expired",
            ChallengeEnded::Withdrawn => "withdrawn",
            ChallengeEnded::Left => "left",
            ChallengeEnded::Busy => "busy",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Challenge {
    pub(super) to: QueueConn,
    pub(super) from_handle: String,
    pub(super) to_handle: String,
    pub(super) deadline: Instant,
}

impl Matchmaker {
    /// Why `conn` cannot play a challenge now, if it cannot.
    fn challenge_busy(&self, conn: QueueConn) -> Option<Refused> {
        if self.in_custom_lobby(conn) {
            return Some(Refused::InLobby);
        }
        let id = self.of.get(&conn)?;
        (self.parties[id].state != State::Idle).then_some(Refused::PartyBusy)
    }

    /// Whether `conn` may try a challenge now; spends its turn when it may.
    /// The door asks before it looks for the player named, so every attempt
    /// counts, found or not.
    pub fn challenge_turn(&mut self, conn: QueueConn, now: Instant) -> bool {
        if !self.of.contains_key(&conn)
            || self
                .challenge_sent
                .get(&conn)
                .is_some_and(|at| now.saturating_duration_since(*at) < CHALLENGE_GAP)
        {
            return false;
        }
        self.challenge_sent.insert(conn, now);
        true
    }

    /// `from` (`from_handle`) challenges `to` (`to_handle`) to a 1v1. The
    /// door has spent `from`'s [`Self::challenge_turn`] and checked that
    /// both are in the same hub world and neither blocked the other.
    pub fn challenge(
        &mut self,
        from: QueueConn,
        to: QueueConn,
        from_handle: String,
        to_handle: String,
        now: Instant,
    ) -> Vec<Out> {
        let refuse = |reason| vec![Out::Refused { conn: from, reason }];
        if from == to || !self.of.contains_key(&from) || !self.of.contains_key(&to) {
            return refuse(Refused::NoSuchPlayer);
        }
        if let Some(reason) = self.challenge_busy(from) {
            return refuse(reason);
        }
        let mut incoming = self
            .challenges
            .iter()
            .filter(|(sender, challenge)| challenge.to == to && **sender != from);
        // Answers name the challenger, so two of one name cannot both wait
        // (guest handles repeat).
        if incoming
            .clone()
            .any(|(_, challenge)| challenge.from_handle.eq_ignore_ascii_case(&from_handle))
            || incoming.by_ref().count() >= MAX_INCOMING
        {
            return refuse(Refused::ChallengeBusy);
        }
        let mut out = Vec::new();
        // One challenge each: a new one withdraws the old.
        if let Some(old) = self.challenges.remove(&from)
            && old.to != to
        {
            out.push(Out::ChallengeEnded {
                conn: old.to,
                with: old.from_handle,
                reason: ChallengeEnded::Withdrawn,
            });
        }
        self.challenges.insert(
            from,
            Challenge {
                to,
                from_handle: from_handle.clone(),
                to_handle: to_handle.clone(),
                deadline: now + CHALLENGE_TTL,
            },
        );
        out.push(Out::Challenged {
            conn: to,
            from: from_handle,
            remaining: CHALLENGE_TTL,
        });
        out.push(Out::ChallengeSent {
            conn: from,
            to: to_handle,
        });
        out
    }

    /// `to` answers the challenge from the player named `from_handle`.
    /// Accepting starts a 1v1 when both are free and still `together` (in
    /// one hub world); a challenged player who is busy is refused and keeps
    /// the challenge (to answer after the search). A pair no longer together
    /// is refused too: the door's next [`Self::prune_challenges`] closes it.
    pub fn answer_challenge(
        &mut self,
        to: QueueConn,
        from_handle: &str,
        accept: bool,
        together: impl Fn(QueueConn, QueueConn) -> bool,
        now: Instant,
    ) -> Vec<Out> {
        let found = self.challenges.iter().find_map(|(from, challenge)| {
            (challenge.to == to
                && now < challenge.deadline
                && challenge.from_handle.eq_ignore_ascii_case(from_handle))
            .then_some(*from)
        });
        let Some(from) = found.filter(|from| !accept || together(*from, to)) else {
            return vec![Out::Refused {
                conn: to,
                reason: Refused::ChallengeGone,
            }];
        };
        if accept && let Some(reason) = self.challenge_busy(to) {
            return vec![Out::Refused { conn: to, reason }];
        }
        let challenge = self.challenges.remove(&from).expect("found above");
        if !accept {
            return vec![Out::ChallengeEnded {
                conn: from,
                with: challenge.to_handle,
                reason: ChallengeEnded::Declined,
            }];
        }
        if self.challenge_busy(from).is_some() {
            return vec![
                Out::ChallengeEnded {
                    conn: to,
                    with: challenge.from_handle,
                    reason: ChallengeEnded::Busy,
                },
                Out::ChallengeEnded {
                    conn: from,
                    with: challenge.to_handle,
                    reason: ChallengeEnded::Busy,
                },
            ];
        }
        // A challenge the other way between the two ends quietly with this
        // match; neither player's other challenges stand once matched.
        if self
            .challenges
            .get(&to)
            .is_some_and(|reverse| reverse.to == from)
        {
            self.challenges.remove(&to);
        }
        let mut out = self.drop_challenges(from);
        out.extend(self.drop_challenges(to));
        out.push(Out::StartChallenge { seats: [from, to] });
        out
    }

    /// `conn` takes back its open challenge, if any; the other player is
    /// told it was withdrawn.
    pub fn withdraw_challenge(&mut self, conn: QueueConn) -> Vec<Out> {
        self.challenges
            .remove(&conn)
            .map(|sent| Out::ChallengeEnded {
                conn: sent.to,
                with: sent.from_handle,
                reason: ChallengeEnded::Withdrawn,
            })
            .into_iter()
            .collect()
    }

    /// Closes every challenge `conn` sent or holds (it left the hub, the
    /// game, or was matched), telling the other player.
    pub fn drop_challenges(&mut self, conn: QueueConn) -> Vec<Out> {
        let mut out = Vec::new();
        if let Some(sent) = self.challenges.remove(&conn) {
            out.push(Out::ChallengeEnded {
                conn: sent.to,
                with: sent.from_handle,
                reason: ChallengeEnded::Withdrawn,
            });
        }
        let mut held: Vec<QueueConn> = self
            .challenges
            .iter()
            .filter(|(_, challenge)| challenge.to == conn)
            .map(|(from, _)| *from)
            .collect();
        held.sort_unstable();
        for from in held {
            let challenge = self.challenges.remove(&from).expect("listed above");
            out.push(Out::ChallengeEnded {
                conn: from,
                with: challenge.to_handle,
                reason: ChallengeEnded::Left,
            });
        }
        out
    }

    /// Closes challenges whose two players are no longer `together` (in
    /// the same hub world), as [`Self::drop_challenges`] would for whichever
    /// left. The door calls it on its tick.
    pub fn prune_challenges(
        &mut self,
        together: impl Fn(QueueConn, QueueConn) -> bool,
        in_hub: impl Fn(QueueConn) -> bool,
    ) -> Vec<Out> {
        let mut apart: Vec<QueueConn> = self
            .challenges
            .iter()
            .filter(|(from, challenge)| !together(**from, challenge.to))
            .map(|(from, _)| *from)
            .collect();
        apart.sort_unstable();
        let mut out = Vec::new();
        for from in apart {
            let challenge = self.challenges.remove(&from).expect("listed above");
            // Whoever is still in the hub is told the other went.
            if in_hub(challenge.to) {
                out.push(Out::ChallengeEnded {
                    conn: challenge.to,
                    with: challenge.from_handle,
                    reason: ChallengeEnded::Withdrawn,
                });
            }
            if in_hub(from) {
                out.push(Out::ChallengeEnded {
                    conn: from,
                    with: challenge.to_handle,
                    reason: ChallengeEnded::Left,
                });
            }
        }
        out
    }

    /// Ends challenges whose time is up, telling both players.
    pub(super) fn expire_challenges(&mut self, now: Instant) -> Vec<Out> {
        let mut expired: Vec<QueueConn> = self
            .challenges
            .iter()
            .filter(|(_, challenge)| now >= challenge.deadline)
            .map(|(from, _)| *from)
            .collect();
        expired.sort_unstable();
        let mut out = Vec::new();
        for from in expired {
            let challenge = self.challenges.remove(&from).expect("listed above");
            out.push(Out::ChallengeEnded {
                conn: challenge.to,
                with: challenge.from_handle,
                reason: ChallengeEnded::Expired,
            });
            out.push(Out::ChallengeEnded {
                conn: from,
                with: challenge.to_handle,
                reason: ChallengeEnded::Expired,
            });
        }
        out
    }

    /// Open challenges, for metrics and tests.
    pub fn challenges(&self) -> usize {
        self.challenges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matchmaker::Mode;

    fn mm() -> Matchmaker {
        Matchmaker::new(Duration::from_secs(15), |bound| bound / 2)
    }
    fn connected(conns: &[QueueConn], now: Instant) -> Matchmaker {
        let mut mm = mm();
        for conn in conns {
            mm.connect(*conn, u128::from(*conn), now);
        }
        mm
    }
    fn name(conn: QueueConn) -> String {
        format!("Player{conn}#0000")
    }
    /// As the door does: a turn first, then the challenge.
    fn send(mm: &mut Matchmaker, from: QueueConn, to: QueueConn, now: Instant) -> Vec<Out> {
        assert!(mm.challenge_turn(from, now), "{from} has a turn");
        mm.challenge(from, to, name(from), name(to), now)
    }
    fn ended(conn: QueueConn, with: QueueConn, reason: ChallengeEnded) -> Out {
        Out::ChallengeEnded {
            conn,
            with: name(with),
            reason,
        }
    }
    fn refused(conn: QueueConn, reason: Refused) -> Vec<Out> {
        vec![Out::Refused { conn, reason }]
    }
    /// Every pair shares a world.
    fn both(_: QueueConn, _: QueueConn) -> bool {
        true
    }

    #[test]
    fn an_accepted_challenge_starts_a_1v1_for_the_two() {
        let now = Instant::now();
        let mut mm = connected(&[1, 2], now);
        assert_eq!(
            send(&mut mm, 1, 2, now),
            vec![
                Out::Challenged {
                    conn: 2,
                    from: name(1),
                    remaining: CHALLENGE_TTL,
                },
                Out::ChallengeSent {
                    conn: 1,
                    to: name(2),
                },
            ]
        );
        assert_eq!(mm.challenges(), 1);
        // Answered by name, in any case.
        let out = mm.answer_challenge(2, &name(1).to_uppercase(), true, both, now);
        assert_eq!(out, vec![Out::StartChallenge { seats: [1, 2] }]);
        assert_eq!(mm.challenges(), 0);
        // Answered once only.
        assert_eq!(
            mm.answer_challenge(2, &name(1), true, both, now),
            refused(2, Refused::ChallengeGone)
        );
    }

    #[test]
    fn declining_tells_the_challenger_and_strangers_cannot_answer() {
        let now = Instant::now();
        let mut mm = connected(&[1, 2, 3], now);
        send(&mut mm, 1, 2, now);
        // Only the challenged player answers it.
        assert_eq!(
            mm.answer_challenge(3, &name(1), true, both, now),
            refused(3, Refused::ChallengeGone)
        );
        assert_eq!(
            mm.answer_challenge(2, &name(3), true, both, now),
            refused(2, Refused::ChallengeGone)
        );
        // Accepting needs the pair still in one world; declining does not.
        assert_eq!(
            mm.answer_challenge(2, &name(1), true, |_, _| false, now),
            refused(2, Refused::ChallengeGone)
        );
        assert_eq!(
            mm.answer_challenge(2, &name(1), false, |_, _| false, now),
            vec![ended(1, 2, ChallengeEnded::Declined)]
        );
        assert_eq!(mm.challenges(), 0);
    }

    #[test]
    fn challenges_expire_for_both_players() {
        let now = Instant::now();
        let mut mm = connected(&[1, 2], now);
        send(&mut mm, 1, 2, now);
        let almost = now + CHALLENGE_TTL - Duration::from_millis(1);
        assert_eq!(mm.tick(almost), Vec::new());
        assert_eq!(
            mm.tick(now + CHALLENGE_TTL),
            vec![
                ended(2, 1, ChallengeEnded::Expired),
                ended(1, 2, ChallengeEnded::Expired),
            ]
        );
        assert_eq!(mm.challenges(), 0);
        // Even an answer racing the tick finds it gone.
        let mut mm = connected(&[1, 2], now);
        send(&mut mm, 1, 2, now);
        assert_eq!(
            mm.answer_challenge(2, &name(1), true, both, now + CHALLENGE_TTL),
            refused(2, Refused::ChallengeGone)
        );
    }

    #[test]
    fn one_challenge_each_rate_limited_and_a_few_held_at_once() {
        let now = Instant::now();
        let mut mm = connected(&[1, 2, 3, 4, 5, 6], now);
        // Every attempt spends the turn, even one naming nobody.
        assert_eq!(send(&mut mm, 1, 1, now), refused(1, Refused::NoSuchPlayer));
        assert!(!mm.challenge_turn(1, now));
        let now = now + CHALLENGE_GAP;
        assert_eq!(send(&mut mm, 1, 9, now), refused(1, Refused::NoSuchPlayer));
        let now = now + CHALLENGE_GAP;
        send(&mut mm, 1, 2, now);
        assert!(!mm.challenge_turn(1, now + CHALLENGE_GAP - Duration::from_millis(1)));
        assert!(
            !mm.challenge_turn(77, now),
            "an unknown connection has none"
        );
        // A new challenge withdraws the old one.
        let later = now + CHALLENGE_GAP;
        let out = send(&mut mm, 1, 3, later);
        assert_eq!(out[0], ended(2, 1, ChallengeEnded::Withdrawn));
        assert_eq!(mm.challenges(), 1);
        // Challenging the same player again only renews it.
        let again = send(&mut mm, 1, 3, later + CHALLENGE_GAP);
        assert!(matches!(again[0], Out::Challenged { conn: 3, .. }));
        assert_eq!(mm.challenges(), 1);
        // Player 3 holds at most MAX_INCOMING.
        send(&mut mm, 2, 3, later);
        send(&mut mm, 4, 3, later);
        assert_eq!(mm.challenges(), MAX_INCOMING);
        assert_eq!(
            send(&mut mm, 5, 3, later),
            refused(5, Refused::ChallengeBusy)
        );
        // ...but someone already holding one may renew theirs.
        assert!(matches!(
            send(&mut mm, 4, 3, later + CHALLENGE_GAP)[0],
            Out::Challenged { conn: 3, .. }
        ));
        // Two challengers of one name cannot both wait on one player: the
        // answer could not tell them apart.
        let mut mm = connected(&[1, 2, 3], now);
        assert!(mm.challenge_turn(1, now));
        mm.challenge(1, 3, "Guest#0007".into(), name(3), now);
        assert!(mm.challenge_turn(2, now));
        assert_eq!(
            mm.challenge(2, 3, "guest#0007".into(), name(3), now),
            refused(2, Refused::ChallengeBusy)
        );
        // Withdrawing tells the challenged player.
        assert_eq!(
            mm.withdraw_challenge(1),
            vec![Out::ChallengeEnded {
                conn: 3,
                with: "Guest#0007".into(),
                reason: ChallengeEnded::Withdrawn,
            }]
        );
        assert_eq!(mm.withdraw_challenge(1), Vec::new());
        assert_eq!(mm.challenges(), 0);
    }

    #[test]
    fn busy_players_neither_challenge_nor_play() {
        let now = Instant::now();
        let mut mm = connected(&[1, 2, 3], now);
        // Searching: no challenge out.
        mm.queue(3, Mode::Duel, now);
        assert_eq!(send(&mut mm, 3, 1, now), refused(3, Refused::PartyBusy));
        // A challenged player who is searching keeps the challenge until free.
        send(&mut mm, 1, 3, now);
        assert_eq!(
            mm.answer_challenge(3, &name(1), true, both, now),
            refused(3, Refused::PartyBusy)
        );
        // Free again, it can play.
        mm.cancel(3);
        assert_eq!(
            mm.answer_challenge(3, &name(1), true, both, now),
            vec![Out::StartChallenge { seats: [1, 3] }]
        );
        // A challenger who went searching meanwhile: both are told.
        send(&mut mm, 2, 1, now + CHALLENGE_GAP);
        mm.queue(2, Mode::Duel, now);
        assert_eq!(
            mm.answer_challenge(1, &name(2), true, both, now),
            vec![
                ended(1, 2, ChallengeEnded::Busy),
                ended(2, 1, ChallengeEnded::Busy),
            ]
        );
        // In a custom lobby: refused.
        mm.cancel(2);
        mm.create_lobby(2, Mode::Duel, now);
        assert_eq!(
            send(&mut mm, 2, 1, now + 2 * CHALLENGE_GAP),
            refused(2, Refused::InLobby)
        );
    }

    #[test]
    fn a_match_drops_both_players_other_challenges() {
        let now = Instant::now();
        let mut mm = connected(&[1, 2, 3, 4], now);
        send(&mut mm, 1, 2, now);
        send(&mut mm, 3, 1, now);
        send(&mut mm, 2, 4, now);
        let out = mm.answer_challenge(2, &name(1), true, both, now);
        // 3's challenge to 1 and 2's to 4 are gone; then the match.
        assert_eq!(
            out,
            vec![
                ended(3, 1, ChallengeEnded::Left),
                ended(4, 2, ChallengeEnded::Withdrawn),
                Out::StartChallenge { seats: [1, 2] },
            ]
        );
        assert_eq!(mm.challenges(), 0);
    }

    #[test]
    fn challenging_each_other_ends_quietly_in_one_match() {
        let now = Instant::now();
        let mut mm = connected(&[1, 2], now);
        send(&mut mm, 1, 2, now);
        send(&mut mm, 2, 1, now);
        assert_eq!(mm.challenges(), 2);
        assert_eq!(
            mm.answer_challenge(2, &name(1), true, both, now),
            vec![Out::StartChallenge { seats: [1, 2] }]
        );
        assert_eq!(mm.challenges(), 0);
    }

    #[test]
    fn leaving_the_game_or_the_hub_closes_challenges() {
        let now = Instant::now();
        let mut mm = connected(&[1, 2, 3], now);
        send(&mut mm, 1, 2, now);
        send(&mut mm, 3, 1, now);
        let out = mm.disconnect(1, now);
        assert!(out.contains(&ended(2, 1, ChallengeEnded::Withdrawn)));
        assert!(out.contains(&ended(3, 1, ChallengeEnded::Left)));
        assert_eq!(mm.challenges(), 0);
        // Out of the same world: whoever stayed is told.
        let mut mm = connected(&[1, 2], now);
        send(&mut mm, 1, 2, now);
        assert_eq!(mm.prune_challenges(|_, _| true, |_| true), Vec::new());
        assert_eq!(
            mm.prune_challenges(|_, _| false, |conn| conn == 1),
            vec![ended(1, 2, ChallengeEnded::Left)]
        );
        assert_eq!(mm.challenges(), 0);
    }
}
