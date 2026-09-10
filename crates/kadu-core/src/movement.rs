use crate::fighter::Fighter;
use crate::fixed::Fixed;
use crate::ruleset::Ruleset;
use crate::types::{Facing, FighterState};

/// Phase 4: integrate movement, apply gravity, clamp to arena walls and
/// floor, and resolve fighter-vs-fighter push-apart.
pub fn integrate(fighters: &mut [Fighter; 2], rs: &Ruleset) {
    for f in fighters.iter_mut() {
        let dir_sign = Fixed::from_int(f.facing.sign());
        match f.state {
            FighterState::WalkFwd => f.velocity.x = rs.walk_speed_per_tick * dir_sign,
            FighterState::WalkBack => f.velocity.x = -rs.walk_speed_per_tick * dir_sign,
            FighterState::DashFwd => {
                let sign = Fixed::from_int(if f.dash_forward { f.facing.sign() } else { -f.facing.sign() });
                f.velocity.x = rs.dash_speed_per_tick * sign;
            }
            FighterState::DashBack => {
                let sign = Fixed::from_int(if f.dash_forward { f.facing.sign() } else { -f.facing.sign() });
                f.velocity.x = rs.dash_speed_per_tick * sign;
            }
            FighterState::Idle
            | FighterState::Crouch
            | FighterState::BlockStand
            | FighterState::BlockCrouch
            | FighterState::BlockStun
            | FighterState::HitStun
            | FighterState::AttackStartup
            | FighterState::AttackActive
            | FighterState::AttackRecovery
            | FighterState::Knockdown
            | FighterState::WakeUp
            | FighterState::GuardCrush
            | FighterState::Dizzy
            | FighterState::ThrowAttempt
            | FighterState::ThrowWhiff
            | FighterState::Thrown
            | FighterState::RoundFreeze
            | FighterState::RoundOver => {
                f.velocity.x = Fixed::ZERO;
            }
            FighterState::JumpRise | FighterState::JumpFall => {
                // horizontal drift from the jump's initiating direction persists,
                // vertical handled by gravity below
            }
        }

        if f.state.is_airborne() {
            f.velocity.y = f.velocity.y - rs.gravity_per_tick;
        }

        f.position = f.position + f.velocity;

        if f.position.y <= Fixed::ZERO {
            f.position.y = Fixed::ZERO;
            f.velocity.y = Fixed::ZERO;
            if f.state == FighterState::JumpFall {
                f.enter_state(FighterState::Idle);
            }
        }
        let max_y = rs.ceiling - rs.fighter_height;
        if f.position.y > max_y {
            f.position.y = max_y;
            if f.velocity.y > Fixed::ZERO {
                f.velocity.y = Fixed::ZERO;
            }
        }

        let half_w = rs.fighter_width / Fixed::from_int(2);
        let min_x = half_w;
        let max_x = rs.arena_width - half_w;
        if f.position.x < min_x {
            f.position.x = min_x;
        }
        if f.position.x > max_x {
            f.position.x = max_x;
        }
    }

    // Push-apart: fighters may not overlap horizontally beyond their combined half-widths.
    let half_w = rs.fighter_width / Fixed::from_int(2);
    let min_sep = half_w + half_w;
    let (left_idx, right_idx) = if fighters[0].position.x <= fighters[1].position.x { (0, 1) } else { (1, 0) };
    let sep = fighters[right_idx].position.x - fighters[left_idx].position.x;
    if sep < min_sep {
        let overlap = min_sep - sep;
        let push = overlap / Fixed::from_int(2);
        fighters[left_idx].position.x = fighters[left_idx].position.x - push;
        fighters[right_idx].position.x = fighters[right_idx].position.x + push;
        let half_w2 = half_w;
        let min_x = half_w2;
        let max_x = rs.arena_width - half_w2;
        fighters[left_idx].position.x = fighters[left_idx].position.x.clamp(min_x, max_x);
        fighters[right_idx].position.x = fighters[right_idx].position.x.clamp(min_x, max_x);
    }

    // Facing: fighters always face each other, flip when they cross.
    if fighters[0].position.x <= fighters[1].position.x {
        fighters[0].facing = Facing::Right;
        fighters[1].facing = Facing::Left;
    } else {
        fighters[0].facing = Facing::Left;
        fighters[1].facing = Facing::Right;
    }
}
