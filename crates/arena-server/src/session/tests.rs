use super::*;
use arena_protocol::{FighterState, WireRejection};
use arena_sim::arena::{ATTACK_COOLDOWN, ArenaCommand, MAX_ROUND_TICKS};

const LIMITS: Limits = Limits {
    queue: 3,
    start_timeout: 10,
    grace: 5,
    linger: 3,
    spectator_delay: DEFAULT_SPECTATOR_DELAY,
    max_spectators: DEFAULT_MAX_SPECTATORS,
};

fn arena(team_size: usize) -> Arena {
    Arena::practice(arena_map::colosseum().layout, team_size).unwrap()
}

fn session_with(arena: Arena) -> Session {
    Session::new(arena, 42, LIMITS, Arc::default())
}

/// Every seat attached as connection `100 + seat`, and the start tick done.
fn running(arena: Arena) -> Session {
    let mut session = session_with(arena);
    for seat in 0..session.seats.len() {
        session.attach(seat, conn(seat));
    }
    session.tick();
    assert_eq!(session.phase(), Phase::Running);
    session
}

fn conn(seat: usize) -> ConnId {
    100 + seat as ConnId
}

fn frame(seq: u32) -> InputFrame {
    InputFrame {
        seq,
        movement: [0, -1000],
        ..InputFrame::default()
    }
}

fn snapshot(out: &[Out], to: ConnId) -> Option<&Snapshot> {
    out.iter().find_map(|out| match out {
        Out::Send {
            conn,
            message: ServerMessage::Snapshot(snapshot),
        } if *conn == to => Some(snapshot),
        _ => None,
    })
}

/// A 1v1 arena where actor 0 stands in range of actor 1, who has 25 health
/// and actor 0's cooldown is ready.
fn one_hit_from_victory() -> Arena {
    let mut arena = arena(1);
    while arena.can_attack(0, 1).is_err() {
        arena.step(&[ArenaCommand::Move {
            actor: 0,
            x: 0,
            z: -1000,
        }]);
        assert!(arena.tick() < 1_000, "actor 0 should reach range");
    }
    for _ in 0..3 {
        arena.step(&[ArenaCommand::Attack {
            actor: 0,
            target: 1,
        }]);
        for _ in 0..ATTACK_COOLDOWN {
            arena.step(&[]);
        }
    }
    assert_eq!(arena.fighters()[1].health, 25);
    arena
}

#[test]
fn matches_wait_for_every_seat_before_the_rules_advance() {
    let mut session = session_with(arena(1));
    let out = session.attach(0, conn(0));
    let Out::Send {
        message: ServerMessage::Welcome(welcome),
        ..
    } = &out[0]
    else {
        panic!("{out:?}");
    };
    assert_eq!(
        (
            welcome.actor,
            welcome.team_size,
            welcome.tick,
            welcome.tick_rate,
            welcome.round_ticks,
            welcome.map
        ),
        (0, 1, 0, 30, MAX_ROUND_TICKS, 42)
    );
    assert!(snapshot(&out, conn(0)).is_some());
    let before = session.arena().clone();
    session.receive(0, conn(0), frame(1));
    let out = session.tick();
    assert_eq!(session.phase(), Phase::Waiting);
    // Early inputs are acknowledged but never act.
    assert_eq!(snapshot(&out, conn(0)).unwrap().ack, 1);
    assert_eq!(session.arena(), &before);
    session.attach(1, conn(1));
    session.tick();
    assert_eq!(session.phase(), Phase::Running);
    assert_eq!(session.arena().tick(), 0, "starting does not advance");
    session.tick();
    assert_eq!(session.arena().tick(), 1);

    // A match nobody fills ends at the start timeout.
    let mut lonely = session_with(arena(1));
    lonely.attach(0, conn(0));
    for _ in 1..LIMITS.start_timeout {
        lonely.tick();
    }
    assert_eq!(lonely.phase(), Phase::Waiting);
    assert_eq!(
        lonely.tick(),
        vec![Out::Close {
            conn: conn(0),
            error: Some(ErrorCode::MatchOver)
        }]
    );
    assert!(lonely.is_ended());
}

#[test]
fn inputs_apply_one_per_tick_in_order_and_only_to_their_seat() {
    let mut session = running(arena(1));
    let mut replay = session.arena().clone();
    let second = InputFrame {
        seq: 2,
        movement: [1000, 0],
        facing: 77,
        jump: true,
        action: arena_protocol::WireAction::None,
        view_tick: 0,
    };
    session.receive(0, conn(0), frame(1));
    session.receive(0, conn(0), second);
    // Each input is exactly one tick of rules, so a client replaying its
    // own unacknowledged inputs predicts the same state.
    for (expected_ack, input) in [(1, frame(1)), (2, second)] {
        let out = session.tick();
        let mut slot = ActorInput::default();
        input.merge_into(&mut slot);
        replay.advance(&[slot]);
        assert_eq!(session.arena(), &replay);
        let own = snapshot(&out, conn(0)).unwrap();
        assert_eq!((own.ack, own.hash), (expected_ack, replay.state_hash()));
        assert_eq!(
            own.fighters[0],
            FighterState::new(&replay.fighters()[0], true)
        );
        let other = snapshot(&out, conn(1)).unwrap();
        assert_eq!(other.ack, 0);
        assert_eq!(other.fighters, own.fighters, "everyone sees the same state");
    }
    assert!(session.arena().fighters()[0].height > 0);
    assert_eq!(session.arena().fighters()[0].facing, 77);
    assert_eq!(
        session.arena().fighters()[1],
        arena(1).fighters()[1],
        "the other seat did not move"
    );
}

#[test]
fn clumps_drain_one_per_tick_and_only_a_lasting_backlog_is_folded() {
    let mut session = Session::new(arena(1), 42, Limits::default(), Arc::default());
    session.attach(0, conn(0));
    session.attach(1, conn(1));
    session.tick();
    // Nothing arrived: the fighter stands still, and the tick counts as starved.
    let start = session.arena().fighters()[0];
    session.tick();
    assert_eq!(session.arena().fighters()[0].position, start.position);
    assert_eq!(Metrics::get(&session.metrics.inputs_starved), 2);
    // A clump of ten (jitter, or a tab catching up) is applied one per
    // tick, in order, with nothing folded; each snapshot says what waits.
    for seq in 1..=10 {
        session.receive(0, conn(0), frame(seq));
    }
    for ack in 1..=10u32 {
        let out = session.tick();
        let own = snapshot(&out, conn(0)).unwrap();
        assert_eq!((own.ack, own.queued), (ack, 10 - ack as u8));
        assert_eq!(snapshot(&out, conn(1)).unwrap().queued, 0, "per seat");
    }
    assert_eq!(Metrics::get(&session.metrics.inputs_folded), 0);
    // A client sending two per tick keeps the queue long; after a second
    // above the backlog the excess folds (a jump among them is kept).
    let mut seq = 10;
    for _ in 0..FOLD_AFTER + 2 * MAX_BACKLOG as u64 {
        for _ in 0..2 {
            seq += 1;
            session.receive(
                0,
                conn(0),
                InputFrame {
                    jump: seq == 30,
                    ..frame(seq)
                },
            );
        }
        session.tick();
    }
    assert!(Metrics::get(&session.metrics.inputs_folded) > 0);
    let queued = snapshot(&session.tick(), conn(0)).unwrap().queued;
    assert!(usize::from(queued) <= MAX_BACKLOG + 2, "{queued}");
    // The jitter metric saw the empty ticks, the clump and the doubles.
    let [zeros, _, twos, _, four_or_more] = session.metrics.arrivals();
    assert!(zeros > 0 && twos > 0 && four_or_more == 1);
}

#[test]
fn stale_foreign_and_overflowing_inputs_are_dropped() {
    let mut session = running(arena(1));
    let start = session.arena().fighters()[0].position;
    session.receive(
        0,
        conn(0),
        InputFrame {
            jump: true,
            ..frame(5)
        },
    );
    session.receive(0, conn(0), frame(5));
    session.receive(0, conn(0), frame(3));
    assert_eq!(Metrics::get(&session.metrics.inputs_stale), 2);
    // A connection that does not hold the seat is ignored.
    session.receive(0, 999, frame(100));
    for seq in 6..=9 {
        session.receive(0, conn(0), frame(seq));
    }
    // The queue holds three: 5 and 6 were dropped, oldest first.
    assert_eq!(Metrics::get(&session.metrics.inputs_overflow), 2);
    for expected in [7, 8, 9] {
        let out = session.tick();
        assert_eq!(snapshot(&out, conn(0)).unwrap().ack, expected);
    }
    let fighter = session.arena().fighters()[0];
    assert_eq!(
        fighter.height, 0,
        "the dropped oldest input carried the jump"
    );
    assert_ne!(fighter.position, start);
    // Nothing queued: the next tick is idle.
    let before = session.arena().fighters()[0].position;
    session.tick();
    assert_eq!(session.arena().fighters()[0].position, before);
}

