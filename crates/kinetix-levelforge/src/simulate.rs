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

//! Headless auto-paddle bot simulator and per-world difficulty curve statistics.

use kinetix_core::config::{GameConfig, LevelDef};
use kinetix_core::physics::{GameEvent, GameWorld, SessionStatus};

pub const BOT_DT: f32 = 1.0 / 120.0;
pub const BOT_MAX_TIME_SEC: f32 = 180.0;
pub const BOT_OFFSET_REFRESH_STEPS: usize = 30;
pub const BOT_MAX_JITTER_PX: f32 = 34.0;
pub const DEFAULT_SEEDS: [u64; 5] = [101, 202, 303, 404, 505];

#[derive(Debug, Clone)]
pub struct SeedRunResult {
    pub seed: u64,
    pub cleared: bool,
    pub time_sec: f32,
    pub balls_lost: u32,
    pub powerups_collected: usize,
}

#[derive(Debug, Clone)]
pub struct LevelSimSummary {
    pub level_id: usize,
    pub world: usize,
    pub name: String,
    pub boss: bool,
    pub runs: Vec<SeedRunResult>,
    pub clear_count: usize,
    pub median_clear_time: f32,
    pub avg_balls_lost: f32,
    pub avg_powerups: f32,
}

#[derive(Debug, Clone)]
pub struct WorldStats {
    pub world: usize,
    pub sector_count: usize,
    pub avg_bricks: f32,
    pub tough_ratio: f32,
    pub special_ratio: f32,
    pub clear_rate: f32,
    pub median_clear_time: f32,
    pub avg_losses: f32,
}

/// Simple xorshift64* PRNG for deterministic bot jitter.
struct BotRng(u64);

impl BotRng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15).max(1))
    }

    fn next_signed(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let unit = ((v >> 40) as u32 as f32) / ((1u32 << 24) as f32);
        unit * 2.0 - 1.0
    }
}

/// Plays a single level with the auto-paddle bot on a given seed.
pub fn simulate_level_seed(
    config: &GameConfig,
    level: &LevelDef,
    seed: u64,
    max_time_sec: f32,
) -> SeedRunResult {
    let slice = std::slice::from_ref(level);
    let mut world = GameWorld::new(config.clone(), slice);
    world.reseed(seed);
    let mut bot_rng = BotRng::new(seed ^ 0xA5A5_5A5A_F0F0_0F0F);
    let mut events = Vec::with_capacity(32);

    let max_steps = (max_time_sec / BOT_DT).ceil() as usize;
    let mut current_jitter = bot_rng.next_signed() * BOT_MAX_JITTER_PX;
    let mut balls_lost = 0u32;
    let mut prev_lives = world.lives;
    let mut powerups_collected = 0usize;

    for step in 0..max_steps {
        if world.status != SessionStatus::Playing {
            break;
        }
        if step % BOT_OFFSET_REFRESH_STEPS == 0 {
            current_jitter = bot_rng.next_signed() * BOT_MAX_JITTER_PX;
        }

        // Launch any stuck ball or fire lasers when active.
        if world.balls.iter().any(|b| b.stuck_offset.is_some()) || world.laser_active {
            world.launch_or_fire(&mut events);
        }

        // Track the lowest descending ball; fallback to lowest ball overall.
        let target_ball_x = world
            .balls
            .iter()
            .filter(|b| b.stuck_offset.is_none() && b.vy > 0.0)
            .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
            .or_else(|| {
                world
                    .balls
                    .iter()
                    .filter(|b| b.stuck_offset.is_none())
                    .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
            })
            .map(|b| b.x);

        if let Some(bx) = target_ball_x {
            // If a bonus capsule is near the paddle and balls are safe above, catch the capsule.
            let safe_for_capsule = world
                .balls
                .iter()
                .all(|b| b.stuck_offset.is_some() || b.y < 680.0 || b.vy < 0.0);
            let bonus_cap = world
                .capsules
                .iter()
                .filter(|c| !c.kind.is_malus() && c.y > 450.0)
                .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

            if safe_for_capsule {
                if let Some(cap) = bonus_cap {
                    world.set_paddle_target(cap.x);
                } else {
                    let aim_x = if let Some(boss) = &world.boss {
                        boss.x + boss.w * 0.5
                    } else if let Some(b) = world.bricks.iter().find(|b| b.is_destructible()) {
                        b.center().0
                    } else {
                        world.config.arena.width * 0.5
                    };
                    let steer = ((bx - aim_x) * 0.09).clamp(-32.0, 32.0);
                    world.set_paddle_target(bx + steer + current_jitter);
                }
            } else {
                let aim_x = if let Some(boss) = &world.boss {
                    boss.x + boss.w * 0.5
                } else if let Some(b) = world.bricks.iter().find(|b| b.is_destructible()) {
                    b.center().0
                } else {
                    world.config.arena.width * 0.5
                };
                let steer = ((bx - aim_x) * 0.09).clamp(-32.0, 32.0);
                world.set_paddle_target(bx + steer + current_jitter);
            }
        }

        world.update(BOT_DT, &mut events);

        if world.lives < prev_lives {
            balls_lost += prev_lives - world.lives;
            prev_lives = world.lives;
        }
        for ev in events.drain(..) {
            if matches!(ev, GameEvent::PowerUpCollected { .. }) {
                powerups_collected += 1;
            }
        }
    }

    SeedRunResult {
        seed,
        cleared: world.status == SessionStatus::Victory,
        time_sec: world.level_elapsed,
        balls_lost,
        powerups_collected,
    }
}

