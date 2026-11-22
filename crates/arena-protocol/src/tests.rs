use super::*;
use arena_sim::arena::ArenaCommand;

fn colosseum(team_size: usize) -> Arena {
    Arena::practice(arena_map::colosseum().layout, team_size).unwrap()
}

fn input(seq: u32) -> InputFrame {
    InputFrame {
        seq,
        movement: [-1000, 707],
        facing: 0xC000,
        jump: true,
        action: InputFrame::pulse(3),
        view_tick: 0,
    }
}

fn client_samples() -> Vec<ClientMessage> {
    vec![
        ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            ticket: vec![7; MAX_TICKET],
        },
        ClientMessage::Hello {
            protocol: 0,
            ticket: Vec::new(),
        },
        ClientMessage::Input(input(u32::MAX)),
        ClientMessage::Input(InputFrame {
            seq: 42,
            action: WireAction::CancelCast,
            view_tick: u64::MAX,
            ..InputFrame::default()
        }),
        ClientMessage::Input(InputFrame::default()),
        ClientMessage::Ping { nonce: 0xDEAD_BEEF },
    ]
}

/// A full 3v3 snapshot with every event kind, at the event limit.
fn busy_snapshot() -> Snapshot {
    let mut arena = colosseum(3);
    arena.step(&[ArenaCommand::Jump { actor: 4 }]);
    let mut events = Vec::new();
    for index in 0..MAX_EVENTS {
        let event = match index % 8 {
            0 => WireEvent::Jumped { actor: 5 },
            1 => WireEvent::Hit {
                actor: 1,
                target: 4,
                damage: u16::MAX,
            },
            2 => WireEvent::Rejected(WireRejection::LineOfSight),
            3 => WireEvent::Finished(WireResult::Won { team: 1 }),
            4 => WireEvent::CastStarted {
                actor: 0,
                spell: WireSpell::RiftBolt,
                target: Some(1),
                duration_ticks: 45,
            },
            5 => WireEvent::CastCompleted {
                actor: 0,
                spell: WireSpell::RiftBolt,
                target: Some(1),
            },
            6 => WireEvent::CastInterrupted {
                actor: 2,
                spell: WireSpell::Mend,
                reason: WireInterruptReason::Interrupted,
                by: Some(3),
            },
            _ => WireEvent::Healed {
                actor: 2,
                target: 0,
                amount: 30,
            },
        };
        events.push(TickEvent {
            tick: u64::MAX - index as u64,
            event,
        });
    }
    let mut snapshot = Snapshot::capture(&arena, u32::MAX, events, &[true; MAX_FIGHTERS]);
    snapshot.result = Some(WireResult::Won { team: 1 });
    snapshot.scoreboard = Some(vec![
        FighterScore {
            damage_done: u32::MAX,
            damage_taken: u32::MAX,
            healing_done: u32::MAX,
            healing_received: u32::MAX,
            killing_blows: u16::MAX,
        };
        MAX_FIGHTERS
    ]);
    for fighter in &mut snapshot.fighters {
        // Worst-case varints everywhere while respecting valid invariants.
        fighter.x = i32::MIN;
        fighter.z = i32::MAX;
        fighter.gcd_remaining = u32::MAX;
        fighter.lockout_remaining = u32::MAX;
        fighter.cooldowns = [u32::MAX; SpellId::COUNT];
        fighter.active_cast = Some(WireCast {
            spell: WireSpell::RiftBolt,
            target: Some(5),
            remaining_ticks: 45,
            total_ticks: 45,
        });
    }
    snapshot
}

fn server_samples() -> Vec<ServerMessage> {
    vec![
        ServerMessage::Welcome(Welcome {
            protocol: PROTOCOL_VERSION,
            actor: 5,
            team_size: 3,
            tick: 1_234_567,
            tick_rate: 30,
            round_ticks: MAX_ROUND_TICKS,
            rules: RULES_VERSION,
            map: u64::MAX,
        }),
        ServerMessage::Snapshot(Snapshot::capture(
            &colosseum(1),
            0,
            Vec::new(),
            &[true, false],
        )),
        ServerMessage::Snapshot(busy_snapshot()),
        ServerMessage::Pong { nonce: 1 },
        ServerMessage::Error(ErrorCode::VersionMismatch),
        ServerMessage::Error(ErrorCode::ServerShutdown),
    ]
}

#[test]
fn every_message_round_trips_within_its_frame_limit() {
    for message in client_samples() {
        let frame = message.encode();
        assert!(frame.len() <= MAX_CLIENT_FRAME, "{message:?}");
        assert_eq!(ClientMessage::decode(&frame), Ok(message));
    }
    for message in server_samples() {
        let frame = message.encode();
        assert!(frame.len() <= MAX_SERVER_FRAME, "{}", frame.len());
        assert_eq!(ServerMessage::decode(&frame), Ok(message));
    }
}

#[test]
fn ordinary_traffic_is_small() {
    // Budgets, not measurements of a network: at 30 Hz a 3v3 snapshot
    // without events costs each player under 6 KB/s, and input under 0.6 KB/s.
    assert!(ClientMessage::Input(input(1_000_000)).encode().len() <= 20);
    let quiet = ServerMessage::Snapshot(Snapshot::capture(
        &colosseum(3),
        70_000,
        Vec::new(),
        &[true; MAX_FIGHTERS],
    ));
    assert!(quiet.encode().len() <= 200, "{}", quiet.encode().len());
}

