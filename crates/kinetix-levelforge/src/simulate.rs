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

//! Headless paddle bot simulator, fairness gate and per-world difficulty statistics.
//!
//! The bot is a rate-limited player: it keeps a desired paddle position and moves the paddle
//! toward it at a bounded speed, so it can lose balls. Two presets (`good`, `casual`) bracket
//! the human range; the fairness bands below are calibrated on the legacy levels.

use crate::validate::LEGACY_LEVEL_COUNT;
use kinetix_core::config::{GameConfig, LevelDef};
use kinetix_core::physics::{GameEvent, GameWorld, SessionStatus};
use std::collections::VecDeque;

/// Fixed simulation step of the bot (seconds).
pub const BOT_DT: f32 = 1.0 / 120.0;
/// Hard step cap, expressed as a multiple of the sudden-death time, so a stalled run ends.
pub const BOT_MAX_TIME_FACTOR: f32 = 3.0;
/// Seeds used for every level and preset (at least five; eight gives 12.5% resolution).
pub const DEFAULT_SEEDS: [u64; 8] = [101, 202, 303, 404, 505, 606, 707, 808];

/// Seed mixing constant for the bot's own aim-error generator.
const BOT_RNG_SALT: u64 = 0xA5A5_5A5A_F0F0_0F0F;
/// Seed scrambling constant of the bot generator.
const BOT_RNG_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
/// Output multiplier of the xorshift64* generator.
const BOT_RNG_MULT: u64 = 0x2545_F491_4F6C_DD1D;
/// Largest fraction of the paddle half-width the bot offsets to steer a rebound.
const AIM_MAX_RATIO: f32 = 0.85;
/// Bricks within this vertical distance count as the same row when choosing an aim point.
const AIM_ROW_TOLERANCE_PX: f32 = 4.0;
/// Balls above this y (or rising) leave time to fetch a bonus capsule.
const CAPSULE_SAFE_BALL_Y: f32 = 680.0;
/// Capsules above this y are too far from the paddle to chase.
const CAPSULE_MIN_Y: f32 = 450.0;
/// Minimum vertical speed component for the ballistic predictor to be meaningful.
const MIN_PREDICT_VY: f32 = 0.05;

/// Tunable profile of a simulated player.
#[derive(Debug, Clone, Copy)]
pub struct BotPreset {
    pub name: &'static str,
    /// Maximum paddle travel speed (pixels per second).
    pub paddle_speed: f32,
    /// Reaction delay applied to the observed ball state (seconds).
    pub reaction_delay_sec: f32,
    /// Aim error range, in paddle half-widths, redrawn after each paddle hit.
    pub aim_error_half_widths: f32,
}

/// Skilled player.
pub const PRESET_GOOD: BotPreset = BotPreset {
    name: "good",
    paddle_speed: 1350.0,
    reaction_delay_sec: 0.12,
    aim_error_half_widths: 0.35,
};

/// Casual player: slower, later to react, sloppier.
pub const PRESET_CASUAL: BotPreset = BotPreset {
    name: "casual",
    paddle_speed: 900.0,
    reaction_delay_sec: 0.22,
    aim_error_half_widths: 1.1,
};

/// Both presets, in report order.
pub const PRESETS: [BotPreset; 2] = [PRESET_GOOD, PRESET_CASUAL];

/// Measured range of one metric over the calibration levels.
#[derive(Debug, Clone, Copy)]
pub struct MetricBand {
    pub min: f32,
    pub max: f32,
}

/// Calibrated bands of one preset.
#[derive(Debug, Clone, Copy)]
pub struct PresetBands {
    pub clear_rate: MetricBand,
    pub median_clear_time: MetricBand,
    pub losses_per_run: MetricBand,
    pub sudden_death_share: MetricBand,
}

// Calibration: both presets over the 12 legacy non-boss levels (the first 15 levels minus
// the 3 bosses), 8 seeds each, measured on 2026-11-09 with `simulate` against the legacy
// physics. Min/max per metric:
//   good:   clear rate 0.125..0.625, median clear time 17.0..41.3 s,
//           losses per run 0.250..0.875, sudden-death share 0.375..0.875
//   casual: clear rate 0.000..0.125, median clear time 10.6..36.3 s (7 of 12 levels had a
//           cleared run), losses per run 1.000..3.000, sudden-death share 0.000..1.000
// Re-measure and update these values whenever the physics, the bot or the legacy levels change.
pub const GOOD_BANDS: PresetBands = PresetBands {
    clear_rate: MetricBand {
        min: 0.125,
        max: 0.625,
    },
    median_clear_time: MetricBand {
        min: 17.0,
        max: 41.3,
    },
    losses_per_run: MetricBand {
        min: 0.25,
        max: 0.875,
    },
    sudden_death_share: MetricBand {
        min: 0.375,
        max: 0.875,
    },
};

