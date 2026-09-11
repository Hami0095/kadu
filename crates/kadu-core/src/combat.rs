use crate::fighter::Fighter;
use crate::geometry::WorldRect;
use crate::ruleset::Ruleset;
use crate::state_machine::{attack_kind_for_hitbox, throw_is_active};
use crate::types::{AttackKind, FighterState};

#[derive(Debug, Clone, Copy)]
struct Snapshot {
    hitbox: Option<WorldRect>,
    is_throw_active: bool,
    hurtboxes: [WorldRect; 6],
    grounded: bool,
    in_hitstun_or_blockstun: bool,
}

fn snapshot(f: &Fighter, rs: &Ruleset) -> Snapshot {
    let crouched = f.is_crouched_stance() || (f.attack_kind.is_some() && f.attack_crouching);
    let origin = f.position;
    let right = f.facing.is_right();

    let hurt_regions = if crouched {
        [
            rs.hurt_head_crouch,
            rs.hurt_torso_crouch,
            rs.hurt_arm_left_crouch,
            rs.hurt_arm_right_crouch,
            rs.hurt_leg_left_crouch,
            rs.hurt_leg_right_crouch,
        ]
    } else {
        [rs.hurt_head, rs.hurt_torso, rs.hurt_arm_left, rs.hurt_arm_right, rs.hurt_leg_left, rs.hurt_leg_right]
    };
    let hurtboxes = hurt_regions.map(|r| r.to_world(origin, right));

    let hitbox = attack_kind_for_hitbox(f).map(|kind| {
        let rect = match (kind, f.attack_crouching) {
            (AttackKind::Light, true) => rs.light_hitbox_crouch,
            (AttackKind::Medium, true) => rs.medium_hitbox_crouch,
            (AttackKind::Light, false) => rs.light_hitbox,
            (AttackKind::Medium, false) => rs.medium_hitbox,
            (AttackKind::Heavy, _) => rs.heavy_hitbox,
        };
        rect.to_world(origin, right)
    });

    Snapshot {
        hitbox,
        is_throw_active: throw_is_active(f, rs),
        hurtboxes,
        grounded: f.state.is_grounded(),
        in_hitstun_or_blockstun: f.state.is_blockstun_or_hitstun(),
    }
}

pub struct HitLog {
    pub attacker_idx: usize,
    pub defender_idx: usize,
    pub blocked: bool,
    pub damage: i32,
    pub hard_knockdown: bool,
    pub is_throw: bool,
    /// The move that connected. `None` for a throw (throws aren't an
    /// `AttackKind`).
    pub kind: Option<AttackKind>,
    /// True for the synthetic zero-damage entries logged when two throws
    /// tech each other. `attacker_idx` is whichever fighter's own throw got
    /// teched; `is_throw` is also true on these.
    pub is_tech: bool,
}

pub struct RoundWarnLog {
    pub fighter_idx: usize,
    pub second_warning: bool,
}

/// Pushes attacker and defender apart along the line already connecting
/// them (defender moves `defender_push` away from the attacker, attacker
/// moves `attacker_push` away from the defender), clamped to the arena
/// walls. This is what makes block strings end naturally instead of
/// requiring a frame-data rule to stop them.
fn apply_pushback(fighters: &mut [Fighter; 2], rs: &Ruleset, attacker: usize, defender: usize, defender_push: crate::fixed::Fixed, attacker_push: crate::fixed::Fixed) {
    use crate::fixed::Fixed;
    let dir = if fighters[defender].position.x >= fighters[attacker].position.x { Fixed::ONE } else { -Fixed::ONE };
    fighters[defender].position.x = fighters[defender].position.x + dir * defender_push;
    fighters[attacker].position.x = fighters[attacker].position.x - dir * attacker_push;

    let half_w = rs.fighter_width / Fixed::from_int(2);
    let min_x = half_w;
    let max_x = rs.arena_width - half_w;
    for idx in [attacker, defender] {
        fighters[idx].position.x = fighters[idx].position.x.clamp(min_x, max_x);
    }
}