#[test]
fn the_wire_layout_is_pinned() {
    // Golden frames: changing them requires a PROTOCOL_VERSION bump.
    assert_eq!(PROTOCOL_VERSION, 5);
    assert_eq!(
        ClientMessage::Hello {
            protocol: 5,
            ticket: vec![0xAB, 0xCD],
        }
        .encode(),
        [0, 5, 2, 0xAB, 0xCD]
    );
    assert_eq!(
        ClientMessage::Input(InputFrame {
            view_tick: 120,
            ..input(300)
        })
        .encode(),
        // variant 1, seq 300 (varint), x -1000 / z 707 (zigzag varints),
        // facing 0xC000, jump, WireAction::Cast { Pulse (0), Some(3) }, view_tick 120.
        [
            1, 0xAC, 0x02, 0xCF, 0x0F, 0x86, 0x0B, 0x80, 0x80, 0x03, 1, 1, 0, 1, 3, 120
        ]
    );
    // Scoreboard golden suffix: `scoreboard: None, events: []` ends in `[0, 0]`;
    // `scoreboard: Some([s0, s1]), events: []` pins exact `FighterScore` field order.
    let mut base_finished = Snapshot::capture(&colosseum(1), 0, Vec::new(), &[true, true]);
    base_finished.result = Some(WireResult::Won { team: 0 });
    let without_board = ServerMessage::Snapshot(base_finished.clone()).encode();
    assert_eq!(&without_board[without_board.len() - 2..], &[0, 0]);
    let with_board = ServerMessage::Snapshot(base_finished.with_scoreboard(vec![
        FighterScore {
            damage_done: 125,
            damage_taken: 40,
            healing_done: 30,
            healing_received: 0,
            killing_blows: 1,
        },
        FighterScore {
            damage_done: 40,
            damage_taken: 125,
            healing_done: 0,
            healing_received: 0,
            killing_blows: 0,
        },
    ]))
    .encode();
    let prefix_len = without_board.len() - 2;
    assert_eq!(&with_board[..prefix_len], &without_board[..prefix_len]);
    assert_eq!(
        &with_board[prefix_len..],
        &[1, 2, 125, 40, 30, 0, 1, 40, 125, 0, 0, 0, 0]
    );
    // The browser runtime reads these two-byte error frames itself (so a
    // tab whose engine is paused still stops on a final error); its tests
    // pin the same table in web/src/domain/network.ts.
    let codes = [
        ErrorCode::VersionMismatch,
        ErrorCode::BadTicket,
        ErrorCode::MatchFull,
        ErrorCode::MatchOver,
        ErrorCode::RateLimited,
        ErrorCode::Malformed,
        ErrorCode::HelloTimeout,
        ErrorCode::ServerShutdown,
        ErrorCode::Replaced,
    ];
    for (index, code) in codes.into_iter().enumerate() {
        assert_eq!(ServerMessage::Error(code).encode(), [3, index as u8]);
    }
    // Welcome is variant 0, which the runtime keeps when trimming a backlog.
    assert_eq!(server_samples()[0].encode()[0], 0);
    assert_eq!(server_samples()[1].encode()[0], 1);
}

