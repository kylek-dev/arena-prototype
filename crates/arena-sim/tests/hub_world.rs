//! Hub worlds (ADR 0057): the arena's rules with a dynamic roster, one
//! friendly team, training enemies that never act and no end.

use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{
    ActorInput, Arena, ArenaCommand, ArenaEvent, Axes, BufferedCast, CastAction, HUB_PLAYER_CAP,
    HUB_RESPAWN_TICKS, InterruptReason, MAX_CAST_BUFFER_TICKS, MAX_HEALTH, MAX_LAG_COMP_TICKS,
    MAX_ROSTER, MAX_ROUND_TICKS, Occupant, Position, Rejection, Rules, Slot, vacant_fighter,
};
use arena_sim::hub::{TRAINING_POSTS, plaza};
use arena_sim::spells::SpellId;

const DUMMIES: usize = TRAINING_POSTS.len();

fn cast(spell: SpellId, target: usize) -> ActorInput {
    ActorInput {
        action: Some(CastAction::Cast {
            spell,
            target: Some(target),
        }),
        ..ActorInput::default()
    }
}

fn inputs(pairs: &[(usize, ActorInput)]) -> Vec<ActorInput> {
    let len = pairs.iter().map(|(actor, _)| actor + 1).max().unwrap_or(0);
    let mut inputs = vec![ActorInput::default(); len];
    for &(actor, input) in pairs {
        inputs[actor] = input;
    }
    inputs
}

/// A world with `players` arrivals, each moved to `places[i]` (if given).
fn world(players: usize, places: &[Position]) -> (Arena, Vec<usize>) {
    let mut arena = Arena::hub();
    let slots: Vec<usize> = (0..players).map(|_| arena.join().unwrap()).collect();
    let mut fighters = arena.fighters().to_vec();
    for (&slot, &place) in slots.iter().zip(places) {
        fighters[slot].position = place;
    }
    let arena = Arena::restore_hub(
        arena.tick(),
        arena.slots().to_vec(),
        fighters,
        arena.next_id(),
    )
    .unwrap();
    (arena, slots)
}

fn rejected(events: &[ArenaEvent], actor: usize) -> Vec<Rejection> {
    events
        .iter()
        .filter_map(|event| match *event {
            ArenaEvent::Rejected { actor: a, reason } if a == actor => Some(reason),
            _ => None,
        })
        .collect()
}

const NEAR_DUMMY: Position = Position {
    x: TRAINING_POSTS[0].0.x,
    z: TRAINING_POSTS[0].0.z + 3_000,
};

#[test]
fn a_new_world_has_its_training_enemies_and_no_players() {
    let arena = Arena::hub();
    assert_eq!(arena.rules(), Rules::Hub);
    assert_eq!(arena.players(), 0);
    assert_eq!(arena.fighters().len(), DUMMIES);
    for (index, (home, facing)) in TRAINING_POSTS.iter().enumerate() {
        let slot = arena.slots()[index];
        assert_eq!(slot.occupant, Occupant::TrainingEnemy);
        assert_eq!(arena.team(index), Some(1));
        assert_eq!(arena.fighters()[index].position, *home);
        assert_eq!(arena.fighters()[index].facing, *facing);
        assert_eq!(arena.fighters()[index].health, MAX_HEALTH);
        assert!(plaza().walkable(*home));
    }
    assert_eq!(arena.result(), None);
}

#[test]
fn players_join_with_fresh_ids_up_to_the_cap_and_reuse_vacant_slots() {
    let mut arena = Arena::hub();
    let slots: Vec<usize> = (0..HUB_PLAYER_CAP).map(|_| arena.join().unwrap()).collect();
    assert_eq!(
        slots,
        (DUMMIES..DUMMIES + HUB_PLAYER_CAP).collect::<Vec<_>>()
    );
    assert!(arena.join().is_none(), "a full world takes nobody");
    assert!(arena.fighters().len() <= MAX_ROSTER);
    let ids: Vec<u32> = slots.iter().map(|&slot| arena.slots()[slot].id).collect();
    let distinct: std::collections::BTreeSet<u32> = ids.iter().copied().collect();
    assert_eq!(distinct.len(), HUB_PLAYER_CAP, "ids are distinct");
    for &slot in &slots {
        assert_eq!(arena.team(slot), Some(0));
        assert!(plaza().walkable(arena.fighters()[slot].position));
    }

    let leaver = slots[3];
    let old_id = arena.slots()[leaver].id;
    assert_eq!(arena.leave(leaver), Some(Vec::new()));
    assert_eq!(arena.slots()[leaver].occupant, Occupant::Vacant);
    assert_eq!(arena.team(leaver), None);
    assert_eq!(arena.players(), HUB_PLAYER_CAP - 1);
    assert_eq!(
        arena.leave(leaver),
        None,
        "a vacant slot cannot leave twice"
    );
    assert_eq!(arena.leave(0), None, "training enemies never leave");

    let newcomer = arena.join().unwrap();
    assert_eq!(newcomer, leaver, "the first vacant slot is reused");
    assert!(arena.slots()[newcomer].id > old_id, "never the old id");
    assert!(!ids.contains(&arena.slots()[newcomer].id));
    assert_eq!(arena.fighters()[newcomer].health, MAX_HEALTH);
}

