use super::*;
use crate::world::WorldReport;

fn none(_: QueueConn, _: QueueConn) -> bool {
    false
}

fn alone() -> Placement<'static> {
    Placement::default()
}

fn reply(worlds: &[(u64, &[u64])]) -> DirectoryReply {
    DirectoryReply {
        draining: false,
        worlds: worlds
            .iter()
            .map(|(instance, players)| WorldReport {
                instance: format!("{instance:016x}"),
                players: players.to_vec(),
                capacity: HUB_PLAYER_CAP,
            })
            .collect(),
    }
}

#[test]
fn players_go_to_the_fullest_world_with_room_or_a_new_one() {
    let now = Instant::now();
    let mut worlds = Worlds::default();
    assert_eq!(worlds.place(1, 101, alone(), none, || 7, now), Ok(7));
    assert_eq!(worlds.place(2, 102, alone(), none, || 8, now), Ok(7));
    assert_eq!(
        worlds.place(1, 101, alone(), none, || 9, now),
        Err("in_hub")
    );
    // Fill world 7; the next arrival opens another.
    for conn in 3..=HUB_PLAYER_CAP as u64 {
        assert_eq!(
            worlds.place(conn, 100 + conn, alone(), none, || 99, now),
            Ok(7)
        );
    }
    assert_eq!(worlds.place(50, 150, alone(), none, || 8, now), Ok(8));
    // Someone leaves world 7: it is the fullest with room again.
    assert!(worlds.leave(2));
    assert!(!worlds.leave(2));
    assert_eq!(worlds.place(51, 151, alone(), none, || 9, now), Ok(7));
    // Chat listens only to placements the world has confirmed.
    assert_eq!(worlds.members(1), vec![1]);
    let in_seven: Vec<u64> = worlds
        .placed
        .values()
        .filter(|placed| placed.instance == 7)
        .map(|placed| placed.player)
        .collect();
    worlds.reconcile(&reply(&[(7, &in_seven)]), now);
    assert_eq!(worlds.members(1).len(), HUB_PLAYER_CAP);
    assert!(worlds.members(999).is_empty());
    assert_eq!(worlds.counts(), (HUB_PLAYER_CAP + 1, 2));
}

#[test]
fn following_goes_beside_the_friend_or_is_refused() {
    let now = Instant::now();
    let mut worlds = Worlds::default();
    worlds.place(1, 101, alone(), none, || 7, now).unwrap();
    let follow = |friend: &'static [QueueConn]| Placement {
        follow: friend,
        following: true,
        party: &[],
    };
    assert_eq!(worlds.place(3, 103, follow(&[1]), none, || 9, now), Ok(7));
    assert_eq!(
        worlds.place(4, 104, follow(&[5]), none, || 9, now),
        Err("not_in_hub")
    );
    // A friend who blocked you (or whom you blocked) cannot be found.
    let blocked = |a: QueueConn, b: QueueConn| (a, b) == (4, 1);
    assert_eq!(
        worlds.place(4, 104, follow(&[1]), blocked, || 9, now),
        Err("not_in_hub")
    );
    // World 7 holds 1 and 3; fill it.
    for conn in 10..10 + HUB_PLAYER_CAP as u64 - 2 {
        worlds
            .place(conn, conn + 100, follow(&[1]), none, || 9, now)
            .unwrap();
    }
    assert_eq!(
        worlds.place(4, 104, follow(&[1]), none, || 9, now),
        Err("hub_full")
    );
}

/// Puts `conn` straight into `instance` (as an earlier placement would).
fn put(worlds: &mut Worlds, conn: QueueConn, instance: u64, now: Instant) {
    let player = 1000 + conn;
    worlds.placed.insert(
        conn,
        Placed {
            instance,
            player,
            issued: now,
            confirmed: true,
        },
    );
    worlds.by_player.insert(player, conn);
    worlds.last.insert(conn, instance);
}

