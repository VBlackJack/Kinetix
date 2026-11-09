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

use crate::validate::{LEGACY_LEVEL_COUNT, SECTORS_PER_WORLD};
use kinetix_core::config::{GameConfig, LevelDef};
use kinetix_core::physics::{GameEvent, GameWorld, SessionStatus};
use std::collections::BTreeMap;
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
    // 0.35 half-widths: chosen in calibration on 2026-11-09 (the casual preset already
    // covers weak aim, so the good preset stays precise).
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
//   good:   clear rate 0.000..0.750, median clear time 13.7..40.9 s,
//           losses per run 0.125..1.375, sudden-death share 0.250..1.000
//   casual: clear rate 0.000..0.125, median clear time 17.9..33.2 s (4 of 12 levels had a
//           cleared run), losses per run 1.250..3.000, sudden-death share 0.000..0.875
// Re-measured the same day after the bot honoured its reaction delay right after the
// history reset and read ball data only from the delayed sample. The legacy level 13
// (SPECTRAL VAULT) is then no longer cleared by the good preset (0 of 8 seeds), hence the
// "no seed cleared" rule below applies to generated levels only.
// Re-measure and update these values whenever the physics, the bot or the legacy levels change.
pub const GOOD_BANDS: PresetBands = PresetBands {
    clear_rate: MetricBand {
        min: 0.0,
        max: 0.75,
    },
    median_clear_time: MetricBand {
        min: 13.7,
        max: 40.9,
    },
    losses_per_run: MetricBand {
        min: 0.125,
        max: 1.375,
    },
    sudden_death_share: MetricBand {
        min: 0.25,
        max: 1.0,
    },
};

pub const CASUAL_BANDS: PresetBands = PresetBands {
    clear_rate: MetricBand {
        min: 0.0,
        max: 0.125,
    },
    median_clear_time: MetricBand {
        min: 17.9,
        max: 33.2,
    },
    losses_per_run: MetricBand {
        min: 1.25,
        max: 3.0,
    },
    sudden_death_share: MetricBand {
        min: 0.0,
        max: 0.875,
    },
};

/// Relative margin added around every calibrated band (fraction of the bound).
pub const BAND_MARGIN_REL: f32 = 0.30;
/// Absolute margin added around the clear-rate and sudden-death-share bands (one seed of eight).
pub const BAND_MARGIN_RATE_ABS: f32 = 0.125;
/// Absolute margin added around the losses-per-run band (balls).
pub const BAND_MARGIN_LOSSES_ABS: f32 = 0.25;
/// Safety margin (levels) below the measured count for the casual loss gate.
const CASUAL_LOSING_MARGIN: usize = 2;
/// Non-boss legacy levels on which the casual preset lost balls, measured on 2026-11-09:
/// all 12 of the 12 (the gate counts the same population as the calibration).
const CASUAL_MEASURED_LOSING_LEGACY_LEVELS: usize = 12;
/// The `casual` preset must lose balls on at least this many legacy levels (measured count
/// minus a margin of 2), else the bot cannot lose, is rejected, and every other metric is
/// meaningless.
pub const CASUAL_MIN_LOSING_LEGACY_LEVELS: usize =
    CASUAL_MEASURED_LOSING_LEGACY_LEVELS - CASUAL_LOSING_MARGIN;

/// Generated non-boss levels: the good bot must clear at least this many seeds (of
/// `DEFAULT_SEEDS`) before sudden death. Bosses only need one clear (see `fairness_issues`).
pub const GENERATED_MIN_GOOD_CLEARS: usize = 2;
/// Generated non-boss levels: explicit cap on the good bot's sudden-death share. It is not
/// derived from the calibrated band, whose widened upper bound (1.425) can never be exceeded
/// by a share that is at most 1.0.
pub const GENERATED_MAX_GOOD_SUDDEN_DEATH_SHARE: f32 = 0.875;

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
/// Bricks and boss are read from the current world state, not delayed: they change slowly and
/// only the ball is subject to the reaction delay.
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