#[test]
fn malformed_frames_are_rejected_not_trusted() {
    let hello = client_samples()[0].encode();
    assert_eq!(ClientMessage::decode(&[]), Err(DecodeError::Empty));
    assert_eq!(
        ClientMessage::decode(&vec![2; MAX_CLIENT_FRAME + 1]),
        Err(DecodeError::Oversized)
    );
    // Truncated anywhere.
    for cut in 1..hello.len() {
        assert_eq!(
            ClientMessage::decode(&hello[..cut]),
            Err(DecodeError::Malformed),
            "{cut}"
        );
    }
    // Trailing bytes.
    let mut padded = ClientMessage::Ping { nonce: 1 }.encode();
    padded.push(0);
    assert_eq!(ClientMessage::decode(&padded), Err(DecodeError::Malformed));
    // Unknown variants, invalid bool and enum/option tags.
    assert_eq!(ClientMessage::decode(&[9, 0]), Err(DecodeError::Malformed));
    let mut bad_bool = ClientMessage::Input(input(1)).encode();
    let jump = bad_bool.len() - 5;
    bad_bool[jump] = 2;
    assert_eq!(
        ClientMessage::decode(&bad_bool),
        Err(DecodeError::Malformed)
    );
    let mut bad_action = ClientMessage::Input(input(1)).encode();
    let action_tag = bad_action.len() - 4;
    bad_action[action_tag] = 7;
    assert_eq!(
        ClientMessage::decode(&bad_action),
        Err(DecodeError::Malformed)
    );
    let mut bad_spell = ClientMessage::Input(input(1)).encode();
    let spell_tag = bad_spell.len() - 3;
    bad_spell[spell_tag] = 99;
    assert_eq!(
        ClientMessage::decode(&bad_spell),
        Err(DecodeError::Malformed)
    );
    // A length prefix promising more than the frame holds.
    assert_eq!(
        ClientMessage::decode(&[0, 1, 0xFF, 0xFF, 0x03, 1, 2]),
        Err(DecodeError::Malformed)
    );
    // Well formed but over a field limit.
    let long = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket: vec![0; MAX_TICKET + 1],
    };
    assert_eq!(
        ClientMessage::decode(&long.encode()),
        Err(DecodeError::OverLimit)
    );
    let mut crowded = Snapshot::capture(&colosseum(3), 0, Vec::new(), &[]);
    crowded.fighters.push(crowded.fighters[0]);
    assert_eq!(
        ServerMessage::decode(&ServerMessage::Snapshot(crowded).encode()),
        Err(DecodeError::OverLimit)
    );
    let mut noisy = busy_snapshot();
    noisy.events.push(noisy.events[0]);
    assert_eq!(
        ServerMessage::decode(&ServerMessage::Snapshot(noisy).encode()),
        Err(DecodeError::OverLimit)
    );
    for invalid_round_ticks in [0, MAX_ROUND_TICKS + 1] {
        let bad_welcome = ServerMessage::Welcome(Welcome {
            protocol: PROTOCOL_VERSION,
            actor: 0,
            team_size: 1,
            tick: 0,
            tick_rate: 30,
            round_ticks: invalid_round_ticks,
            rules: RULES_VERSION,
            map: 1,
        });
        assert_eq!(
            ServerMessage::decode(&bad_welcome.encode()),
            Err(DecodeError::OverLimit)
        );
    }

    // Corrupted combat/cast invariants in snapshots are rejected at decode time.
    let base_snap = Snapshot::capture(&colosseum(1), 0, Vec::new(), &[true, true]);
    let bad_snapshots = vec![
        {
            let mut s = base_snap.clone();
            s.fighters[0].health = arena_sim::arena::MAX_HEALTH + 1;
            s
        },
        {
            let mut s = base_snap.clone();
            s.fighters[0].resource = arena_sim::spells::MAX_RESOURCE + 1;
            s
        },
        {
            let mut s = base_snap.clone();
            s.fighters[0].active_cast = Some(WireCast {
                spell: WireSpell::Pulse,
                target: Some(1),
                remaining_ticks: 1,
                total_ticks: 1,
            });
            s
        },
        {
            let mut s = base_snap.clone();
            s.fighters[0].active_cast = Some(WireCast {
                spell: WireSpell::RiftBolt,
                target: Some(1),
                remaining_ticks: 0,
                total_ticks: 45,
            });
            s
        },
        {
            let mut s = base_snap.clone();
            s.fighters[0].active_cast = Some(WireCast {
                spell: WireSpell::RiftBolt,
                target: Some(1),
                remaining_ticks: 46,
                total_ticks: 45,
            });
            s
        },
        {
            let mut s = base_snap.clone();
            s.fighters[0].active_cast = Some(WireCast {
                spell: WireSpell::RiftBolt,
                target: Some(2),
                remaining_ticks: 10,
                total_ticks: 45,
            });
            s
        },
        {
            let mut s = base_snap.clone();
            s.fighters[0].health = 0;
            s.fighters[0].active_cast = Some(WireCast {
                spell: WireSpell::RiftBolt,
                target: Some(1),
                remaining_ticks: 10,
                total_ticks: 45,
            });
            s
        },
        {
            let mut s = base_snap.clone();
            s.events.push(TickEvent {
                tick: 1,
                event: WireEvent::CastStarted {
                    actor: 0,
                    spell: WireSpell::RiftBolt,
                    target: Some(1),
                    duration_ticks: 0,
                },
            });
            s
        },
        {
            let mut s = base_snap.clone();
            s.events.push(TickEvent {
                tick: 1,
                event: WireEvent::CastStarted {
                    actor: 0,
                    spell: WireSpell::Pulse,
                    target: Some(1),
                    duration_ticks: 10,
                },
            });
            s
        },
    ];
    for bad in bad_snapshots {
        assert_eq!(
            ServerMessage::decode(&ServerMessage::Snapshot(bad).encode()),
            Err(DecodeError::OverLimit)
        );
    }
}

#[test]
fn arbitrary_bytes_never_panic_and_decoded_messages_respect_limits() {
    // Deterministic xorshift noise, plus mutations of valid frames.
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let seeds: Vec<Vec<u8>> = client_samples().iter().map(ClientMessage::encode).collect();
    for round in 0..20_000 {
        let frame: Vec<u8> = if round % 2 == 0 {
            let length = (next() % 48) as usize;
            (0..length).map(|_| next() as u8).collect()
        } else {
            let mut frame = seeds[round / 2 % seeds.len()].clone();
            let at = next() as usize % frame.len();
            frame[at] = next() as u8;
            frame
        };
        if let Ok(message) = ClientMessage::decode(&frame) {
            if let ClientMessage::Hello { ticket, .. } = &message {
                assert!(ticket.len() <= MAX_TICKET);
            }
            // Anything accepted re-decodes to itself.
            assert_eq!(ClientMessage::decode(&message.encode()), Ok(message));
        }
        let _ = ServerMessage::decode(&frame);
        let _ = peek_hello_protocol(&frame);
    }
}

#[test]
fn a_hello_reveals_its_version_even_from_a_future_client() {
    let current = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket: vec![1, 2, 3],
    };
    assert_eq!(
        peek_hello_protocol(&current.encode()),
        Some(PROTOCOL_VERSION)
    );
    // A future Hello with fields this build does not know.
    let future = [0, 0xE7, 0x07, 0x42, 0x42, 0x42];
    assert_eq!(peek_hello_protocol(&future), Some(999));
    assert_eq!(ClientMessage::decode(&future), Err(DecodeError::Malformed));
    assert_eq!(
        peek_hello_protocol(&ClientMessage::Ping { nonce: 0 }.encode()),
        None
    );
    assert_eq!(peek_hello_protocol(&[]), None);
    assert_eq!(peek_hello_protocol(&[0]), None);
}