#[test]
fn matches_have_no_roster_changes() {
    let mut arena = Arena::practice(match_layout(), 1).unwrap();
    assert_eq!(arena.rules(), Rules::Match);
    assert!(arena.join().is_none());
    assert_eq!(arena.leave(0), None);
    assert!(arena.slots().is_empty());
    assert_eq!(arena.team(0), Some(0));
    assert_eq!(arena.team(1), Some(1));
}

/// A small match layout: two rooms of spawns facing each other.
fn match_layout() -> arena_sim::map::Layout {
    use arena_sim::map::{Edge, Layout, Room, Spawn};
    let room = |team, edge| Room {
        team,
        edge,
        offset: 0,
        width: 8_000,
        depth: 6_000,
        spawns: vec![
            Spawn {
                across: -2_000,
                back: 3_000,
            },
            Spawn {
                across: 0,
                back: 3_000,
            },
            Spawn {
                across: 2_000,
                back: 3_000,
            },
        ],
    };
    Layout {
        half_x: 30_000,
        half_z: 30_000,
        pillars: Vec::new(),
        rooms: vec![room(0, Edge::South), room(1, Edge::North)],
    }
}

#[test]
fn players_are_friendly_harmful_spells_hit_only_training_enemies() {
    let next_to = Position {
        x: NEAR_DUMMY.x + 2_000,
        z: NEAR_DUMMY.z,
    };
    let (mut arena, players) = world(2, &[NEAR_DUMMY, next_to]);
    let (a, b) = (players[0], players[1]);

    // Harmful at a player: refused.
    let events = arena.advance(&inputs(&[(a, cast(SpellId::Pulse, b))]));
    assert_eq!(rejected(&events, a), vec![Rejection::InvalidTarget]);
    assert_eq!(arena.fighters()[b].health, MAX_HEALTH);

    // Helpful at a training enemy: refused.
    let events = arena.advance(&inputs(&[(b, cast(SpellId::Mend, 0))]));
    assert_eq!(rejected(&events, b), vec![Rejection::InvalidTarget]);

    // Harmful at a training enemy: lands.
    let events = arena.advance(&inputs(&[(a, cast(SpellId::Pulse, 0))]));
    assert!(events.contains(&ArenaEvent::Hit {
        actor: a,
        target: 0,
        damage: 25
    }));
    assert_eq!(arena.fighters()[0].health, MAX_HEALTH - 25);

    // Helpful at another player: starts (Mend is a timed cast).
    let events = arena.advance(&inputs(&[(b, cast(SpellId::Mend, a))]));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ArenaEvent::CastStarted { actor, spell: SpellId::Mend, target: Some(t), .. } if *actor == b && *t == a)),
        "{events:?}"
    );
}

#[test]
fn training_enemies_and_vacant_slots_never_act() {
    let (mut arena, players) = world(2, &[NEAR_DUMMY]);
    arena.leave(players[1]).unwrap();
    let vacant = players[1];
    let everything = ActorInput {
        movement: Some(Axes { x: 1_000, z: 0 }),
        facing: Some(7),
        jump: true,
        action: Some(CastAction::Cast {
            spell: SpellId::Pulse,
            target: Some(players[0]),
        }),
        ..ActorInput::default()
    };
    let before = arena.fighters()[0];
    let events = arena.advance(&inputs(&[(0, everything), (vacant, everything)]));
    assert_eq!(rejected(&events, 0), vec![Rejection::InvalidActor; 4]);
    assert_eq!(rejected(&events, vacant), vec![Rejection::InvalidActor; 4]);
    assert_eq!(arena.fighters()[0].position, before.position);
    assert_eq!(arena.fighters()[0].facing, before.facing);
    assert_eq!(arena.fighters()[players[0]].health, MAX_HEALTH);

    // Nobody can aim at a vacant slot.
    let events = arena.advance(&inputs(&[(players[0], cast(SpellId::Mend, vacant))]));
    assert_eq!(
        rejected(&events, players[0]),
        vec![Rejection::InvalidTarget]
    );
}

