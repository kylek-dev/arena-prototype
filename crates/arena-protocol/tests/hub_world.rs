//! Hub worlds on the wire (ADR 0057 slice 2): variable rosters in
//! snapshots, the hub `Welcome`, the member relay, and prediction.

use arena_protocol::delta::SnapshotDelta;
use arena_protocol::prediction::Predictor;
use arena_protocol::{
    DecodeError, FighterScore, Handle, MAX_SERVER_FRAME, MAX_WORLD_FIGHTERS, Member,
    PROTOCOL_VERSION, RULES_VERSION, ServerMessage, Snapshot, Tags, TickEvent, Welcome, WireCast,
    WireEvent, WireOccupant, WireResult, WireSpell,
};
use arena_sim::arena::{ActorInput, Arena, ArenaEvent, Axes, HUB_PLAYER_CAP, MAX_ROUND_TICKS};

fn world(players: usize) -> (Arena, Vec<usize>) {
    let mut arena = Arena::hub();
    let slots = (0..players).map(|_| arena.join().unwrap()).collect();
    (arena, slots)
}

fn snapshot(arena: &Arena) -> Snapshot {
    Snapshot::capture(arena, 0, Vec::new(), &[true; MAX_WORLD_FIGHTERS])
}

fn decode(snapshot: Snapshot) -> Result<ServerMessage, DecodeError> {
    ServerMessage::decode(&ServerMessage::Snapshot(snapshot).encode())
}

fn refused(snapshot: Snapshot) -> bool {
    decode(snapshot) == Err(DecodeError::OverLimit)
}

#[test]
fn a_world_snapshot_round_trips_and_rebuilds_the_world() {
    let (mut arena, players) = world(HUB_PLAYER_CAP);
    arena.leave(players[2]).unwrap();
    for _ in 0..5 {
        arena.advance(&[]);
    }
    let sent = snapshot(&arena);
    assert_eq!(sent.hash, arena.state_hash());
    let frame = ServerMessage::Snapshot(sent.clone()).encode();
    assert!(frame.len() <= MAX_SERVER_FRAME);
    let Ok(ServerMessage::Snapshot(received)) = ServerMessage::decode(&frame) else {
        panic!("decodes");
    };
    assert_eq!(received, sent);
    let rebuilt = received.hub_arena().expect("a valid world");
    assert_eq!(rebuilt.state_hash(), received.hash);
    assert_eq!(rebuilt.slots(), arena.slots());
    assert_eq!(rebuilt.fighters(), arena.fighters());
    assert_eq!(
        received.roster.as_ref().unwrap().slots[players[2]].occupant,
        WireOccupant::Vacant
    );
    // A match snapshot has no world.
    let practice = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
    assert!(snapshot(&practice).roster.is_none());
    assert!(snapshot(&practice).hub_arena().is_none());
}

#[test]
fn world_snapshots_stay_small() {
    // A full world without events: 16 fighters and their slots.
    let (arena, _) = world(HUB_PLAYER_CAP);
    let size = ServerMessage::Snapshot(snapshot(&arena)).encode().len();
    assert!(size <= 1_200, "{size}");
}

