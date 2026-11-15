use super::*;
use arena_protocol::{PROTOCOL_VERSION, Snapshot, TickEvent, Welcome, WireRejection, WireResult};
use arena_sim::arena::ArenaCommand;

fn layout() -> Layout {
    arena_map::colosseum().layout
}

fn welcome(actor: u8, team_size: u8) -> Vec<u8> {
    ServerMessage::Welcome(Welcome {
        protocol: PROTOCOL_VERSION,
        actor,
        team_size,
        tick: 0,
        tick_rate: 30,
        rules: arena_protocol::RULES_VERSION,
        map: 7,
    })
    .encode()
}

fn snapshot(arena: &Arena, ack: u32, events: Vec<TickEvent>, connected: &[bool]) -> Vec<u8> {
    ServerMessage::Snapshot(Snapshot::capture(arena, ack, events, connected)).encode()
}

fn feed(generation: u32, frames: Vec<Vec<u8>>) -> Feed {
    Feed { generation, frames }
}

/// A session welcomed as `seat` in a `team_size` match.
fn joined(seat: u8, team_size: u8) -> Session {
    let mut session = Session::default();
    session.receive(&feed(1, vec![welcome(seat, team_size)]), 0.0, &layout());
    assert_eq!(session.phase, Phase::Waiting);
    session
}

#[test]
fn seats_are_renumbered_so_the_local_player_is_actor_zero() {
    assert_eq!(seat_order(0, 2), vec![0, 1]);
    assert_eq!(seat_order(1, 2), vec![1, 0]);
    assert_eq!(seat_order(3, 6), vec![3, 0, 1, 2, 5, 4]);
    for count in [2, 4, 6] {
        for seat in 0..count {
            let order = seat_order(seat, count);
            let mut sorted = order.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..count).collect::<Vec<_>>(), "a permutation");
            assert_eq!(order[0], seat);
            for (local, actor) in order.iter().enumerate() {
                // Even locals are this seat's team; odd locals the enemy.
                assert_eq!(local % 2 == 0, actor % 2 == seat % 2);
            }
        }
    }
}

#[test]
fn every_new_socket_restarts_the_sequence_and_frames_wait_for_one() {
    let mut session = Session::default();
    // Generation 0: no socket has opened yet, so nothing is trusted.
    session.receive(&feed(0, vec![welcome(0, 1)]), 0.0, &layout());
    assert_eq!(session.phase, Phase::Connecting);
    // Not joined yet: no intent or pings are sent.
    assert_eq!(session.input([0, 0], 0, false, None, 0.0), None);
    assert_eq!(session.ping(0.0), None);
    session.receive(&feed(1, vec![welcome(0, 1)]), 0.0, &layout());
    assert!(session.joined());
    session.input([0, 0], 0, false, None, 0.0).unwrap();
    session.input([0, 0], 0, false, None, 0.0).unwrap();
    // A new socket (the runtime has greeted it) numbers inputs from 1.
    session.receive(&feed(2, vec![welcome(0, 1)]), 0.0, &layout());
    let frame = session.input([0, 0], 0, false, None, 0.0).unwrap();
    let Ok(ClientMessage::Input(input)) = ClientMessage::decode(&frame) else {
        panic!()
    };
    assert_eq!(input.seq, 1);
    // A closed socket's last frames still arrive under its generation, so
    // the server's final word is not lost.
    session.receive(
        &feed(2, vec![ServerMessage::Error(ErrorCode::Replaced).encode()]),
        0.0,
        &layout(),
    );
    assert_eq!(session.phase, Phase::Closed(ErrorCode::Replaced));
}