#[test]
fn merged_inputs_keep_one_command_of_each_kind_and_never_lose_a_press() {
    let mut slot = ActorInput::default();
    input(1).merge_into(&mut slot);
    InputFrame {
        seq: 2,
        movement: [5, 6],
        facing: 9,
        jump: false,
        action: WireAction::None,
        view_tick: 0,
    }
    .merge_into(&mut slot);
    assert_eq!(
        slot,
        ActorInput {
            movement: Some(Axes { x: 5, z: 6 }),
            facing: Some(9),
            jump: true,
            action: ActorInput::pulse(3),
            rewind_ticks: 0,
        }
    );
    InputFrame {
        action: WireAction::Cast {
            spell: WireSpell::RiftBolt,
            target: Some(1),
        },
        ..InputFrame::default()
    }
    .merge_into(&mut slot);
    assert_eq!(
        slot.action,
        Some(CastAction::Cast {
            spell: SpellId::RiftBolt,
            target: Some(1),
        }),
        "the latest combat action wins"
    );
    InputFrame {
        action: WireAction::CancelCast,
        ..InputFrame::default()
    }
    .merge_into(&mut slot);
    assert_eq!(slot.action, Some(CastAction::Cancel));
    // A client-chosen target is only intent: the rules still judge it.
    let mut arena = colosseum(1);
    let mut slot = ActorInput::default();
    InputFrame {
        action: InputFrame::pulse(u8::MAX),
        ..InputFrame::default()
    }
    .merge_into(&mut slot);
    assert_eq!(
        arena.advance(&[slot]),
        vec![
            ArenaEvent::Moved { actor: 0 },
            ArenaEvent::Rejected {
                actor: 0,
                reason: Rejection::InvalidTarget
            }
        ]
    );
}

#[test]
fn recipients_see_only_their_own_rejections_and_no_movement_noise() {
    let mut arena = colosseum(1);
    let events = arena.step(&[
        ArenaCommand::Move {
            actor: 0,
            x: 0,
            z: -1000,
        },
        ArenaCommand::Jump { actor: 1 },
        ArenaCommand::Attack {
            actor: 0,
            target: 1,
        },
        ArenaCommand::Attack {
            actor: 1,
            target: 1,
        },
    ]);
    let seen = |recipient| -> Vec<WireEvent> {
        events
            .iter()
            .filter_map(|event| WireEvent::for_recipient(event, recipient))
            .collect()
    };
    assert_eq!(
        seen(0),
        vec![
            WireEvent::Jumped { actor: 1 },
            WireEvent::Rejected(WireRejection::OutOfRange)
        ]
    );
    assert_eq!(
        seen(1),
        vec![
            WireEvent::Jumped { actor: 1 },
            WireEvent::Rejected(WireRejection::InvalidTarget)
        ]
    );
    // Cast/interrupt/heal events translate cleanly and drop out-of-range actor indices.
    assert_eq!(
        WireEvent::for_recipient(
            &ArenaEvent::CastStarted {
                actor: 0,
                spell: SpellId::RiftBolt,
                target: Some(1),
                duration_ticks: 45,
            },
            1
        ),
        Some(WireEvent::CastStarted {
            actor: 0,
            spell: WireSpell::RiftBolt,
            target: Some(1),
            duration_ticks: 45,
        })
    );
    assert_eq!(
        WireEvent::for_recipient(
            &ArenaEvent::CastInterrupted {
                actor: 0,
                spell: SpellId::RiftBolt,
                reason: InterruptReason::Interrupted,
                by: Some(1),
            },
            0
        ),
        Some(WireEvent::CastInterrupted {
            actor: 0,
            spell: WireSpell::RiftBolt,
            reason: WireInterruptReason::Interrupted,
            by: Some(1),
        })
    );
    assert_eq!(
        WireEvent::for_recipient(
            &ArenaEvent::Healed {
                actor: 0,
                target: 2,
                amount: 30,
            },
            1
        ),
        Some(WireEvent::Healed {
            actor: 0,
            target: 2,
            amount: 30,
        })
    );
    // Scripted commands naming impossible actors never reach anyone.
    assert_eq!(
        WireEvent::for_recipient(&ArenaEvent::Jumped { actor: 300 }, 0),
        None
    );
    assert_eq!(
        WireEvent::for_recipient(
            &ArenaEvent::Rejected {
                actor: usize::MAX,
                reason: Rejection::InvalidActor
            },
            0
        ),
        None
    );
    assert_eq!(
        WireEvent::for_recipient(&ArenaEvent::Finished(RoundResult::Draw), 3),
        Some(WireEvent::Finished(WireResult::Draw))
    );
    assert_eq!(
        WireEvent::for_recipient(&ArenaEvent::Finished(RoundResult::TimeLimitDraw), 3),
        Some(WireEvent::Finished(WireResult::TimeLimitDraw))
    );
}