#[test]
fn inconsistent_world_snapshots_are_refused() {
    let (mut arena, players) = world(3);
    arena.leave(players[2]).unwrap();
    let good = snapshot(&arena);
    assert!(decode(good.clone()).is_ok());
    assert_eq!(good.fighters.len() % 2, 1, "worlds need not be even");

    let mut short = good.clone();
    short.roster.as_mut().unwrap().slots.pop();
    assert!(refused(short));

    let mut finished = good.clone();
    finished.result = Some(WireResult::Draw);
    assert!(refused(finished));

    let mut scored = good.clone();
    scored.scoreboard = Some(vec![FighterScore::default(); good.fighters.len()]);
    assert!(refused(scored));

    let mut ended = good.clone();
    ended.events.push(TickEvent {
        tick: 1,
        event: WireEvent::Finished(WireResult::Draw),
    });
    assert!(refused(ended));

    let mut duplicate = good.clone();
    let roster = duplicate.roster.as_mut().unwrap();
    roster.slots[players[1]].id = roster.slots[players[0]].id;
    assert!(refused(duplicate));

    // A training enemy with a cast would act.
    let mut casting = good.clone();
    casting.fighters[0].active_cast = Some(WireCast {
        spell: WireSpell::RiftBolt,
        target: Some(players[0] as u8),
        remaining_ticks: 1,
        total_ticks: 45,
    });
    assert!(refused(casting));

    // Nothing may aim at a vacant slot.
    let mut aimed = good.clone();
    aimed.fighters[players[0]].active_cast = Some(WireCast {
        spell: WireSpell::Mend,
        target: Some(players[2] as u8),
        remaining_ticks: 1,
        total_ticks: 45,
    });
    assert!(refused(aimed));

    // An event naming a slot past the roster.
    let mut stray = good.clone();
    stray.events.push(TickEvent {
        tick: 1,
        event: WireEvent::Jumped {
            actor: good.fighters.len() as u8,
        },
    });
    assert!(refused(stray));

    // Past the largest roster.
    let (full, _) = world(HUB_PLAYER_CAP);
    let mut over = snapshot(&full);
    // An extra vacant slot, valid in every other way.
    over.fighters.push(arena_protocol::FighterState::new(
        &arena_sim::arena::vacant_fighter(),
        false,
    ));
    let roster = over.roster.as_mut().unwrap();
    roster.slots.push(arena_sim::arena::Slot::VACANT.into());
    assert!(over.fighters.len() > MAX_WORLD_FIGHTERS);
    assert!(refused(over));

    // A match snapshot keeps its own rules: an odd roster is refused.
    let practice = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
    let mut odd = snapshot(&practice);
    odd.fighters.push(odd.fighters[0]);
    assert!(refused(odd));
}

#[test]
fn events_name_any_slot_of_a_world() {
    let (_, players) = world(HUB_PLAYER_CAP);
    let last = *players.last().unwrap();
    assert_eq!(last, MAX_WORLD_FIGHTERS - 1);
    let event = ArenaEvent::Jumped { actor: last };
    assert_eq!(
        WireEvent::for_recipient(&event, last),
        Some(WireEvent::Jumped { actor: last as u8 })
    );
    let beyond = ArenaEvent::Jumped {
        actor: MAX_WORLD_FIGHTERS,
    };
    assert_eq!(WireEvent::for_recipient(&beyond, 0), None);
}

fn hub_welcome() -> Welcome {
    Welcome {
        protocol: PROTOCOL_VERSION,
        actor: 4,
        team_size: 0,
        tick: 9,
        tick_rate: 30,
        round_ticks: MAX_ROUND_TICKS,
        rules: RULES_VERSION,
        map: 1,
        spectator: false,
        hub: true,
    }
}

#[test]
fn hub_welcomes_are_checked() {
    let ok =
        |welcome: Welcome| ServerMessage::decode(&ServerMessage::Welcome(welcome).encode()).is_ok();
    assert!(ok(hub_welcome()));
    assert!(ok(Welcome {
        actor: (MAX_WORLD_FIGHTERS - 1) as u8,
        ..hub_welcome()
    }));
    assert!(!ok(Welcome {
        actor: MAX_WORLD_FIGHTERS as u8,
        ..hub_welcome()
    }));
    assert!(!ok(Welcome {
        spectator: true,
        actor: 0,
        ..hub_welcome()
    }));
    assert!(!ok(Welcome {
        team_size: 1,
        ..hub_welcome()
    }));
    assert!(!ok(Welcome {
        round_ticks: 600,
        ..hub_welcome()
    }));
    assert!(!ok(Welcome {
        tick_rate: 0,
        ..hub_welcome()
    }));
    // A match welcome still needs a team.
    assert!(!ok(Welcome {
        hub: false,
        ..hub_welcome()
    }));
}

#[test]
fn the_member_relay_is_checked() {
    let decode = |message: ServerMessage| ServerMessage::decode(&message.encode());
    let member = Member {
        id: 5,
        handle: Handle::new("Rook7", 42),
        tags: Tags { admin: true },
        appearance: None,
    };
    assert_eq!(
        decode(ServerMessage::Member(Box::new(member.clone()))),
        Ok(ServerMessage::Member(Box::new(member.clone())))
    );
    assert_eq!(
        decode(ServerMessage::Member(Box::new(Member { id: 0, ..member }))),
        Err(DecodeError::OverLimit)
    );
    assert_eq!(
        decode(ServerMessage::MemberLeft { id: 5 }),
        Ok(ServerMessage::MemberLeft { id: 5 })
    );
    assert_eq!(
        decode(ServerMessage::MemberLeft { id: 0 }),
        Err(DecodeError::OverLimit)
    );
}