#[test]
fn a_reconnect_replaces_the_old_connection_and_restarts_its_sequence() {
    let mut session = running(arena(1));
    session.receive(0, conn(0), frame(50));
    assert_eq!(snapshot(&session.tick(), conn(0)).unwrap().ack, 50);
    let out = session.attach(0, 200);
    assert_eq!(
        out[0],
        Out::Close {
            conn: conn(0),
            error: Some(ErrorCode::Replaced)
        }
    );
    assert!(matches!(
        out[1],
        Out::Send {
            conn: 200,
            message: ServerMessage::Welcome(_)
        }
    ));
    // The old connection's input 50 was applied, but nothing of the new
    // one has been: its welcome snapshot acknowledges nothing.
    assert_eq!(snapshot(&out, 200).unwrap().ack, 0);
    session.receive(0, 200, frame(1));
    session.receive(0, conn(0), frame(51));
    // The old connection's late detach does not unseat the new one.
    session.detach(0, conn(0));
    let before = session.arena().fighters()[0].position;
    let out = session.tick();
    assert_ne!(session.arena().fighters()[0].position, before);
    assert_eq!(snapshot(&out, 200).unwrap().ack, 1);
    assert!(snapshot(&out, conn(0)).is_none());
    assert_eq!(
        session.connections().collect::<Vec<_>>(),
        vec![200, conn(1)]
    );
}

#[test]
fn rejections_reach_only_the_actor_that_was_refused() {
    let mut session = running(arena(1));
    session.receive(
        1,
        conn(1),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(0),
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    let rejected = |to| {
        snapshot(&out, to)
            .unwrap()
            .events
            .iter()
            .any(|event| event.event == WireEvent::Rejected(WireRejection::OutOfRange))
    };
    assert!(rejected(conn(1)));
    assert!(!rejected(conn(0)));
    assert_eq!(session.arena().fighters()[0].health, 100);
}

#[test]
fn finished_matches_report_the_result_linger_then_close() {
    let mut session = running(one_hit_from_victory());
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    let expected_board = vec![
        FighterScore {
            damage_done: 25,
            damage_taken: 0,
            healing_done: 0,
            healing_received: 0,
            killing_blows: 1,
        },
        FighterScore {
            damage_done: 0,
            damage_taken: 25,
            healing_done: 0,
            healing_received: 0,
            killing_blows: 0,
        },
    ];
    for seat in 0..2 {
        let final_state = snapshot(&out, conn(seat)).unwrap();
        assert_eq!(
            final_state.result,
            Some(arena_protocol::WireResult::Won { team: 0 })
        );
        assert_eq!(final_state.scoreboard.as_ref(), Some(&expected_board));
        assert!(final_state.events.iter().any(|event| matches!(
            event.event,
            WireEvent::Finished(arena_protocol::WireResult::Won { team: 0 })
        )));
    }
    assert!(matches!(session.phase(), Phase::Finished { .. }));
    let tick = session.arena().tick();
    for _ in 1..LIMITS.linger {
        session.receive(0, conn(0), frame(2));
        assert_eq!(session.tick(), vec![]);
    }
    assert_eq!(session.arena().tick(), tick, "a finished round stays put");
    assert_eq!(
        session.tick(),
        vec![
            Out::Close {
                conn: conn(0),
                error: Some(ErrorCode::MatchOver)
            },
            Out::Close {
                conn: conn(1),
                error: Some(ErrorCode::MatchOver)
            },
        ]
    );
    assert!(session.is_ended());
    assert_eq!(
        session.attach(0, 300),
        vec![Out::Close {
            conn: 300,
            error: Some(ErrorCode::MatchOver)
        }]
    );
}

#[test]
fn abandoned_matches_end_after_the_grace_period_but_not_while_anyone_stays() {
    let mut session = running(arena(1));
    session.detach(0, conn(0));
    for _ in 0..LIMITS.grace * 3 {
        session.tick();
    }
    assert_eq!(session.phase(), Phase::Running);
    session.detach(1, conn(1));
    for _ in 1..LIMITS.grace {
        assert_eq!(session.tick(), vec![]);
        assert_eq!(session.phase(), Phase::Running);
    }
    assert_eq!(session.tick(), vec![]);
    assert!(session.is_ended());
}

#[test]
fn unknown_seats_are_refused_and_shutdown_closes_everyone() {
    let mut session = running(arena(1));
    assert_eq!(
        session.attach(2, 7),
        vec![Out::Close {
            conn: 7,
            error: Some(ErrorCode::BadTicket)
        }]
    );
    assert_eq!(
        session.shut_down(),
        vec![
            Out::Close {
                conn: conn(0),
                error: Some(ErrorCode::ServerShutdown)
            },
            Out::Close {
                conn: conn(1),
                error: Some(ErrorCode::ServerShutdown)
            },
        ]
    );
    assert!(session.is_ended());
}

#[test]
fn disconnected_seats_are_reported_idle_and_never_forfeit() {
    let mut session = running(arena(1));
    session.receive(0, conn(0), frame(1));
    session.tick();
    let walked = session.arena().fighters()[0];
    session.detach(0, conn(0));
    let out = session.tick();
    let seen = snapshot(&out, conn(1)).unwrap();
    assert!(!seen.fighters[0].connected);
    assert!(seen.fighters[1].connected);
    assert_eq!(session.connected(), vec![false, true]);
    // Long past the grace period: still running, fighter idle in place.
    for _ in 0..LIMITS.grace * 10 {
        session.tick();
    }
    assert_eq!(session.phase(), Phase::Running);
    assert_eq!(session.arena().result(), None);
    let idle = session.arena().fighters()[0];
    assert_eq!(
        (idle.position, idle.height, idle.health),
        (walked.position, walked.height, walked.health)
    );
    // Rejoining restores the seat for everyone.
    let out = session.attach(0, 500);
    assert!(snapshot(&out, 500).unwrap().fighters[0].connected);
    let out = session.tick();
    assert!(snapshot(&out, conn(1)).unwrap().fighters[0].connected);
}

/// A client that predicts its own movement, with its inputs reaching the
/// server two ticks late, against the real session.
fn predicting(late: impl Fn(u32) -> bool, jump_at: Option<u32>) -> (u64, u64) {
    use arena_protocol::prediction::Predictor;
    let layout = arena_map::colosseum().layout;
    let mut session = running(arena(1));
    let mut predictor = Predictor::default();
    // (arrival tick, frame), in sending order; TCP keeps them ordered.
    let mut in_flight: std::collections::VecDeque<(u32, InputFrame)> = Default::default();
    let mut worst = 0;
    for seq in 1..=60u32 {
        let input = InputFrame {
            jump: jump_at == Some(seq),
            ..frame(seq)
        };
        let mut slot = ActorInput::default();
        input.merge_into(&mut slot);
        let out = session.tick();
        if let Some(snapshot) = snapshot(&out, conn(0)) {
            let fighters = snapshot
                .fighters
                .iter()
                .map(FighterState::combatant)
                .collect();
            if let Some(moved) =
                predictor.reconcile(&layout, snapshot.tick, fighters, None, 0, snapshot.ack)
            {
                worst = worst.max(moved);
            }
        }
        predictor.apply(seq, slot);
        // Inputs land two ticks after sending; a late one lands a tick
        // later, holding up the ones behind it.
        let arrives =
            (seq + 2 + u32::from(late(seq))).max(in_flight.back().map_or(0, |(at, _)| *at));
        in_flight.push_back((arrives, input));
        while in_flight.front().is_some_and(|(at, _)| *at <= seq) {
            let (_, frame) = in_flight.pop_front().unwrap();
            session.receive(0, conn(0), frame);
        }
    }
    (predictor.corrections(), worst)
}

#[test]
fn a_predicting_client_agrees_with_the_server_while_running_on_the_ground() {
    // Steady delivery: never corrected.
    assert_eq!(predicting(|_| false, None), (0, 0));
    // A late input leaves an idle tick, but on the ground that only delays
    // the same steps: still never corrected.
    assert_eq!(predicting(|seq| seq % 10 == 0, None), (0, 0));
    // In the air, an idle tick still advances the arc, so a late input
    // mid-jump is corrected, by at most one tick of motion (a step across
    // plus a tick of rise or fall).
    let (corrections, worst) = predicting(|seq| seq == 30, Some(20));
    assert!(corrections > 0);
    let tick_of_motion = (arena_sim::arena::MOVE_PER_TICK + arena_sim::arena::JUMP_SPEED) as u64;
    assert!(worst > 0 && worst <= tick_of_motion, "{worst}");
}

/// Outcome of a simulated browser client against the real session.
#[derive(Debug)]
struct Simulated {
    folded: u64,
    overflowed: u64,
    starved: u64,
    corrections: u64,
    /// Average inputs queued for the client over the last 10 s.
    late_queue: f64,
}

/// A browser-like client for 60 s: fixed 30 Hz inputs sent on 240 Hz
/// frames with jitter, starting right on the server's tick boundaries, a
/// clock `drift` fast, and a burst of seven catch-up inputs at 20 s (a tab
/// back from the background). It predicts its own movement, and paces its
/// input interval from the snapshots when `paced`.
fn browser_like(paced: bool, drift: f64, clump: Option<f64>) -> Simulated {
    browser_starting_after(0.0, paced, drift, clump)
}

/// As `browser_like`, but the client's engine only starts `late` seconds
/// after the match does (the page is still loading), so the server first
/// sees an empty queue and the client has no tick-rate measurement yet.
fn browser_starting_after(late: f64, paced: bool, drift: f64, clump: Option<f64>) -> Simulated {
    use arena_protocol::prediction::{Pacer, Predictor, TickRate};
    let layout = arena_map::colosseum().layout;
    // The production limits (an eight-input queue).
    let mut session = Session::new(arena(1), 42, Limits::default(), Arc::default());
    session.attach(0, conn(0));
    session.attach(1, conn(1));
    session.tick();
    // Only this client's seat is measured; the opponent steps away (the
    // match runs on while anyone is connected).
    session.detach(1, conn(1));
    let mut predictor = Predictor::default();
    let mut pacer = Pacer::default();
    // The client's own clock, which runs `drift` fast against the server's.
    let mut tick_rate = TickRate::default();
    let mut local_time = 0.0;
    let mut backlog = Vec::new();
    let mut rng = 0x2545_F491_4F6C_DD1D_u64;
    let mut noise = move |scale: f64| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % 10_000) as f64 / 10_000.0 * scale
    };
    let tick = 1.0 / 30.0;
    let frame = 1.0 / 240.0;
    // The engine's clock starts when it does: no catch-up for the time
    // before it loaded.
    let (mut accumulator, mut seq, mut frame_time) = (0.0, 0u32, late);
    let mut stalled = false;
    let mut in_flight: std::collections::VecDeque<(f64, InputFrame)> = Default::default();
    let mut queued = Vec::new();
    for server_tick in 1..=1800u32 {
        let tick_at = f64::from(server_tick) * tick;
        // Client frames up to this server tick.
        while frame_time < tick_at && tick_at >= late {
            frame_time += (frame + noise(0.0005) - 0.00025) / (1.0 + drift);
            accumulator += frame;
            local_time += frame;
            if server_tick == 600 && !stalled {
                stalled = true;
                accumulator += 7.0 * tick; // stalled, then catching up at once
            }
            // Paced clients send at the server's rate as measured on their
            // own clock, stretched to steer the queue; unpaced ones assume
            // the nominal rate.
            let interval = if paced {
                pacer.factor() / tick_rate.rate()
            } else {
                tick
            };
            let mut sent = 0;
            while accumulator >= interval {
                accumulator -= interval;
                if sent == 2 {
                    continue; // at most two a frame, like the client
                }
                sent += 1;
                seq += 1;
                let input = frame_input(seq);
                let mut slot = ActorInput::default();
                input.merge_into(&mut slot);
                predictor.apply(seq, slot);
                // A relay that forwards in clumps holds each input until
                // its next flush; order is kept.
                let mut arrives = frame_time + noise(0.002);
                if let Some(period) = clump {
                    arrives = ((arrives / period).floor() + 1.0) * period;
                }
                let arrives = arrives.max(in_flight.back().map_or(0.0, |(at, _)| *at));
                in_flight.push_back((arrives, input));
            }
        }
        while in_flight.front().is_some_and(|(at, _)| *at <= tick_at) {
            let (_, input) = in_flight.pop_front().unwrap();
            session.receive(0, conn(0), input);
        }
        let out = session.tick();
        let snapshot = snapshot(&out, conn(0)).unwrap();
        // Snapshots wait in the runtime's queue until the engine starts,
        // then arrive together in its first frame.
        backlog.push(snapshot.tick);
        if tick_at >= late {
            pacer.observe(snapshot.queued);
            tick_rate.observe_frame(local_time, backlog.drain(..));
        }
        if server_tick > 1500 {
            queued.push(f64::from(snapshot.queued));
        }
        let fighters = snapshot
            .fighters
            .iter()
            .map(FighterState::combatant)
            .collect();
        predictor.reconcile(&layout, snapshot.tick, fighters, None, 0, snapshot.ack);
    }
    Simulated {
        folded: Metrics::get(&session.metrics.inputs_folded),
        overflowed: Metrics::get(&session.metrics.inputs_overflow),
        // The first tick, before any input could arrive, does not count.
        starved: Metrics::get(&session.metrics.inputs_starved).saturating_sub(1),
        corrections: predictor.corrections(),
        late_queue: queued.iter().sum::<f64>() / queued.len() as f64,
    }
}