#[test]
fn a_world_never_ends_and_the_fallen_stand_up_at_home() {
    let (mut arena, players) = world(1, &[NEAR_DUMMY]);
    let a = players[0];
    let mut fell_at = None;
    for _ in 0..30 * TICKS_PER_SECOND {
        let events = arena.advance(&inputs(&[(a, cast(SpellId::Pulse, 0))]));
        assert!(!events.iter().any(|e| matches!(e, ArenaEvent::Finished(_))));
        if arena.fighters()[0].health == 0 {
            fell_at = Some(arena.tick() - 1);
            break;
        }
    }
    let fell_at = fell_at.expect("the training enemy falls");
    assert_eq!(arena.result(), None);
    assert_eq!(
        arena.slots()[0].respawn_at,
        Some(fell_at + HUB_RESPAWN_TICKS)
    );

    // Down for the wait: no target, no regeneration.
    while arena.tick() < fell_at + HUB_RESPAWN_TICKS {
        let events = arena.advance(&inputs(&[(a, cast(SpellId::Pulse, 0))]));
        assert!(!rejected(&events, a).is_empty());
        assert_eq!(arena.fighters()[0].health, 0);
    }
    arena.advance(&[]);
    let enemy = arena.fighters()[0];
    assert_eq!(enemy.health, MAX_HEALTH);
    assert_eq!(enemy.position, TRAINING_POSTS[0].0);
    assert_eq!(arena.slots()[0].respawn_at, None);
    assert_eq!(arena.slots()[0].occupant, Occupant::TrainingEnemy);
}

#[test]
fn the_clock_never_ends_a_world() {
    let (mut arena, _) = world(1, &[]);
    for _ in 0..=MAX_ROUND_TICKS {
        assert!(arena.advance(&[]).is_empty());
    }
    assert_eq!(arena.result(), None);
    assert!(arena.tick() > MAX_ROUND_TICKS);
}

#[test]
fn leaving_interrupts_casts_aimed_at_the_leaver_so_a_newcomer_is_never_hit() {
    let next_to = Position {
        x: NEAR_DUMMY.x + 2_000,
        z: NEAR_DUMMY.z,
    };
    let (mut arena, players) = world(2, &[NEAR_DUMMY, next_to]);
    let (healer, leaver) = (players[0], players[1]);
    arena.advance(&inputs(&[(healer, cast(SpellId::Mend, leaver))]));
    assert!(arena.fighters()[healer].active_cast.is_some());

    let events = arena.leave(leaver).unwrap();
    assert_eq!(
        events,
        vec![ArenaEvent::CastInterrupted {
            actor: healer,
            spell: SpellId::Mend,
            reason: InterruptReason::TargetDead,
            by: None,
        }]
    );
    assert!(arena.fighters()[healer].active_cast.is_none());
    assert_eq!(
        arena.fighters()[healer].gcd_remaining,
        0,
        "the cast never completed"
    );
    let newcomer = arena.join().unwrap();
    assert_eq!(newcomer, leaver);
    for _ in 0..3 * TICKS_PER_SECOND {
        let events = arena.advance(&[]);
        assert!(!events.iter().any(|e| matches!(
            e,
            ArenaEvent::CastCompleted { .. } | ArenaEvent::Healed { .. }
        )));
    }
}

#[test]
fn hub_worlds_replay_identically_and_hash_their_roster() {
    let run = || {
        let mut arena = Arena::hub();
        let mut hashes = Vec::new();
        for tick in 0..200u32 {
            if tick % 40 == 0 {
                arena.join();
            }
            if tick == 120 {
                arena.leave(DUMMIES + 1);
            }
            let walk = ActorInput {
                movement: Some(Axes { x: 0, z: -1_000 }),
                jump: tick % 25 == 0,
                ..ActorInput::default()
            };
            let mut slots = vec![ActorInput::default(); arena.fighters().len()];
            for (actor, slot) in arena.slots().iter().enumerate() {
                if slot.occupant == Occupant::Player {
                    slots[actor] = walk;
                }
            }
            arena.advance(&slots);
            hashes.push(arena.state_hash());
        }
        (arena, hashes)
    };
    let (first, first_hashes) = run();
    let (second, second_hashes) = run();
    assert_eq!(first, second);
    assert_eq!(first_hashes, second_hashes);

    // The roster is part of the digest: same fighters, different slots.
    let mut slots = first.slots().to_vec();
    let fighters = first.fighters().to_vec();
    let player = slots
        .iter()
        .position(|slot| slot.occupant == Occupant::Player)
        .unwrap();
    let same = Arena::restore_hub(
        first.tick(),
        slots.clone(),
        fighters.clone(),
        first.next_id(),
    );
    assert_eq!(same.unwrap().state_hash(), first.state_hash());
    slots[player].id = first.next_id();
    let renumbered = Arena::restore_hub(
        first.tick(),
        slots.clone(),
        fighters.clone(),
        first.next_id() + 1,
    );
    assert_ne!(renumbered.unwrap().state_hash(), first.state_hash());
    slots[player].respawn_at = Some(1);
    let fallen = Arena::restore_hub(first.tick(), slots, fighters, first.next_id() + 1);
    assert_ne!(fallen.unwrap().state_hash(), first.state_hash());
}