#[test]
fn deltas_are_not_used_for_worlds() {
    let (mut arena, _) = world(2);
    let first = snapshot(&arena);
    arena.advance(&[]);
    let second = snapshot(&arena);
    assert!(SnapshotDelta::from_pair(&first, &second).is_none());
    assert!(SnapshotDelta::apply(&first, &[1]).is_err());
}

#[test]
fn prediction_in_a_world_replays_unacknowledged_movement_exactly() {
    let (mut server, players) = world(2);
    let me = players[1];
    let walk = ActorInput {
        movement: Some(Axes { x: 0, z: -1_000 }),
        ..ActorInput::default()
    };
    let mut predictor = Predictor::default();
    let mut sent = Vec::new();
    for seq in 1..=10u32 {
        predictor.apply(seq, walk);
        sent.push(walk);
    }
    // The server has applied the first four.
    let mut inputs = vec![ActorInput::default(); server.fighters().len()];
    inputs[me] = walk;
    for _ in 0..4 {
        server.advance(&inputs);
    }
    let mut at_four = snapshot(&server);
    at_four.ack = 4;
    assert_eq!(
        predictor.reconcile_hub(&at_four, me),
        None,
        "first reconcile"
    );
    for _ in 4..10 {
        server.advance(&inputs);
    }
    assert_eq!(predictor.predicted(), Some(server.fighters()[me]));
    assert_eq!(predictor.pending(), 6);

    let mut at_ten = snapshot(&server);
    at_ten.ack = 10;
    assert_eq!(
        predictor.reconcile_hub(&at_ten, me),
        Some(0),
        "no correction"
    );
    assert_eq!(predictor.pending(), 0);

    // Slots that are not players are never predicted.
    assert_eq!(predictor.reconcile_hub(&at_ten, 0), None);
    assert_eq!(predictor.predicted(), None);
    let practice = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
    assert_eq!(predictor.reconcile_hub(&snapshot(&practice), 0), None);
}

#[test]
fn more_malformed_hub_frames_are_refused() {
    // A world with no fighters at all.
    let (arena, _) = world(0);
    let mut empty = snapshot(&arena);
    empty.fighters.clear();
    empty.roster.as_mut().unwrap().slots.clear();
    assert!(refused(empty));

    // Members with a bad handle or a bad look.
    let decode = |message: ServerMessage| ServerMessage::decode(&message.encode());
    let mut bad_handle = ServerMessage::Member(Box::new(Member {
        id: 3,
        handle: Handle::new("Rook7", 42),
        tags: Tags::default(),
        appearance: None,
    }))
    .encode();
    // Corrupt the handle's first name byte into a space (not a letter).
    bad_handle[3] = b' ';
    assert_eq!(
        ServerMessage::decode(&bad_handle),
        Err(DecodeError::OverLimit)
    );
    let bad_look = arena_protocol::WireAppearance {
        ancestry: "../x".into(),
        body: "male".into(),
        skin_tone: "fair".into(),
        hair_color: "black".into(),
        facial_hair_color: "black".into(),
        eyes: "blue".into(),
        build: 0,
        groups: Vec::new(),
        variants: Vec::new(),
    };
    assert_eq!(
        decode(ServerMessage::Member(Box::new(Member {
            id: 3,
            handle: None,
            tags: Tags::default(),
            appearance: Some(bad_look),
        }))),
        Err(DecodeError::OverLimit)
    );

    // The wider event bound never reaches matches: an actor past a 3v3's
    // six fighters is still refused.
    let practice = Arena::practice(arena_map::colosseum().layout, 3).unwrap();
    let mut stray = snapshot(&practice);
    stray.events.push(TickEvent {
        tick: 1,
        event: WireEvent::Jumped { actor: 6 },
    });
    assert!(refused(stray));
}

#[test]
fn a_corrected_world_prediction_counts_the_correction() {
    let (mut server, players) = world(1);
    let me = players[0];
    let mut predictor = Predictor::default();
    assert_eq!(predictor.reconcile_hub(&snapshot(&server), me), None);
    // The client thinks it walked; the server never got it.
    predictor.apply(
        1,
        ActorInput {
            movement: Some(Axes { x: 1_000, z: 0 }),
            ..ActorInput::default()
        },
    );
    server.advance(&[]);
    let mut now = snapshot(&server);
    now.ack = 1;
    let moved = predictor.reconcile_hub(&now, me).unwrap();
    assert!(moved > 0);
    assert_eq!(predictor.corrections(), 1);
    assert_eq!(predictor.predicted(), Some(server.fighters()[me]));
}
