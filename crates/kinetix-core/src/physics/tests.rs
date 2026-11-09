/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use super::*;
use crate::test_support::{levels, world};

const EPS: f32 = 1e-3;
const TINY_DT: f32 = 1e-4;
const CONTACT_SUBSTEPS: usize = 6;
const BOSS_TEST_HP: u32 = 18;
const BOSS_TEST_SPEED: f32 = 120.0;
const BOSS_PIN_FRAMES: usize = 90;
const BOSS_DRAIN_TEST_HP: u32 = 1000;
const BOSS_PIN_DT: f32 = 1.0 / 60.0;
const BOSS_MEGA_DAMAGE: u32 = 2;
const PORTAL_TEST_COOLDOWN: f32 = 0.45;
const MOVING_SHIFT: f32 = 25.0;

fn standard(hp: u8, max_hp: u8, color_idx: usize) -> BrickKind {
    BrickKind::Standard {
        hp,
        max_hp,
        color_idx,
    }
}

fn brick(x: f32, y: f32, kind: BrickKind) -> Brick {
    Brick::new(x, y, 100.0, BRICK_HEIGHT, 0.0, kind, 0.0)
}

/// World with no bricks, no boss and one free ball at (x, y) moving along (vx, vy).
fn empty_world(x: f32, y: f32, vx: f32, vy: f32) -> GameWorld {
    let mut w = world();
    w.bricks.clear();
    w.boss = None;
    w.balls[0].stuck_offset = None;
    w.balls[0].x = x;
    w.balls[0].y = y;
    w.balls[0].vx = vx;
    w.balls[0].vy = vy;
    w
}

fn step(w: &mut GameWorld, n: usize) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for _ in 0..n {
        w.step_balls(TINY_DT, &mut events);
    }
    events
}

fn brick_hp(w: &GameWorld, idx: usize) -> u8 {
    match w.bricks[idx].kind {
        BrickKind::Standard { hp, .. } | BrickKind::Moving { hp, .. } => hp,
        other => panic!("unexpected brick kind {other:?}"),
    }
}

#[test]
fn wall_bounce_reflects_horizontal_velocity() {
    let wall = world().config.arena.wall_side;
    let mut w = empty_world(wall + 5.0, 500.0, -1.0, 0.0);
    let events = step(&mut w, 1);
    assert!(w.balls[0].vx > 0.0);
    assert!(w.balls[0].x - w.balls[0].radius >= wall - EPS);
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::WallHit { .. })));
}

#[test]
fn paddle_bounce_direction_follows_impact_side() {
    for (offset, expect_right) in [(40.0f32, true), (-40.0f32, false)] {
        let mut w = empty_world(0.0, 0.0, 0.0, 1.0);
        let px = w.paddle_x;
        let top = w.config.arena.paddle_y - w.config.paddle.height * 0.5;
        w.balls[0].x = px + offset;
        w.balls[0].y = top - w.balls[0].radius + 2.0;
        step(&mut w, 1);
        let ball = &w.balls[0];
        assert!(ball.vy < 0.0, "ball must go up");
        assert_eq!(ball.vx > 0.0, expect_right);
    }
}

#[test]
fn brick_hit_reflects_and_damages_once() {
    let mut w = empty_world(950.0, 400.0 - 9.0 + 2.0, 0.0, 1.0);
    w.bricks.push(brick(900.0, 400.0, standard(2, 2, 4)));
    step(&mut w, 1);
    assert!(w.balls[0].vy < 0.0);
    assert_eq!(brick_hp(&w, 0), 1);
    step(&mut w, CONTACT_SUBSTEPS);
    assert_eq!(brick_hp(&w, 0), 1, "no repeated damage once separated");
}