#[test]
fn restore_hub_refuses_inconsistent_rosters() {
    let (arena, players) = world(2, &[]);
    let slots = arena.slots().to_vec();
    let fighters = arena.fighters().to_vec();
    let next = arena.next_id();
    let ok = |slots: Vec<_>, fighters: Vec<_>, next| Arena::restore_hub(0, slots, fighters, next);
    assert!(ok(slots.clone(), fighters.clone(), next).is_some());

    // Lengths differ.
    assert!(ok(slots[..slots.len() - 1].to_vec(), fighters.clone(), next).is_none());
    // Duplicate ids.
    let mut dup = slots.clone();
    dup[players[1]].id = dup[players[0]].id;
    assert!(ok(dup, fighters.clone(), next).is_none());
    // An id not yet handed out.
    let mut future = slots.clone();
    future[players[1]].id = next;
    assert!(ok(future, fighters.clone(), next).is_none());
    // A vacant slot with a standing fighter.
    let mut vacant = slots.clone();
    vacant[players[1]] = Slot::VACANT;
    assert!(ok(vacant.clone(), fighters.clone(), next).is_none());
    // ...or a placeholder other than the rules' own.
    let mut aimed = fighters.clone();
    aimed[players[1]] = vacant_fighter();
    assert!(ok(vacant.clone(), aimed.clone(), next).is_some());
    let mut moved = aimed.clone();
    moved[players[1]].position.x = 5;
    assert!(ok(vacant.clone(), moved, next).is_none());
    // A vacant slot with an id or a respawn, an occupied one with id 0.
    for broken in [
        Slot {
            id: 3,
            ..Slot::VACANT
        },
        Slot {
            respawn_at: Some(9),
            ..Slot::VACANT
        },
    ] {
        let mut bad = vacant.clone();
        bad[players[1]] = broken;
        assert!(ok(bad, aimed.clone(), next).is_none());
    }
    let mut zero = slots.clone();
    zero[players[0]].id = 0;
    assert!(ok(zero, fighters.clone(), next).is_none());
    // No next id 0 (id 0 marks a vacant slot).
    let empty = Arena::hub();
    assert!(ok(empty.slots().to_vec(), empty.fighters().to_vec(), 0).is_none());
    // More slots than a roster holds.
    let mut many_slots = vacant.clone();
    let mut many_fighters = aimed.clone();
    while many_slots.len() <= MAX_ROSTER {
        many_slots.push(Slot::VACANT);
        many_fighters.push(vacant_fighter());
    }
    assert!(ok(many_slots, many_fighters, next).is_none());
    // ...or aimed at.
    aimed[players[0]].buffered_cast = Some(arena_sim::arena::BufferedCast {
        spell: SpellId::Mend,
        target: Some(players[1]),
    });
    assert!(ok(vacant, aimed, next).is_none());
    // Training enemies never act: no cast, held spell or jump.
    let mut casting = fighters.clone();
    casting[0].buffered_cast = Some(BufferedCast {
        spell: SpellId::Pulse,
        target: Some(players[0]),
    });
    assert!(ok(slots.clone(), casting, next).is_none());
    let mut casting = fighters.clone();
    casting[0].active_cast = Some(arena_sim::arena::ActiveCastState {
        spell: SpellId::RiftBolt,
        target: Some(players[0]),
        remaining_ticks: 1,
        total_ticks: 45,
    });
    assert!(ok(slots.clone(), casting, next).is_none());
    let mut jumping = fighters.clone();
    jumping[0].height = 10;
    assert!(ok(slots.clone(), jumping, next).is_none());
    // Too many players.
    let (full, _) = world(HUB_PLAYER_CAP, &[]);
    let mut slots = full.slots().to_vec();
    slots[0].occupant = Occupant::Player;
    assert!(ok(slots, full.fighters().to_vec(), full.next_id()).is_none());
}

