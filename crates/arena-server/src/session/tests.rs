use super::*;
use arena_protocol::{FighterState, WireRejection};
use arena_sim::arena::{ATTACK_COOLDOWN, ArenaCommand, Axes};

const LIMITS: Limits = Limits {
    queue: 3,
    start_timeout: 10,
    grace: 5,
    linger: 3,
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
            welcome.map
        ),
        (0, 1, 0, 30, 42)
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
fn inputs_drive_only_their_own_seat_and_merge_once_per_tick() {
    let mut session = running(arena(1));
    let mut replay = session.arena().clone();
    session.receive(0, conn(0), frame(1));
    session.receive(
        0,
        conn(0),
        InputFrame {
            seq: 2,
            movement: [1000, 0],
            facing: 77,
            jump: true,
            attack: None,
        },
    );
    let out = session.tick();
    // Identical to the rules advanced directly with the merged slot.
    replay.advance(&[ActorInput {
        movement: Some(Axes { x: 1000, z: 0 }),
        facing: Some(77),
        jump: true,
        attack: None,
    }]);
    assert_eq!(session.arena(), &replay);
    let own = snapshot(&out, conn(0)).unwrap();
    assert_eq!((own.ack, own.hash, own.tick), (2, replay.state_hash(), 1));
    assert_eq!(
        own.fighters[0],
        FighterState::new(&replay.fighters()[0], true)
    );
    assert!(own.fighters[0].height > 0);
    assert_eq!(own.fighters[0].facing, 77);
    let other = snapshot(&out, conn(1)).unwrap();
    assert_eq!(other.ack, 0);
    assert_eq!(other.fighters, own.fighters, "everyone sees the same state");
    assert_eq!(
        session.arena().fighters()[1],
        arena(1).fighters()[1],
        "the other seat did not move"
    );
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
    assert_eq!(Metrics::get(&session.metrics.inputs_overflow), 2);
    let out = session.tick();
    assert_eq!(snapshot(&out, conn(0)).unwrap().ack, 9);
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
            attack: Some(0),
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
            attack: Some(1),
            ..InputFrame::default()
        },
    );
    let out = session.tick();
    for seat in 0..2 {
        let final_state = snapshot(&out, conn(seat)).unwrap();
        assert_eq!(
            final_state.result,
            Some(arena_protocol::WireResult::Won { team: 0 })
        );
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
