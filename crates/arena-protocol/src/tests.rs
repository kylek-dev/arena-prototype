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
        attack: Some(3),
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
        let event = match index % 4 {
            0 => WireEvent::Jumped { actor: 5 },
            1 => WireEvent::Hit {
                actor: 1,
                target: 4,
                damage: u16::MAX,
            },
            2 => WireEvent::Rejected(WireRejection::LineOfSight),
            _ => WireEvent::Finished(WireResult::Won { team: 1 }),
        };
        events.push(TickEvent {
            tick: u64::MAX - index as u64,
            event,
        });
    }
    let mut snapshot = Snapshot::capture(&arena, u32::MAX, events, &[true; MAX_FIGHTERS]);
    for fighter in &mut snapshot.fighters {
        // Worst-case varints everywhere.
        fighter.x = i32::MIN;
        fighter.z = i32::MAX;
        fighter.ready_at = u64::MAX;
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
    // without events costs each player under 6 KB/s, and input under 0.5 KB/s.
    assert!(ClientMessage::Input(input(1_000_000)).encode().len() <= 16);
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
    assert_eq!(PROTOCOL_VERSION, 1);
    assert_eq!(
        ClientMessage::Hello {
            protocol: 1,
            ticket: vec![0xAB, 0xCD],
        }
        .encode(),
        [0, 1, 2, 0xAB, 0xCD]
    );
    assert_eq!(
        ClientMessage::Input(input(300)).encode(),
        // variant 1, seq 300 (varint), x -1000 / z 707 (zigzag varints),
        // facing 0xC000, jump, Some(3).
        [
            1, 0xAC, 0x02, 0xCF, 0x0F, 0x86, 0x0B, 0x80, 0x80, 0x03, 1, 1, 3
        ]
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
    // Unknown variants, invalid bool and option tags.
    assert_eq!(ClientMessage::decode(&[9, 0]), Err(DecodeError::Malformed));
    let mut bad_bool = ClientMessage::Input(input(1)).encode();
    let jump = bad_bool.len() - 3;
    bad_bool[jump] = 2;
    assert_eq!(
        ClientMessage::decode(&bad_bool),
        Err(DecodeError::Malformed)
    );
    let mut bad_option = ClientMessage::Input(input(1)).encode();
    let tag = bad_option.len() - 2;
    bad_option[tag] = 7;
    assert_eq!(
        ClientMessage::decode(&bad_option),
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
        attack: None,
    }
    .merge_into(&mut slot);
    assert_eq!(
        slot,
        ActorInput {
            movement: Some(Axes { x: 5, z: 6 }),
            facing: Some(9),
            jump: true,
            attack: Some(3),
        }
    );
    InputFrame {
        attack: Some(1),
        ..InputFrame::default()
    }
    .merge_into(&mut slot);
    assert_eq!(slot.attack, Some(1), "the latest target wins");
    // A client-chosen target is only intent: the rules still judge it.
    let mut arena = colosseum(1);
    let mut slot = ActorInput::default();
    InputFrame {
        attack: Some(u8::MAX),
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
        assert_eq!((wire.x, wire.z), (fighter.position.x, fighter.position.z));
    }
    assert_eq!(snapshot.fighters[2].facing, 77);
    // Wire state converts back to the exact rule state.
    let fighters: Vec<_> = snapshot
        .fighters
        .iter()
        .map(FighterState::combatant)
        .collect();
    assert_eq!(fighters, arena.fighters());
    for result in [RoundResult::Draw, RoundResult::Won { team: 1 }] {
        assert_eq!(RoundResult::from(WireResult::from(result)), result);
    }
    for reason in [
        Rejection::Cooldown,
        Rejection::LineOfSight,
        Rejection::Duplicate,
    ] {
        assert_eq!(Rejection::from(WireRejection::from(reason)), reason);
    }
    assert!(snapshot.fighters[3].height > 0);
}

#[test]
fn the_debug_dump_is_readable_json() {
    let json = ClientMessage::Input(input(5)).to_debug_json();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["Input"]["seq"], 5);
    assert_eq!(value["Input"]["attack"], 3);
    let json = ServerMessage::Error(ErrorCode::BadTicket).to_debug_json();
    assert_eq!(json, r#"{"Error":"BadTicket"}"#);
    let snapshot = ServerMessage::Snapshot(busy_snapshot()).to_debug_json();
    let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(value["Snapshot"]["fighters"].as_array().unwrap().len(), 6);
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
    snapshots.push(with_queued);

    for snapshot in snapshots {
        let expected = ServerMessage::Snapshot(snapshot.clone()).encode();
        let with_shared = EncodedSnapshotBody::new(
            snapshot.tick,
            snapshot.hash,
            snapshot.result,
            &snapshot.fighters,
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
            None,
        );
        assert_eq!(
            with_per_seat.encode_for_seat(snapshot.ack, snapshot.queued, &snapshot.events),
            expected
        );
    }
}
