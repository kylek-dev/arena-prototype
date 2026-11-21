use super::*;
use arena_protocol::{PROTOCOL_VERSION, Snapshot, TickEvent, Welcome, WireRejection, WireResult};
use arena_sim::arena::{ActorInput, Arena, ArenaCommand, Axes};

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
    assert_eq!(
        snapshot_state_hash(server.tick(), server.fighters(), server.result()),
        server.state_hash()
    );
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

/// A running 1v1 seen from seat 0: one server tick so far.
fn running_session() -> (Session, Arena) {
    let mut server = Arena::practice(layout(), 1).unwrap();
    server.step(&[]);
    let mut session = joined(0, 1);
    session.receive(
        &feed(1, vec![snapshot(&server, 0, vec![], &[true, true])]),
        0.0,
        &layout(),
    );
    assert_eq!(session.phase, Phase::Playing);
    (session, server)
}

fn forward() -> ActorInput {
    ActorInput {
        movement: Some(Axes { x: 0, z: -1000 }),
        facing: Some(0),
        jump: false,
        attack: None,
    }
}

#[test]
fn the_players_own_fighter_moves_at_once_and_agrees_with_the_server() {
    let (mut session, mut server) = running_session();
    let start = session.own().unwrap().current;
    assert_eq!(start, place(&server.fighters()[0]));
    // The input moves the fighter before any server reply.
    session.input([0, -1000], 0, false, None, 0.0).unwrap();
    let own = session.own().unwrap();
    let step = arena_sim::arena::MOVE_PER_TICK as f32 / 1000.0;
    assert_eq!(own.previous, start);
    assert!((own.current.z - (start.z - step)).abs() < 1e-4);
    assert_eq!(session.stats.predicted_ticks, 1);
    // Half-way through the tick it is drawn half-way along the step.
    assert!((own.at(0.5).z - (start.z - step / 2.0)).abs() < 1e-4);
    // The server applies the same input: no correction, nothing pending.
    server.advance(&[forward()]);
    session.receive(
        &feed(1, vec![snapshot(&server, 1, vec![], &[true, true])]),
        0.05,
        &layout(),
    );
    assert_eq!(session.stats.corrections, 0);
    assert_eq!(session.stats.predicted_ticks, 0);
    assert_eq!(session.own().unwrap().current, own.current);
    assert_eq!(session.own().unwrap().offset, Vec3::ZERO);
    assert_eq!(session.predicted(), Some(server.fighters()[0]));
    // The HUD and rendering show the predicted fighter, not the delayed view.
    let view = session.view(0.0, 0.05).unwrap();
    assert_ne!(view.positions[0], own.current, "the view itself is delayed");
}

#[test]
fn a_disagreeing_server_is_eased_in_rather_than_snapped() {
    let (mut session, mut server) = running_session();
    session.input([0, -1000], 0, false, None, 0.0).unwrap();
    let drawn = session.own().unwrap().at(1.0);
    // The server acknowledges the input but the fighter stood still.
    server.advance(&[]);
    session.receive(
        &feed(1, vec![snapshot(&server, 1, vec![], &[true, true])]),
        0.05,
        &layout(),
    );
    assert_eq!(session.stats.corrections, 1);
    assert_eq!(
        session.stats.last_correction_mm,
        Some(arena_sim::arena::MOVE_PER_TICK as u64)
    );
    let own = session.own().unwrap();
    assert_eq!(own.current, place(&server.fighters()[0]));
    // At first it is still drawn where it was ...
    assert!((own.at(1.0) - drawn).length() < 1e-4);
    // ... and the difference fades within a few frames.
    for _ in 0..30 {
        session.view(1.0 / 60.0, 0.05);
    }
    assert!((session.own().unwrap().at(1.0) - own.current).length() < 1e-3);
    let status: serde_json::Value = serde_json::from_str(&session.status_json()).unwrap();
    assert_eq!(status["corrections"], 1);
    assert_eq!(status["lastCorrectionMm"], arena_sim::arena::MOVE_PER_TICK);
    assert_eq!(status["predictedTicks"], 0);
}

#[test]
fn nothing_is_predicted_while_waiting_and_a_new_socket_starts_over() {
    let mut session = joined(0, 1);
    let server = Arena::practice(layout(), 1).unwrap();
    session.receive(
        &feed(1, vec![snapshot(&server, 0, vec![], &[true, false])]),
        0.0,
        &layout(),
    );
    assert_eq!(session.phase, Phase::Waiting);
    session.input([0, -1000], 0, false, None, 0.0).unwrap();
    assert_eq!(session.own(), None, "nothing is drawn ahead while waiting");
    assert_eq!(session.stats.predicted_ticks, 1, "but the input is kept");
    // The match starts with that input still in flight; the server applies
    // it on its first tick and the prediction agrees.
    let mut server = server;
    let mut slot = ActorInput::default();
    InputFrame {
        seq: 1,
        movement: [0, -1000],
        facing: 0,
        jump: false,
        attack: None,
    }
    .merge_into(&mut slot);
    server.advance(&[slot]);
    session.receive(
        &feed(1, vec![snapshot(&server, 1, vec![], &[true, true])]),
        0.1,
        &layout(),
    );
    assert_eq!(session.phase, Phase::Playing);
    assert_eq!(session.stats.corrections, 0);
    assert_eq!(session.predicted(), Some(server.fighters()[0]));
    let (mut session, _) = running_session();
    session.input([0, -1000], 0, false, None, 0.0).unwrap();
    assert!(session.own().is_some());
    session.receive(&feed(2, vec![]), 0.1, &layout());
    assert_eq!(
        session.own(),
        None,
        "a reconnect waits for the server again"
    );
}

