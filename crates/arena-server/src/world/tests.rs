use super::*;
use arena_protocol::{Handle, WireAction, WireOccupant};

const TRAINING_ENEMIES: usize = arena_sim::hub::TRAINING_POSTS.len();

/// Every snapshot in full, so tests read each tick's alone; deltas have
/// tests of their own.
const LIMITS: WorldLimits = WorldLimits {
    queue: 32,
    grace: 5,
    idle: 20,
    keyframe_every: 1,
};

fn world() -> World {
    World::new(0xAB, Arc::default(), None, LIMITS, Arc::default())
}

fn sent(out: &[Out], to: ConnId) -> Vec<ServerMessage> {
    out.iter()
        .filter_map(|out| match out {
            Out::Send { conn, message } if *conn == to => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn closed(out: &[Out], to: ConnId) -> Option<Option<ErrorCode>> {
    out.iter().find_map(|out| match out {
        Out::Close { conn, error } if *conn == to => Some(*error),
        _ => None,
    })
}

fn join(world: &mut World, player: u64, conn: ConnId) -> Vec<Out> {
    world.attach(
        player,
        conn,
        Handle::new("Rook", player as u16),
        Tags::default(),
        None,
    )
}

fn snapshot_for(out: &[Out], to: ConnId) -> Snapshot {
    sent(out, to)
        .into_iter()
        .rev()
        .find_map(|message| match message {
            ServerMessage::Snapshot(snapshot) => Some(snapshot),
            _ => None,
        })
        .expect("a snapshot")
}

fn member_ids(messages: &[ServerMessage]) -> Vec<u32> {
    messages
        .iter()
        .filter_map(|message| match message {
            ServerMessage::Member(member) => Some(member.id),
            _ => None,
        })
        .collect()
}

fn input(seq: u32, x: i16) -> InputFrame {
    InputFrame {
        seq,
        movement: [x, 0],
        facing: 0,
        jump: false,
        action: WireAction::None,
        view_tick: 0,
        cast_buffer: 0,
    }
}

#[test]
fn arrivals_are_welcomed_into_a_slot_and_hear_who_is_there() {
    let mut world = world();
    let out = join(&mut world, 1, 10);
    let messages = sent(&out, 10);
    let ServerMessage::Welcome(welcome) = &messages[0] else {
        panic!("welcome first: {messages:?}");
    };
    assert!(welcome.hub && !welcome.spectator);
    assert_eq!(usize::from(welcome.actor), TRAINING_ENEMIES);
    assert_eq!(welcome.team_size, 0);
    let first_id = world.arena().slots()[TRAINING_ENEMIES].id;
    assert_eq!(member_ids(&messages), vec![first_id]);
    let snapshot = snapshot_for(&out, 10);
    assert!(snapshot.hub_arena().is_some());
    assert_eq!(snapshot.hash, world.arena().state_hash());

    // The second arrival: the first hears of them, they hear of both.
    let out = join(&mut world, 2, 20);
    let second_id = world.arena().slots()[TRAINING_ENEMIES + 1].id;
    assert_eq!(member_ids(&sent(&out, 10)), vec![second_id]);
    assert_eq!(member_ids(&sent(&out, 20)), vec![first_id, second_id]);
    assert_eq!(world.report().players, vec![1, 2]);
    assert_eq!(world.report().instance, "00000000000000ab");
}

#[test]
fn a_full_world_turns_arrivals_away() {
    let mut world = world();
    for player in 0..HUB_PLAYER_CAP as u64 {
        assert!(closed(&join(&mut world, player, player), player).is_none());
    }
    let out = join(&mut world, 99, 99);
    assert_eq!(closed(&out, 99), Some(Some(ErrorCode::MatchFull)));
}

#[test]
fn a_reconnect_takes_the_same_place_back_and_replaces_the_old_connection() {
    let mut world = world();
    join(&mut world, 1, 10);
    let slot = world.players[0].slot;
    world.receive(10, input(1, 1_000));
    world.tick();
    let out = join(&mut world, 1, 11);
    assert_eq!(closed(&out, 10), Some(Some(ErrorCode::Replaced)));
    let ServerMessage::Welcome(welcome) = &sent(&out, 11)[0] else {
        panic!("welcome");
    };
    assert_eq!(usize::from(welcome.actor), slot);
    assert_eq!(world.player_count(), 1);
    // The new connection starts its own sequence.
    world.receive(11, input(1, 1_000));
    assert_eq!(world.players[0].pending.len(), 1);
    // The old one no longer acts.
    world.receive(10, input(5, 1_000));
    assert_eq!(world.players[0].pending.len(), 1);
}

#[test]
fn inputs_move_players_one_per_tick_and_are_acknowledged() {
    let mut world = world();
    join(&mut world, 1, 10);
    let slot = world.players[0].slot;
    let before = world.arena().fighters()[slot].position;
    for seq in 1..=3 {
        world.receive(10, input(seq, 1_000));
    }
    world.receive(10, input(2, 1_000)); // a repeat: dropped
    let out = world.tick();
    let snapshot = snapshot_for(&out, 10);
    assert_eq!(snapshot.ack, 1);
    assert_eq!(snapshot.queued, 2);
    assert!(world.arena().fighters()[slot].position.x > before.x);
    world.tick();
    let out = world.tick();
    assert_eq!(snapshot_for(&out, 10).ack, 3);
}

#[test]
fn players_who_stay_away_leave_after_their_grace() {
    let mut world = world();
    join(&mut world, 1, 10);
    join(&mut world, 2, 20);
    let leaver = world.players[1].slot;
    let leaver_id = world.players[1].member.id;
    world.detach(20);
    for _ in 0..LIMITS.grace - 1 {
        let out = world.tick();
        assert!(
            sent(&out, 10)
                .iter()
                .all(|m| !matches!(m, ServerMessage::MemberLeft { .. }))
        );
    }
    let out = world.tick();
    let messages = sent(&out, 10);
    assert!(messages.contains(&ServerMessage::MemberLeft { id: leaver_id }));
    let snapshot = snapshot_for(&out, 10);
    assert_eq!(
        snapshot.roster.unwrap().slots[leaver].occupant,
        WireOccupant::Vacant
    );
    assert_eq!(world.report().players, vec![1]);
}

#[test]
fn an_empty_world_ends_after_its_idle_time_and_not_before() {
    let mut world = world();
    join(&mut world, 1, 10);
    for _ in 0..3 * LIMITS.idle {
        world.tick();
    }
    assert!(!world.is_ended(), "someone is there");
    world.detach(10);
    for _ in 0..LIMITS.grace {
        world.tick();
    }
    assert_eq!(world.player_count(), 0);
    for _ in 0..LIMITS.idle - 1 {
        world.tick();
    }
    assert!(!world.is_ended());
    world.tick();
    assert!(world.is_ended());
    let out = join(&mut world, 3, 30);
    assert_eq!(closed(&out, 30), Some(Some(ErrorCode::ServerShutdown)));
}

#[test]
fn blocked_players_never_see_each_other() {
    let mut world = world();
    join(&mut world, 1, 10);
    join(&mut world, 2, 20);
    join(&mut world, 3, 30);
    let (a, b) = (world.players[0].slot, world.players[1].slot);
    let (a_id, b_id) = (world.players[0].member.id, world.players[1].member.id);

    let out = world.set_blocks(block_set([(2, 1)]));
    assert_eq!(sent(&out, 10), vec![ServerMessage::MemberLeft { id: b_id }]);
    assert_eq!(sent(&out, 20), vec![ServerMessage::MemberLeft { id: a_id }]);
    assert!(sent(&out, 30).is_empty(), "others are not told");
    assert!(
        world.set_blocks(block_set([(1, 2)])).is_empty(),
        "the same pairs again"
    );

    // B jumps; C mends B.
    let jump = InputFrame {
        jump: true,
        ..input(1, 0)
    };
    world.receive(20, jump);
    world.receive(
        30,
        InputFrame {
            action: WireAction::Cast {
                spell: arena_protocol::WireSpell::Mend,
                target: Some(b as u8),
            },
            ..input(1, 0)
        },
    );
    // A tries to mend B, whom A cannot see: nothing happens.
    world.receive(
        10,
        InputFrame {
            action: WireAction::Cast {
                spell: arena_protocol::WireSpell::Mend,
                target: Some(b as u8),
            },
            ..input(1, 0)
        },
    );
    let out = world.tick();
    assert!(world.arena().fighters()[a].active_cast.is_none());
    let c = world.players[2].slot;
    assert!(world.arena().fighters()[c].active_cast.is_some());

    let seen_by_a = snapshot_for(&out, 10);
    let roster = seen_by_a.roster.as_ref().unwrap();
    assert_eq!(roster.slots[b].occupant, WireOccupant::Vacant);
    assert_eq!(
        seen_by_a.fighters[b],
        FighterState::new(&vacant_fighter(), false)
    );
    assert_eq!(seen_by_a.fighters[c].active_cast.unwrap().target, None);
    // Only A's own refusal, as for an empty slot; nothing about B.
    assert_eq!(
        seen_by_a
            .events
            .iter()
            .map(|item| item.event)
            .collect::<Vec<_>>(),
        vec![WireEvent::Rejected(
            arena_protocol::WireRejection::InvalidTarget
        )]
    );
    // What A sees is a world the rules could hold, and the hash is its own.
    assert_eq!(seen_by_a.hash, seen_by_a.hub_arena().unwrap().state_hash());
    assert_ne!(seen_by_a.hash, world.arena().state_hash());
    let decoded = ServerMessage::decode(&ServerMessage::Snapshot(seen_by_a).encode());
    assert!(decoded.is_ok());
    // C sees everyone, with the true hash.
    let seen_by_c = snapshot_for(&out, 30);
    assert_eq!(seen_by_c.hash, world.arena().state_hash());
    assert!(!seen_by_c.events.is_empty());

    // Unblocked: they see each other again.
    let out = world.set_blocks(block_set([]));
    assert_eq!(member_ids(&sent(&out, 10)), vec![b_id]);
    assert_eq!(member_ids(&sent(&out, 20)), vec![a_id]);
}

#[test]
fn a_blocked_arrival_is_hidden_from_the_start() {
    let mut world = world();
    join(&mut world, 1, 10);
    world.set_blocks(block_set([(1, 2)]));
    let out = join(&mut world, 2, 20);
    assert!(member_ids(&sent(&out, 10)).is_empty());
    let b = world.players[1].slot;
    let theirs = sent(&out, 20);
    assert_eq!(member_ids(&theirs), vec![world.players[1].member.id]);
    let a = world.players[0].slot;
    let first = snapshot_for(&out, 20);
    assert_eq!(
        first.roster.unwrap().slots[a].occupant,
        WireOccupant::Vacant
    );
    let out = world.tick();
    let seen_by_a = snapshot_for(&out, 10);
    assert_eq!(
        seen_by_a.roster.unwrap().slots[b].occupant,
        WireOccupant::Vacant
    );
}

#[test]
fn shutting_down_closes_everyone() {
    let mut world = world();
    join(&mut world, 1, 10);
    join(&mut world, 2, 20);
    let out = world.shut_down();
    assert_eq!(closed(&out, 10), Some(Some(ErrorCode::ServerShutdown)));
    assert_eq!(closed(&out, 20), Some(Some(ErrorCode::ServerShutdown)));
    assert!(world.is_ended());
    assert!(world.tick().is_empty());
}

#[test]
fn a_fresh_world_starts_with_the_servers_blocks() {
    let mut world = World::new(1, block_set([(1, 2)]), None, LIMITS, Arc::default());
    join(&mut world, 1, 10);
    let out = join(&mut world, 2, 20);
    assert!(member_ids(&sent(&out, 10)).is_empty());
    assert_eq!(
        member_ids(&sent(&out, 20)),
        vec![world.players[1].member.id]
    );
}

#[test]
fn a_block_ends_casts_between_the_pair() {
    let mut world = world();
    join(&mut world, 1, 10);
    join(&mut world, 2, 20);
    let (a, b) = (world.players[0].slot, world.players[1].slot);
    // Stand them side by side and have A start mending B.
    world.receive(
        10,
        InputFrame {
            action: WireAction::Cast {
                spell: arena_protocol::WireSpell::Mend,
                target: Some(b as u8),
            },
            ..input(1, 0)
        },
    );
    world.tick();
    assert!(
        world.arena().fighters()[a].active_cast.is_some(),
        "neighbours in range"
    );
    world.set_blocks(block_set([(1, 2)]));
    assert!(world.arena().fighters()[a].active_cast.is_none());
    let out = world.tick();
    let mine = snapshot_for(&out, 10);
    assert!(mine.events.iter().any(|item| matches!(
        item.event,
        WireEvent::CastInterrupted { actor, .. } if usize::from(actor) == a
    )));
}

#[test]
fn reconnecting_after_a_block_changes_gets_the_right_members() {
    let mut world = world();
    join(&mut world, 1, 10);
    join(&mut world, 2, 20);
    world.detach(10);
    world.set_blocks(block_set([(1, 2)]));
    world.tick();
    let out = join(&mut world, 1, 11);
    assert_eq!(
        member_ids(&sent(&out, 11)),
        vec![world.players[0].member.id]
    );
    // B's grace runs out later: A, who blocked them, hears nothing.
    world.detach(20);
    let mut heard = Vec::new();
    for _ in 0..=LIMITS.grace {
        heard.extend(sent(&world.tick(), 11));
    }
    assert!(
        !heard
            .iter()
            .any(|message| matches!(message, ServerMessage::MemberLeft { .. }))
    );
    assert_eq!(world.player_count(), 1);
}

#[test]
fn the_map_digest_names_the_plaza_layout() {
    assert_eq!(plaza_digest(), plaza_digest());
    let mut world = world();
    let out = join(&mut world, 1, 10);
    let ServerMessage::Welcome(welcome) = &sent(&out, 10)[0] else {
        panic!("welcome");
    };
    assert_eq!(welcome.map, plaza_digest());
}

#[test]
fn only_players_the_door_placed_may_be_in_a_world() {
    let mut world = world();
    join(&mut world, 1, 10);
    join(&mut world, 2, 20);
    let b_id = world.players[1].member.id;
    let allowed: Allowed = Some(Arc::new([1].into_iter().collect()));
    let out = world.set_allowed(allowed);
    assert_eq!(closed(&out, 20), Some(Some(ErrorCode::MatchOver)));
    let out = world.tick();
    assert!(sent(&out, 10).contains(&ServerMessage::MemberLeft { id: b_id }));
    assert_eq!(world.report().players, vec![1]);
    // Their ticket no longer lets them in.
    assert_eq!(
        closed(&join(&mut world, 2, 21), 21),
        Some(Some(ErrorCode::BadTicket))
    );
    assert!(
        closed(&join(&mut world, 1, 11), 11).is_none(),
        "the placed one may reconnect"
    );
}

#[test]
fn snapshots_go_as_deltas_between_keyframes_and_in_full_after_a_drop() {
    let mut world = World::new(
        1,
        Arc::default(),
        None,
        WorldLimits {
            keyframe_every: 4,
            ..LIMITS
        },
        Arc::default(),
    );
    let out = join(&mut world, 1, 10);
    let mut client = snapshot_for(&out, 10);
    let mut kinds = Vec::new();
    for seq in 1..=9 {
        world.receive(10, input(seq, 1_000));
        let out = world.tick();
        match sent(&out, 10).pop().unwrap() {
            ServerMessage::Snapshot(snapshot) => {
                kinds.push('F');
                client = snapshot;
            }
            ServerMessage::WorldDelta(delta) => {
                kinds.push('d');
                // A client rebuilds exactly what the server holds.
                client = delta.apply(&client).unwrap();
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(Some(&client), world.players[0].sent.as_ref());
        assert_eq!(client.hash, client.hub_arena().unwrap().state_hash());
        if seq == 6 {
            // The connection could not take a frame: the next goes in full.
            world.resend_full(10);
        }
    }
    // The join sent a full one; then three deltas a keyframe, and a full
    // one right after the drop.
    assert_eq!(kinds.iter().collect::<String>(), "dddFddFdd");
}

/// Moves the fighter in `slot` to `at` (as walking there would).
fn place(world: &mut World, slot: usize, at: arena_sim::arena::Position) {
    let mut fighters = world.arena.fighters().to_vec();
    fighters[slot].position = at;
    world.arena = Arena::restore_hub(
        world.arena.tick(),
        world.arena.slots().to_vec(),
        fighters,
        world.arena.next_id(),
    )
    .unwrap();
}

#[test]
fn players_far_apart_are_out_of_view_and_training_enemies_never() {
    use arena_sim::arena::Position;
    use arena_sim::hub::INTEREST_RADIUS;
    let mut world = world();
    join(&mut world, 1, 10);
    join(&mut world, 2, 20);
    let (a, b) = (world.players[0].slot, world.players[1].slot);
    let seen = |world: &mut World| {
        let out = world.tick();
        let roster = snapshot_for(&out, 10).roster.unwrap();
        (
            roster.slots[b].occupant != WireOccupant::Vacant,
            roster.slots[0].occupant == WireOccupant::TrainingEnemy,
            sent(&out, 10)
                .iter()
                .any(|m| matches!(m, ServerMessage::MemberLeft { .. })),
        )
    };
    place(&mut world, a, Position { x: 0, z: 30_000 });
    // Just inside the radius: seen.
    place(
        &mut world,
        b,
        Position {
            x: 0,
            z: 30_000 - INTEREST_RADIUS + 100,
        },
    );
    assert_eq!(seen(&mut world), (true, true, false));
    // A little past it: still seen (the margin).
    place(
        &mut world,
        b,
        Position {
            x: 0,
            z: 30_000 - INTEREST_RADIUS - 2_000,
        },
    );
    assert_eq!(seen(&mut world), (true, true, false));
    // Past the margin: out of view, but still a member (no MemberLeft);
    // the training enemies, much farther, are always shown.
    place(
        &mut world,
        b,
        Position {
            x: 0,
            z: 30_000 - INTEREST_RADIUS - INTEREST_MARGIN - 1_000,
        },
    );
    assert_eq!(seen(&mut world), (false, true, false));
    // Back a little past the radius: not yet (the margin only keeps).
    place(
        &mut world,
        b,
        Position {
            x: 0,
            z: 30_000 - INTEREST_RADIUS - 2_000,
        },
    );
    assert_eq!(seen(&mut world), (false, true, false));
    place(
        &mut world,
        b,
        Position {
            x: 0,
            z: 30_000 - INTEREST_RADIUS + 100,
        },
    );
    assert_eq!(seen(&mut world), (true, true, false));
}

#[test]
fn a_delta_chain_follows_someone_out_of_view_and_back() {
    use arena_sim::arena::Position;
    use arena_sim::hub::INTEREST_RADIUS;
    let mut world = World::new(
        1,
        Arc::default(),
        None,
        WorldLimits {
            keyframe_every: 100,
            ..LIMITS
        },
        Arc::default(),
    );
    join(&mut world, 1, 10);
    join(&mut world, 2, 20);
    let out = world.tick();
    let ServerMessage::Snapshot(first) = sent(&out, 10).pop().unwrap() else {
        panic!("full after the second arrival changed the roster");
    };
    let mut client_a = first;
    let (a, b) = (world.players[0].slot, world.players[1].slot);
    place(&mut world, a, Position { x: 0, z: 30_000 });
    let far = 30_000 - INTEREST_RADIUS - INTEREST_MARGIN - 1_000;
    let mut seen = Vec::new();
    for step in 0..6 {
        let z = if (2..4).contains(&step) { far } else { 25_000 };
        place(&mut world, b, Position { x: 0, z });
        // B jumps every tick: an event A must not hear while B is away.
        world.receive(
            20,
            InputFrame {
                jump: true,
                ..input(step + 1, 0)
            },
        );
        let out = world.tick();
        match sent(&out, 10).pop().unwrap() {
            ServerMessage::WorldDelta(delta) => client_a = delta.apply(&client_a).unwrap(),
            ServerMessage::Snapshot(snapshot) => client_a = snapshot,
            other => panic!("{other:?}"),
        }
        assert_eq!(client_a.hash, client_a.hub_arena().unwrap().state_hash());
        let shown = client_a.roster.as_ref().unwrap().slots[b].occupant != WireOccupant::Vacant;
        if !shown {
            assert!(
                !client_a
                    .events
                    .iter()
                    .any(|item| matches!(item.event, WireEvent::Jumped { actor } if usize::from(actor) == b)),
                "no event about someone out of view"
            );
        }
        seen.push(shown);
    }
    assert_eq!(seen, [true, true, false, false, true, true]);
}
