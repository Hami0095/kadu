use crate::fixed::{Fixed, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Facing {
    Right,
    Left,
}

impl Facing {
    pub fn is_right(self) -> bool {
        matches!(self, Facing::Right)
    }

    pub fn opposite(self) -> Facing {
        match self {
            Facing::Right => Facing::Left,
            Facing::Left => Facing::Right,
        }
    }

    pub fn sign(self) -> i32 {
        match self {
            Facing::Right => 1,
            Facing::Left => -1,
        }
    }
}

/// 9-way direction, in local terms (Forward is relative to a fighter's facing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    Neutral,
    Forward,
    Back,
    Up,
    Down,
    UpForward,
    UpBack,
    DownForward,
    DownBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttackKind {
    Light,
    Medium,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Intent {
    None,
    Move(Direction),
    Attack(AttackKind),
    Block,
    Crouch,
    Dash(Direction),
    Jump(Direction),
    Throw,
}

impl Default for Intent {
    fn default() -> Self {
        Intent::None
    }
}

/// Every state a fighter can occupy. Transitions are total and explicit:
/// the tick loop's state-advance function match-covers every variant, so an
/// unhandled state is a compile error, not a silent no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FighterState {
    Idle,
    WalkFwd,
    WalkBack,
    Crouch,
    DashFwd,
    DashBack,
    JumpRise,
    JumpFall,
    AttackStartup,
    AttackActive,
    AttackRecovery,
    BlockStand,
    BlockCrouch,
    BlockStun,
    HitStun,
    Knockdown,
    WakeUp,
    GuardCrush,
    Dizzy,
    ThrowAttempt,
    ThrowWhiff,
    Thrown,
    RoundFreeze,
    RoundOver,
}

impl FighterState {
    pub fn is_airborne(self) -> bool {
        matches!(self, FighterState::JumpRise | FighterState::JumpFall)
    }

    pub fn is_attack(self) -> bool {
        matches!(
            self,
            FighterState::AttackStartup | FighterState::AttackActive | FighterState::AttackRecovery
        )
    }

    pub fn is_blockstun_or_hitstun(self) -> bool {
        matches!(self, FighterState::BlockStun | FighterState::HitStun)
    }

    pub fn is_neutral(self) -> bool {
        matches!(
            self,
            FighterState::Idle
                | FighterState::WalkFwd
                | FighterState::WalkBack
                | FighterState::Crouch
        )
    }

    pub fn is_grounded(self) -> bool {
        !self.is_airborne()
    }

    pub fn accepts_new_intent(self) -> bool {
        matches!(
            self,
            FighterState::Idle
                | FighterState::WalkFwd
                | FighterState::WalkBack
                | FighterState::Crouch
                | FighterState::JumpRise
                | FighterState::JumpFall
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FighterView {
    pub position: Vec2,
    pub velocity: Vec2,
    pub facing: Facing,
    pub vitality: i32,
    pub surge: i32,
    pub guard: i32,
    pub state: FighterState,
    pub state_tick: u16,
    pub combo_count: u8,
    pub airborne: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub tick: u32,
    pub round: u8,
    pub ticks_remaining: u32,
    pub me: FighterView,
    pub opponent: FighterView,
    pub distance: Fixed,
    pub my_corner_distance: Fixed,
    pub opponent_corner_distance: Fixed,
}

pub trait Agent {
    fn name(&self) -> &str;
    fn decide(&mut self, obs: &Observation) -> Intent;
    fn reset(&mut self);
}