pub const CASUAL_BANDS: PresetBands = PresetBands {
    clear_rate: MetricBand {
        min: 0.0,
        max: 0.125,
    },
    median_clear_time: MetricBand {
        min: 10.6,
        max: 36.3,
    },
    losses_per_run: MetricBand { min: 1.0, max: 3.0 },
    sudden_death_share: MetricBand { min: 0.0, max: 1.0 },
};

/// Relative margin added around every calibrated band (fraction of the bound).
pub const BAND_MARGIN_REL: f32 = 0.30;
/// Absolute margin added around the clear-rate and sudden-death-share bands (one seed of eight).
pub const BAND_MARGIN_RATE_ABS: f32 = 0.125;
/// Absolute margin added around the losses-per-run band (balls).
pub const BAND_MARGIN_LOSSES_ABS: f32 = 0.25;
/// The `casual` preset must lose balls on at least this many legacy levels (measured: all 15),
/// else the bot cannot lose, is rejected, and every other metric is meaningless.
pub const CASUAL_MIN_LOSING_LEGACY_LEVELS: usize = 6;

/// Outcome of one bot run (one level, one preset, one seed).
#[derive(Debug, Clone)]
pub struct SeedRunResult {
    pub seed: u64,
    /// Cleared with the configured lives: before sudden death for regular levels, any time
    /// within the step cap for boss levels.
    pub cleared: bool,
    /// Level time at the end of the run (seconds).
    pub time_sec: f32,
    pub balls_lost: u32,
    /// The run reached the sudden-death time (it is stopped there).
    pub reached_sudden_death: bool,
}

/// Aggregated metrics of one preset on one level.
#[derive(Debug, Clone)]
pub struct PresetSummary {
    pub preset: BotPreset,
    pub runs: Vec<SeedRunResult>,
    /// Fraction of seeds cleared before sudden death.
    pub clear_rate: f32,
    /// Median time over cleared runs only (`None` when no run cleared).
    pub median_clear_time: Option<f32>,
    pub losses_per_run: f32,
    /// Fraction of runs that reached sudden death.
    pub sudden_death_share: f32,
}

/// Both presets' results for one level.
#[derive(Debug, Clone)]
pub struct LevelSimSummary {
    pub level_id: usize,
    pub world: usize,
    pub name: String,
    pub boss: bool,
    pub good: PresetSummary,
    pub casual: PresetSummary,
}

impl LevelSimSummary {
    pub fn preset(&self, idx: usize) -> &PresetSummary {
        if idx == 0 {
            &self.good
        } else {
            &self.casual
        }
    }
}

/// Per-preset world aggregates.
#[derive(Debug, Clone)]
pub struct WorldPresetStats {
    pub preset: &'static str,
    pub clear_rate: f32,
    pub median_clear_time: Option<f32>,
    pub losses_per_run: f32,
    pub sudden_death_share: f32,
}

#[derive(Debug, Clone)]
pub struct WorldStats {
    pub world: usize,
    pub sector_count: usize,
    pub avg_bricks: f32,
    pub tough_ratio: f32,
    pub special_ratio: f32,
    pub per_preset: Vec<WorldPresetStats>,
}

/// Simple xorshift64* PRNG for deterministic bot aim error.
struct BotRng(u64);

impl BotRng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(BOT_RNG_GAMMA).max(1))
    }

    fn next_signed(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(BOT_RNG_MULT);
        let unit = ((v >> 40) as u32 as f32) / ((1u32 << 24) as f32);
        unit * 2.0 - 1.0
    }
}

