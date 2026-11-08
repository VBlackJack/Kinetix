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
    Brick {
        x,
        y,
        w: 100.0,
        h: BRICK_HEIGHT,
        vx: 0.0,
        kind,
        hit_flash: 0.0,
    }
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
    assert!(w.last_powerup_key.is_none());
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
