use arena_sim::{Command, Duel, TICKS_PER_SECOND};
fn main() {
    let mut duel = Duel::default();
    for tick in 0..(5 * TICKS_PER_SECOND) {
        let mut commands = Vec::new();
        if tick == 0 {
            commands.push(Command::Move {
                actor: 0,
                direction: 1,
            });
        }
        if tick % TICKS_PER_SECOND == 0 {
            commands.push(Command::Attack {
                actor: 0,
                target: 1,
            });
        }
        for event in duel.step(&commands) {
            println!("tick={tick}: {event:?}");
        }
        if duel.winner().is_some() {
            break;
        }
    }
    assert_eq!(duel.winner(), Some(0), "scripted duel must finish");
    println!("winner={:?}; state={:?}", duel.winner(), duel.fighters());
}