/// Position and direction of the tracked ball at one step.
#[derive(Debug, Clone, Copy)]
struct BallSample {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

/// Folds an unbounded x coordinate into `[lo, hi]` as if reflected by two side walls.
pub fn fold_reflect(x: f32, lo: f32, hi: f32) -> f32 {
    let span = hi - lo;
    if span <= 0.0 {
        return lo;
    }
    let period = span * 2.0;
    let mut t = (x - lo).rem_euclid(period);
    if t > span {
        t = period - t;
    }
    lo + t
}

/// Bottom edge of the lowest obstacle (brick of any kind or boss), or the top wall if none.
fn lowest_obstacle_bottom(world: &GameWorld) -> f32 {
    let mut bottom = world.config.arena.wall_top;
    for b in &world.bricks {
        bottom = bottom.max(b.y + b.h);
    }
    if let Some(boss) = &world.boss {
        bottom = bottom.max(boss.y + boss.h);
    }
    bottom
}

/// Picks the ball the bot watches: the lowest descending free ball, else the lowest free ball.
fn tracked_ball(world: &GameWorld) -> Option<BallSample> {
    let free = || world.balls.iter().filter(|b| b.stuck_offset.is_none());
    let by_y = |a: &&kinetix_core::physics::Ball, b: &&kinetix_core::physics::Ball| {
        a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal)
    };
    free()
        .filter(|b| b.vy > 0.0)
        .max_by(by_y)
        .or_else(|| free().max_by(by_y))
        .map(|b| BallSample {
            x: b.x,
            y: b.y,
            vx: b.vx,
            vy: b.vy,
        })
}

/// Point the bot tries to send the ball to: the boss, else the lowest destructible brick
/// (nearest to `from_x` among the lowest), else the arena centre.
fn aim_point(world: &GameWorld, from_x: f32) -> (f32, f32) {
    if let Some(boss) = &world.boss {
        return (boss.x + boss.w * 0.5, boss.y + boss.h * 0.5);
    }
    let mut best: Option<(f32, f32)> = None;
    for brick in world.bricks.iter().filter(|b| b.is_destructible()) {
        let (cx, cy) = brick.center();
        let better = match best {
            None => true,
            Some((bx, by)) => {
                cy > by + AIM_ROW_TOLERANCE_PX
                    || ((cy - by).abs() <= AIM_ROW_TOLERANCE_PX
                        && (cx - from_x).abs() < (bx - from_x).abs())
            }
        };
        if better {
            best = Some((cx, cy));
        }
    }
    best.unwrap_or((world.config.arena.width * 0.5, world.config.arena.wall_top))
}

/// Desired paddle x for the (delayed) observed sample: tracks the ball above the obstacle
/// band, and below it predicts the landing x and offsets the paddle to steer the rebound.
fn desired_paddle_x(world: &GameWorld, sample: BallSample, radius: f32) -> f32 {
    let arena = &world.config.arena;
    let below_band = sample.y > lowest_obstacle_bottom(world) + radius;
    if sample.vy <= MIN_PREDICT_VY || !below_band {
        return sample.x;
    }
    let paddle_top = arena.paddle_y - world.config.paddle.height * 0.5;
    let dy = (paddle_top - radius - sample.y).max(0.0);
    let lo = arena.wall_side + radius;
    let hi = arena.width - arena.wall_side - radius;
    let landing = fold_reflect(sample.x + sample.vx / sample.vy * dy, lo, hi);

    let (tx, ty) = aim_point(world, landing);
    let rise = (paddle_top - ty).max(1.0);
    let wanted_angle = (tx - landing).atan2(rise);
    let max_angle = world.config.paddle.max_bounce_angle_deg.to_radians();
    let ratio = (wanted_angle / max_angle).clamp(-AIM_MAX_RATIO, AIM_MAX_RATIO);
    landing - ratio * world.paddle_width * 0.5
}

/// x of the lowest falling bonus capsule worth catching, when no ball is close to the paddle.
fn capsule_target(world: &GameWorld) -> Option<f32> {
    let safe = world
        .balls
        .iter()
        .all(|b| b.stuck_offset.is_some() || b.vy < 0.0 || b.y < CAPSULE_SAFE_BALL_Y);
    if !safe {
        return None;
    }
    world
        .capsules
        .iter()
        .filter(|c| !c.kind.is_malus() && c.y > CAPSULE_MIN_Y)
        .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
        .map(|c| c.x)
}