/// x of the lowest falling bonus capsule worth catching, when the (delayed) tracked ball is
/// not close to the paddle. The ball data comes from the delayed sample; capsule positions are
/// read from the current world state, a deliberate approximation (capsules fall slowly).
fn capsule_target(world: &GameWorld, seen: BallSample) -> Option<f32> {
    let safe = seen.vy < 0.0 || seen.y < CAPSULE_SAFE_BALL_Y;
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
                // Hold the paddle until a sample at least `delay_steps` old exists, so the
                // reaction delay also applies right after the history was cleared.
                if history.len() > delay_steps {
                    let seen = history[0];
                    let want = match capsule_target(&world, seen) {
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
            match h.join() {
                Ok(done) => {
                    for (slot, res) in done {
                        slots[slot] = Some(res);
                    }
                }
                // A panicked worker must not silently drop its runs.
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }
    });
    assert!(
        slots.iter().all(Option::is_some),
        "every simulation slot must be filled"
    );

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
                debug_assert_eq!(runs.len(), seeds.len());
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

/// Which side of a calibrated band a check enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BandSide {
    /// Both bounds (legacy levels, which are the calibration).
    Both,
    /// Only the bound that makes a level too hard (generated levels): easy levels are allowed.
    HardOnly,
}

/// Which bound of a metric band is the "too hard" one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardBound {
    /// A value below the lower bound is too hard (clear rate).
    Lower,
    /// A value above the upper bound is too hard (clear time, losses, sudden-death share).
    Upper,
}

/// True for levels whose fairness is judged one-sidedly (the generated levels).
fn is_generated_level(level_id: usize) -> bool {
    level_id > LEGACY_LEVEL_COUNT
}

/// One metric of one level to compare against its calibrated band.
struct MetricCheck<'a> {
    who: &'a str,
    metric: &'a str,
    value: f32,
    band: MetricBand,
    abs_margin: f32,
    side: BandSide,
    hard: HardBound,
}