/// Running in a circle, so every step moves (and every lost step shows).
fn frame_input(seq: u32) -> InputFrame {
    let angle = f64::from(seq) * 0.05;
    InputFrame {
        seq,
        movement: [(angle.cos() * 1000.0) as i16, (angle.sin() * 1000.0) as i16],
        ..InputFrame::default()
    }
}

#[test]
fn paced_clients_hold_a_small_cushion_through_jitter_clumps_drift_and_bursts() {
    // Smooth delivery with a slightly fast clock and a background-tab burst.
    let smooth = browser_like(true, 0.005, None);
    assert_eq!(
        (smooth.folded, smooth.overflowed, smooth.corrections),
        (0, 0, 0),
        "{smooth:?}"
    );
    assert!(smooth.late_queue <= 2.5, "{smooth:?}");
    // A relay that forwards in 50 ms clumps: the queue grows into a jitter
    // buffer just deep enough that the server rarely waits.
    let clumped = browser_like(true, 0.005, Some(0.05));
    assert_eq!(
        (clumped.folded, clumped.overflowed, clumped.corrections),
        (0, 0, 0),
        "{clumped:?}"
    );
    assert!(clumped.starved < 30, "{clumped:?}");
    assert!(clumped.late_queue <= 4.0, "{clumped:?}");
    // A client clock 9% fast against the server's (a virtual machine's
    // miscalibrated clock was measured this far off): far beyond what pacing
    // alone can steer, but the client follows the measured tick rate.
    let skewed = browser_like(true, 0.09, Some(0.05));
    assert_eq!(
        (skewed.folded, skewed.overflowed, skewed.corrections),
        (0, 0, 0),
        "{skewed:?}"
    );
    assert!(skewed.late_queue <= 4.0, "{skewed:?}");
    // The browser test's case: the engine starts three seconds after the
    // match (still loading) and receives the queued snapshots in its first
    // frame, with the server's clock 9% slow. The client has no
    // rate measurement yet and the queue starts empty.
    let late_start = browser_starting_after(3.0, true, 0.09, Some(0.05));
    eprintln!("RESULT late start {late_start:?}");
    assert_eq!(
        (
            late_start.folded,
            late_start.overflowed,
            late_start.corrections
        ),
        (0, 0, 0),
        "{late_start:?}"
    );
    // Unpaced, a clock 2% fast keeps the queue long until it folds, each
    // fold costing a predicted step.
    let unpaced = browser_like(false, 0.02, Some(0.05));
    assert!(unpaced.folded > 0 && unpaced.corrections > 0, "{unpaced:?}");
    eprintln!(
        "RESULT smooth {smooth:?}\nRESULT clumped {clumped:?}\nRESULT skewed {skewed:?}\nRESULT unpaced {unpaced:?}"
    );
}