#[test]
fn centre_inside_moving_brick_is_pushed_out_and_damaged_once() {
    let mut w = empty_world(950.0, 410.0, 0.0, 1.0);
    let mut moving = brick(
        900.0,
        400.0,
        BrickKind::Moving {
            hp: 2,
            color_idx: 3,
        },
    );
    moving.vx = MOVING_BRICK_SPEED;
    w.bricks.push(moving);
    step(&mut w, CONTACT_SUBSTEPS * 2);
    assert_eq!(w.bricks.len(), 1, "one contact must not kill a 2 HP brick");
    assert_eq!(brick_hp(&w, 0), 1);
    let ball = &w.balls[0];
    assert!(
        ball.y + ball.radius <= 400.0 + EPS,
        "ball must be outside the brick"
    );
    assert!(ball.vy < 0.0);
}

#[test]
fn ball_leaving_a_brick_it_is_inside_is_not_reflected_nor_damaging() {
    let mut w = empty_world(950.0, 410.0, 0.0, -1.0);
    w.bricks.push(brick(900.0, 400.0, standard(2, 2, 4)));
    step(&mut w, CONTACT_SUBSTEPS);
    assert_eq!(brick_hp(&w, 0), 2);
    assert!(w.balls[0].vy < 0.0);
}

#[test]
fn centre_inside_boss_damages_once() {
    let mut w = empty_world(960.0, BOSS_Y + 10.0, 0.0, 1.0);
    w.boss = Some(BossEntity {
        x: 845.0,
        y: BOSS_Y,
        w: BOSS_WIDTH,
        h: BOSS_HEIGHT,
        vx: 0.0,
        hp: 18,
        max_hp: 18,
        fire_timer: 100.0,
        hit_flash: 0.0,
    });
    step(&mut w, CONTACT_SUBSTEPS * 2);
    let boss = w.boss.as_ref().expect("boss alive");
    assert_eq!(boss.hp, 17);
    assert!(w.balls[0].y + w.balls[0].radius <= BOSS_Y + EPS);
}

#[test]
fn fireball_pierces_without_reflecting() {
    let mut w = empty_world(950.0, 400.0 - 9.0 + 2.0, 0.0, 1.0);
    w.balls[0].fireball = true;
    w.bricks.push(brick(900.0, 400.0, standard(2, 2, 4)));
    step(&mut w, 1);
    assert!(w.balls[0].vy > 0.0, "piercing ball keeps its direction");
    assert!(w.bricks.is_empty());
}

#[test]
fn moving_bricks_keep_clear_of_walls() {
    let mut w = world();
    w.bricks.clear();
    let wall = w.config.arena.wall_side;
    let mut moving = brick(
        wall + 1.0,
        300.0,
        BrickKind::Moving {
            hp: 2,
            color_idx: 3,
        },
    );
    moving.vx = -MOVING_BRICK_SPEED;
    w.bricks.push(moving);
    let mut events = Vec::new();
    w.update(0.01, &mut events);
    assert!(w.bricks[0].x >= wall + w.config.ball.mega_radius * 2.0);
}

fn boss_at(x: f32, vx: f32) -> BossEntity {
    BossEntity {
        x,
        y: BOSS_Y,
        w: BOSS_WIDTH,
        h: BOSS_HEIGHT,
        vx,
        hp: BOSS_TEST_HP,
        max_hp: BOSS_TEST_HP,
        fire_timer: 1000.0,
        hit_flash: 0.0,
    }
}

#[test]
fn boss_keeps_clear_of_walls() {
    let mut w = empty_world(960.0, 700.0, 0.0, 0.0);
    let wall = w.config.arena.wall_side;
    let margin = w.config.ball.mega_radius * 2.0;
    w.boss = Some(boss_at(wall + 1.0, -BOSS_TEST_SPEED));
    let mut events = Vec::new();
    w.step_boss_and_hazards(0.01, &mut events);
    assert!(w.boss.as_ref().unwrap().x >= wall + margin);

    let right = w.config.arena.width - wall;
    w.boss = Some(boss_at(right - BOSS_WIDTH - 1.0, BOSS_TEST_SPEED));
    w.step_boss_and_hazards(0.01, &mut events);
    let boss = w.boss.as_ref().unwrap();
    assert!(boss.x + boss.w <= right - margin);
}

