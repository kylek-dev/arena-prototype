//! Fixed-tick duel foundation. Commands resolve in input order, then time advances.
//! No wall clock, random numbers, graphics, or I/O.
pub const TICKS_PER_SECOND: u64 = 30;
pub const COOLDOWN: u64 = TICKS_PER_SECOND;
  

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fighter {
    pub health: u16,
    pub position: i32,
    pub ready_at: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Move { actor: usize, direction: i32 },
    Attack { actor: usize, target: usize },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Moved {
        actor: usize,
        position: i32,
    },
    Hit {
        actor: usize,
        target: usize,
        damage: u16,
    },
    Won {
        actor: usize,
    },
    Rejected,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duel {
    tick: u64,
    fighters: [Fighter; 2],
    winner: Option<usize>,
}
impl Default for Duel {
    fn default() -> Self {
        Self {
            tick: 0,
            fighters: [
                Fighter {
                    health: 100,
                    position: -2,
                    ready_at: 0,
                },
                Fighter {
                    health: 100,
                    position: 2,
                    ready_at: 0,
                },
            ],
            winner: None,
        }
    }
}
impl Duel {
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn fighters(&self) -> &[Fighter; 2] {
        &self.fighters
    }
    pub fn winner(&self) -> Option<usize> {
        self.winner
    }
    /// Caller schedules ticks; a future server authenticates actors and bounds queues.
    /// Input order breaks ties. Each actor can move at most once per tick.
    pub fn step(&mut self, commands: &[Command]) -> Vec<Event> {
        let mut events = Vec::new();
        let mut moved = [false; 2];
        for &command in commands {
            if self.winner.is_some() {
                events.push(Event::Rejected);
                continue;
            }
            match command {
                Command::Move { actor, direction }
                    if actor < 2 && matches!(direction, -1 | 1) && !moved[actor] =>
                {
                    let fighter = &mut self.fighters[actor];
                    fighter.position = (fighter.position + direction).clamp(-20, 20);
                    moved[actor] = true;
                    events.push(Event::Moved {
                        actor,
                        position: fighter.position,
                    });
                }
                Command::Attack { actor, target }
                    if actor < 2
                        && target < 2
                        && actor != target
                        && self.tick >= self.fighters[actor].ready_at
                        && (self.fighters[actor].position - self.fighters[target].position)
                            .abs()
                            <= 3 =>
                {
                    let damage = 25.min(self.fighters[target].health);
                    self.fighters[target].health -= damage;
                    self.fighters[actor].ready_at = self.tick + COOLDOWN;
                    events.push(Event::Hit {
                        actor,
                        target,
                        damage,
                    });
                    if self.fighters[target].health == 0 {
                        self.winner = Some(actor);
                        events.push(Event::Won { actor });
                    }
                }
                _ => events.push(Event::Rejected),
            }
        }
        self.tick += 1;
        events
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const ATTACK: Command = Command::Attack {
        actor: 0,
        target: 1,
    };
    const APPROACH: Command = Command::Move {
        actor: 0,
        direction: 1,
    };
    #[test]
    fn range_and_cooldown_are_enforced() {
        let mut duel = Duel::default();
        assert_eq!(duel.step(&[ATTACK]), vec![Event::Rejected]);
        duel.step(&[APPROACH, ATTACK]);
        assert_eq!(duel.fighters()[1].health, 75);
        assert_eq!(duel.step(&[ATTACK]), vec![Event::Rejected]);
        while duel.tick() < 31 {
            duel.step(&[]);
        }
        duel.step(&[ATTACK]);
        assert_eq!(duel.fighters()[1].health, 50);
    }
    #[test]
    fn invalid_inputs_and_spam_cannot_bypass_movement_limits() {
        let mut duel = Duel::default();
        let events = duel.step(&[
            Command::Move {
                actor: usize::MAX,
                direction: 1,
            },
            Command::Move {
                actor: 0,
                direction: i32::MAX,
            },
            Command::Attack {
                actor: 0,
                target: 0,
            },
            Command::Attack {
                actor: 0,
                target: usize::MAX,
            },
            APPROACH,
            APPROACH,
        ]);
        assert_eq!(events.iter().filter(|e| **e == Event::Rejected).count(), 5);
        assert_eq!(duel.fighters()[0].position, -1);
        for _ in 0..100 {
            duel.step(&[APPROACH]);
        }
        assert_eq!(duel.fighters()[0].position, 20);
    }
    #[test]
    fn replay_is_repeatable_and_match_end_is_final() {
        let mut first = Duel::default();
        let mut second = Duel::default();
        for tick in 0..100 {
            let commands = if tick == 0 {
                vec![APPROACH, ATTACK]
            } else {
                vec![ATTACK]
            };
            assert_eq!(first.step(&commands), second.step(&commands));
            assert_eq!(first, second);
        }
        assert_eq!(first.winner(), Some(0));
        assert_eq!(first.fighters()[1].health, 0);
        let before = *first.fighters();
        assert_eq!(first.step(&[APPROACH, ATTACK]), vec![Event::Rejected; 2]);
        assert_eq!(*first.fighters(), before);
    }
}





pub mod arena;
pub mod effects;  