#[test]
fn tick_encoded_matches_tick_wire_bytes_including_per_seat_rejections() {
    let mut a = running(arena(2));
    let mut b = running(arena(2));
    for step in 0..4u32 {
        let feed = |session: &mut Session| {
            session.receive(0, conn(0), frame(step + 1));
            if step == 1 {
                // Seat 1 gets a per-seat rejection; seat 2 jumps (shared event).
                session.receive(
                    1,
                    conn(1),
                    InputFrame {
                        seq: 1,
                        action: InputFrame::pulse(0),
                        ..InputFrame::default()
                    },
                );
                session.receive(
                    2,
                    conn(2),
                    InputFrame {
                        seq: 1,
                        jump: true,
                        ..InputFrame::default()
                    },
                );
            }
            if step == 2 {
                session.detach(3, conn(3));
            }
        };
        feed(&mut a);
        feed(&mut b);
        let expected: Vec<(ConnId, Vec<u8>)> = a
            .tick()
            .into_iter()
            .map(|out| match out {
                Out::Send { conn, message } => (conn, message.encode()),
                Out::Close { .. } => panic!("unexpected close"),
            })
            .collect();
        let mut actual = Vec::new();
        b.tick_encoded(|out| match out {
            EncodedOut::Frame { conn, frame } => actual.push((conn, frame)),
            EncodedOut::SharedFrame { conn, frame } => actual.push((conn, frame.to_vec())),
            EncodedOut::Close { .. } => panic!("unexpected close"),
        });
        assert_eq!(actual, expected, "step {step}");
    }
}

#[test]
fn round_clock_expiry_finishes_session_as_time_limit_draw_lingers_and_closes() {
    let limited = arena(1).with_round_limit(4).unwrap();
    let mut session = session_with(limited);
    let out = session.attach(0, conn(0));
    let Out::Send {
        message: ServerMessage::Welcome(welcome),
        ..
    } = &out[0]
    else {
        panic!("{out:?}");
    };
    assert_eq!(welcome.round_ticks, 4);
    session.attach(1, conn(1));
    // Transition Waiting -> Running on tick 0; the round clock has not ticked yet.
    session.tick();
    assert_eq!(session.phase(), Phase::Running);
    assert_eq!(session.arena().remaining_ticks(), 4);
    for expected_tick in 1..=3u64 {
        let out = session.tick();
        assert_eq!(session.phase(), Phase::Running);
        let seen = snapshot(&out, conn(0)).unwrap();
        assert_eq!(seen.tick, expected_tick);
        assert_eq!(seen.result, None);
    }
    // Tick 4 reaches the round limit: both seats receive TimeLimitDraw and Finished event.
    let out = session.tick();
    assert!(matches!(session.phase(), Phase::Finished { .. }));
    assert_eq!(
        session
            .metrics
            .finished(arena_sim::arena::RoundResult::TimeLimitDraw),
        1
    );
    for seat in 0..2 {
        let seen = snapshot(&out, conn(seat)).unwrap();
        assert_eq!(seen.tick, 4);
        assert_eq!(seen.result, Some(WireResult::TimeLimitDraw));
        assert!(seen.events.iter().any(|event| {
            event.tick == 3 && event.event == WireEvent::Finished(WireResult::TimeLimitDraw)
        }));
    }
    // Lingers before closing with MatchOver.
    for _ in 1..LIMITS.linger {
        assert_eq!(session.tick(), vec![]);
    }
    assert_eq!(
        session.tick(),
        vec![
            Out::Close {
                conn: conn(0),
                error: Some(ErrorCode::MatchOver),
            },
            Out::Close {
                conn: conn(1),
                error: Some(ErrorCode::MatchOver),
            },
        ]
    );
    assert!(session.is_ended());
}

#[test]
fn session_enforces_authoritative_casts_interrupts_lockout_and_reconnect_preservation() {
    use arena_protocol::{WireAction, WireInterruptReason, WireSpell};
    use arena_sim::arena::{Combatant, Position};
    use arena_sim::spells::{
        INTERRUPT_LOCKOUT_TICKS, MAX_RESOURCE, RIFT_BOLT_CAST_TICKS, RIFT_BOLT_COST,
    };

    let layout = arena_map::colosseum().layout;
    let fighters = vec![
        Combatant::new(Position { x: 0, z: 2_500 }, 0),
        Combatant::new(Position { x: 0, z: -2_500 }, arena_sim::arena::HALF_TURN),
    ];
    let close = Arena::restore(layout, 0, fighters, None).unwrap();
    let mut session = running(close);

    // Seat 0 starts Rift Bolt; both seats see CastStarted and active_cast in snapshots.
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 1,
            action: WireAction::Cast {
                spell: WireSpell::RiftBolt,
                target: Some(1),
            },
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    let snap0 = snapshot(&out, conn(0)).unwrap();
    let snap1 = snapshot(&out, conn(1)).unwrap();
    assert_eq!(snap0.hash, snap1.hash);
    assert_eq!(snap0.fighters[0].resource, MAX_RESOURCE - RIFT_BOLT_COST);
    assert_eq!(
        snap0.fighters[0].active_cast.unwrap().remaining_ticks,
        RIFT_BOLT_CAST_TICKS
    );
    assert!(snap1.events.iter().any(|e| matches!(
        e.event,
        WireEvent::CastStarted {
            actor: 0,
            spell: WireSpell::RiftBolt,
            target: Some(1),
            duration_ticks: RIFT_BOLT_CAST_TICKS,
        }
    )));

    // Seat 1 interrupts with Quell; Seat 0 gets locked out.
    session.receive(
        1,
        conn(1),
        InputFrame {
            seq: 1,
            action: WireAction::Cast {
                spell: WireSpell::Quell,
                target: Some(0),
            },
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    let snap0 = snapshot(&out, conn(0)).unwrap();
    assert_eq!(snap0.fighters[0].active_cast, None);
    assert_eq!(
        snap0.fighters[0].lockout_remaining,
        INTERRUPT_LOCKOUT_TICKS - 1
    );
    assert!(snap0.events.iter().any(|e| {
        e.event
            == WireEvent::CastInterrupted {
                actor: 0,
                spell: WireSpell::RiftBolt,
                reason: WireInterruptReason::Interrupted,
                by: Some(1),
            }
    }));

    // Reconnecting Seat 0 preserves lockout, resource and cooldowns verbatim.
    let reconnect = session.attach(0, 900);
    let restored = snapshot(&reconnect, 900).unwrap();
    assert_eq!(
        restored.fighters[0].lockout_remaining,
        INTERRUPT_LOCKOUT_TICKS - 1
    );
    assert_eq!(restored.fighters[0].resource, MAX_RESOURCE - RIFT_BOLT_COST);

    // Attempting to cast while locked out returns Silenced to Seat 0 ONLY.
    session.receive(
        0,
        900,
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    assert!(
        snapshot(&out, 900)
            .unwrap()
            .events
            .iter()
            .any(|e| e.event == WireEvent::Rejected(WireRejection::Silenced))
    );
    assert!(
        !snapshot(&out, conn(1))
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e.event, WireEvent::Rejected(_)))
    );
}

#[test]
fn session_enforces_monotonic_bounded_view_tick_for_targeting_lag_comp() {
    use arena_sim::arena::{ATTACK_DAMAGE, ATTACK_RANGE, Combatant, Position};
    let layout = arena_map::colosseum().layout;
    // Start fighters at 5,800 mm apart (within Pulse 6,000 mm range).
    let fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 5_800, z: 0 }, 0),
    ];
    let arena = Arena::restore(layout, 0, fighters, None).unwrap();
    let mut session = Session::new(arena, 1, Limits::default(), Arc::default());
    session.attach(0, conn(0));
    session.attach(1, conn(1));
    // Transition Waiting -> Running.
    session.tick();
    // Tick 1: record in-range position (5,800 mm) in Arena history.
    session.tick();
    assert_eq!(session.arena().tick(), 1);

    // Seat 1 flees +X for 3 ticks (5,800 -> 6,040 -> 6,280 -> 6,520 mm).
    for seq in 1..=3u32 {
        session.receive(
            1,
            conn(1),
            InputFrame {
                seq,
                movement: [1000, 0],
                ..InputFrame::default()
            },
        );
        session.tick();
    }
    assert_eq!(session.arena().tick(), 4);
    assert_eq!(session.arena().fighters()[1].position.x, 6_520);

    // 1. Future view_tick (u64::MAX) clamps to post_tick (zero rewind) -> OutOfRange!
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            view_tick: u64::MAX,
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    assert!(
        snapshot(&out, conn(0))
            .unwrap()
            .events
            .iter()
            .any(|e| e.event == WireEvent::Rejected(WireRejection::OutOfRange))
    );

    // 2. Monotonic clamp: because seat 0 already claimed view_tick >= 5, attempting to
    // shop backwards to tick 1 must NOT grant rewind -> still OutOfRange!
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 2,
            action: InputFrame::pulse(1),
            view_tick: 1,
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    assert!(
        snapshot(&out, conn(0))
            .unwrap()
            .events
            .iter()
            .any(|e| e.event == WireEvent::Rejected(WireRejection::OutOfRange))
    );

    // 3. Reconnecting must NOT reset monotonic max_view_tick.
    session.attach(0, 777);
    session.receive(
        0,
        777,
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            view_tick: 1,
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    assert!(
        snapshot(&out, 777)
            .unwrap()
            .events
            .iter()
            .any(|e| e.event == WireEvent::Rejected(WireRejection::OutOfRange))
    );

    // 4. Honest lagged client (fresh session) reporting view_tick = 1 (when target
    // was at 5,800 mm) hits cleanly within the 6-tick cap!
    let fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 5_800, z: 0 }, 0),
    ];
    let arena = Arena::restore(arena_map::colosseum().layout, 0, fighters, None).unwrap();
    let mut honest = Session::new(arena, 1, Limits::default(), Arc::default());
    honest.attach(0, conn(0));
    honest.attach(1, conn(1));
    honest.tick(); // Waiting -> Running
    honest.tick(); // Tick 1 records 5,800 mm
    for seq in 1..=3u32 {
        honest.receive(
            1,
            conn(1),
            InputFrame {
                seq,
                movement: [1000, 0],
                ..InputFrame::default()
            },
        );
        honest.tick();
    }
    assert!(honest.arena().fighters()[1].position.x > ATTACK_RANGE);
    honest.receive(
        0,
        conn(0),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            view_tick: 1,
            ..InputFrame::default()
        },
    );
    let out = honest.tick();
    let snap0 = snapshot(&out, conn(0)).unwrap();
    assert!(snap0.events.iter().any(|e| matches!(
        e.event,
        WireEvent::Hit {
            actor: 0,
            target: 1,
            damage
        } if damage == ATTACK_DAMAGE
    )));
}