#[test]
fn a_catch_up_burst_sends_two_inputs_a_frame_and_predicts_only_those() {
    let (mut session, _) = running_session();
    let start = session.own().unwrap().current;
    // A tab back from the background runs seven fixed ticks in one frame.
    let sent = (0..7)
        .filter_map(|_| session.input([0, -1000], 0, false, None, 5.0))
        .count();
    assert_eq!(sent, 2);
    assert_eq!(session.stats.skipped_inputs, 5);
    assert_eq!(
        (
            session.stats.steps_per_second,
            session.stats.sent_per_second
        ),
        (7, 2)
    );
    assert_eq!(session.stats.predicted_ticks, 2);
    let step = arena_sim::arena::MOVE_PER_TICK as f32 / 1000.0;
    assert!((session.own().unwrap().current.z - (start.z - 2.0 * step)).abs() < 1e-4);
    // The next frame sends again.
    assert!(session.input([0, -1000], 0, false, None, 5.01).is_some());
}

#[test]
fn sending_is_paced_to_keep_one_input_queued_on_the_server() {
    let (mut session, mut server) = running_session();
    // The first snapshot shows an empty queue: send a little faster to
    // build one input of cushion.
    assert!(session.pace() < 1.0);
    let snapshot_with = |server: &Arena, queued: u8| {
        let mut snapshot = Snapshot::capture(server, 0, vec![], &[true, true]);
        snapshot.queued = queued;
        ServerMessage::Snapshot(snapshot).encode()
    };
    // A long queue slows sending (a longer interval) ...
    for _ in 0..60 {
        server.step(&[]);
        session.receive(&feed(1, vec![snapshot_with(&server, 4)]), 0.0, &layout());
    }
    assert!(session.pace() > 1.03, "{}", session.pace());
    assert!(session.pace() <= 1.0 + arena_protocol::prediction::MAX_PACE);
    // ... an empty one speeds it up, and one queued input holds it steady.
    for _ in 0..60 {
        server.step(&[]);
        session.receive(&feed(1, vec![snapshot_with(&server, 0)]), 0.0, &layout());
    }
    assert!(session.pace() < 0.97, "{}", session.pace());
    for _ in 0..120 {
        server.step(&[]);
        session.receive(&feed(1, vec![snapshot_with(&server, 1)]), 0.0, &layout());
    }
    assert!((session.pace() - 1.0).abs() < 0.002, "{}", session.pace());
    let status: serde_json::Value = serde_json::from_str(&session.status_json()).unwrap();
    assert!((status["queued"].as_f64().unwrap() - 1.0).abs() < 0.1);
    assert_eq!(status["queuedMin"], 1);
    // Not running: plain rules tick.
    assert_eq!(joined(0, 1).pace(), 1.0);
}

#[test]
fn returning_from_a_background_tab_keeps_the_server_rate_and_full_speed() {
    // A server 9% slow against this clock. The tab hides for ten seconds,
    // then the queued snapshots trickle in over the next six.
    let mut server = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
    server.step(&[]);
    let mut session = joined(0, 1);
    // Parsed once: this loop runs about ten thousand frames.
    let layout = layout();
    let local_per_tick = 1.09 / 30.0;
    let truth = 30.0 / 1.09;
    let mut pending: std::collections::VecDeque<(f64, Vec<u8>)> = Default::default();
    for tick in 1..1200u64 {
        server.step(&[]);
        let sent = tick as f64 * local_per_tick;
        let arrives = if (300..600).contains(&tick) {
            sent + 10.0 - (tick - 300) as f64 / 30.0 + (tick - 300) as f64 * 0.02
        } else if (600..800).contains(&tick) {
            sent + 6.0 * (800 - tick) as f64 / 200.0
        } else {
            sent
        };
        pending.push_back((arrives, snapshot(&server, 0, vec![], &[true, true])));
    }
    let mut now = 0.0;
    let mut worst: f64 = 0.0;
    // Frames the runtime holds while the engine is paused, in order.
    let mut held: Vec<Vec<u8>> = Vec::new();
    let hidden = 300.0 * local_per_tick..300.0 * local_per_tick + 10.0;
    while now < 1200.0 * local_per_tick {
        now += 1.0 / 240.0;
        while pending.front().is_some_and(|(at, _)| *at <= now) {
            held.push(pending.pop_front().unwrap().1);
        }
        if hidden.contains(&now) {
            continue;
        }
        session.receive(&feed(1, std::mem::take(&mut held)), now, &layout);
        // After the first estimate (about a second of snapshots).
        if now > 2.0 {
            worst = worst.max((session.tick_rate() - truth).abs());
        }
    }
    assert!(
        worst < 0.5,
        "the server rate was misjudged by {worst} ticks/s"
    );
}