#[test]
fn party_mates_and_the_last_world_come_before_the_fullest() {
    let now = Instant::now();
    let mut worlds = Worlds::default();
    // World 7 is the fullest (three); 8 and 9 hold one each.
    for conn in 1..=3 {
        put(&mut worlds, conn, 7, now);
    }
    put(&mut worlds, 10, 8, now);
    put(&mut worlds, 11, 9, now);
    // A party-mate's world first.
    let party = Placement {
        follow: &[],
        following: false,
        party: &[10],
    };
    assert_eq!(worlds.place(4, 104, party, none, || 99, now), Ok(8));
    // Then the world this connection was last in.
    worlds.last.insert(5, 9);
    assert_eq!(worlds.place(5, 105, alone(), none, || 99, now), Ok(9));
    // A party-mate beats the last world.
    worlds.last.insert(6, 9);
    assert_eq!(worlds.place(6, 106, party, none, || 99, now), Ok(8));
    // Otherwise the fullest with room.
    assert_eq!(worlds.place(12, 112, alone(), none, || 99, now), Ok(7));
}

#[test]
fn block_pairs_name_players_sharing_a_world() {
    let now = Instant::now();
    let mut worlds = Worlds::default();
    worlds.place(1, 501, alone(), none, || 7, now).unwrap();
    worlds.place(2, 302, alone(), none, || 7, now).unwrap();
    worlds.place(3, 303, alone(), none, || 7, now).unwrap();
    let full: Vec<QueueConn> = (10..10 + HUB_PLAYER_CAP as u64 - 3).collect();
    for conn in &full {
        worlds
            .place(*conn, conn + 1000, alone(), none, || 7, now)
            .unwrap();
    }
    worlds.place(4, 404, alone(), none, || 8, now).unwrap();
    let hidden = |a: QueueConn, b: QueueConn| {
        let pair = (a.min(b), a.max(b));
        pair == (1, 2) || pair == (1, 4)
    };
    // 1 and 4 are in different worlds: no pair for them.
    assert_eq!(worlds.block_pairs(hidden), vec![(302, 501)]);
}

#[test]
fn the_directory_confirms_placements_and_drops_the_departed() {
    let start = Instant::now();
    let mut worlds = Worlds::default();
    worlds.place(1, 101, alone(), none, || 7, start).unwrap();
    worlds.place(2, 102, alone(), none, || 7, start).unwrap();
    // 1 connected; 2 has not yet (and is within its window).
    assert!(worlds.reconcile(&reply(&[(7, &[101])]), start).is_empty());
    // 1 leaves the world; 2 never came.
    let later = start + CONFIRM_WITHIN;
    assert_eq!(worlds.reconcile(&reply(&[(7, &[])]), later), vec![1, 2]);
    assert!(worlds.is_empty());
    assert_eq!(worlds.instance_of(1), None);
}

#[test]
fn players_the_world_still_holds_count_toward_its_room() {
    let now = Instant::now();
    let mut worlds = Worlds::default();
    worlds.place(1, 101, alone(), none, || 7, now).unwrap();
    // The world reports eleven more (players placed by an earlier door,
    // or in their grace after the door forgot them).
    let others: Vec<u64> = (900..900 + HUB_PLAYER_CAP as u64 - 1).collect();
    let mut all = vec![101];
    all.extend(&others);
    worlds.reconcile(&reply(&[(7, &all)]), now);
    assert_eq!(worlds.place(2, 102, alone(), none, || 8, now), Ok(8));
}

#[test]
fn player_ids_belong_to_one_connection() {
    let now = Instant::now();
    let mut worlds = Worlds::default();
    assert_eq!(worlds.player_for(1, || 55), 55);
    assert_eq!(worlds.player_for(1, || 66), 55, "stable while open");
    worlds.place(1, 55, alone(), none, || 7, now).unwrap();
    assert_eq!(
        worlds.place(2, 55, alone(), none, || 7, now),
        Err("bad_request")
    );
    worlds.gone(1);
    assert_eq!(worlds.player_for(1, || 77), 77);
}