#[test]
fn mega_ball_squeezed_beside_boss_damages_it_at_most_once_per_cooldown() {
    let mut w = empty_world(0.0, 0.0, -1.0, 0.0);
    let wall = w.config.arena.wall_side;
    let radius = w.config.ball.mega_radius;
    w.balls[0].mega = true;
    w.balls[0].radius = radius;
    w.balls[0].x = wall + radius;
    w.balls[0].y = BOSS_Y + BOSS_HEIGHT * 0.5;
    let mut boss = boss_at(wall + radius * 2.0 + 1.0, 0.0);
    boss.hp = BOSS_DRAIN_TEST_HP;
    boss.max_hp = BOSS_DRAIN_TEST_HP;
    w.boss = Some(boss);
    let mut events = Vec::new();
    for _ in 0..BOSS_PIN_FRAMES {
        w.update(BOSS_PIN_DT, &mut events);
    }
    let lost = BOSS_DRAIN_TEST_HP - w.boss.as_ref().expect("boss alive").hp;
    let elapsed = BOSS_PIN_FRAMES as f32 * BOSS_PIN_DT;
    let max_hits = (elapsed / BOSS_HIT_COOLDOWN_SEC).ceil() as u32 + 1;
    assert!(lost > 0, "scenario must actually hit the boss");
    assert!(
        lost <= BOSS_MEGA_DAMAGE * max_hits,
        "boss lost {lost} HP, expected at most {} ",
        BOSS_MEGA_DAMAGE * max_hits
    );
}

#[test]
fn cooling_portal_does_not_mask_neighbour_brick() {
    let mut w = empty_world(950.0, 400.0 - 9.0 + 2.0, 0.0, 1.0);
    w.balls[0].portal_cooldown = PORTAL_TEST_COOLDOWN;
    // Portal listed first so it would be returned as the first contact.
    w.bricks.push(brick(900.0, 400.0, BrickKind::Portal));
    w.bricks.push(brick(300.0, 700.0, BrickKind::Portal));
    w.bricks.push(brick(900.0, 400.0, standard(2, 2, 4)));
    step(&mut w, 1);
    assert_eq!(brick_hp(&w, 2), 1, "neighbouring brick must take the hit");
}

#[test]
fn painting_a_cell_removes_shifted_moving_bricks() {
    let mut w = world();
    w.bricks.clear();
    let grid = BrickGrid::new(&w.config.arena, EDITOR_COLS);
    let (ox, oy) = grid.cell_origin(3, 2);
    let mut moving = brick(
        ox + MOVING_SHIFT,
        oy,
        BrickKind::Moving {
            hp: 2,
            color_idx: 3,
        },
    );
    moving.w = grid.brick_w();
    moving.vx = MOVING_BRICK_SPEED;
    w.bricks.push(moving);
    w.paint_brick_at(ox + 2.0, oy + 2.0, Some(standard(1, 1, 0)));
    assert_eq!(w.bricks.len(), 1);
    assert!(matches!(w.bricks[0].kind, BrickKind::Standard { .. }));
    w.paint_brick_at(ox + 2.0, oy + 2.0, None);
    assert!(w.bricks.is_empty());
}

#[test]
fn explosion_destroys_bricks_in_radius_only() {
    let mut w = empty_world(960.0, 600.0, 0.0, -1.0);
    let r = w.config.gameplay.explosion_radius;
    w.bricks.push(brick(500.0, 300.0, standard(1, 1, 0)));
    w.bricks
        .push(brick(500.0 + r * 0.5, 300.0, standard(2, 2, 4)));
    w.bricks
        .push(brick(500.0, 300.0 + r * 0.5, BrickKind::Indestructible));
    w.bricks
        .push(brick(500.0 + r * 3.0, 300.0, standard(1, 1, 0)));
    let mut events = Vec::new();
    w.trigger_explosion(550.0, 313.0, &mut events);
    assert_eq!(w.bricks.len(), 2);
    assert!(w
        .bricks
        .iter()
        .any(|b| matches!(b.kind, BrickKind::Indestructible)));
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::Explosion { .. })));
}