#[test]
fn session_view_tick_zero_cap_and_backlog_folding_preserve_lag_comp_boundaries() {
    use arena_sim::arena::{ATTACK_DAMAGE, Combatant, MAX_LAG_COMP_TICKS, Position};
    let layout = arena_map::colosseum().layout;

    // 1. view_tick == 0 yields 0 rewind without modifying max_view_tick, so a
    // subsequent honest view_tick still works; fleeing past MAX_LAG_COMP_TICKS
    // is strictly rejected even with view_tick = 1.
    let fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 5_800, z: 0 }, 0),
    ];
    let arena = Arena::restore(layout.clone(), 0, fighters, None).unwrap();
    let mut session = Session::new(arena, 1, Limits::default(), Arc::default());
    session.attach(0, conn(0));
    session.attach(1, conn(1));
    session.tick(); // Waiting -> Running
    session.tick(); // tick 1 records 5,800 mm
    for seq in 1..=2u32 {
        session.receive(
            1,
            conn(1),
            InputFrame {
                seq,
                movement: [1000, 0],
                ..InputFrame::default()
            },
        );
        session.tick();
    }
    // At tick 3, target is at 6,280 mm (out of 6,000 mm range).
    // view_tick = 0 must reject OutOfRange without poisoning max_view_tick.
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            view_tick: 0,
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    assert!(
        snapshot(&out, conn(0))
            .unwrap()
            .events
            .iter()
            .any(|e| e.event == WireEvent::Rejected(WireRejection::OutOfRange))
    );
    // Honest view_tick = 1 on the next tick (rewind = 5 - 1 = 4 <= 6) still hits!
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 2,
            action: InputFrame::pulse(1),
            view_tick: 1,
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    assert!(snapshot(&out, conn(0)).unwrap().events.iter().any(|e| {
        matches!(
            e.event,
            WireEvent::Hit {
                actor: 0,
                target: 1,
                damage
            } if damage == ATTACK_DAMAGE
        )
    }));

    // Advance past MAX_LAG_COMP_TICKS + 2 ticks and reset Pulse cooldown so we test the cap.
    for _ in 0..(arena_sim::spells::PULSE_COOLDOWN_TICKS + u32::from(MAX_LAG_COMP_TICKS)) {
        session.tick();
    }
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 3,
            action: InputFrame::pulse(1),
            view_tick: 1,
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    assert!(
        snapshot(&out, conn(0))
            .unwrap()
            .events
            .iter()
            .any(|e| e.event == WireEvent::Rejected(WireRejection::OutOfRange))
    );

    // 2. Backlog folding: when [Cast(Pulse, view_tick=1), None(view_tick=post_tick)]
    // fold into one tick after FOLD_AFTER ticks over MAX_BACKLOG, the cast keeps
    // its originating frame's rewind_ticks and still lands!
    let fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 5_800, z: 0 }, 0),
    ];
    let arena = Arena::restore(layout, 0, fighters, None).unwrap();
    let mut folding = Session::new(arena, 1, Limits::default(), Arc::default());
    folding.attach(0, conn(0));
    folding.attach(1, conn(1));
    folding.tick(); // Waiting -> Running
    // Keep seat 0's queue above MAX_BACKLOG for FOLD_AFTER - 1 ticks so the next tick folds.
    let mut seq0 = 0u32;
    for _ in 0..=(MAX_BACKLOG + 1) {
        seq0 += 1;
        folding.receive(
            0,
            conn(0),
            InputFrame {
                seq: seq0,
                ..InputFrame::default()
            },
        );
    }
    for _ in 0..FOLD_AFTER {
        folding.tick();
        seq0 += 1;
        folding.receive(
            0,
            conn(0),
            InputFrame {
                seq: seq0,
                ..InputFrame::default()
            },
        );
    }
    let in_range_tick = folding.arena().tick();
    // Now move enemy out of range over 2 ticks while keeping seat 0's backlog full,
    // and enqueue a Cast with view_tick = in_range_tick followed by a None frame
    // with view_tick = in_range_tick + 2 that will fold together with it.
    for seq1 in 1..=2u32 {
        folding.receive(
            1,
            conn(1),
            InputFrame {
                seq: seq1,
                movement: [1000, 0],
                ..InputFrame::default()
            },
        );
        // Drain one tick and push an extra frame to trigger folding on tick 2.
        folding.tick();
        seq0 += 1;
        folding.receive(
            0,
            conn(0),
            InputFrame {
                seq: seq0,
                ..InputFrame::default()
            },
        );
    }
    assert!(folding.arena().fighters()[1].position.x > arena_sim::arena::ATTACK_RANGE);
    // Clear existing idle frames and set up exact fold: 2 frames to fold + MAX_BACKLOG trailing.
    let drain = folding.seats[0].pending.len();
    for _ in 0..drain {
        folding.seats[0].pending.pop_front();
    }
    folding.seats[0].over_backlog_since = Some(folding.clock.saturating_sub(FOLD_AFTER));
    seq0 += 1;
    folding.receive(
        0,
        conn(0),
        InputFrame {
            seq: seq0,
            action: InputFrame::pulse(1),
            view_tick: in_range_tick,
            ..InputFrame::default()
        },
    );
    seq0 += 1;
    folding.receive(
        0,
        conn(0),
        InputFrame {
            seq: seq0,
            action: WireAction::None,
            view_tick: folding.arena().tick() + 1,
            ..InputFrame::default()
        },
    );
    for _ in 0..MAX_BACKLOG {
        seq0 += 1;
        folding.receive(
            0,
            conn(0),
            InputFrame {
                seq: seq0,
                ..InputFrame::default()
            },
        );
    }
    let out = folding.tick();
    assert!(snapshot(&out, conn(0)).unwrap().events.iter().any(|e| {
        matches!(
            e.event,
            WireEvent::Hit {
                actor: 0,
                target: 1,
                damage
            } if damage == ATTACK_DAMAGE
        )
    }));
}