#[test]
fn snapshots_carry_the_authoritative_state_and_its_hash() {
    let mut arena = colosseum(2);
    arena.step(&[
        ArenaCommand::Face {
            actor: 2,
            facing: 77,
        },
        ArenaCommand::Jump { actor: 3 },
        ArenaCommand::Cast {
            actor: 0,
            spell: SpellId::Mend,
            target: Some(0),
        },
    ]);
    let snapshot = Snapshot::capture(&arena, 41, Vec::new(), &[true, false, true]);
    assert_eq!(snapshot.tick, 1);
    assert_eq!(snapshot.ack, 41);
    assert_eq!(snapshot.hash, arena.state_hash());
    assert_eq!(snapshot.result, None);
    assert_eq!(snapshot.fighters.len(), 4);
    for (index, (wire, fighter)) in snapshot.fighters.iter().zip(arena.fighters()).enumerate() {
        // Actor 3 has no entry: missing means disconnected.
        let connected = index != 1 && index != 3;
        assert_eq!(*wire, FighterState::new(fighter, connected));
        assert_eq!(wire.connected, connected);
        assert_eq!(wire.health, fighter.health);
        assert_eq!(wire.resource, fighter.resource);
        assert_eq!(wire.gcd_remaining, fighter.gcd_remaining);
        assert_eq!((wire.x, wire.z), (fighter.position.x, fighter.position.z));
    }
    assert_eq!(snapshot.fighters[2].facing, 77);
    assert!(snapshot.fighters[0].active_cast.is_some());
    // Wire state converts back to the exact rule state.
    let fighters: Vec<_> = snapshot
        .fighters
        .iter()
        .map(FighterState::combatant)
        .collect();
    assert_eq!(fighters, arena.fighters());
    for result in [
        RoundResult::Draw,
        RoundResult::TimeLimitDraw,
        RoundResult::Won { team: 1 },
    ] {
        assert_eq!(RoundResult::from(WireResult::from(result)), result);
    }
    for reason in [
        Rejection::Cooldown,
        Rejection::GlobalCooldown,
        Rejection::AlreadyCasting,
        Rejection::NotEnoughResource,
        Rejection::Moving,
        Rejection::Silenced,
        Rejection::NotCasting,
        Rejection::LineOfSight,
        Rejection::Duplicate,
    ] {
        assert_eq!(Rejection::from(WireRejection::from(reason)), reason);
    }
    for interrupt in [
        InterruptReason::Cancelled,
        InterruptReason::Moved,
        InterruptReason::Interrupted,
        InterruptReason::Silenced,
        InterruptReason::OutOfRange,
        InterruptReason::LineOfSight,
        InterruptReason::TargetDead,
        InterruptReason::CasterDead,
    ] {
        assert_eq!(
            InterruptReason::from(WireInterruptReason::from(interrupt)),
            interrupt
        );
    }
    assert!(snapshot.fighters[3].height > 0);
}

#[test]
fn the_debug_dump_is_readable_json() {
    let json = ClientMessage::Input(input(5)).to_debug_json();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["Input"]["seq"], 5);
    assert_eq!(value["Input"]["action"]["Cast"]["spell"], "Pulse");
    assert_eq!(value["Input"]["action"]["Cast"]["target"], 3);
    let json = ServerMessage::Error(ErrorCode::BadTicket).to_debug_json();
    assert_eq!(json, r#"{"Error":"BadTicket"}"#);
    let snapshot = ServerMessage::Snapshot(busy_snapshot()).to_debug_json();
    let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(value["Snapshot"]["fighters"].as_array().unwrap().len(), 6);
    assert_eq!(value["Snapshot"]["scoreboard"].as_array().unwrap().len(), 6);
}

#[test]
fn encoded_snapshot_body_matches_full_server_message_bytes() {
    let mut snapshots = vec![
        Snapshot::capture(&colosseum(1), 0, Vec::new(), &[true, false]),
        busy_snapshot(),
    ];
    let mut with_queued = Snapshot::capture(
        &colosseum(3),
        123_456,
        vec![TickEvent {
            tick: 7,
            event: WireEvent::Jumped { actor: 2 },
        }],
        &[true; MAX_FIGHTERS],
    );
    with_queued.queued = 9;
    with_queued.result = Some(WireResult::Draw);
    with_queued.scoreboard = Some(
        (0..MAX_FIGHTERS as u32)
            .map(|i| FighterScore {
                damage_done: i * 25,
                damage_taken: (5 - i) * 20,
                healing_done: i * 10,
                healing_received: i * 5,
                killing_blows: u16::from(i == 0),
            })
            .collect(),
    );
    snapshots.push(with_queued);

    for snapshot in snapshots {
        let expected = ServerMessage::Snapshot(snapshot.clone()).encode();
        let with_shared = EncodedSnapshotBody::new(
            snapshot.tick,
            snapshot.hash,
            snapshot.result,
            &snapshot.fighters,
            snapshot.scoreboard.as_deref(),
            Some(&snapshot.events),
        );
        assert_eq!(
            with_shared.encode_for_seat(snapshot.ack, snapshot.queued, &[]),
            expected
        );
        let with_per_seat = EncodedSnapshotBody::new(
            snapshot.tick,
            snapshot.hash,
            snapshot.result,
            &snapshot.fighters,
            snapshot.scoreboard.as_deref(),
            None,
        );
        assert_eq!(
            with_per_seat.encode_for_seat(snapshot.ack, snapshot.queued, &snapshot.events),
            expected
        );
    }
}