#[test]
fn lightning_hits_up_to_configured_targets() {
    let mut w = empty_world(960.0, 600.0, 0.0, -1.0);
    let targets = w.config.gameplay.lightning_targets;
    for i in 0..(targets + 2) {
        w.bricks
            .push(brick(200.0 + i as f32 * 150.0, 300.0, standard(1, 1, 0)));
    }
    let mut events = Vec::new();
    w.trigger_chain_lightning(200.0, 300.0, &mut events);
    let bolts = events
        .iter()
        .filter(|e| matches!(e, GameEvent::LightningBolt { .. }))
        .count();
    assert_eq!(bolts, targets);
    assert_eq!(w.bricks.len(), 2);
}

#[test]
fn portal_warps_ball_to_the_other_portal() {
    let mut w = empty_world(950.0, 400.0 - 9.0 + 2.0, 0.0, 1.0);
    w.bricks.push(brick(900.0, 400.0, BrickKind::Portal));
    w.bricks.push(brick(300.0, 700.0, BrickKind::Portal));
    let events = step(&mut w, 1);
    assert!((w.balls[0].x - 350.0).abs() < EPS);
    assert!((w.balls[0].y - (700.0 + BRICK_HEIGHT * 0.5)).abs() < EPS);
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::PortalWarp { .. })));
}

#[test]
fn powerups_apply_their_effects() {
    let mut w = world();
    w.apply_powerup(PowerUpKind::MultiBall);
    assert_eq!(w.balls.len(), 3);

    let base = w.paddle_width;
    w.apply_powerup(PowerUpKind::Expand);
    assert!((w.paddle_width - (base + w.config.paddle.width_step)).abs() < EPS);
    w.apply_powerup(PowerUpKind::Shrink);
    assert!((w.paddle_width - base).abs() < EPS);

    let lives = w.lives;
    w.apply_powerup(PowerUpKind::ExtraLife);
    assert_eq!(w.lives, lives + 1);
}

#[test]
fn boss_hazard_shrinks_paddle_without_score_or_toast() {
    let mut w = world();
    let before = w.paddle_width;
    let score = w.score;
    w.hazards.push(HazardProjectile {
        x: w.paddle_x,
        y: w.config.arena.paddle_y,
        vy: 0.0,
        radius: HAZARD_RADIUS,
    });
    let mut events = Vec::new();
    w.step_boss_and_hazards(TINY_DT, &mut events);
    assert!(w.paddle_width < before);
    assert_eq!(w.score, score);
    assert!(w.last_powerup.is_none());
    assert!(w.hazards.is_empty());
}

#[test]
fn losing_the_last_ball_ends_the_game() {
    let mut w = empty_world(960.0, 1200.0, 0.0, 1.0);
    w.bricks.push(brick(900.0, 300.0, standard(1, 1, 0)));
    let lives = w.lives;
    let mut events = Vec::new();
    w.update(TINY_DT, &mut events);
    assert_eq!(w.lives, lives - 1);
    assert_eq!(w.status, SessionStatus::Playing);

    w.lives = 1;
    w.balls[0].stuck_offset = None;
    w.balls[0].y = 1200.0;
    w.update(TINY_DT, &mut events);
    assert_eq!(w.status, SessionStatus::GameOver);
}

#[test]
fn clearing_every_destructible_brick_wins() {
    let mut w = empty_world(950.0, 400.0 - 9.0 + 2.0, 0.0, 1.0);
    w.bricks.push(brick(900.0, 400.0, standard(1, 1, 0)));
    w.bricks
        .push(brick(100.0, 700.0, BrickKind::Indestructible));
    let mut events = Vec::new();
    w.update(TINY_DT, &mut events);
    assert_eq!(w.status, SessionStatus::Victory);
}