#[test]
fn snapshots_are_shown_from_this_seats_side_and_checked_against_their_hash() {
    let mut server = Arena::practice(layout(), 1).unwrap();
    server.step(&[ArenaCommand::Face {
        actor: 1,
        facing: 0x4000,
    }]);
    let mut session = joined(1, 1);
    let mut frame = snapshot(&server, 0, vec![], &[false, true]);
    session.receive(&feed(1, vec![frame.clone()]), 0.0, &layout());
    assert_eq!(session.phase, Phase::Playing);
    let view = session.view(0.0, 0.0).unwrap();
    // Local 0 is server actor 1, and the other seat is shown disconnected.
    assert_eq!(view.fighters[0], server.fighters()[1]);
    assert_eq!(view.fighters[1], server.fighters()[0]);
    assert_eq!(view.connected, vec![true, false]);
    assert!((view.facings[0] - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert_eq!(session.stats.hash_mismatches, 0);
    // A snapshot whose state does not match its hash is counted.
    let mut wrong = match ServerMessage::decode(&frame).unwrap() {
        ServerMessage::Snapshot(snapshot) => snapshot,
        _ => unreachable!(),
    };
    wrong.hash ^= 1;
    wrong.tick += 1;
    frame = ServerMessage::Snapshot(wrong).encode();
    session.receive(&feed(1, vec![frame]), 0.0, &layout());
    assert_eq!(session.stats.hash_mismatches, 1);
}

#[test]
fn inputs_carry_this_seats_intent_with_targets_in_server_numbering() {
    let mut session = joined(1, 1);
    let frame = session
        .input([500, -1000], 0x8000, true, Some(1), 0.0)
        .unwrap();
    assert_eq!(
        ClientMessage::decode(&frame),
        Ok(ClientMessage::Input(InputFrame {
            seq: 1,
            movement: [500, -1000],
            facing: 0x8000,
            jump: true,
            // Local 1 (the enemy) is server actor 0.
            attack: Some(0),
        }))
    );
    // An acknowledgement 0.1 s later measures input latency.
    let mut server = Arena::practice(layout(), 1).unwrap();
    server.step(&[]);
    session.receive(
        &feed(1, vec![snapshot(&server, 1, vec![], &[true, true])]),
        0.1,
        &layout(),
    );
    assert!((session.stats.input_ms.unwrap() - 100.0).abs() < 1e-6);
    // Pings: at most once a second, and a pong measures the round trip.
    let ping = session.ping(1.0).unwrap();
    assert_eq!(session.ping(1.5), None);
    let Ok(ClientMessage::Ping { nonce }) = ClientMessage::decode(&ping) else {
        panic!()
    };
    session.receive(
        &feed(1, vec![ServerMessage::Pong { nonce }.encode()]),
        1.08,
        &layout(),
    );
    assert!((session.stats.rtt_ms.unwrap() - 80.0).abs() < 1e-6);
}

#[test]
fn the_view_trails_the_newest_snapshot_and_interpolates_between_ticks() {
    let mut server = Arena::practice(layout(), 1).unwrap();
    let mut session = joined(0, 1);
    let mut frames = Vec::new();
    for _ in 0..10 {
        server.step(&[ArenaCommand::Move {
            actor: 0,
            x: 0,
            z: -1000,
        }]);
        frames.push(snapshot(&server, 0, vec![], &[true, true]));
    }
    session.receive(&feed(1, frames), 0.0, &layout());
    // Newest tick 10: the view starts three ticks behind, at tick 7.
    let view = session.view(0.0, 0.0).unwrap();
    assert_eq!(view.tick, 7);
    let z = |tick: f32| arena_sim::arena::MOVE_PER_TICK as f32 * -tick / 1000.0;
    let spawn = Arena::practice(layout(), 1).unwrap().fighters()[0]
        .position
        .z as f32
        / 1000.0;
    assert!((view.positions[0].z - (spawn + z(7.0))).abs() < 1e-3);
    // Half a tick later the view is halfway between ticks 7 and 8.
    let view = session.view(1.0 / 60.0, 1.0 / 60.0).unwrap();
    assert!((view.positions[0].z - (spawn + z(7.5))).abs() < 1e-3);
    assert_eq!(session.stats.underruns, 0);
    assert!((session.stats.delay_ms - 100.0).abs() < 1.0);
    // A second without snapshots: the view snaps forward, then holds at
    // the newest state instead of extrapolating.
    let view = session.view(1.0 / 60.0, 1.0).unwrap();
    assert_eq!(view.tick, 10);
    assert!((view.positions[0].z - (spawn + z(10.0))).abs() < 1e-3);
    assert_eq!(session.stats.underruns, 1);
}

#[test]
fn events_show_when_the_view_reaches_them_and_rejections_at_once() {
    let mut server = Arena::practice(layout(), 1).unwrap();
    let mut session = joined(1, 1);
    let mut frames = Vec::new();
    for tick in 0..10u64 {
        server.step(&[]);
        let events = if tick == 8 {
            vec![
                TickEvent {
                    tick,
                    event: WireEvent::Hit {
                        actor: 0,
                        target: 1,
                        damage: 25,
                    },
                },
                TickEvent {
                    tick,
                    event: WireEvent::Rejected(WireRejection::Cooldown),
                },
            ]
        } else {
            vec![]
        };
        frames.push(snapshot(&server, 0, events, &[true, true]));
    }
    let rejections = session.receive(&feed(1, frames), 0.0, &layout());
    assert_eq!(rejections, vec![Rejection::Cooldown]);
    // The view starts at tick 7: the hit (during tick 8) is not shown yet.
    assert!(session.view(0.0, 0.0).unwrap().shown.is_empty());
    let mut shown = Vec::new();
    for frame in 1..=6 {
        let now = f64::from(frame) / 30.0;
        shown.extend(session.view(1.0 / 30.0, now).unwrap().shown);
    }
    // Server actor 0 hit server actor 1, who is this seat (local 0).
    assert_eq!(
        shown,
        vec![Shown::Hit {
            actor: 1,
            target: 0,
            damage: 25
        }]
    );
}

#[test]
fn results_and_errors_are_reported_from_this_players_side() {
    let mut server = Arena::practice(layout(), 1).unwrap();
    server.step(&[]);
    let finished = |team| {
        let mut session = joined(1, 1);
        let arena = Arena::restore(
            layout(),
            server.tick(),
            server.fighters().to_vec(),
            Some(RoundResult::Won { team }),
        )
        .unwrap();
        let events = vec![TickEvent {
            tick: 0,
            event: WireEvent::Finished(WireResult::Won { team: team as u8 }),
        }];
        session.receive(
            &feed(1, vec![snapshot(&arena, 0, events, &[true, true])]),
            0.0,
            &layout(),
        );
        assert_eq!(session.phase, Phase::Finished);
        session.view(1.0, 1.0).unwrap().shown
    };
    assert_eq!(finished(1), vec![Shown::Finished { won: Some(true) }]);
    assert_eq!(finished(0), vec![Shown::Finished { won: Some(false) }]);

    let mut session = joined(0, 1);
    session.receive(
        &feed(1, vec![ServerMessage::Error(ErrorCode::MatchOver).encode()]),
        0.0,
        &layout(),
    );
    assert_eq!(session.phase, Phase::Closed(ErrorCode::MatchOver));
    assert!(!session.joined());
    let status = session.status_json();
    let value: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert_eq!(value["phase"], "closed");
    assert_eq!(value["error"], "MatchOver");
    assert_eq!(value["seat"], 0);
    // The test welcome announced map digest 7, not this build's colosseum.
    assert_eq!(value["mapDiffers"], true);
    assert_eq!(
        fnv1a(arena_map::COLOSSEUM_JSON.as_bytes()),
        arena_map::COLOSSEUM_JSON
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b))
                .wrapping_mul(0x100_0000_01b3))
    );
    assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    // Garbage from the socket closes the session rather than panicking.
    let mut session = joined(0, 1);
    session.receive(&feed(1, vec![vec![0xff, 0xff]]), 0.0, &layout());
    assert_eq!(session.phase, Phase::Closed(ErrorCode::Malformed));
}