#[test]
fn scoreboard_tracks_damage_effective_healing_killing_blows_and_reconnects() {
    use arena_protocol::WireSpell;
    use arena_sim::arena::{Combatant, Position};
    use arena_sim::spells::MEND_CAST_TICKS;

    let layout = arena_map::colosseum().layout;
    let fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 3_000, z: 0 }, 0),
    ];
    let restored = Arena::restore(layout, 0, fighters, None).unwrap();
    let mut session = running(restored);
    let mut seq = [0u32; 2];

    let mut observed_damage_done = [0u32; 2];
    let mut observed_damage_taken = [0u32; 2];
    let mut observed_healing_done = [0u32; 2];
    let mut observed_healing_received = [0u32; 2];

    let observe = |out: &[Out],
                   dmg_done: &mut [u32; 2],
                   dmg_taken: &mut [u32; 2],
                   heal_done: &mut [u32; 2],
                   heal_recv: &mut [u32; 2]| {
        let snap = snapshot(out, conn(0))
            .or_else(|| snapshot(out, conn(1)))
            .or_else(|| {
                out.iter().find_map(|o| match o {
                    Out::Send {
                        message: ServerMessage::Snapshot(s),
                        ..
                    } => Some(s),
                    _ => None,
                })
            })
            .unwrap();
        if snap.result.is_none() {
            assert_eq!(snap.scoreboard, None, "running snapshots omit scoreboard");
        }
        for tick_event in &snap.events {
            match tick_event.event {
                WireEvent::Hit {
                    actor,
                    target,
                    damage,
                } => {
                    dmg_done[usize::from(actor)] += u32::from(damage);
                    dmg_taken[usize::from(target)] += u32::from(damage);
                }
                WireEvent::Healed {
                    actor,
                    target,
                    amount,
                } => {
                    heal_done[usize::from(actor)] += u32::from(amount);
                    heal_recv[usize::from(target)] += u32::from(amount);
                }
                _ => {}
            }
        }
    };

    // Seat 0 hits seat 1 with Pulse (25 dmg -> 75 HP).
    seq[0] += 1;
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: seq[0],
            action: InputFrame::pulse(1),
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    observe(
        &out,
        &mut observed_damage_done,
        &mut observed_damage_taken,
        &mut observed_healing_done,
        &mut observed_healing_received,
    );

    // Seat 1 casts Mend on self (30 heal, capped to 25 missing HP -> effective 25!).
    // Also seat 0 casts Mend on self at full HP (100 HP -> 0 effective healing!).
    seq[1] += 1;
    session.receive(
        1,
        conn(1),
        InputFrame {
            seq: seq[1],
            action: WireAction::Cast {
                spell: WireSpell::Mend,
                target: Some(1),
            },
            ..InputFrame::default()
        },
    );
    // 1 tick to start the timed cast in Phase 4b, plus MEND_CAST_TICKS in Phase 4a to complete.
    for _ in 0..=MEND_CAST_TICKS {
        let out = session.tick();
        observe(
            &out,
            &mut observed_damage_done,
            &mut observed_damage_taken,
            &mut observed_healing_done,
            &mut observed_healing_received,
        );
    }
    assert_eq!(session.arena().fighters()[1].health, 100);
    assert_eq!(observed_healing_done[1], 25, "overheal is excluded");

    // Seat 0 casts Mend at full health (0 effective healing).
    seq[0] += 1;
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: seq[0],
            action: WireAction::Cast {
                spell: WireSpell::Mend,
                target: Some(0),
            },
            ..InputFrame::default()
        },
    );
    for _ in 0..=MEND_CAST_TICKS {
        let out = session.tick();
        observe(
            &out,
            &mut observed_damage_done,
            &mut observed_damage_taken,
            &mut observed_healing_done,
            &mut observed_healing_received,
        );
    }
    assert_eq!(observed_healing_done[0], 0);

    // Seat 0 disconnects and reconnects on a new connection id: accumulated stats survive!
    session.detach(0, conn(0));
    let reconnect_conn = 777;
    let attach_out = session.attach(0, reconnect_conn);
    assert_eq!(
        snapshot(&attach_out, reconnect_conn).unwrap().scoreboard,
        None
    );
    seq[0] = 0;

    // Seat 1 hits seat 0 once (25 dmg) and seat 0 finishes seat 1 with 4 Pulses (100 dmg).
    seq[1] += 1;
    session.receive(
        1,
        conn(1),
        InputFrame {
            seq: seq[1],
            action: InputFrame::pulse(0),
            ..InputFrame::default()
        },
    );
    let mut final_out = Vec::new();
    for round in 0..4 {
        seq[0] += 1;
        session.receive(
            0,
            reconnect_conn,
            InputFrame {
                seq: seq[0],
                action: InputFrame::pulse(1),
                ..InputFrame::default()
            },
        );
        let out = session.tick();
        observe(
            &out,
            &mut observed_damage_done,
            &mut observed_damage_taken,
            &mut observed_healing_done,
            &mut observed_healing_received,
        );
        if round == 3 {
            final_out = out;
            break;
        }
        for _ in 0..ATTACK_COOLDOWN {
            let out = session.tick();
            observe(
                &out,
                &mut observed_damage_done,
                &mut observed_damage_taken,
                &mut observed_healing_done,
                &mut observed_healing_received,
            );
        }
    }

    let s0 = snapshot(&final_out, reconnect_conn).unwrap();
    let s1 = snapshot(&final_out, conn(1)).unwrap();
    assert_eq!(
        s0.scoreboard, s1.scoreboard,
        "every seat sees identical totals"
    );
    let board = s0
        .scoreboard
        .as_ref()
        .expect("final snapshot carries scoreboard");
    assert_eq!(
        board,
        &vec![
            FighterScore {
                damage_done: 125,
                damage_taken: 25,
                healing_done: 0,
                healing_received: 0,
                killing_blows: 1,
            },
            FighterScore {
                damage_done: 25,
                damage_taken: 125,
                healing_done: 25,
                healing_received: 25,
                killing_blows: 0,
            },
        ]
    );
    for seat in 0..2 {
        assert_eq!(board[seat].damage_done, observed_damage_done[seat]);
        assert_eq!(board[seat].damage_taken, observed_damage_taken[seat]);
        assert_eq!(board[seat].healing_done, observed_healing_done[seat]);
        assert_eq!(
            board[seat].healing_received,
            observed_healing_received[seat]
        );
    }
}