#[test]
fn built_in_levels_keep_their_grid_layout() {
    let lv = levels();
    let mut w = world();
    w.load_level(0, &lv);
    let first = &w.bricks[0];
    assert!((first.x - 208.0).abs() < EPS);
    assert!((first.y - BRICK_TOP_OFFSET).abs() < EPS);
    assert!((first.w - 95.6).abs() < EPS);
    let second = &w.bricks[1];
    assert!((second.x - (208.0 + 95.6 + BRICK_GAP)).abs() < EPS);
}

/// Brick whose centre lies at (`cx`, `cy`).
fn brick_centered(cx: f32, cy: f32, kind: BrickKind) -> Brick {
    brick(cx - 50.0, cy - BRICK_HEIGHT * 0.5, kind)
}

#[test]
fn explosion_chain_does_not_skip_bricks_in_range() {
    // Order matters: with a recursive scan, detonating B swap-removes Q and moves L below the
    // outer cursor, so L (in range of the first blast) used to survive.
    let mut w = empty_world(960.0, 600.0, 0.0, -1.0);
    let y = 313.0;
    w.bricks.push(brick_centered(1220.0, y, standard(1, 1, 0))); // Q: in range of B only
    w.bricks
        .push(brick_centered(1100.0, y, BrickKind::Explosive)); // B: in range of A
    w.bricks.push(brick_centered(880.0, y, standard(1, 1, 0))); // L: in range of A only
    let mut events = Vec::new();
    w.trigger_explosion(1000.0, y, &mut events);
    assert!(w.bricks.is_empty(), "left {} bricks", w.bricks.len());
    let blasts = events
        .iter()
        .filter(|e| matches!(e, GameEvent::Explosion { .. }))
        .count();
    assert_eq!(blasts, 2);
}

#[test]
fn fireball_destroying_explosive_brick_explodes_once() {
    let mut w = empty_world(550.0, 313.0, 0.0, -1.0);
    w.balls[0].fireball = true;
    w.bricks.push(brick(500.0, 300.0, BrickKind::Explosive));
    let events = step(&mut w, 1);
    assert!(w.bricks.is_empty());
    let blasts = events
        .iter()
        .filter(|e| matches!(e, GameEvent::Explosion { .. }))
        .count();
    assert_eq!(blasts, 1);
}

#[test]
fn brick_hit_reports_points_only_when_destroyed() {
    let mut w = empty_world(960.0, 600.0, 0.0, -1.0);
    w.bricks.push(brick(500.0, 300.0, standard(2, 2, 0)));
    let mut events = Vec::new();
    assert!(!w.damage_brick(0, false, &mut events));
    assert!(matches!(
        events.last(),
        Some(GameEvent::BrickHit {
            destroyed: false,
            points: 0,
            ..
        })
    ));
    let before = w.score;
    assert!(w.damage_brick(0, false, &mut events));
    let Some(GameEvent::BrickHit {
        destroyed, points, ..
    }) = events.last()
    else {
        panic!("expected a BrickHit");
    };
    assert!(*destroyed);
    assert!(*points > 0);
    assert_eq!(w.score - before, *points);
}

#[test]
fn boss_kill_points_use_the_configured_multiplier() {
    let mut w = empty_world(0.0, 0.0, 0.0, 0.0);
    let (bx, by) = (800.0, 400.0);
    w.boss = Some(BossEntity {
        x: bx,
        y: by,
        w: 100.0,
        h: 40.0,
        vx: 0.0,
        hp: 1,
        max_hp: 1,
        fire_timer: 1000.0,
        hit_flash: 0.0,
    });
    w.balls[0].x = bx + 50.0;
    w.balls[0].y = by + 40.0 + w.balls[0].radius - 1.0;
    w.balls[0].vy = -1.0;
    let expected = w.config.gameplay.base_brick_score * w.config.gameplay.boss_kill_score_mult;
    let events = step(&mut w, 1);
    assert!(w.boss.is_none());
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::BrickHit { destroyed: true, points, multiplier: NO_MULTIPLIER, .. } if *points == expected
    )));
    assert_eq!(w.score, expected);
}