/// Phases 5 and 6: resolve every active hitbox against every opposing
/// hurtbox from a pre-damage snapshot, then apply damage, guard, surge,
/// hitstun/blockstun, hitstop and combo scaling.
pub fn resolve(fighters: &mut [Fighter; 2], rs: &Ruleset, tick: u32) -> Vec<HitLog> {
    let snaps = [snapshot(&fighters[0], rs), snapshot(&fighters[1], rs)];
    let mut logs = Vec::new();

    // --- Throws ---
    let throwing: [bool; 2] = [snaps[0].is_throw_active, snaps[1].is_throw_active];
    if throwing[0] && throwing[1] {
        // Simultaneous attempt within the tech window: no damage, push apart.
        let attempted_close = match (fighters[0].last_throw_attempt_tick, fighters[1].last_throw_attempt_tick) {
            (Some(a), Some(b)) => (a as i64 - b as i64).unsigned_abs() as u32 <= rs.throw_tech_window_ticks,
            _ => true,
        };
        if attempted_close && !fighters[0].attack_hit_registered && !fighters[1].attack_hit_registered {
            fighters[0].attack_hit_registered = true;
            fighters[1].attack_hit_registered = true;
            let push = rs.fighter_width;
            fighters[0].position.x = fighters[0].position.x - push;
            fighters[1].position.x = fighters[1].position.x + push;
            for (i, j) in [(0usize, 1usize), (1, 0)] {
                logs.push(HitLog { attacker_idx: i, defender_idx: j, blocked: false, damage: 0, hard_knockdown: false, is_throw: true, kind: None, is_tech: true });
            }
        }
    } else {
        for (i, j) in [(0usize, 1usize), (1, 0)] {
            if !throwing[i] || fighters[i].attack_hit_registered {
                continue;
            }
            let range = rs.throw_hitbox.w + rs.throw_hitbox.x;
            let distance = (fighters[i].position.x - fighters[j].position.x).abs();
            let defender_throwable = snaps[j].grounded && !snaps[j].in_hitstun_or_blockstun && fighters[j].state != FighterState::Thrown;
            if distance <= range && defender_throwable {
                fighters[i].attack_hit_registered = true;
                let dmg = rs.throw.damage.min(fighters[j].vitality);
                fighters[j].vitality -= dmg;
                fighters[i].total_hits_landed += 1;
                fighters[i].total_damage_dealt += dmg;
                fighters[i].surge = (fighters[i].surge + dmg / 8).min(rs.surge_max);
                fighters[j].surge = (fighters[j].surge + dmg / 12).min(rs.surge_max);
                fighters[i].hitstop = rs.throw.hitstop;
                fighters[j].hitstop = rs.throw.hitstop;
                fighters[j].enter_state(FighterState::Thrown);
                logs.push(HitLog { attacker_idx: i, defender_idx: j, blocked: false, damage: dmg, hard_knockdown: false, is_throw: true, kind: None, is_tech: false });
            }
        }
    }

    // --- Strikes ---
    for (i, j) in [(0usize, 1usize), (1, 0)] {
        let Some(hitbox) = snaps[i].hitbox else { continue };
        if fighters[i].attack_hit_registered {
            continue;
        }
        let hit = snaps[j].hurtboxes.iter().any(|hb| hitbox.overlaps(hb));
        if !hit {
            continue;
        }
        fighters[i].attack_hit_registered = true;
        let kind = fighters[i].attack_kind.expect("hitbox implies attack_kind");
        let spec = rs.move_spec(kind);

        let blocking = matches!(fighters[j].state, FighterState::BlockStand | FighterState::BlockCrouch);
        let blocked = blocking
            && if fighters[i].attack_crouching {
                fighters[j].state == FighterState::BlockCrouch
            } else if fighters[i].attack_jumping {
                fighters[j].state == FighterState::BlockStand
            } else {
                true
            };

        let crouch_scale_num: i32 = if fighters[i].attack_crouching { 80 } else { 100 };

        if blocked {
            let chip = (spec.block_damage * crouch_scale_num) / 100;
            let new_vit = (fighters[j].vitality - chip).max(1);
            fighters[j].vitality = new_vit;
            fighters[j].guard -= spec.guard_damage;
            fighters[i].hitstop = spec.hitstop;
            fighters[j].hitstop = spec.hitstop;
            if fighters[j].guard <= 0 {
                fighters[j].guard = 0;
                fighters[j].enter_state(FighterState::GuardCrush);
            } else {
                fighters[j].stun_ticks_target = spec.blockstun;
                fighters[j].enter_state(FighterState::BlockStun);
            }
            apply_pushback(fighters, rs, i, j, rs.pushback_block_defender, rs.pushback_block_attacker);
            logs.push(HitLog { attacker_idx: i, defender_idx: j, blocked: true, damage: chip, hard_knockdown: false, is_throw: false, kind: Some(kind), is_tech: false });
        } else {
            let hit_number = fighters[j].combo_count as usize;
            let scale_pct = rs
                .combo_scaling_pct
                .get(hit_number)
                .copied()
                .unwrap_or(*rs.combo_scaling_pct.last().unwrap_or(&100))
                .max(rs.combo_floor_pct);
            let base = (spec.damage * crouch_scale_num) / 100;
            let dmg = ((base * scale_pct) / 100).max((spec.damage * rs.combo_floor_pct) / 100).max(1);
            let dmg = dmg.min(fighters[j].vitality);

            fighters[j].vitality -= dmg;
            fighters[i].total_hits_landed += 1;
            fighters[i].total_damage_dealt += dmg;
            fighters[i].surge = (fighters[i].surge + dmg / 8).min(rs.surge_max);
            fighters[j].surge = (fighters[j].surge + dmg / 12).min(rs.surge_max);
            fighters[i].hitstop = spec.hitstop;
            fighters[j].hitstop = spec.hitstop;

            fighters[j].combo_count = fighters[j].combo_count.saturating_add(1);
            let hard_knockdown = fighters[j].combo_count >= rs.combo_hard_knockdown_hit;
            if hard_knockdown {
                fighters[j].enter_state(FighterState::Knockdown);
            } else {
                fighters[j].stun_ticks_target = spec.hitstun;
                fighters[j].enter_state(FighterState::HitStun);
            }
            apply_pushback(fighters, rs, i, j, rs.pushback_hit_defender, rs.pushback_hit_attacker);
            logs.push(HitLog { attacker_idx: i, defender_idx: j, blocked: false, damage: dmg, hard_knockdown, is_throw: false, kind: Some(kind), is_tech: false });
        }
    }

    let _ = tick;
    logs
}

/// Guard regenerates at 1/tick while not blocking and not in blockstun.
pub fn regen_guard(fighters: &mut [Fighter; 2], rs: &Ruleset) {
    for f in fighters.iter_mut() {
        let blocking_or_stunned = matches!(
            f.state,
            FighterState::BlockStand | FighterState::BlockCrouch | FighterState::BlockStun | FighterState::GuardCrush
        );
        if !blocking_or_stunned && f.guard < rs.guard_max {
            f.guard = (f.guard + 1).min(rs.guard_max);
        }
    }
}