#[test]
fn welcome_and_snapshot_bounds_reject_invalid_teams_actors_and_results() {
    let base_welcome = Welcome {
        protocol: PROTOCOL_VERSION,
        actor: 0,
        team_size: 1,
        tick: 0,
        tick_rate: 30,
        round_ticks: MAX_ROUND_TICKS,
        rules: RULES_VERSION,
        map: 1,
    };
    let bad_welcomes = [
        Welcome {
            team_size: 0,
            ..base_welcome
        },
        Welcome {
            team_size: (MAX_FIGHTERS / 2) as u8 + 1,
            ..base_welcome
        },
        Welcome {
            actor: 2,
            team_size: 1,
            ..base_welcome
        },
        Welcome {
            actor: 6,
            team_size: 3,
            ..base_welcome
        },
        Welcome {
            tick_rate: 0,
            ..base_welcome
        },
        Welcome {
            round_ticks: 0,
            ..base_welcome
        },
        Welcome {
            round_ticks: MAX_ROUND_TICKS + 1,
            ..base_welcome
        },
    ];
    for welcome in bad_welcomes {
        assert_eq!(
            ServerMessage::decode(&ServerMessage::Welcome(welcome).encode()),
            Err(DecodeError::OverLimit),
            "{welcome:?}"
        );
    }

    let base_snap = Snapshot::capture(&colosseum(1), 0, Vec::new(), &[true, true]);
    let bad_snapshots = vec![
        // Empty or odd fighter rosters cannot represent a valid team match.
        Snapshot {
            fighters: Vec::new(),
            ..base_snap.clone()
        },
        Snapshot {
            fighters: vec![base_snap.fighters[0]],
            ..base_snap.clone()
        },
        Snapshot {
            fighters: vec![base_snap.fighters[0]; 3],
            ..base_snap.clone()
        },
        // Invalid team index in snapshot result or Finished event.
        Snapshot {
            result: Some(WireResult::Won { team: 2 }),
            ..base_snap.clone()
        },
        Snapshot {
            events: vec![TickEvent {
                tick: 1,
                event: WireEvent::Finished(WireResult::Won { team: 2 }),
            }],
            ..base_snap.clone()
        },
        // Out-of-roster actor indices for a 1v1 (2-fighter) snapshot.
        Snapshot {
            events: vec![TickEvent {
                tick: 1,
                event: WireEvent::Jumped { actor: 2 },
            }],
            ..base_snap.clone()
        },
        Snapshot {
            events: vec![TickEvent {
                tick: 1,
                event: WireEvent::Hit {
                    actor: 0,
                    target: 3,
                    damage: 10,
                },
            }],
            ..base_snap.clone()
        },
        Snapshot {
            events: vec![TickEvent {
                tick: 1,
                event: WireEvent::Healed {
                    actor: 2,
                    target: 0,
                    amount: 10,
                },
            }],
            ..base_snap.clone()
        },
        Snapshot {
            events: vec![TickEvent {
                tick: 1,
                event: WireEvent::CastCompleted {
                    actor: 0,
                    spell: WireSpell::RiftBolt,
                    target: Some(2),
                },
            }],
            ..base_snap.clone()
        },
        Snapshot {
            events: vec![TickEvent {
                tick: 1,
                event: WireEvent::CastInterrupted {
                    actor: 0,
                    spell: WireSpell::RiftBolt,
                    reason: WireInterruptReason::Interrupted,
                    by: Some(2),
                },
            }],
            ..base_snap.clone()
        },
        // Scoreboard cannot appear on an unfinished snapshot.
        Snapshot {
            result: None,
            scoreboard: Some(vec![FighterScore::default(); 2]),
            ..base_snap.clone()
        },
        // Scoreboard length must match the roster count.
        Snapshot {
            result: Some(WireResult::Draw),
            scoreboard: Some(Vec::new()),
            ..base_snap.clone()
        },
        Snapshot {
            result: Some(WireResult::Draw),
            scoreboard: Some(vec![FighterScore::default(); 1]),
            ..base_snap.clone()
        },
        Snapshot {
            result: Some(WireResult::Draw),
            scoreboard: Some(vec![FighterScore::default(); 3]),
            ..base_snap.clone()
        },
        Snapshot {
            result: Some(WireResult::TimeLimitDraw),
            scoreboard: Some(vec![FighterScore::default(); MAX_FIGHTERS + 1]),
            ..base_snap.clone()
        },
        // Instant spells cannot produce CastInterrupted.
        Snapshot {
            events: vec![TickEvent {
                tick: 1,
                event: WireEvent::CastInterrupted {
                    actor: 0,
                    spell: WireSpell::Pulse,
                    reason: WireInterruptReason::Interrupted,
                    by: Some(1),
                },
            }],
            ..base_snap.clone()
        },
        Snapshot {
            events: vec![TickEvent {
                tick: 1,
                event: WireEvent::CastInterrupted {
                    actor: 0,
                    spell: WireSpell::Quell,
                    reason: WireInterruptReason::Cancelled,
                    by: None,
                },
            }],
            ..base_snap.clone()
        },
    ];
    for snap in bad_snapshots {
        assert_eq!(
            ServerMessage::decode(&ServerMessage::Snapshot(snap.clone()).encode()),
            Err(DecodeError::OverLimit),
            "{snap:?}"
        );
    }

    assert_eq!(ServerMessage::decode(&[]), Err(DecodeError::Empty));
    assert_eq!(
        ServerMessage::decode(&vec![0; MAX_SERVER_FRAME + 1]),
        Err(DecodeError::Oversized)
    );
}