#[test]
fn shield_hits_reset_when_a_level_loads() {
    let mut w = world();
    w.apply_powerup(PowerUpKind::Shield);
    assert_eq!(w.shield_hits, w.config.gameplay.shield_hits);
    w.load_level(1, &levels());
    assert_eq!(w.shield_hits, 0);

    w.shield_hits = 2;
    w.load_from_rows(&["1".to_string()], false, 1);
    assert_eq!(w.shield_hits, 0);
}

#[test]
fn apply_powerup_records_the_kind() {
    let mut w = world();
    assert!(w.last_powerup.is_none());
    w.apply_powerup(PowerUpKind::Magnet);
    assert_eq!(w.last_powerup, Some(PowerUpKind::Magnet));
    assert!((w.last_powerup_timer - w.config.gameplay.powerup_toast_sec).abs() < EPS);
}

#[test]
fn reseed_is_deterministic_and_seed_dependent() {
    let draws = |seed: u64| {
        let mut w = world();
        w.reseed(seed);
        (0..8).map(|_| w.next_rand_f32()).collect::<Vec<f32>>()
    };
    assert_eq!(draws(42), draws(42));
    assert_ne!(draws(42), draws(43));
    assert_ne!(draws(0), draws(1));
    let mut w = world();
    w.reseed(0);
    assert_ne!(w.rng_state, 0);
}

#[test]
fn new_worlds_share_the_default_sequence() {
    let (mut a, mut b) = (world(), world());
    assert_eq!(a.next_rand_f32(), b.next_rand_f32());
}

/// Leaves the world one frame away from winning, with no destructible brick left.
fn arm_victory(w: &mut GameWorld) {
    w.bricks.retain(|b| !b.is_destructible());
    w.boss = None;
    w.balls[0].stuck_offset = None;
    w.balls[0].x = 960.0;
    w.balls[0].y = 600.0;
    w.balls[0].vx = 0.0;
    w.balls[0].vy = -1.0;
}

fn win_and_unlock(w: &mut GameWorld) -> usize {
    arm_victory(w);
    let mut events = Vec::new();
    w.update(TINY_DT, &mut events);
    assert_eq!(w.status, SessionStatus::Victory);
    w.max_unlocked_level
}

#[test]
fn clearing_the_board_in_the_editor_does_not_unlock() {
    let mut w = world();
    w.clear_bricks();
    assert_eq!(win_and_unlock(&mut w), 0);
}

#[test]
fn painting_then_winning_does_not_unlock() {
    let mut w = world();
    w.paint_brick_at(960.0, BRICK_TOP_OFFSET + 1.0, None);
    assert!(!w.is_campaign_board());
    assert_eq!(win_and_unlock(&mut w), 0);
}

#[test]
fn importing_a_code_then_winning_does_not_unlock() {
    let mut w = world();
    let code = w.export_level_code();
    w.import_level_code(&code).expect("valid code");
    assert!(!w.is_campaign_board());
    assert_eq!(win_and_unlock(&mut w), 0);
}

#[test]
fn campaign_clear_unlocks_next_sector_and_is_capped() {
    let lv = levels();
    let mut w = world();
    assert_eq!(win_and_unlock(&mut w), 1);

    w.load_level(lv.len() - 1, &lv);
    assert_eq!(w.max_unlocked_level, 1, "loading must not unlock");
    assert_eq!(win_and_unlock(&mut w), lv.len() - 1);
}

#[test]
fn combo_multiplier_steps_and_caps() {
    assert_eq!(combo_multiplier(0, 5, 4), NO_MULTIPLIER);
    assert_eq!(combo_multiplier(4, 5, 4), NO_MULTIPLIER);
    assert_eq!(combo_multiplier(5, 5, 4), 2);
    assert_eq!(combo_multiplier(100, 5, 4), 4);
    // A zero step must not divide by zero.
    assert_eq!(combo_multiplier(3, 0, 10), 4);
}

