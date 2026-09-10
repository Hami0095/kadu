use crate::fixed::Vec2;
use crate::types::{AttackKind, Facing, FighterState, FighterView, Intent};

#[derive(Debug, Clone)]
pub struct Fighter {
    pub position: Vec2,
    pub velocity: Vec2,
    pub facing: Facing,
    pub vitality: i32,
    pub surge: i32,
    pub guard: i32,
    pub state: FighterState,
    pub state_tick: u16,
    pub combo_count: u8,
    pub hitstop: u16,

    /// Intent persists between agent polls; only overwritten when the agent is polled.
    pub current_intent: Intent,

    /// Which normal is currently mid-execution, if any.
    pub attack_kind: Option<AttackKind>,
    pub attack_crouching: bool,
    pub attack_jumping: bool,
    pub attack_hit_registered: bool,

    pub dash_forward: bool,

    /// Global tick of the most recent throw attempt, for the tech window.
    pub last_throw_attempt_tick: Option<u32>,

    /// Target duration for the current HitStun/BlockStun, set when entered.
    pub stun_ticks_target: u16,

    pub total_hits_landed: u32,
    pub total_damage_dealt: i32,

    pub warnings_this_round: u8,
}

impl Fighter {
    pub fn new(position: Vec2, facing: Facing, vitality: i32, guard_max: i32) -> Fighter {
        Fighter {
            position,
            velocity: Vec2::ZERO,
            facing,
            vitality,
            surge: 0,
            guard: guard_max,
            state: FighterState::RoundFreeze,
            state_tick: 0,
            combo_count: 0,
            hitstop: 0,
            current_intent: Intent::None,
            attack_kind: None,
            attack_crouching: false,
            attack_jumping: false,
            attack_hit_registered: false,
            dash_forward: true,
            last_throw_attempt_tick: None,
            stun_ticks_target: 0,
            total_hits_landed: 0,
            total_damage_dealt: 0,
            warnings_this_round: 0,
        }
    }

    pub fn is_crouched_stance(&self) -> bool {
        matches!(self.state, FighterState::Crouch | FighterState::BlockCrouch)
    }

    pub fn view(&self) -> FighterView {
        FighterView {
            position: self.position,
            velocity: self.velocity,
            facing: self.facing,
            vitality: self.vitality,
            surge: self.surge,
            guard: self.guard,
            state: self.state,
            state_tick: self.state_tick,
            combo_count: self.combo_count,
            airborne: self.state.is_airborne(),
        }
    }

    pub fn enter_state(&mut self, state: FighterState) {
        self.state = state;
        self.state_tick = 0;
    }
}