#[test]
fn killing_blows_handle_simultaneous_team_kills_overkill_mutual_draws_and_time_limit() {
    use arena_sim::arena::{Combatant, Position};

    let layout = arena_map::colosseum().layout;

    // 1. 2v2 combined lethal hits: enemy 1 has 40 HP; seat 0 (25) + seat 2 (25) both hit
    // on the same tick. Seat 2 brings remaining HP from 15 -> 0 and gets the single KB.
    let mut fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 2_000, z: 0 }, 0),
        Combatant::new(Position { x: 4_000, z: 0 }, 0),
        Combatant::new(Position { x: 20_000, z: 0 }, 0),
    ];
    fighters[1].health = 40;
    fighters[3].health = 0;
    let mut combined = running(Arena::restore(layout.clone(), 0, fighters, None).unwrap());
    for seat in [0, 2] {
        combined.receive(
            seat,
            conn(seat),
            InputFrame {
                seq: 1,
                action: InputFrame::pulse(1),
                ..InputFrame::default()
            },
        );
    }
    let out = combined.tick();
    let board = snapshot(&out, conn(0))
        .unwrap()
        .scoreboard
        .as_ref()
        .unwrap();
    assert_eq!(board[0].killing_blows, 0);
    assert_eq!(board[2].killing_blows, 1);
    assert_eq!(board[0].damage_done, 25);
    assert_eq!(board[2].damage_done, 25);
    assert_eq!(board[1].damage_taken, 50);
    assert_eq!(board.iter().map(|s| s.killing_blows).sum::<u16>(), 1);

    // 2. Simultaneous overkill hits: enemy 1 has 20 HP; both seat 0 and seat 2 hit for 20.
    // Exactly one KB is credited (no double-counting a single death).
    let mut fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 2_000, z: 0 }, 0),
        Combatant::new(Position { x: 4_000, z: 0 }, 0),
        Combatant::new(Position { x: 20_000, z: 0 }, 0),
    ];
    fighters[1].health = 20;
    fighters[3].health = 0;
    let mut overkill = running(Arena::restore(layout.clone(), 0, fighters, None).unwrap());
    for seat in [0, 2] {
        overkill.receive(
            seat,
            conn(seat),
            InputFrame {
                seq: 1,
                action: InputFrame::pulse(1),
                ..InputFrame::default()
            },
        );
    }
    let out = overkill.tick();
    let board = snapshot(&out, conn(0))
        .unwrap()
        .scoreboard
        .as_ref()
        .unwrap();
    assert_eq!(board[0].killing_blows, 1);
    assert_eq!(board[2].killing_blows, 0);
    assert_eq!(board.iter().map(|s| s.killing_blows).sum::<u16>(), 1);

    // 3. 1v1 mutual elimination draw: both fighters at 25 HP strike each other on the same tick.
    let mut fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 2_000, z: 0 }, 0),
    ];
    fighters[0].health = 25;
    fighters[1].health = 25;
    let mut mutual = running(Arena::restore(layout.clone(), 0, fighters, None).unwrap());
    mutual.receive(
        0,
        conn(0),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            ..InputFrame::default()
        },
    );
    mutual.receive(
        1,
        conn(1),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(0),
            ..InputFrame::default()
        },
    );
    let out = mutual.tick();
    let snap = snapshot(&out, conn(0)).unwrap();
    assert_eq!(snap.result, Some(WireResult::Draw));
    let board = snap.scoreboard.as_ref().unwrap();
    assert_eq!((board[0].killing_blows, board[1].killing_blows), (1, 1));

    // 4. Time-limit draw with zero combat still delivers an all-zero scoreboard.
    let timed = Arena::practice(layout.clone(), 1)
        .unwrap()
        .with_round_limit(2)
        .unwrap();
    let mut session = running(timed);
    assert_eq!(snapshot(&session.tick(), conn(0)).unwrap().scoreboard, None);
    let out = session.tick();
    let snap = snapshot(&out, conn(0)).unwrap();
    assert_eq!(snap.result, Some(WireResult::TimeLimitDraw));
    assert_eq!(snap.scoreboard, Some(vec![FighterScore::default(); 2]));

    // 5. Teammate healing (`actor != target`) credits `healing_done` to the caster (seat 0)
    // and `healing_received` to the recipient (seat 2) without swapping or duplicating.
    let mut fighters = vec![
        Combatant::new(Position { x: 0, z: 0 }, 0),
        Combatant::new(Position { x: 2_000, z: 0 }, 0),
        Combatant::new(Position { x: 4_000, z: 0 }, 0),
        Combatant::new(Position { x: 20_000, z: 0 }, 0),
    ];
    fighters[1].health = 25;
    fighters[2].health = 60;
    fighters[3].health = 0;
    let mut team_heal = running(Arena::restore(layout, 0, fighters, None).unwrap());
    team_heal.receive(
        0,
        conn(0),
        InputFrame {
            seq: 1,
            action: WireAction::Cast {
                spell: arena_protocol::WireSpell::Mend,
                target: Some(2),
            },
            ..InputFrame::default()
        },
    );
    for _ in 0..=arena_sim::spells::MEND_CAST_TICKS {
        team_heal.tick();
    }
    assert_eq!(team_heal.arena().fighters()[2].health, 90);
    // Seat 2 finishes enemy 1 to end the match and inspect the final scoreboard.
    team_heal.receive(
        2,
        conn(2),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            ..InputFrame::default()
        },
    );
    let out = team_heal.tick();
    let board = snapshot(&out, conn(0))
        .unwrap()
        .scoreboard
        .as_ref()
        .unwrap();
    assert_eq!(
        (board[0].healing_done, board[0].healing_received),
        (30, 0),
        "healer gets healing_done only"
    );
    assert_eq!(
        (board[2].healing_done, board[2].healing_received),
        (0, 30),
        "recipient gets healing_received only"
    );
    assert_eq!(board[2].killing_blows, 1);
}

#[test]
fn tick_encoded_matches_tick_bytes_on_finished_scoreboard_with_and_without_rejections() {
    for with_rejection in [false, true] {
        let mut plain = running(one_hit_from_victory());
        let mut encoded = running(one_hit_from_victory());
        for s in [&mut plain, &mut encoded] {
            s.receive(
                0,
                conn(0),
                InputFrame {
                    seq: 1,
                    action: InputFrame::pulse(1),
                    ..InputFrame::default()
                },
            );
            if with_rejection {
                // Seat 1 attempts an invalid movement axis so seat 1 gets a private rejection
                // on the exact final tick while seat 0 does not.
                s.receive(
                    1,
                    conn(1),
                    InputFrame {
                        seq: 1,
                        movement: [2_000, 0],
                        ..InputFrame::default()
                    },
                );
            }
        }
        let plain_frames: Vec<(ConnId, Vec<u8>)> = plain
            .tick()
            .into_iter()
            .filter_map(|out| match out {
                Out::Send { conn, message } => Some((conn, message.encode())),
                Out::Close { .. } => None,
            })
            .collect();
        let mut fast_frames = Vec::new();
        encoded.tick_encoded(|out| {
            if let EncodedOut::Frame { conn, frame } = out {
                fast_frames.push((conn, frame));
            }
        });
        assert_eq!(fast_frames, plain_frames, "with_rejection={with_rejection}");
        for (_, frame) in &fast_frames {
            let Ok(ServerMessage::Snapshot(snap)) = ServerMessage::decode(frame) else {
                panic!("expected decodable final snapshot");
            };
            assert!(snap.result.is_some());
            assert_eq!(snap.scoreboard.as_ref().map(Vec::len), Some(2));
        }
    }
}

#[test]
fn session_locks_first_appearance_per_seat_and_relays_to_connected_players() {
    use arena_protocol::WireAppearance;

    let look = |body: &str, hair: &str| WireAppearance {
        ancestry: "human".into(),
        body: body.into(),
        skin_tone: "natural".into(),
        hair_color: hair.into(),
        eyes: "brown".into(),
        build: 20,
        groups: vec![("hair".into(), "Hair_Buns".into())],
        variants: vec![],
    };
    let mut session = session_with(arena(1));
    let look0 = look("female", "auburn");
    let look1 = look("male", "black");

    let out0 = session.attach_with_appearance(0, conn(0), Some(look0.clone()));
    assert!(out0.iter().any(|item| matches!(
        item,
        Out::Send {
            conn: c,
            message: ServerMessage::Appearances(looks),
        } if *c == conn(0) && looks == &vec![Some(look0.clone()), None]
    )));

    let out1 = session.attach_with_appearance(1, conn(1), Some(look1.clone()));
    for seat_conn in [conn(0), conn(1)] {
        assert!(out1.iter().any(|item| matches!(
            item,
            Out::Send {
                conn: c,
                message: ServerMessage::Appearances(looks),
            } if *c == seat_conn && looks == &vec![Some(look0.clone()), Some(look1.clone())]
        )));
    }

    // Reconnecting with a different appearance does not replace the locked look.
    let rejoin = session.attach_with_appearance(0, 999, Some(look("male", "violet")));
    assert_eq!(
        session.appearances(),
        vec![Some(look0.clone()), Some(look1.clone())]
    );
    assert!(rejoin.iter().any(|item| matches!(
        item,
        Out::Send {
            conn: 999,
            message: ServerMessage::Appearances(looks),
        } if looks == &vec![Some(look0.clone()), Some(look1.clone())]
    )));

    // Joining first with None also locks None for the match (cannot inject a look on reconnect).
    let mut none_first = session_with(arena(1));
    none_first.attach_with_appearance(0, conn(0), None);
    none_first.attach_with_appearance(0, 888, Some(look0));
    assert_eq!(none_first.appearances(), vec![None, None]);
}