fn check_metric(issues: &mut Vec<String>, params: MetricCheck<'_>) {
    let MetricCheck {
        who,
        metric,
        value,
        band,
        abs_margin,
        side,
        hard,
    } = params;
    let (lo, hi) = band_bounds(band, abs_margin);
    let enforce_lower = side == BandSide::Both || hard == HardBound::Lower;
    let enforce_upper = side == BandSide::Both || hard == HardBound::Upper;
    let too_low = value < lo && enforce_lower;
    let too_high = value > hi && enforce_upper;
    if too_low || too_high {
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
    let generated = is_generated_level(level.level_id);
    let is_good = sum.preset.name == PRESET_GOOD.name;
    // Generated levels are judged on the good bot's hard-side limits only: easy teaching
    // levels and breathers are allowed, so the casual preset is not checked at all.
    if generated && !is_good {
        return;
    }
    let side = if generated {
        BandSide::HardOnly
    } else {
        BandSide::Both
    };
    if is_good && generated {
        let clears = sum.runs.iter().filter(|r| r.cleared).count();
        if clears < GENERATED_MIN_GOOD_CLEARS {
            issues.push(format!(
                "{who}: cleared {clears} of {} seeds before sudden death, expected at least {GENERATED_MIN_GOOD_CLEARS}",
                sum.runs.len()
            ));
        }
        if sum.sudden_death_share > GENERATED_MAX_GOOD_SUDDEN_DEATH_SHARE {
            issues.push(format!(
                "{who}: sudden-death share {:.2} above the cap {GENERATED_MAX_GOOD_SUDDEN_DEATH_SHARE:.3}",
                sum.sudden_death_share
            ));
        }
    }
    check_metric(
        issues,
        MetricCheck {
            who: &who,
            metric: "clear rate",
            value: sum.clear_rate,
            band: bands.clear_rate,
            abs_margin: BAND_MARGIN_RATE_ABS,
            side,
            hard: HardBound::Lower,
        },
    );
    // The casual preset legitimately clears nothing on some levels, so a missing median
    // is only a problem through the clear-rate checks above.
    if let Some(t) = sum.median_clear_time {
        check_metric(
            issues,
            MetricCheck {
                who: &who,
                metric: "median clear time",
                value: t,
                band: bands.median_clear_time,
                abs_margin: 0.0,
                side,
                hard: HardBound::Upper,
            },
        );
    }
    check_metric(
        issues,
        MetricCheck {
            who: &who,
            metric: "losses per run",
            value: sum.losses_per_run,
            band: bands.losses_per_run,
            abs_margin: BAND_MARGIN_LOSSES_ABS,
            side,
            hard: HardBound::Upper,
        },
    );
    // Generated levels use the explicit cap above instead of the band-derived bound.
    if !generated {
        check_metric(
            issues,
            MetricCheck {
                who: &who,
                metric: "sudden-death share",
                value: sum.sudden_death_share,
                band: bands.sudden_death_share,
                abs_margin: BAND_MARGIN_RATE_ABS,
                side,
                hard: HardBound::Upper,
            },
        );
    }
}

/// Fairness issues of a simulated set. Legacy regular levels: the full two-sided calibrated
/// bands for both presets. Generated regular levels (id > `LEGACY_LEVEL_COUNT`): "not too hard"
/// only, i.e. the good bot clears at least `GENERATED_MIN_GOOD_CLEARS` seeds before sudden
/// death and stays inside the hard-side limits (clear rate lower bound, median clear time
/// upper bound, losses upper bound) plus the explicit `GENERATED_MAX_GOOD_SUDDEN_DEATH_SHARE`
/// cap; the casual preset is not checked. Bosses: the good bot clears at least one seed.
/// Empty means fair.
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
/// cannot lose and every other metric is meaningless. The measured threshold only holds for the
/// full calibration population, so the gate is applied only when all
/// `CASUAL_MEASURED_LOSING_LEGACY_LEVELS` non-boss legacy levels are in the run; partial runs
/// (for example `simulate --world N`) skip it.
pub fn casual_loss_gate(summaries: &[LevelSimSummary]) -> Result<(), String> {
    let legacy: Vec<&LevelSimSummary> = summaries
        .iter()
        .filter(|s| s.level_id >= 1 && s.level_id <= LEGACY_LEVEL_COUNT && !s.boss)
        .collect();
    if legacy.len() < CASUAL_MEASURED_LOSING_LEGACY_LEVELS {
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

/// Sectors per reporting block of the per-block table.
pub const SECTORS_PER_BLOCK: usize = 10;
/// Label of the legacy prologue row of the per-block table.
pub const PROLOGUE_BLOCK_LABEL: &str = "prologue";

/// Aggregates of one block of sectors (the legacy prologue, or 10 sectors of one world).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockStats {
    pub label: String,
    pub levels: usize,
    /// Mean of the per-level good-bot clear rates.
    pub good_clear_rate: f32,
    /// Median of the per-level good-bot median clear times (levels never cleared are skipped).
    pub good_median_clear_time: Option<f32>,
    /// Mean of the per-level casual-bot clear rates.
    pub casual_clear_rate: f32,
}

/// Block key ordering the prologue first, then by world and block index.
fn block_key(level: &LevelSimSummary) -> (usize, usize, usize) {
    if level.level_id <= LEGACY_LEVEL_COUNT {
        (0, 0, 0)
    } else {
        let sector = (level.level_id - 1) % SECTORS_PER_WORLD;
        (1, level.world, sector / SECTORS_PER_BLOCK)
    }
}

/// Per-block statistics, deterministic and ordered: legacy prologue first (levels 1 to
/// `LEGACY_LEVEL_COUNT`), then blocks of `SECTORS_PER_BLOCK` sectors per world.
pub fn block_stats(summaries: &[LevelSimSummary]) -> Vec<BlockStats> {
    let mut groups: BTreeMap<(usize, usize, usize), Vec<&LevelSimSummary>> = BTreeMap::new();
    for s in summaries {
        groups.entry(block_key(s)).or_default().push(s);
    }
    groups
        .into_iter()
        .map(|(key, members)| {
            let label = if key.0 == 0 {
                PROLOGUE_BLOCK_LABEL.to_string()
            } else {
                let first = key.2 * SECTORS_PER_BLOCK + 1;
                format!("W{} S{}-{}", key.1, first, first + SECTORS_PER_BLOCK - 1)
            };
            let count = members.len() as f32;
            let medians: Vec<f32> = members
                .iter()
                .filter_map(|m| m.good.median_clear_time)
                .collect();
            BlockStats {
                label,
                levels: members.len(),
                good_clear_rate: members.iter().map(|m| m.good.clear_rate).sum::<f32>() / count,
                good_median_clear_time: median(&medians),
                casual_clear_rate: members.iter().map(|m| m.casual.clear_rate).sum::<f32>() / count,
            }
        })
        .collect()
}

/// A generated sector whose good-bot clear rate is at or below this value is "hard".
pub const HARD_GOOD_CLEAR_RATE: f32 = 0.375;
/// Target mean good-bot clear rate of the regular generated levels of each world (the
/// campaign gets harder world after world).
pub const WORLD_GOOD_TARGET: [f32; 8] = [0.50, 0.48, 0.46, 0.44, 0.42, 0.40, 0.39, 0.38];
/// A world mean may exceed its target by this much.
pub const WORLD_GOOD_TOLERANCE: f32 = 0.03;
/// A world mean may exceed the previous world mean by this much (non-increasing curve).
pub const WORLD_MEAN_STEP_TOLERANCE: f32 = 0.02;
/// The first generated world mean must exceed the last world mean by at least this much.
pub const MIN_CAMPAIGN_DESCENT: f32 = 0.06;
/// Longest allowed run of consecutive hard sectors (bosses counted).
pub const MAX_CONSECUTIVE_HARD_SECTORS: usize = 2;
/// Number of trailing levels of the previous world fed to the hard-run scan, so a run crossing
/// a world boundary is caught.
const HARD_RUN_LOOKBACK: usize = MAX_CONSECUTIVE_HARD_SECTORS;

/// Whether a generated level's good-bot clear rate makes it a hard sector.
fn is_hard(summary: &LevelSimSummary) -> bool {
    summary.good.clear_rate <= HARD_GOOD_CLEAR_RATE
}

/// World a generated level id belongs to (`world = index / SECTORS_PER_WORLD + 1`).
fn world_of_id(level_id: usize) -> usize {
    (level_id - 1) / SECTORS_PER_WORLD + 1
}

/// Generated levels of `world` (ids beyond the legacy pack), ordered by id.
fn generated_in_world(world: usize, summaries: &[LevelSimSummary]) -> Vec<&LevelSimSummary> {
    let mut list: Vec<&LevelSimSummary> = summaries
        .iter()
        .filter(|s| s.level_id > LEGACY_LEVEL_COUNT && world_of_id(s.level_id) == world)
        .collect();
    list.sort_by_key(|s| s.level_id);
    list
}

/// Mean good-bot clear rate over the regular generated levels of `world`, `None` when it has none.
pub fn world_mean_good_clear(world: usize, summaries: &[LevelSimSummary]) -> Option<f32> {
    let rates: Vec<f32> = generated_in_world(world, summaries)
        .into_iter()
        .filter(|s| !s.boss)
        .map(|s| s.good.clear_rate)
        .collect();
    if rates.is_empty() {
        None
    } else {
        Some(rates.iter().sum::<f32>() / rates.len() as f32)
    }
}

/// Whether every generated sector of `world` is present in `summaries`.
pub fn is_world_complete(world: usize, summaries: &[LevelSimSummary]) -> bool {
    if world == 0 {
        return false;
    }
    let start = ((world - 1) * SECTORS_PER_WORLD).max(LEGACY_LEVEL_COUNT);
    let end = world * SECTORS_PER_WORLD;
    let present = generated_in_world(world, summaries).len();
    present > 0 && present == end - start
}

/// Target mean of a world; worlds past the table use the last entry.
fn world_target(world: usize) -> f32 {
    WORLD_GOOD_TARGET
        .get(world - 1)
        .copied()
        .unwrap_or(WORLD_GOOD_TARGET[WORLD_GOOD_TARGET.len() - 1])
}

/// Difficulty-curve issues of one world: its mean against the target table, its mean against the
/// previous world's mean (when that world is in `summaries`), and runs of consecutive hard
/// sectors (bosses counted) ending in this world. `summaries` may hold the whole pack so far;
/// only levels of `world` (and the previous world's trailing levels) are inspected. Meant to
/// be called by a generator at each world boundary, and by [`campaign_curve_issues`].
pub fn world_curve_issues(world: usize, summaries: &[LevelSimSummary]) -> Vec<String> {
    let mut issues = Vec::new();
    if world == 0 {
        return issues;
    }
    if let Some(mean) = world_mean_good_clear(world, summaries) {
        let target = world_target(world);
        if mean > target + WORLD_GOOD_TOLERANCE {
            issues.push(format!(
                "world {world}: mean good clear rate {:.1}% exceeds the target {:.1}% by more than {:.1} points",
                mean * 100.0,
                target * 100.0,
                WORLD_GOOD_TOLERANCE * 100.0
            ));
        }
        if world > 1 {
            if let Some(prev) = world_mean_good_clear(world - 1, summaries) {
                if mean > prev + WORLD_MEAN_STEP_TOLERANCE {
                    issues.push(format!(
                        "world {world}: mean good clear rate {:.1}% rises above world {} ({:.1}%) by more than {:.1} points",
                        mean * 100.0,
                        world - 1,
                        prev * 100.0,
                        WORLD_MEAN_STEP_TOLERANCE * 100.0
                    ));
                }
            }
        }
    }

    let mut sequence: Vec<&LevelSimSummary> = Vec::new();
    if world > 1 {
        let previous = generated_in_world(world - 1, summaries);
        let skip = previous.len().saturating_sub(HARD_RUN_LOOKBACK);
        sequence.extend(previous.into_iter().skip(skip));
    }
    let lookback_len = sequence.len();
    sequence.extend(generated_in_world(world, summaries));
    let mut run = 0usize;
    let mut last_id = 0usize;
    for (i, s) in sequence.iter().enumerate() {
        if last_id != 0 && s.level_id != last_id + 1 {
            run = 0;
        }
        last_id = s.level_id;
        if is_hard(s) {
            run += 1;
            if run == MAX_CONSECUTIVE_HARD_SECTORS + 1 && i >= lookback_len {
                issues.push(format!(
                    "world {world}: more than {MAX_CONSECUTIVE_HARD_SECTORS} consecutive hard sectors ending at level {} ({})",
                    s.level_id, s.name
                ));
            }
        } else {
            run = 0;
        }
    }
    issues
}

/// Campaign difficulty-curve issues over the complete worlds of `summaries` (legacy levels are
/// ignored; incomplete worlds are skipped, so partial runs and early generation stay quiet):
/// per-world target, non-increasing world means within tolerance, no run of three hard sectors,
/// and, once all `WORLD_GOOD_TARGET` worlds are complete, a first-to-last descent of at least
/// `MIN_CAMPAIGN_DESCENT`. Empty means the curve is acceptable.
pub fn campaign_curve_issues(summaries: &[LevelSimSummary]) -> Vec<String> {
    let mut issues = Vec::new();
    let worlds: Vec<usize> = (1..=WORLD_GOOD_TARGET.len())
        .filter(|&w| is_world_complete(w, summaries))
        .collect();
    for &world in &worlds {
        issues.extend(world_curve_issues(world, summaries));
    }
    if worlds.len() == WORLD_GOOD_TARGET.len() {
        let first_world = worlds[0];
        let last_world = worlds[worlds.len() - 1];
        if let (Some(first), Some(last)) = (
            world_mean_good_clear(first_world, summaries),
            world_mean_good_clear(last_world, summaries),
        ) {
            if first - last < MIN_CAMPAIGN_DESCENT {
                issues.push(format!(
                    "campaign descent {:.1} points from world {first_world} ({:.1}%) to world {last_world} ({:.1}%) is below the minimum {:.1}",
                    (first - last) * 100.0,
                    first * 100.0,
                    last * 100.0,
                    MIN_CAMPAIGN_DESCENT * 100.0
                ));
            }
        }
    }
    issues
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

    /// Losses per run far above any calibrated band.
    const OUT_OF_BAND_LOSSES: f32 = 50.0;

    #[test]
    fn casual_bot_loses_balls_on_legacy_levels() {
        let (config, levels) = legacy_non_boss();
        let sums = simulate_levels(&config, &levels, &DEFAULT_SEEDS);
        casual_loss_gate(&sums).expect("the casual bot must be able to lose");
    }

    #[test]
    fn gate_rejects_a_bot_that_never_loses() {
        let (config, levels) = legacy_non_boss();
        let mut sums = simulate_levels(&config, &levels, &DEFAULT_SEEDS[..2]);
        for s in &mut sums {
            for r in &mut s.casual.runs {
                r.balls_lost = 0;
            }
        }
        assert!(casual_loss_gate(&sums).is_err());
        // A partial run (fewer than all legacy non-boss levels) skips the gate.
        sums.truncate(CASUAL_MEASURED_LOSING_LEGACY_LEVELS - 1);
        assert!(casual_loss_gate(&sums).is_ok());
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
        sums[0].good.losses_per_run = OUT_OF_BAND_LOSSES;
        assert!(!fairness_issues(&sums).is_empty());
    }

    #[test]
    fn generated_level_must_be_cleared_by_the_good_bot() {
        let (config, levels) = legacy_non_boss();
        let mut sums = simulate_levels(&config, &levels[..1], &DEFAULT_SEEDS[..5]);
        for r in &mut sums[0].good.runs {
            r.cleared = false;
        }
        sums[0].good.clear_rate = 0.0;
        sums[0].good.median_clear_time = None;
        sums[0].level_id = LEGACY_LEVEL_COUNT + 1;
        assert!(fairness_issues(&sums)
            .iter()
            .any(|i| i.contains("cleared 0 of")));
    }

    /// Builds a summary of a generated level from the first legacy level with chosen good-bot
    /// metrics, to exercise the one-sided limits. Exactly `clears` of the 8 seeds are cleared.
    fn generated_summary(
        clears: usize,
        clear_rate: f32,
        median: Option<f32>,
        losses: f32,
        sd_share: f32,
    ) -> Vec<LevelSimSummary> {
        let (config, levels) = legacy_non_boss();
        let mut sums = simulate_levels(&config, &levels[..1], &DEFAULT_SEEDS);
        let s = &mut sums[0];
        s.level_id = LEGACY_LEVEL_COUNT + 1;
        for (i, r) in s.good.runs.iter_mut().enumerate() {
            r.cleared = i < clears;
        }
        s.good.clear_rate = clear_rate;
        s.good.median_clear_time = median;
        s.good.losses_per_run = losses;
        s.good.sudden_death_share = sd_share;
        sums
    }

    #[test]
    fn easy_generated_level_cleared_quickly_by_the_good_bot_passes() {
        // Far outside the legacy two-sided bands on the easy side: full clear rate, very short
        // median, no losses, no sudden death; the casual bot never loses either.
        let mut sums = generated_summary(8, 1.0, Some(3.0), 0.0, 0.0);
        let c = &mut sums[0].casual;
        c.clear_rate = 1.0;
        c.median_clear_time = Some(3.0);
        c.losses_per_run = 0.0;
        c.sudden_death_share = 0.0;
        for r in &mut c.runs {
            r.balls_lost = 0;
        }
        assert_eq!(fairness_issues(&sums), Vec::<String>::new());
        // The same numbers on a legacy level still fail the two-sided bands.
        sums[0].level_id = 1;
        assert!(!fairness_issues(&sums).is_empty());
    }

    #[test]
    fn too_hard_generated_level_fails_on_each_hard_side_limit() {
        let ok = (4, 0.5, Some(20.0), 0.5, 0.5);
        let build = |c: usize, r: f32, m: Option<f32>, l: f32, d: f32| {
            fairness_issues(&generated_summary(c, r, m, l, d))
        };
        assert!(build(ok.0, ok.1, ok.2, ok.3, ok.4).is_empty());
        // Each hard-side limit fails on its own while the others stay reachable and fine.
        assert!(build(ok.0, ok.1, Some(60.0), ok.3, ok.4)
            .iter()
            .any(|i| i.contains("median clear time")));
        assert!(build(ok.0, ok.1, ok.2, 5.0, ok.4)
            .iter()
            .any(|i| i.contains("losses per run")));
        assert!(build(ok.0, ok.1, ok.2, ok.3, 1.0)
            .iter()
            .any(|i| i.contains("sudden-death share")));
        assert!(build(
            ok.0,
            ok.1,
            ok.2,
            ok.3,
            GENERATED_MAX_GOOD_SUDDEN_DEATH_SHARE
        )
        .is_empty());
        assert!(build(ok.0, -1.0, ok.2, ok.3, ok.4)
            .iter()
            .any(|i| i.contains("clear rate")));
        assert!(build(0, 0.0, None, ok.3, ok.4)
            .iter()
            .any(|i| i.contains("cleared 0 of")));
    }

    #[test]
    fn generated_level_needs_two_of_eight_clears_but_a_boss_needs_one() {
        let one = fairness_issues(&generated_summary(1, 0.125, Some(20.0), 0.5, 0.5));
        assert!(one.iter().any(|i| i.contains("cleared 1 of 8")), "{one:?}");
        let two = fairness_issues(&generated_summary(
            GENERATED_MIN_GOOD_CLEARS,
            0.25,
            Some(20.0),
            0.5,
            0.5,
        ));
        assert!(two.is_empty(), "{two:?}");
        let mut boss = generated_summary(1, 0.125, Some(20.0), 0.5, 0.5);
        boss[0].boss = true;
        assert!(fairness_issues(&boss).is_empty());
    }

    #[test]
    fn block_stats_group_prologue_and_ten_sector_blocks() {
        let (config, levels) = legacy_non_boss();
        let mut sums = simulate_levels(&config, &levels[..1], &DEFAULT_SEEDS[..2]);
        let template = sums[0].clone();
        let mut put = |id: usize, world: usize, good: f32, median: Option<f32>, casual: f32| {
            let mut s = template.clone();
            s.level_id = id;
            s.world = world;
            s.good.clear_rate = good;
            s.good.median_clear_time = median;
            s.casual.clear_rate = casual;
            sums.push(s);
        };
        put(16, 1, 1.0, Some(10.0), 0.5);
        put(20, 1, 0.5, Some(20.0), 0.0);
        put(21, 1, 0.0, None, 0.0);
        put(101, 2, 1.0, Some(8.0), 1.0);
        let blocks = block_stats(&sums);
        let labels: Vec<&str> = blocks.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["prologue", "W1 S11-20", "W1 S21-30", "W2 S1-10"]);
        assert_eq!(blocks[0].levels, 1);
        assert_eq!(blocks[1].levels, 2);
        assert!((blocks[1].good_clear_rate - 0.75).abs() < f32::EPSILON);
        assert_eq!(blocks[1].good_median_clear_time, Some(15.0));
        assert!((blocks[1].casual_clear_rate - 0.25).abs() < f32::EPSILON);
        assert_eq!(blocks[2].good_median_clear_time, None);
        assert_eq!(block_stats(&sums), blocks);
    }

    /// Builds summaries for the given worlds from a template; `rate(id)` gives the good clear
    /// rate of the generated level `id`, `boss(id)` flags bosses.
    fn curve_summaries(
        worlds: std::ops::RangeInclusive<usize>,
        rate: impl Fn(usize) -> f32,
        boss: impl Fn(usize) -> bool,
    ) -> Vec<LevelSimSummary> {
        let (config, levels) = legacy_non_boss();
        let template = simulate_levels(&config, &levels[..1], &DEFAULT_SEEDS[..1])
            .pop()
            .expect("one summary");
        let mut out = Vec::new();
        for world in worlds {
            let start = ((world - 1) * SECTORS_PER_WORLD).max(LEGACY_LEVEL_COUNT) + 1;
            for id in start..=world * SECTORS_PER_WORLD {
                let mut s = template.clone();
                s.level_id = id;
                s.world = world;
                s.boss = boss(id);
                s.good.clear_rate = rate(id);
                out.push(s);
            }
        }
        out
    }

    /// Exactly the target of the world of generated level `id`.
    fn on_target(id: usize) -> f32 {
        WORLD_GOOD_TARGET[world_of_id(id) - 1]
    }

    #[test]
    fn campaign_curve_accepts_a_descending_full_campaign() {
        let sums = curve_summaries(1..=8, on_target, |_| false);
        let issues = campaign_curve_issues(&sums);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn campaign_curve_rejects_a_world_mean_above_its_target() {
        let sums = curve_summaries(
            1..=2,
            |id| {
                if id > SECTORS_PER_WORLD {
                    0.60
                } else {
                    on_target(id)
                }
            },
            |_| false,
        );
        let issues = campaign_curve_issues(&sums);
        assert!(
            issues.iter().any(|i| i.contains("exceeds the target")),
            "{issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.contains("rises above world 1")),
            "{issues:?}"
        );
    }

    #[test]
    fn campaign_curve_rejects_a_flat_campaign() {
        let sums = curve_summaries(1..=8, |_| 0.45, |_| false);
        let issues = campaign_curve_issues(&sums);
        assert!(
            issues.iter().any(|i| i.contains("campaign descent")),
            "{issues:?}"
        );
    }

    #[test]
    fn campaign_curve_skips_incomplete_worlds() {
        let mut sums = curve_summaries(1..=1, on_target, |_| false);
        sums.truncate(sums.len() - 1);
        for s in &mut sums {
            s.good.clear_rate = 1.0;
        }
        assert!(campaign_curve_issues(&sums).is_empty());
        assert!(campaign_curve_issues(&[]).is_empty());
    }

    #[test]
    fn three_consecutive_hard_sectors_are_rejected_and_bosses_count() {
        let rate = |id: usize| {
            if (30..=32).contains(&id) {
                HARD_GOOD_CLEAR_RATE
            } else {
                on_target(id)
            }
        };
        let sums = curve_summaries(1..=1, rate, |_| false);
        let issues = world_curve_issues(1, &sums);
        assert!(
            issues.iter().any(|i| i.contains("consecutive hard")),
            "{issues:?}"
        );

        // Two hard sectors are fine.
        let two = |id: usize| {
            if (30..=31).contains(&id) {
                HARD_GOOD_CLEAR_RATE
            } else {
                on_target(id)
            }
        };
        let sums = curve_summaries(1..=1, two, |_| false);
        assert!(world_curve_issues(1, &sums).is_empty());

        // A hard boss between two hard sectors counts toward the run.
        let sums = curve_summaries(1..=1, rate, |id| id == 31);
        assert!(world_curve_issues(1, &sums)
            .iter()
            .any(|i| i.contains("consecutive hard")));
    }

    #[test]
    fn hard_run_crossing_a_world_boundary_is_rejected() {
        let rate = |id: usize| {
            if (99..=101).contains(&id) {
                HARD_GOOD_CLEAR_RATE
            } else {
                on_target(id)
            }
        };
        let sums = curve_summaries(1..=2, rate, |_| false);
        let issues = campaign_curve_issues(&sums);
        assert!(
            issues.iter().any(|i| i.contains("consecutive hard")),
            "{issues:?}"
        );
    }

    #[test]
    fn bosses_do_not_move_the_world_mean() {
        let mut sums = curve_summaries(1..=1, on_target, |id| id == 50);
        for s in &mut sums {
            if s.boss {
                s.good.clear_rate = 0.0;
            }
        }
        let mean = world_mean_good_clear(1, &sums).expect("mean");
        assert!(
            (mean - on_target(LEGACY_LEVEL_COUNT + 1)).abs() < 1e-4,
            "{mean}"
        );
    }
}