/// Plays one level with one preset on one seed.
pub fn simulate_level_seed(
    config: &GameConfig,
    level: &LevelDef,
    preset: BotPreset,
    seed: u64,
) -> SeedRunResult {
    let slice = std::slice::from_ref(level);
    let mut world = GameWorld::new(config.clone(), slice);
    world.reseed(seed);
    let mut bot_rng = BotRng::new(seed ^ BOT_RNG_SALT);
    let mut events = Vec::with_capacity(32);

    let sudden_death = config.gameplay.sudden_death_time_sec;
    // Sudden death only strikes bricks, so a boss is fought to the end (up to the step cap).
    let boss_level = level.boss;
    let max_steps = (sudden_death * BOT_MAX_TIME_FACTOR / BOT_DT).ceil() as usize;
    let delay_steps = (preset.reaction_delay_sec / BOT_DT).round() as usize;
    let max_step_px = preset.paddle_speed * BOT_DT;
    let ball_radius = config.ball.default_radius;

    let mut history: VecDeque<BallSample> = VecDeque::with_capacity(delay_steps + 2);
    let mut aim_ratio = bot_rng.next_signed() * preset.aim_error_half_widths;
    let mut balls_lost = 0u32;
    let mut prev_lives = world.lives;
    let mut reached_sudden_death = false;

    for _ in 0..max_steps {
        if world.status != SessionStatus::Playing {
            break;
        }
        if world.level_elapsed >= sudden_death {
            reached_sudden_death = true;
            if !boss_level {
                break;
            }
        }

        // Launch any stuck ball or fire lasers when active.
        if world.balls.iter().any(|b| b.stuck_offset.is_some()) || world.laser_active {
            world.launch_or_fire(&mut events);
        }

        match tracked_ball(&world) {
            Some(sample) => {
                history.push_back(sample);
                while history.len() > delay_steps + 1 {
                    history.pop_front();
                }
                if let Some(&seen) = history.front() {
                    let want = match capsule_target(&world) {
                        Some(x) => x,
                        None => {
                            desired_paddle_x(&world, seen, ball_radius)
                                + aim_ratio * world.paddle_width * 0.5
                        }
                    };
                    let delta = (want - world.paddle_x).clamp(-max_step_px, max_step_px);
                    world.set_paddle_target(world.paddle_x + delta);
                }
            }
            None => history.clear(),
        }

        world.update(BOT_DT, &mut events);

        if world.lives < prev_lives {
            balls_lost += prev_lives - world.lives;
        }
        prev_lives = world.lives;
        for ev in events.drain(..) {
            if matches!(ev, GameEvent::PaddleHit { .. }) {
                aim_ratio = bot_rng.next_signed() * preset.aim_error_half_widths;
            }
        }
    }

    let victory = world.status == SessionStatus::Victory;
    if world.level_elapsed >= sudden_death {
        reached_sudden_death = true;
    }
    SeedRunResult {
        seed,
        cleared: victory && (boss_level || world.level_elapsed < sudden_death),
        time_sec: world.level_elapsed,
        balls_lost,
        reached_sudden_death,
    }
}

/// Median of a slice (mean of the two middle values for even lengths).
pub fn median(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) * 0.5
    })
}

fn summarize(preset: BotPreset, runs: Vec<SeedRunResult>) -> PresetSummary {
    let n = runs.len().max(1) as f32;
    let cleared_times: Vec<f32> = runs
        .iter()
        .filter(|r| r.cleared)
        .map(|r| r.time_sec)
        .collect();
    PresetSummary {
        preset,
        clear_rate: runs.iter().filter(|r| r.cleared).count() as f32 / n,
        median_clear_time: median(&cleared_times),
        losses_per_run: runs.iter().map(|r| r.balls_lost as f32).sum::<f32>() / n,
        sudden_death_share: runs.iter().filter(|r| r.reached_sudden_death).count() as f32 / n,
        runs,
    }
}