/// Simulates a level across multiple seeds and aggregates its metrics.
pub fn simulate_level(
    config: &GameConfig,
    level: &LevelDef,
    seeds: &[u64],
    max_time_sec: f32,
) -> LevelSimSummary {
    let runs: Vec<SeedRunResult> = seeds
        .iter()
        .map(|&s| simulate_level_seed(config, level, s, max_time_sec))
        .collect();
    let clear_count = runs.iter().filter(|r| r.cleared).count();
    let mut times: Vec<f32> = runs.iter().map(|r| r.time_sec).collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_clear_time = if times.is_empty() {
        0.0
    } else {
        times[times.len() / 2]
    };
    let n = runs.len().max(1) as f32;
    let avg_balls_lost = runs.iter().map(|r| r.balls_lost as f32).sum::<f32>() / n;
    let avg_powerups = runs
        .iter()
        .map(|r| r.powerups_collected as f32)
        .sum::<f32>()
        / n;

    LevelSimSummary {
        level_id: level.id,
        world: level.world,
        name: level.name.clone(),
        boss: level.boss,
        runs,
        clear_count,
        median_clear_time,
        avg_balls_lost,
        avg_powerups,
    }
}

/// Computes cell composition metrics for a single level: `(destructible_bricks, tough_ratio, special_ratio)`.
pub fn level_cell_metrics(level: &LevelDef) -> (usize, f32, f32) {
    let mut total_non_empty = 0usize;
    let mut destructible = 0usize;
    let mut tough = 0usize;
    let mut special = 0usize;

    for row in &level.rows {
        for ch in row.chars() {
            if ch == '0' || ch == ' ' {
                continue;
            }
            total_non_empty += 1;
            match ch {
                '1'..='4' => destructible += 1,
                '5' | '6' => {
                    destructible += 1;
                    tough += 1;
                }
                'E' | 'G' | 'M' | 'B' | 'R' => {
                    destructible += 1;
                    if matches!(ch, 'M' | 'G' | 'B' | 'R') {
                        tough += 1;
                    }
                    special += 1;
                }
                '#' | 'T' => {
                    special += 1;
                }
                _ => {}
            }
        }
    }
    let denom_d = destructible.max(1) as f32;
    let denom_t = total_non_empty.max(1) as f32;
    (
        destructible,
        tough as f32 / denom_d,
        special as f32 / denom_t,
    )
}

/// Aggregates world-level statistics from level definitions and optional simulation summaries.
pub fn compute_world_stats(
    levels: &[LevelDef],
    sims: Option<&[LevelSimSummary]>,
) -> Vec<WorldStats> {
    let mut world_ids: Vec<usize> = levels.iter().map(|l| l.world).collect();
    world_ids.sort_unstable();
    world_ids.dedup();

    let mut out = Vec::new();
    for wid in world_ids {
        let w_levels: Vec<&LevelDef> = levels.iter().filter(|l| l.world == wid).collect();
        let count = w_levels.len();
        if count == 0 {
            continue;
        }
        let mut sum_bricks = 0f32;
        let mut sum_tough = 0f32;
        let mut sum_special = 0f32;
        for l in &w_levels {
            let (b, t, s) = level_cell_metrics(l);
            sum_bricks += b as f32;
            sum_tough += t;
            sum_special += s;
        }
        let nf = count as f32;

        let (clear_rate, median_clear_time, avg_losses) = if let Some(all_sims) = sims {
            let w_sims: Vec<&LevelSimSummary> =
                all_sims.iter().filter(|s| s.world == wid).collect();
            if w_sims.is_empty() {
                (0.0, 0.0, 0.0)
            } else {
                let cleared_sectors = w_sims.iter().filter(|s| s.clear_count > 0).count();
                let mut med_times: Vec<f32> = w_sims.iter().map(|s| s.median_clear_time).collect();
                med_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let med = med_times[med_times.len() / 2];
                let losses =
                    w_sims.iter().map(|s| s.avg_balls_lost).sum::<f32>() / w_sims.len() as f32;
                (cleared_sectors as f32 / w_sims.len() as f32, med, losses)
            }
        } else {
            (0.0, 0.0, 0.0)
        };

        out.push(WorldStats {
            world: wid,
            sector_count: count,
            avg_bricks: sum_bricks / nf,
            tough_ratio: sum_tough / nf,
            special_ratio: sum_special / nf,
            clear_rate,
            median_clear_time,
            avg_losses,
        });
    }
    out
}