#[test]
fn one_enemy_down_in_a_team_match_does_not_end_the_round_for_the_hud() {
    // 2v2, this player is seat 1 (team 1): server actors 0 and 2 are the
    // enemies. Enemy actor 0 is defeated; actor 2 still stands.
    let start = Arena::practice(layout(), 2).unwrap();
    let mut fighters = start.fighters().to_vec();
    fighters[0].health = 0;
    let mut server = Arena::restore(layout(), 1, fighters, None).unwrap();
    let mut session = joined(1, 2);
    let mut frames = Vec::new();
    for _ in 0..10 {
        server.step(&[]);
        frames.push(snapshot(&server, 0, vec![], &[true; 4]));
    }
    session.receive(&feed(1, frames), 0.0, &layout());
    assert_eq!(server.result(), None);
    assert_eq!(session.phase, Phase::Playing);
    let view = session.view(0.0, 0.0).unwrap();
    assert_eq!(view.result, None);
    // The HUD's own copy of the arena, in local order, as lib.rs builds it.
    let hud = Arena::restore(layout(), view.tick, view.fighters, view.result).unwrap();
    assert_eq!(hud.result(), None);
    // Local 1 is enemy actor 0 (down), local 3 is enemy actor 2 (standing).
    assert_eq!(seat_order(1, 4), vec![1, 0, 3, 2]);
    assert_eq!(hud.fighters()[1].health, 0);
    assert!(hud.fighters()[3].health > 0);
    assert_eq!(
        hud.can_attack(0, 1),
        Err(arena_sim::arena::Rejection::InvalidTarget)
    );
    assert_ne!(
        hud.can_attack(0, 3),
        Err(arena_sim::arena::Rejection::MatchOver)
    );
}

#[test]
fn server_spawn_headings_face_the_other_team() {
    for size in 1..=3 {
        let arena = Arena::practice(layout(), size).unwrap();
        let fighters = arena.fighters();
        for (actor, fighter) in fighters.iter().enumerate() {
            let heading = yaw(fighter.facing);
            let forward = (-heading.sin(), -heading.cos());
            let enemies: Vec<_> = fighters
                .iter()
                .enumerate()
                .filter(|(other, _)| other % 2 != actor % 2)
                .collect();
            let (ex, ez) = enemies.iter().fold((0.0f32, 0.0f32), |(x, z), (_, f)| {
                (x + f.position.x as f32, z + f.position.z as f32)
            });
            let n = enemies.len() as f32;
            let toward = (
                ex / n - fighter.position.x as f32,
                ez / n - fighter.position.z as f32,
            );
            let length = (toward.0 * toward.0 + toward.1 * toward.1).sqrt();
            let alignment = (forward.0 * toward.0 + forward.1 * toward.1) / length;
            assert!(alignment > 0.9, "{size}v{size} actor {actor}: {alignment}");
        }
    }
}

#[test]
fn a_new_match_hands_over_the_servers_heading_once_per_seat() {
    // Seat 1 (team 1) spawns facing the other way from team 0.
    let server = Arena::practice(layout(), 1).unwrap();
    let mut session = Session::default();
    assert_eq!(session.take_initial_heading(), None, "nothing joined yet");
    session.receive(&feed(1, vec![welcome(1, 1)]), 0.0, &layout());
    assert_eq!(session.take_initial_heading(), None, "no snapshot yet");
    session.receive(
        &feed(1, vec![snapshot(&server, 0, vec![], &[true, true])]),
        0.0,
        &layout(),
    );
    let heading = session.take_initial_heading().unwrap();
    assert!((heading - yaw(server.fighters()[1].facing)).abs() < 1e-6);
    assert!((heading - std::f32::consts::PI).abs() < 1e-3, "{heading}");
    assert_eq!(session.take_initial_heading(), None, "only once");
    // A reconnect to the same seat keeps the player's own heading.
    session.receive(
        &feed(
            2,
            vec![welcome(1, 1), snapshot(&server, 0, vec![], &[true, true])],
        ),
        0.0,
        &layout(),
    );
    assert_eq!(session.take_initial_heading(), None);
    // A different seat (a new match) takes the server's heading again.
    session.receive(
        &feed(
            3,
            vec![welcome(0, 1), snapshot(&server, 0, vec![], &[true, true])],
        ),
        0.0,
        &layout(),
    );
    let heading = session.take_initial_heading().unwrap();
    assert!(heading.abs() < 1e-6, "{heading}");
}
