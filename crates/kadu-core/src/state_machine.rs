//! Per-fighter intent application and state-machine advance. Both are total,
//! explicit functions: the advance match covers every `FighterState`
//! variant, so an unhandled state is a compile error.

use crate::fighter::Fighter;
use crate::fixed::Fixed;
use crate::ruleset::Ruleset;
use crate::types::{AttackKind, Direction, FighterState, Intent};

/// Phase 2: apply an intent to a fighter, but only if the fighter's current
/// state permits a new action. Disallowed intents are silently ignored, as
/// specified.
pub fn apply_intent(f: &mut Fighter, intent: Intent, rs: &Ruleset, distance: Fixed) {
    if !f.state.accepts_new_intent() {
        return;
    }

    let grounded = f.state.is_grounded();

    match intent {
        Intent::None => {
            if grounded && matches!(f.state, FighterState::WalkFwd | FighterState::WalkBack | FighterState::Crouch) {
                f.enter_state(FighterState::Idle);
            }
        }
        Intent::Move(dir) => {
            if !grounded {
                return;
            }
            match dir {
                Direction::Forward | Direction::UpForward | Direction::DownForward => {
                    if f.state != FighterState::WalkFwd {
                        f.enter_state(FighterState::WalkFwd);
                    }
                }
                Direction::Back | Direction::UpBack | Direction::DownBack => {
                    if f.state != FighterState::WalkBack {
                        f.enter_state(FighterState::WalkBack);
                    }
                }
                Direction::Up | Direction::Down | Direction::Neutral => {
                    if f.state != FighterState::Idle {
                        f.enter_state(FighterState::Idle);
                    }
                }
            }
        }
        Intent::Crouch => {
            if grounded && f.state != FighterState::Crouch {
                f.enter_state(FighterState::Crouch);
            }
        }
        Intent::Block => {
            if !grounded {
                return;
            }
            let low = f.is_crouched_stance();
            let target = if low { FighterState::BlockCrouch } else { FighterState::BlockStand };
            if f.state != target {
                f.enter_state(target);
            }
        }
        Intent::Dash(dir) => {
            if !grounded {
                return;
            }
            let forward = !matches!(dir, Direction::Back | Direction::UpBack | Direction::DownBack);
            f.dash_forward = forward;
            f.enter_state(if forward { FighterState::DashFwd } else { FighterState::DashBack });
        }
        Intent::Jump(dir) => {
            if !grounded {
                return;
            }
            f.dash_forward = !matches!(dir, Direction::Back | Direction::UpBack | Direction::DownBack);
            f.velocity.y = rs.jump_velocity_per_tick;
            f.enter_state(FighterState::JumpRise);
        }
        Intent::Attack(kind) => {
            f.attack_kind = Some(kind);
            f.attack_crouching = f.state == FighterState::Crouch;
            f.attack_jumping = f.state.is_airborne();
            f.attack_hit_registered = false;
            f.enter_state(FighterState::AttackStartup);
        }
        Intent::Throw => {
            if !grounded {
                return;
            }
            let range = rs.throw_hitbox.w + rs.throw_hitbox.x;
            if distance <= range {
                f.attack_hit_registered = false;
                f.enter_state(FighterState::ThrowAttempt);
            }
        }
    }
}

pub struct AdvanceEvents {
    pub combo_reset: bool,
}

/// Phase 3: advance the fighter's state machine by exactly one tick. This
/// only handles the fighter's own timers; combat resolution (phase 5/6) may
/// separately force a transition (e.g. into HitStun).
pub fn advance(f: &mut Fighter, rs: &Ruleset) -> AdvanceEvents {
    f.state_tick = f.state_tick.saturating_add(1);
    let mut combo_reset = false;

    match f.state {
        FighterState::Idle | FighterState::WalkFwd | FighterState::WalkBack | FighterState::Crouch => {
            combo_reset = true;
        }
        FighterState::DashFwd | FighterState::DashBack => {
            if f.state_tick >= rs.dash_ticks {
                f.enter_state(FighterState::Idle);
            }
        }
        FighterState::JumpRise => {
            if f.velocity.y <= Fixed::ZERO {
                f.enter_state(FighterState::JumpFall);
            }
        }
        FighterState::JumpFall => {
            // landing is detected and transitioned by the movement integrator
        }
        FighterState::AttackStartup => {
            let m = rs.move_spec(f.attack_kind.expect("attack kind set on entry"));
            if f.state_tick >= m.startup {
                f.enter_state(FighterState::AttackActive);
            }
        }
        FighterState::AttackActive => {
            let m = rs.move_spec(f.attack_kind.expect("attack kind set on entry"));
            if f.state_tick >= m.active {
                f.enter_state(FighterState::AttackRecovery);
            }
        }
        FighterState::AttackRecovery => {
            let m = rs.move_spec(f.attack_kind.expect("attack kind set on entry"));
            if f.state_tick >= m.recovery {
                f.attack_kind = None;
                f.enter_state(FighterState::Idle);
            }
        }
        FighterState::BlockStand | FighterState::BlockCrouch => {
            combo_reset = true;
        }
        FighterState::BlockStun => {
            if f.state_tick >= f.stun_ticks_target {
                f.enter_state(FighterState::BlockStand);
                combo_reset = true;
            }
        }
        FighterState::HitStun => {
            if f.state_tick >= f.stun_ticks_target {
                combo_reset = true;
                f.enter_state(FighterState::Idle);
            }
        }
        FighterState::Knockdown => {
            if f.state_tick >= 30 {
                f.enter_state(FighterState::WakeUp);
            }
        }
        FighterState::WakeUp => {
            if f.state_tick >= 20 {
                combo_reset = true;
                f.enter_state(FighterState::Idle);
            }
        }
        FighterState::GuardCrush => {
            if f.state_tick >= rs.guard_crush_ticks {
                f.guard = rs.guard_crush_refill_to;
                f.enter_state(FighterState::Idle);
            }
        }
        FighterState::Dizzy => {
            if f.state_tick >= 60 {
                f.enter_state(FighterState::Idle);
            }
        }
        FighterState::ThrowAttempt => {
            let m = &rs.throw;
            if f.state_tick >= m.startup + m.active {
                f.enter_state(FighterState::ThrowWhiff);
            }
        }
        FighterState::ThrowWhiff => {
            if f.state_tick >= rs.throw.recovery {
                f.enter_state(FighterState::Idle);
            }
        }
        FighterState::Thrown => {
            if f.state_tick >= rs.throw.recovery {
                f.enter_state(FighterState::Knockdown);
            }
        }
        FighterState::RoundFreeze | FighterState::RoundOver => {
            // driven entirely by Sim's round-phase machinery, not per-fighter timers
        }
    }

    AdvanceEvents { combo_reset }
}

/// Whether the fighter's current attack (or throw) has an active hitbox this tick.
pub fn attack_kind_for_hitbox(f: &Fighter) -> Option<AttackKind> {
    if f.state == FighterState::AttackActive {
        f.attack_kind
    } else {
        None
    }
}

pub fn throw_is_active(f: &Fighter, rs: &Ruleset) -> bool {
    f.state == FighterState::ThrowAttempt && f.state_tick > rs.throw.startup
}