#[test]
fn brick_hit_reports_the_combo_multiplier_used_for_the_award() {
    let mut w = empty_world(960.0, 600.0, 0.0, -1.0);
    let gp = w.config.gameplay.clone();
    w.combo = gp.combo_step * 2;
    w.bricks.push(brick(500.0, 300.0, standard(1, 1, 0)));
    let mut events = Vec::new();
    assert!(w.damage_brick(0, false, &mut events));
    let expected = combo_multiplier(w.combo, gp.combo_step, gp.combo_max_mult);
    let Some(GameEvent::BrickHit {
        points, multiplier, ..
    }) = events.last()
    else {
        panic!("expected a BrickHit");
    };
    assert_eq!(*multiplier, expected);
    assert!(*multiplier > NO_MULTIPLIER);
    assert_eq!(*points, gp.base_brick_score * u64::from(expected));
}

#[test]
fn non_destroying_hits_report_no_multiplier() {
    let mut w = empty_world(960.0, 600.0, 0.0, -1.0);
    w.combo = 50;
    w.bricks.push(brick(500.0, 300.0, standard(2, 2, 0)));
    let mut events = Vec::new();
    assert!(!w.damage_brick(0, false, &mut events));
    assert!(matches!(
        events.last(),
        Some(GameEvent::BrickHit {
            multiplier: NO_MULTIPLIER,
            ..
        })
    ));
}

#[test]
fn bumper_brick_boosts_ball_speed_temporarily_and_takes_two_hits() {
    let mut w = empty_world(950.0, 400.0 - 9.0 + 2.0, 0.0, 1.0);
    w.bricks.push(brick(
        900.0,
        400.0,
        BrickKind::Bumper {
            hp: BUMPER_BRICK_HP,
        },
    ));
    let base = w.balls[0].speed;
    let boost = w.config.ball.bumper_speed_boost;
    step(&mut w, 1);
    assert_eq!(w.bricks.len(), 1);
    assert!(matches!(w.bricks[0].kind, BrickKind::Bumper { hp: 1 }));
    assert!(
        (w.balls[0].speed - base).abs() < EPS,
        "the bumper must not change the base speed, got {}",
        w.balls[0].speed
    );
    assert!(
        w.balls[0].boost > boost * 0.9 && w.balls[0].boost <= boost,
        "expected bumper boost, got {}",
        w.balls[0].boost
    );
    let mut events = Vec::new();
    assert!(w.damage_brick(0, false, &mut events));
    assert!(w.bricks.is_empty());
}

#[test]
fn bumper_boost_decays_back_to_the_base_speed() {
    let mut w = empty_world(960.0, 700.0, 0.0, -1.0);
    // Keep the level alive and the ball parked so only the decay is observed.
    w.bricks.push(brick(900.0, 300.0, standard(1, 1, 0)));
    w.balls[0].stuck_offset = Some(0.0);
    let base = w.balls[0].speed;
    w.balls[0].boost = w.config.ball.bumper_speed_boost;
    let max = w.config.ball.max_speed;
    assert!(w.balls[0].effective_speed(max) > base);
    let decay_sec = w.config.ball.bumper_boost_decay_sec;
    let mut events = Vec::new();
    // Half the decay time leaves about half of the boost.
    let half_frames = (decay_sec * 0.5 / BOSS_PIN_DT).round() as usize;
    for _ in 0..half_frames {
        w.update(BOSS_PIN_DT, &mut events);
    }
    let remaining = w.balls[0].boost / w.config.ball.bumper_speed_boost;
    assert!((remaining - 0.5).abs() < 0.05, "remaining {remaining}");
    // Past the full decay time the ball is back to its base speed.
    let rest_frames = (decay_sec * 0.5 / BOSS_PIN_DT).round() as usize + 2;
    for _ in 0..rest_frames {
        w.update(BOSS_PIN_DT, &mut events);
    }
    assert_eq!(w.balls[0].boost, 0.0);
    assert!((w.balls[0].effective_speed(max) - base).abs() < EPS);
}

