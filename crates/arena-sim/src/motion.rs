//! Walking and jumping, one rules tick at a time: the arena's movement and
//! the hub's ([ADR 0056](../../../docs/decisions/0056-social-hub.md)) are
//! this code, so a jump is the same jump everywhere. Only what stops a body
//! differs, passed in as `slide`: the arena's pillars and rooms, or the
//! hub's plaza edge and columns. Integer millimetres, no clocks.

use crate::arena::{AXIS_LIMIT, Axes, GRAVITY, JUMP_SPEED, MOVE_PER_TICK, Position};

const STILL: Position = Position { x: 0, z: 0 };

/// Where a body is and how it moves through the air.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Motion {
    pub position: Position,
    /// Millimetres above the floor.
    pub height: i32,
    /// Millimetres per tick, upwards.
    pub vertical: i32,
    /// The ground step kept from the take-off: an airborne body keeps it
    /// and cannot steer.
    pub momentum: Position,
}

impl Motion {
    pub const fn standing(position: Position) -> Self {
        Self {
            position,
            height: 0,
            vertical: 0,
            momentum: STILL,
        }
    }

    /// Rising or falling (a jump begun this tick counts).
    pub const fn airborne(&self) -> bool {
        self.height > 0 || self.vertical > 0
    }

    /// Step 1 of a tick: an airborne body slides along its momentum; on the
    /// ground the momentum expires (no stale running jumps). `true` when it
    /// was carried.
    pub fn carry(&mut self, slide: impl Fn(Position, Position) -> Position) -> bool {
        if self.height > 0 {
            self.position = slide(self.position, self.momentum);
            true
        } else {
            self.momentum = STILL;
            false
        }
    }

    /// Step 2, on the ground only: one tick of walking along `axes` (each
    /// in `[-AXIS_LIMIT, AXIS_LIMIT]`, normalized so a full stick walks
    /// `MOVE_PER_TICK`), which becomes the momentum. `true` when it walked
    /// or was asked to.
    pub fn walk(
        &mut self,
        Axes { x, z }: Axes,
        slide: impl Fn(Position, Position) -> Position,
    ) -> bool {
        let length = ((i64::from(x).pow(2) + i64::from(z).pow(2)) as u64)
            .isqrt()
            .max(AXIS_LIMIT as u64) as i32;
        let step = Position {
            x: i32::from(x) * MOVE_PER_TICK / length,
            z: i32::from(z) * MOVE_PER_TICK / length,
        };
        let before = self.position;
        self.position = slide(before, step);
        self.momentum = Position {
            x: self.position.x - before.x,
            z: self.position.z - before.z,
        };
        x != 0 || z != 0 || self.position != before
    }

    /// Step 3: leaves the ground; `false` (nothing changes) when already in
    /// the air.
    pub fn jump(&mut self) -> bool {
        if self.airborne() {
            return false;
        }
        self.vertical = JUMP_SPEED;
        true
    }

    /// The last step of a tick: rising and falling under gravity; landing
    /// stops the body and its momentum.
    pub fn fall(&mut self) {
        if self.airborne() {
            self.height = (self.height + self.vertical).max(0);
            self.vertical -= GRAVITY;
            if self.height == 0 {
                self.vertical = 0;
                self.momentum = STILL;
            }
        }
    }
}