/// Simulates every level with both presets over `seeds`, in parallel and deterministically:
/// each task owns a fixed `(level, preset, seed)` slot and results are placed by slot index.
pub fn simulate_levels(
    config: &GameConfig,
    levels: &[LevelDef],
    seeds: &[u64],
) -> Vec<LevelSimSummary> {
    let per_level = PRESETS.len() * seeds.len();
    let total = levels.len() * per_level;
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(total.max(1));

    let mut slots: Vec<Option<SeedRunResult>> = vec![None; total];
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|w| {
                scope.spawn(move || {
                    (w..total)
                        .step_by(workers)
                        .map(|slot| {
                            let level = &levels[slot / per_level];
                            let preset = PRESETS[(slot % per_level) / seeds.len()];
                            let seed = seeds[slot % seeds.len()];
                            (slot, simulate_level_seed(config, level, preset, seed))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            if let Ok(done) = h.join() {
                for (slot, res) in done {
                    slots[slot] = Some(res);
                }
            }
        }
    });

    levels
        .iter()
        .enumerate()
        .map(|(li, level)| {
            let mut per_preset = Vec::with_capacity(PRESETS.len());
            for (pi, preset) in PRESETS.iter().enumerate() {
                let base = li * per_level + pi * seeds.len();
                let runs: Vec<SeedRunResult> = (0..seeds.len())
                    .filter_map(|si| slots[base + si].clone())
                    .collect();
                per_preset.push(summarize(*preset, runs));
            }
            let casual = per_preset.remove(1);
            let good = per_preset.remove(0);
            LevelSimSummary {
                level_id: level.id,
                world: level.world,
                name: level.name.clone(),
                boss: level.boss,
                good,
                casual,
            }
        })
        .collect()
}

/// Band limits widened by the relative margin and an absolute margin.
fn band_bounds(band: MetricBand, abs_margin: f32) -> (f32, f32) {
    (
        band.min - abs_margin - band.min.abs() * BAND_MARGIN_REL,
        band.max + abs_margin + band.max.abs() * BAND_MARGIN_REL,
    )
}

fn check_metric(
    issues: &mut Vec<String>,
    who: &str,
    metric: &str,
    value: f32,
    band: MetricBand,
    abs_margin: f32,
) {
    let (lo, hi) = band_bounds(band, abs_margin);
    if value < lo || value > hi {
        issues.push(format!(
            "{who}: {metric} {value:.2} outside calibrated band [{lo:.2}, {hi:.2}]"
        ));
    }
}

fn check_preset(
    issues: &mut Vec<String>,
    level: &LevelSimSummary,
    sum: &PresetSummary,
    bands: PresetBands,
) {
    let who = format!(
        "level {} ({}) [{}]",
        level.level_id, level.name, sum.preset.name
    );
    if sum.preset.name == PRESET_GOOD.name && sum.clear_rate <= 0.0 {
        issues.push(format!("{who}: no seed cleared before sudden death"));
    }
    check_metric(
        issues,
        &who,
        "clear rate",
        sum.clear_rate,
        bands.clear_rate,
        BAND_MARGIN_RATE_ABS,
    );
    // The casual preset legitimately clears nothing on some levels, so a missing median
    // is only a problem through the clear-rate checks above.
    if let Some(t) = sum.median_clear_time {
        check_metric(
            issues,
            &who,
            "median clear time",
            t,
            bands.median_clear_time,
            0.0,
        );
    }
    check_metric(
        issues,
        &who,
        "losses per run",
        sum.losses_per_run,
        bands.losses_per_run,
        BAND_MARGIN_LOSSES_ABS,
    );
    check_metric(
        issues,
        &who,
        "sudden-death share",
        sum.sudden_death_share,
        bands.sudden_death_share,
        BAND_MARGIN_RATE_ABS,
    );
}

/// Fairness issues of a simulated set: calibrated bands for regular levels, and "the good bot
/// clears at least one seed" for bosses. Empty means fair.
pub fn fairness_issues(summaries: &[LevelSimSummary]) -> Vec<String> {
    let mut issues = Vec::new();
    for s in summaries {
        if s.boss {
            if !s.good.runs.iter().any(|r| r.cleared) {
                issues.push(format!(
                    "boss level {} ({}): the good bot cleared no seed",
                    s.level_id, s.name
                ));
            }
        } else {
            check_preset(&mut issues, s, &s.good, GOOD_BANDS);
            check_preset(&mut issues, s, &s.casual, CASUAL_BANDS);
        }
    }
    issues
}

/// Bot-validity gate: the casual preset must lose balls on legacy levels, otherwise the bot
/// cannot lose and every other metric is meaningless. Skipped when no legacy level is present.
pub fn casual_loss_gate(summaries: &[LevelSimSummary]) -> Result<(), String> {
    let legacy: Vec<&LevelSimSummary> = summaries
        .iter()
        .filter(|s| s.level_id >= 1 && s.level_id <= LEGACY_LEVEL_COUNT)
        .collect();
    if legacy.is_empty() {
        return Ok(());
    }
    let losing = legacy
        .iter()
        .filter(|s| s.casual.runs.iter().any(|r| r.balls_lost > 0))
        .count();
    if losing < CASUAL_MIN_LOSING_LEGACY_LEVELS {
        return Err(format!(
            "bot rejected: the casual preset lost balls on {losing} legacy level(s), expected at least {CASUAL_MIN_LOSING_LEGACY_LEVELS}"
        ));
    }
    Ok(())
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

        let mut per_preset = Vec::new();
        if let Some(all_sims) = sims {
            let w_sims: Vec<&LevelSimSummary> =
                all_sims.iter().filter(|s| s.world == wid).collect();
            if !w_sims.is_empty() {
                let n = w_sims.len() as f32;
                for (pi, preset) in PRESETS.iter().enumerate() {
                    let sums: Vec<&PresetSummary> = w_sims.iter().map(|s| s.preset(pi)).collect();
                    let medians: Vec<f32> =
                        sums.iter().filter_map(|s| s.median_clear_time).collect();
                    per_preset.push(WorldPresetStats {
                        preset: preset.name,
                        clear_rate: sums.iter().map(|s| s.clear_rate).sum::<f32>() / n,
                        median_clear_time: median(&medians),
                        losses_per_run: sums.iter().map(|s| s.losses_per_run).sum::<f32>() / n,
                        sudden_death_share: sums.iter().map(|s| s.sudden_death_share).sum::<f32>()
                            / n,
                    });
                }
            }
        }

        out.push(WorldStats {
            world: wid,
            sector_count: count,
            avg_bricks: sum_bricks / nf,
            tough_ratio: sum_tough / nf,
            special_ratio: sum_special / nf,
            per_preset,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinetix_core::config::LevelPack;

    const RAW_LEVELS: &str = include_str!("../../../assets/config/levels.json");
    const RAW_CONFIG: &str = include_str!("../../../assets/config/game_config.json");

    fn legacy_non_boss() -> (GameConfig, Vec<LevelDef>) {
        let config: GameConfig = serde_json::from_str(RAW_CONFIG).expect("config");
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("levels");
        let levels = pack
            .levels
            .into_iter()
            .take(LEGACY_LEVEL_COUNT)
            .filter(|l| !l.boss)
            .collect();
        (config, levels)
    }

    #[test]
    fn fold_reflect_bounces_between_walls() {
        assert_eq!(fold_reflect(5.0, 0.0, 10.0), 5.0);
        assert_eq!(fold_reflect(12.0, 0.0, 10.0), 8.0);
        assert_eq!(fold_reflect(-3.0, 0.0, 10.0), 3.0);
        assert_eq!(fold_reflect(23.0, 0.0, 10.0), 3.0);
    }

    #[test]
    fn median_uses_cleared_values_only_and_handles_empty() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
    }

    #[test]
    fn parallel_runs_are_deterministic() {
        let (config, levels) = legacy_non_boss();
        let few = &levels[..2];
        let a = simulate_levels(&config, few, &DEFAULT_SEEDS[..5]);
        let b = simulate_levels(&config, few, &DEFAULT_SEEDS[..5]);
        for (x, y) in a.iter().zip(&b) {
            for pi in 0..PRESETS.len() {
                let (rx, ry) = (&x.preset(pi).runs, &y.preset(pi).runs);
                assert_eq!(rx.len(), ry.len());
                for (p, q) in rx.iter().zip(ry) {
                    assert_eq!(p.seed, q.seed);
                    assert_eq!(p.cleared, q.cleared);
                    assert_eq!(p.balls_lost, q.balls_lost);
                    assert_eq!(p.time_sec.to_bits(), q.time_sec.to_bits());
                }
            }
        }
    }

    #[test]
    fn casual_bot_loses_balls_on_legacy_levels() {
        let (config, levels) = legacy_non_boss();
        let sums = simulate_levels(&config, &levels, &DEFAULT_SEEDS);
        casual_loss_gate(&sums).expect("the casual bot must be able to lose");
    }

    #[test]
    fn gate_rejects_a_bot_that_never_loses() {
        let (config, levels) = legacy_non_boss();
        let mut sums = simulate_levels(&config, &levels[..2], &DEFAULT_SEEDS[..5]);
        for s in &mut sums {
            for r in &mut s.casual.runs {
                r.balls_lost = 0;
            }
        }
        assert!(casual_loss_gate(&sums).is_err());
    }

    #[test]
    fn legacy_levels_sit_inside_their_calibrated_bands() {
        let (config, levels) = legacy_non_boss();
        let sums = simulate_levels(&config, &levels, &DEFAULT_SEEDS);
        let issues = fairness_issues(&sums);
        assert!(issues.is_empty(), "legacy levels out of band: {issues:#?}");
    }

    #[test]
    fn out_of_band_level_is_reported() {
        let (config, levels) = legacy_non_boss();
        let mut sums = simulate_levels(&config, &levels[..1], &DEFAULT_SEEDS[..5]);
        for r in &mut sums[0].good.runs {
            r.cleared = false;
        }
        sums[0].good.clear_rate = 0.0;
        sums[0].good.median_clear_time = None;
        assert!(!fairness_issues(&sums).is_empty());
    }
}