#[test]
fn session_spectators_enforce_anti_ghosting_delay_cap_privacy_and_finish_drain() {
    let limits = Limits {
        spectator_delay: 5,
        max_spectators: 2,
        linger: 3,
        ..Limits::default()
    };
    let short = arena(1).with_round_limit(6).unwrap();
    let mut session = Session::new(short, 1, limits, Arc::default());

    // Two spectators join while Waiting: never start the match before players arrive!
    let w1 = session.attach_spectator(10, 501);
    assert!(w1.iter().any(|item| matches!(
        item,
        Out::Send {
            conn: 501,
            message: ServerMessage::Welcome(Welcome {
                spectator: true,
                actor: 0,
                ..
            }),
        }
    )));
    session.tick();
    assert_eq!(session.phase(), Phase::Waiting);

    // Second spectator fits cap; third is refused with MatchFull; reconnecting with
    // the same player id replaces the old connection instead of hitting the cap.
    assert!(
        session
            .attach_spectator(20, 502)
            .iter()
            .any(|item| matches!(
                item,
                Out::Send {
                    conn: 502,
                    message: ServerMessage::Welcome(_)
                }
            ))
    );
    assert_eq!(
        session.attach_spectator(30, 503),
        vec![Out::Close {
            conn: 503,
            error: Some(ErrorCode::MatchFull),
        }]
    );
    let replaced = session.attach_spectator(20, 504);
    assert!(replaced.contains(&Out::Close {
        conn: 502,
        error: Some(ErrorCode::Replaced),
    }));
    assert_eq!(session.spectator_count(), 2);

    // Players join and transition Waiting -> Running.
    session.attach(0, conn(0));
    session.attach(1, conn(1));
    session.tick();
    assert_eq!(session.phase(), Phase::Running);

    // Tick 1..5: players get live ticks 1..5; spectators get NOTHING yet (5-tick delay)!
    // Also trigger a private rejection on seat 0 on tick 1.
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 1,
            action: InputFrame::pulse(1),
            ..InputFrame::default()
        },
    );
    for expected_live in 1..=5u64 {
        let out = session.tick();
        assert_eq!(snapshot(&out, conn(0)).unwrap().tick, expected_live);
        assert!(
            snapshot(&out, 501).is_none(),
            "spectator must not receive live tick {expected_live} before delay expires"
        );
    }

    // Reconnecting mid-match must NEVER leak live ticks 1..5 (at most tick 0).
    let mid_reconnect = session.attach_spectator(10, 505);
    assert!(
        snapshot(&mid_reconnect, 505).is_none_or(|snap| snap.tick == 0),
        "mid-match spectator reconnect must not leak un-delayed live snapshot"
    );

    // Tick 6: round hits 6-tick time limit (Finished) for players; spectator receives tick 1
    // (6 - 5 = 1), with ZERO private Rejected events!
    let out = session.tick();
    assert!(matches!(session.phase(), Phase::Finished { .. }));
    let live_final = snapshot(&out, conn(0)).unwrap();
    assert_eq!(live_final.tick, 6);
    assert!(live_final.result.is_some());
    let spec_first = snapshot(&out, 505).unwrap();
    assert_eq!(spec_first.tick, 1);
    assert!(
        !spec_first
            .events
            .iter()
            .any(|e| matches!(e.event, WireEvent::Rejected(_))),
        "private Rejected event must never reach spectators"
    );

    // Subsequent finish ticks: players close after linger (3 ticks), while spectators
    // keep draining buffered delayed ticks 2..=6 (including final result & scoreboard)
    // before closing with MatchOver!
    let mut seats_closed = false;
    let mut spec_saw_final = false;
    let mut spec_closed = false;
    for _ in 0..12 {
        let out = session.tick();
        if out.iter().any(|item| {
            matches!(
                item,
                Out::Close {
                    conn: c,
                    error: Some(ErrorCode::MatchOver)
                } if *c == conn(0)
            )
        }) {
            seats_closed = true;
        }
        if let Some(snap) = snapshot(&out, 505)
            && snap.result.is_some()
        {
            assert_eq!(snap.tick, 6);
            assert!(snap.scoreboard.is_some());
            spec_saw_final = true;
        }
        if out.iter().any(|item| {
            matches!(
                item,
                Out::Close {
                    conn: 505,
                    error: Some(ErrorCode::MatchOver)
                }
            )
        }) {
            spec_closed = true;
            break;
        }
    }
    assert!(seats_closed, "seated players closed after normal linger");
    assert!(
        spec_saw_final,
        "spectators received delayed final result and scoreboard"
    );
    assert!(
        spec_closed,
        "spectators closed cleanly with MatchOver after drain"
    );
    assert!(session.is_ended());
}

#[test]
fn session_spectator_tick_encoded_drains_when_empty_refuses_stale_finished_and_handles_zero_linger()
{
    // 1. `tick_encoded` continues draining `spectator_queue` when all seats and spectators
    // temporarily disconnect, preventing burst overflows when a spectator reconnects.
    let limits = Limits {
        spectator_delay: 3,
        max_spectators: 2,
        linger: 3,
        grace: 30,
        ..Limits::default()
    };
    let mut session = Session::new(arena(1), 1, limits, Arc::default());
    session.attach(0, conn(0));
    session.attach(1, conn(1));
    session.attach_spectator(10, 501);
    for _ in 0..4 {
        session.tick_encoded(|_| {});
    }
    session.detach(0, conn(0));
    session.detach(1, conn(1));
    session.detach_spectator(501);
    for _ in 0..5 {
        session.tick_encoded(|_| {});
    }
    let _ = session.attach_spectator(10, 502);
    let mut frames_on_next_tick = 0usize;
    session.tick_encoded(|item| {
        if matches!(item, EncodedOut::SharedFrame { conn: 502, .. }) {
            frames_on_next_tick += 1;
        }
    });
    assert!(
        frames_on_next_tick <= 1,
        "stale delayed entries must expire while empty, got {frames_on_next_tick}"
    );

    // 2. Stale `last_spectator_snapshot` (result: None) must NOT allow attaching during
    // `Phase::Finished` when no final result/scoreboard is queued.
    let short_limits = Limits {
        spectator_delay: 2,
        max_spectators: 2,
        linger: 4,
        ..Limits::default()
    };
    let short = arena(1).with_round_limit(6).unwrap();
    let mut stale_session = Session::new(short, 1, short_limits, Arc::default());
    stale_session.attach(0, conn(0));
    stale_session.attach(1, conn(1));
    stale_session.attach_spectator(10, 501);
    for _ in 0..4 {
        stale_session.tick();
    }
    stale_session.detach_spectator(501);
    for _ in 0..5 {
        stale_session.tick();
    }
    let late = stale_session.attach_spectator(11, 503);
    assert!(
        matches!(
            late.as_slice(),
            [Out::Close {
                conn: 503,
                error: Some(ErrorCode::MatchOver)
            }]
        ),
        "stale non-finished snapshot must not admit new spectators after finish: {late:?}"
    );

    // 3. `linger == 0` with `spectator_delay > 0` still delivers the final result & scoreboard
    // before closing spectators with `MatchOver`.
    let zero_linger = Limits {
        spectator_delay: 3,
        max_spectators: 2,
        linger: 0,
        ..Limits::default()
    };
    let short_zero = arena(1).with_round_limit(3).unwrap();
    let mut zsession = Session::new(short_zero, 1, zero_linger, Arc::default());
    zsession.attach(0, conn(0));
    zsession.attach(1, conn(1));
    zsession.attach_spectator(10, 501);
    let mut saw_final_board = false;
    let mut saw_close = false;
    for _ in 0..10 {
        let out = zsession.tick();
        if let Some(snap) = snapshot(&out, 501)
            && snap.result == Some(WireResult::TimeLimitDraw)
            && snap.scoreboard.is_some()
        {
            saw_final_board = true;
        }
        if out.iter().any(|item| {
            matches!(
                item,
                Out::Close {
                    conn: 501,
                    error: Some(ErrorCode::MatchOver)
                }
            )
        }) {
            saw_close = true;
            break;
        }
    }
    assert!(
        saw_final_board,
        "final scoreboard delivered even with linger == 0"
    );
    assert!(
        saw_close,
        "spectator closed with MatchOver after final delivery"
    );

    // 4. Default spectator delay is 0 (live/real-time): snapshots deliver on the same tick
    // and mid-match joins immediately receive the current live tick.
    assert_eq!(Limits::default().spectator_delay, 0);
    let mut live_session = Session::new(arena(1), 1, Limits::default(), Arc::default());
    live_session.attach(0, conn(0));
    live_session.attach(1, conn(1));
    live_session.tick(); // Waiting -> Running (tick 0)
    live_session.tick(); // tick 1
    let mid = live_session.attach_spectator(42, 601);
    assert!(mid.iter().any(|item| matches!(
        item,
        Out::Send {
            conn: 601,
            message: ServerMessage::Welcome(Welcome {
                spectator: true,
                tick: 1,
                ..
            })
        }
    )));
    assert_eq!(snapshot(&mid, 601).unwrap().tick, 1);
    let next = live_session.tick();
    assert_eq!(snapshot(&next, 601).unwrap().tick, 2);
}