#[test]
fn bumper_boost_never_exceeds_the_speed_cap_and_slow_scales_base_only() {
    let mut w = empty_world(950.0, 400.0 - 9.0 + 2.0, 0.0, 1.0);
    let max = w.config.ball.max_speed;
    w.balls[0].speed = max - 10.0;
    w.bricks.push(brick(
        900.0,
        400.0,
        BrickKind::Bumper {
            hp: BUMPER_BRICK_HP,
        },
    ));
    step(&mut w, 1);
    assert!((w.balls[0].boost - 10.0).abs() < EPS);
    assert!((w.balls[0].effective_speed(max) - max).abs() < EPS);

    let boost = w.balls[0].boost;
    let base = w.balls[0].speed;
    w.apply_powerup(PowerUpKind::Slow);
    assert!((w.balls[0].boost - boost).abs() < EPS);
    assert!((w.balls[0].speed - base * w.config.gameplay.slow_factor).abs() < EPS);
}

#[test]
fn regenerating_brick_heals_after_delay_unless_finished_in_time() {
    let mut w = empty_world(950.0, 700.0, 0.0, -1.0);
    w.bricks.push(brick(
        900.0,
        300.0,
        BrickKind::Regenerating { hp: REGEN_BRICK_HP },
    ));
    let mut events = Vec::new();
    assert!(!w.damage_brick(0, false, &mut events));
    assert!(matches!(
        w.bricks[0].kind,
        BrickKind::Regenerating { hp: 1 }
    ));
    assert!((w.bricks[0].regen_timer - w.config.gameplay.regen_delay_sec).abs() < EPS);

    // Advance past the delay while the ball is stuck on the paddle.
    w.balls[0].stuck_offset = Some(0.0);
    w.update(4.2, &mut events);
    assert!(matches!(
        w.bricks[0].kind,
        BrickKind::Regenerating { hp: REGEN_BRICK_HP }
    ));
    assert_eq!(w.bricks[0].regen_timer, 0.0);

    // Two hits before the timer expires destroys it.
    assert!(!w.damage_brick(0, false, &mut events));
    assert!(w.damage_brick(0, false, &mut events));
    assert!(w.bricks.is_empty());
}

#[test]
fn regenerating_brick_heals_on_time_at_240_hz() {
    let frame_dt = 1.0 / 240.0;
    let mut w = empty_world(950.0, 700.0, 0.0, -1.0);
    w.bricks.push(brick(
        900.0,
        300.0,
        BrickKind::Regenerating { hp: REGEN_BRICK_HP },
    ));
    let mut events = Vec::new();
    assert!(!w.damage_brick(0, false, &mut events));
    w.balls[0].stuck_offset = Some(0.0);
    let total_frames = (w.config.gameplay.regen_delay_sec / frame_dt).round() as usize;
    let early_margin = 3;
    for _ in 0..total_frames - early_margin {
        w.update(frame_dt, &mut events);
    }
    assert!(
        matches!(w.bricks[0].kind, BrickKind::Regenerating { hp: 1 }),
        "healed early"
    );
    for _ in 0..early_margin * 2 {
        w.update(frame_dt, &mut events);
    }
    assert!(matches!(
        w.bricks[0].kind,
        BrickKind::Regenerating { hp: REGEN_BRICK_HP }
    ));
}

#[test]
fn legacy_config_without_bumper_or_regen_fields_still_parses() {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../../../../assets/config/game_config.json"))
            .expect("valid config");
    let ball = value["ball"].as_object_mut().expect("ball");
    ball.remove("bumper_speed_boost");
    ball.remove("bumper_boost_decay_sec");
    value["gameplay"]
        .as_object_mut()
        .expect("gameplay")
        .remove("regen_delay_sec");
    let cfg: crate::config::GameConfig = serde_json::from_value(value).expect("older config");
    assert!(cfg.ball.bumper_speed_boost > 0.0);
    assert!(cfg.ball.bumper_boost_decay_sec > 0.0);
    assert!(cfg.gameplay.regen_delay_sec > 0.0);
}