#[test]
fn adversarial_postcard_wire_corpus_never_panics_or_ooms() {
    // Overlong varint continuation chains must fail fast as Malformed.
    let mut overlong_variant = vec![0x80; 16];
    overlong_variant.push(0x01);
    assert_eq!(
        ClientMessage::decode(&overlong_variant),
        Err(DecodeError::Malformed)
    );
    assert_eq!(
        ServerMessage::decode(&overlong_variant),
        Err(DecodeError::Malformed)
    );
    assert_eq!(peek_hello_protocol(&overlong_variant), None);

    // Huge Vec length prefixes claiming gigabytes in a tiny frame must not allocate or panic.
    let huge_len_varint: [u8; 10] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];
    let mut huge_ticket = vec![0, PROTOCOL_VERSION as u8];
    huge_ticket.extend_from_slice(&huge_len_varint);
    assert_eq!(
        ClientMessage::decode(&huge_ticket),
        Err(DecodeError::Malformed)
    );
    // Snapshot variant 1, tick 0, ack 0, queued 0, hash 0, result None (0), fighters len = u64::MAX.
    let mut huge_fighters = vec![1, 0, 0, 0, 0, 0];
    huge_fighters.extend_from_slice(&huge_len_varint);
    assert_eq!(
        ServerMessage::decode(&huge_fighters),
        Err(DecodeError::Malformed)
    );
    // Snapshot with 2 valid fighters, then events len = u64::MAX.
    let mut huge_events = ServerMessage::Snapshot(Snapshot::capture(
        &colosseum(1),
        0,
        Vec::new(),
        &[true, true],
    ))
    .encode();
    // Replace trailing 0-length events varint with huge length prefix.
    assert_eq!(huge_events.pop(), Some(0));
    huge_events.extend_from_slice(&huge_len_varint);
    assert_eq!(
        ServerMessage::decode(&huge_events),
        Err(DecodeError::Malformed)
    );
    // Snapshot with scoreboard Option = Some (1) and Vec len = u64::MAX must fail without OOM.
    let mut huge_scoreboard = ServerMessage::Snapshot(Snapshot::capture(
        &colosseum(1),
        0,
        Vec::new(),
        &[true, true],
    ))
    .encode();
    // Pop trailing `[0 (scoreboard None), 0 (events len)]` and inject `Some` + huge Vec len.
    assert_eq!(huge_scoreboard.pop(), Some(0));
    assert_eq!(huge_scoreboard.pop(), Some(0));
    huge_scoreboard.push(1);
    huge_scoreboard.extend_from_slice(&huge_len_varint);
    assert_eq!(
        ServerMessage::decode(&huge_scoreboard),
        Err(DecodeError::Malformed)
    );
    // Invalid Option tag (2) for scoreboard.
    let mut bad_scoreboard_tag = ServerMessage::Snapshot(Snapshot::capture(
        &colosseum(1),
        0,
        Vec::new(),
        &[true, true],
    ))
    .encode();
    let tag_idx = bad_scoreboard_tag.len() - 2;
    bad_scoreboard_tag[tag_idx] = 2;
    assert_eq!(
        ServerMessage::decode(&bad_scoreboard_tag),
        Err(DecodeError::Malformed)
    );

    // Invalid ServerMessage and ErrorCode discriminants.
    assert_eq!(ServerMessage::decode(&[4]), Err(DecodeError::Malformed));
    assert_eq!(ServerMessage::decode(&[3, 9]), Err(DecodeError::Malformed));
    assert_eq!(
        ServerMessage::decode(&[3, 0xFF, 0x01]),
        Err(DecodeError::Malformed)
    );

    // peek_hello_protocol boundaries: oversized frame, truncated protocol varint, overlong protocol.
    let mut oversized_hello = vec![0, PROTOCOL_VERSION as u8];
    oversized_hello.resize(MAX_CLIENT_FRAME + 1, 0);
    assert_eq!(peek_hello_protocol(&oversized_hello), None);
    assert_eq!(peek_hello_protocol(&[0, 0x80]), None);
    let mut overlong_proto = vec![0];
    overlong_proto.extend_from_slice(&[0x80; 12]);
    overlong_proto.push(0x01);
    assert_eq!(peek_hello_protocol(&overlong_proto), None);
}