#[test]
fn leaving_drops_held_casts_aimed_at_the_leaver() {
    let next_to = Position {
        x: NEAR_DUMMY.x + 2_000,
        z: NEAR_DUMMY.z,
    };
    let (mut arena, players) = world(2, &[NEAR_DUMMY, next_to]);
    let (healer, leaver) = (players[0], players[1]);
    // Pulse starts the GCD; Mend pressed during it is held (cast buffer).
    arena.advance(&inputs(&[(healer, cast(SpellId::Pulse, 0))]));
    let held = ActorInput {
        cast_buffer_ticks: MAX_CAST_BUFFER_TICKS,
        ..cast(SpellId::Mend, leaver)
    };
    let mut gcd = arena.fighters()[healer].gcd_remaining;
    while gcd > u32::from(held.cast_buffer_ticks) {
        arena.advance(&[]);
        gcd = arena.fighters()[healer].gcd_remaining;
    }
    arena.advance(&inputs(&[(healer, held)]));
    assert!(arena.fighters()[healer].buffered_cast.is_some());
    assert_eq!(arena.leave(leaver), Some(Vec::new()));
    assert!(arena.fighters()[healer].buffered_cast.is_none());
}

#[test]
fn a_newcomer_in_a_reused_slot_cannot_be_hit_where_the_leaver_stood() {
    // The leaver stood beside a player; the newcomer arrives far away.
    let beside = Position {
        x: NEAR_DUMMY.x + 2_000,
        z: NEAR_DUMMY.z,
    };
    let (mut arena, players) = world(2, &[NEAR_DUMMY, beside]);
    let (caster, leaver) = (players[0], players[1]);
    for _ in 0..MAX_LAG_COMP_TICKS {
        arena.advance(&[]);
    }
    arena.leave(leaver).unwrap();
    let newcomer = arena.join().unwrap();
    assert_eq!(newcomer, leaver);
    assert!(
        arena.fighters()[newcomer]
            .position
            .distance_squared(arena.fighters()[caster].position)
            > 40_000i64.pow(2)
    );
    let rewound = ActorInput {
        rewind_ticks: MAX_LAG_COMP_TICKS,
        ..cast(SpellId::Mend, newcomer)
    };
    let events = arena.advance(&inputs(&[(caster, rewound)]));
    assert_eq!(rejected(&events, caster), vec![Rejection::OutOfRange]);
}

#[test]
fn scripted_commands_work_with_a_full_roster() {
    let (mut arena, players) = world(HUB_PLAYER_CAP, &[NEAR_DUMMY]);
    assert_eq!(arena.fighters().len(), MAX_ROSTER);
    let events = arena.step(&[
        ArenaCommand::Attack {
            actor: players[0],
            target: 0,
        },
        ArenaCommand::Jump {
            actor: *players.last().unwrap(),
        },
        ArenaCommand::Jump { actor: MAX_ROSTER },
        ArenaCommand::Jump { actor: 1 },
    ]);
    assert!(events.contains(&ArenaEvent::Hit {
        actor: players[0],
        target: 0,
        damage: 25
    }));
    assert!(events.contains(&ArenaEvent::Jumped {
        actor: *players.last().unwrap()
    }));
    assert_eq!(rejected(&events, MAX_ROSTER), vec![Rejection::InvalidActor]);
    assert_eq!(rejected(&events, 1), vec![Rejection::InvalidActor]);
    assert_eq!(
        arena.can_cast(0, SpellId::Pulse, Some(players[0])),
        Err(Rejection::InvalidActor)
    );
}

#[test]
fn a_world_out_of_ids_takes_nobody_and_changes_nothing() {
    let arena = Arena::hub();
    let mut arena = Arena::restore_hub(
        0,
        arena.slots().to_vec(),
        arena.fighters().to_vec(),
        u32::MAX,
    )
    .unwrap();
    let before = arena.clone();
    assert!(arena.join().is_none());
    assert_eq!(arena, before);
}

#[test]
fn a_world_has_no_clock_and_keeps_its_plaza() {
    let (mut arena, _) = world(1, &[]);
    for _ in 0..10 {
        arena.advance(&[]);
    }
    assert_eq!(arena.remaining_ticks(), arena.max_round_ticks());
    assert!(!arena.relayout(plaza()));
}

#[test]
fn plaza_pillars_stand_inside_their_columns() {
    for (pillar, column) in plaza().pillars.iter().zip(arena_sim::hub::COLUMNS) {
        for corner in pillar.corners() {
            assert!(
                corner.distance_squared(column.center) <= i64::from(column.radius).pow(2),
                "{corner:?} outside {column:?}"
            );
        }
    }
}