#[test]
fn fuzz_client_and_server_frames_preserve_all_wire_invariants() {
    let mut state = 0xD1B5_4A32_D192_ED03_u64;
    let mut next = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    let client_seeds: Vec<Vec<u8>> = client_samples().iter().map(ClientMessage::encode).collect();
    let server_seeds: Vec<Vec<u8>> = server_samples().iter().map(ServerMessage::encode).collect();

    let mutate =
        |seeds: &[Vec<u8>], max_len: usize, round: usize, next: &mut dyn FnMut() -> u64| {
            match round % 7 {
                // Pure random bytes (short and near-limit).
                0 => {
                    let len = if round.is_multiple_of(49) {
                        max_len.saturating_sub(16) + (next() as usize % 33)
                    } else {
                        next() as usize % 96
                    };
                    (0..len).map(|_| next() as u8).collect()
                }
                // Single bit flip in a valid seed.
                1 => {
                    let mut frame = seeds[(next() as usize) % seeds.len()].clone();
                    let at = (next() as usize) % frame.len();
                    frame[at] ^= 1 << (next() % 8);
                    frame
                }
                // Multi-byte overwrite in a valid seed.
                2 => {
                    let mut frame = seeds[(next() as usize) % seeds.len()].clone();
                    for _ in 0..=next() % 3 {
                        let at = (next() as usize) % frame.len();
                        frame[at] = next() as u8;
                    }
                    frame
                }
                // Truncation at a random offset.
                3 => {
                    let mut frame = seeds[(next() as usize) % seeds.len()].clone();
                    let cut = (next() as usize) % frame.len();
                    frame.truncate(cut);
                    frame
                }
                // Byte insertion or trailing append.
                4 => {
                    let mut frame = seeds[(next() as usize) % seeds.len()].clone();
                    let at = (next() as usize) % (frame.len() + 1);
                    frame.insert(at, next() as u8);
                    frame
                }
                // Byte deletion.
                5 => {
                    let mut frame = seeds[(next() as usize) % seeds.len()].clone();
                    let at = (next() as usize) % frame.len();
                    frame.remove(at);
                    frame
                }
                // Cross-frame splice between two valid seeds.
                _ => {
                    let a = &seeds[(next() as usize) % seeds.len()];
                    let b = &seeds[(next() as usize) % seeds.len()];
                    let cut_a = (next() as usize) % (a.len() + 1);
                    let cut_b = (next() as usize) % (b.len() + 1);
                    let mut frame = Vec::with_capacity(cut_a + (b.len() - cut_b));
                    frame.extend_from_slice(&a[..cut_a]);
                    frame.extend_from_slice(&b[cut_b..]);
                    frame
                }
            }
        };

    for round in 0..12_000 {
        let frame = mutate(&client_seeds, MAX_CLIENT_FRAME, round, &mut next);
        let peeked = peek_hello_protocol(&frame);
        if let Ok(message) = ClientMessage::decode(&frame) {
            assert!(frame.len() <= MAX_CLIENT_FRAME);
            match &message {
                ClientMessage::Hello { protocol, ticket } => {
                    assert!(ticket.len() <= MAX_TICKET);
                    assert_eq!(peeked, Some(*protocol));
                }
                _ => assert_eq!(peeked, None),
            }
            let reencoded = message.encode();
            assert_eq!(ClientMessage::decode(&reencoded), Ok(message.clone()));
            assert!(serde_json::from_str::<serde_json::Value>(&message.to_debug_json()).is_ok());
        }
    }

    for round in 0..12_000 {
        let frame = mutate(&server_seeds, MAX_SERVER_FRAME, round, &mut next);
        if let Ok(message) = ServerMessage::decode(&frame) {
            assert!(frame.len() <= MAX_SERVER_FRAME);
            match &message {
                ServerMessage::Welcome(welcome) => {
                    assert!((1..=MAX_FIGHTERS / 2).contains(&usize::from(welcome.team_size)));
                    assert!(usize::from(welcome.actor) < usize::from(welcome.team_size) * 2);
                    assert!(welcome.tick_rate > 0);
                    assert!((1..=MAX_ROUND_TICKS).contains(&welcome.round_ticks));
                }
                ServerMessage::Snapshot(snapshot) => {
                    let count = snapshot.fighters.len();
                    assert!((2..=MAX_FIGHTERS).contains(&count) && count.is_multiple_of(2));
                    assert!(snapshot.events.len() <= MAX_EVENTS);
                    if let Some(WireResult::Won { team }) = snapshot.result {
                        assert!(team <= 1);
                    }
                    if let Some(scores) = &snapshot.scoreboard {
                        assert!(snapshot.result.is_some());
                        assert_eq!(scores.len(), count);
                    }
                    for fighter in &snapshot.fighters {
                        assert!(fighter.combatant().is_valid(count));
                    }
                    for item in &snapshot.events {
                        match item.event {
                            WireEvent::Jumped { actor } => assert!(usize::from(actor) < count),
                            WireEvent::CastStarted {
                                actor,
                                spell,
                                target,
                                duration_ticks,
                            } => {
                                assert!(usize::from(actor) < count);
                                assert!(target.is_none_or(|t| usize::from(t) < count));
                                assert!(SpellId::from(spell).definition().cast_ticks > 0);
                                assert!(duration_ticks > 0);
                            }
                            WireEvent::CastCompleted { actor, target, .. } => {
                                assert!(usize::from(actor) < count);
                                assert!(target.is_none_or(|t| usize::from(t) < count));
                            }
                            WireEvent::CastInterrupted {
                                actor, spell, by, ..
                            } => {
                                assert!(usize::from(actor) < count);
                                assert!(by.is_none_or(|b| usize::from(b) < count));
                                assert!(SpellId::from(spell).definition().cast_ticks > 0);
                            }
                            WireEvent::Hit { actor, target, .. }
                            | WireEvent::Healed { actor, target, .. } => {
                                assert!(usize::from(actor) < count);
                                assert!(usize::from(target) < count);
                            }
                            WireEvent::Finished(WireResult::Won { team }) => assert!(team <= 1),
                            WireEvent::Finished(_) | WireEvent::Rejected(_) => {}
                        }
                    }
                    let body = EncodedSnapshotBody::new(
                        snapshot.tick,
                        snapshot.hash,
                        snapshot.result,
                        &snapshot.fighters,
                        snapshot.scoreboard.as_deref(),
                        Some(&snapshot.events),
                    );
                    assert_eq!(
                        body.encode_for_seat(snapshot.ack, snapshot.queued, &[]),
                        message.encode()
                    );
                }
                ServerMessage::Pong { .. } | ServerMessage::Error(_) => {}
            }
            let reencoded = message.encode();
            assert_eq!(ServerMessage::decode(&reencoded), Ok(message.clone()));
            assert!(serde_json::from_str::<serde_json::Value>(&message.to_debug_json()).is_ok());
        }
    }
}
