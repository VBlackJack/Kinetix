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

//! CLI tool for validating, simulating, generating, writing, and inspecting the KINETIX level campaign.

pub mod bosses;
pub mod generate;
pub mod motifs;
pub mod simulate;
pub mod validate;

use generate::{
    difficulty_shape, generate_sector, hardness_for_rate, is_boss_sector, set_piece_theme,
    SectorError, SectorRequest, MAX_HARDNESS,
};
use kinetix_core::config::{GameConfig, LevelDef, LevelPack};
use simulate::{
    block_stats, campaign_curve_issues, casual_loss_gate, compute_world_stats, fairness_issues,
    simulate_levels, world_curve_issues, world_mean_good_clear, LevelSimSummary, DEFAULT_SEEDS,
    HARD_GOOD_CLEAR_RATE, PRESETS, WORLD_GOOD_TARGET,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};
use validate::{validate_candidate, validate_pack, LEGACY_LEVEL_COUNT, SECTORS_PER_WORLD};

const DEFAULT_LEVELS_PATH: &str = "assets/config/levels.json";
const DEFAULT_CONFIG_PATH: &str = "assets/config/game_config.json";
const DEFAULT_OUT_DIR: &str = "target/levelforge";
const DEFAULT_CSV_FILE: &str = "target/levelforge/simulation.csv";
const DEFAULT_CANDIDATE_FILE: &str = "target/levelforge/candidate-levels.json";
const DEFAULT_SEED: u64 = 0x2026_1108_CAFE_BABE;
/// Candidates generated per sector before `generate` gives up (no forced acceptance).
const GENERATE_ATTEMPT_BUDGET: u64 = 600;
/// Rule-valid candidates simulated together as one batch.
const GENERATE_BATCH_SIZE: usize = 8;
/// Batches simulated per sector at least, so the pick compares enough fair candidates.
const GENERATE_MIN_BATCHES: usize = 3;
/// Seed stride between two attempts of the same sector.
const GENERATE_ATTEMPT_SEED_STRIDE: u64 = 7919;
/// Hardness offsets tried in turn around the wanted hardness of a sector.
const ATTEMPT_HARDNESS_DELTAS: [isize; 5] = [0, 1, -1, 2, -2];
/// Lowest and highest good-bot clear rate a sector is asked to land near.
const DESIRED_RATE_MIN: f32 = 0.25;
const DESIRED_RATE_MAX: f32 = 0.875;
/// The generator aims this far below the target of a world, so its mean stays inside the
/// tolerance of the curve gate.
const MEAN_MARGIN: f32 = 0.015;
/// Sectors over which a deficit against the world target is paid back.
const CORRECTION_HORIZON: f32 = 4.0;
/// Rejection reasons listed per world in the generate report.
const REPORTED_REJECTIONS: usize = 6;
/// Boss sectors of a world.
const BOSS_SECTORS: [usize; 2] = [50, 100];
/// The per-level table is printed only for runs up to this many levels.
const MAX_LEVELS_IN_DETAIL_TABLE: usize = 60;
/// Worlds `generate` can build (the generator has eight world identities).
const MAX_GENERATED_WORLDS: usize = 8;
const MS_PER_SEC: f64 = 1000.0;

fn load_levels(path: &str) -> Result<(LevelPack, Duration), String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    let start = Instant::now();
    let pack = serde_json::from_str::<LevelPack>(&raw)
        .map_err(|e| format!("invalid JSON in {path}: {e}"))?;
    Ok((pack, start.elapsed()))
}

fn load_config(path: &str) -> Result<GameConfig, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    serde_json::from_str::<GameConfig>(&raw).map_err(|e| format!("invalid JSON in {path}: {e}"))
}

fn report_errors(errors: &[validate::ValidationError], context: &str) {
    eprintln!("FAIL: {} validation error(s) in {context}:", errors.len());
    for err in errors {
        eprintln!("  - {err}");
    }
}

fn cmd_validate(levels_path: &str) -> ExitCode {
    let pack = match load_levels(levels_path) {
        Ok((p, _)) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let errors = validate_pack(&pack.levels);
    if errors.is_empty() {
        println!(
            "OK: validated {} levels in {levels_path} with 0 errors.",
            pack.levels.len()
        );
        ExitCode::SUCCESS
    } else {
        report_errors(&errors, levels_path);
        ExitCode::FAILURE
    }
}

fn fmt_time(t: Option<f32>) -> String {
    t.map_or_else(|| "-".to_string(), |v| format!("{v:.1}"))
}

fn write_csv(summaries: &[LevelSimSummary]) {
    if let Err(e) = std::fs::create_dir_all(DEFAULT_OUT_DIR) {
        eprintln!("warning: could not create {DEFAULT_OUT_DIR}: {e}");
        return;
    }
    let mut csv = String::from(
        "level_id,world,name,boss,preset,clear_rate,median_clear_time_s,losses_per_run,sudden_death_share\n",
    );
    for s in summaries {
        for pi in 0..PRESETS.len() {
            let p = s.preset(pi);
            csv.push_str(&format!(
                "{},{},{},{},{},{:.3},{},{:.3},{:.3}\n",
                s.level_id,
                s.world,
                s.name,
                s.boss,
                p.preset.name,
                p.clear_rate,
                fmt_time(p.median_clear_time),
                p.losses_per_run,
                p.sudden_death_share
            ));
        }
    }
    if let Err(e) = std::fs::write(DEFAULT_CSV_FILE, csv) {
        eprintln!("warning: could not write {DEFAULT_CSV_FILE}: {e}");
    }
}

fn print_level_table(summaries: &[LevelSimSummary]) {
    println!("| Lvl | Name | Boss | Preset | Clear | Median (s) | Losses | SD share |");
    println!("|----:|:-----|:----:|:-------|------:|-----------:|-------:|---------:|");
    for s in summaries {
        for pi in 0..PRESETS.len() {
            let p = s.preset(pi);
            println!(
                "| {:>3} | {} | {} | {} | {:>5.3} | {:>10} | {:>6.3} | {:>8.3} |",
                s.level_id,
                s.name,
                if s.boss { "yes" } else { "no" },
                p.preset.name,
                p.clear_rate,
                fmt_time(p.median_clear_time),
                p.losses_per_run,
                p.sudden_death_share
            );
        }
    }
}

fn print_block_table(summaries: &[LevelSimSummary]) {
    println!("| Block | Levels | Good clear (mean) | Good median (s) | Casual clear (mean) |");
    println!("|:------|-------:|------------------:|----------------:|--------------------:|");
    for b in block_stats(summaries) {
        println!(
            "| {} | {:>6} | {:>16.1}% | {:>15} | {:>18.1}% |",
            b.label,
            b.levels,
            b.good_clear_rate * 100.0,
            fmt_time(b.good_median_clear_time),
            b.casual_clear_rate * 100.0
        );
    }
}

fn cmd_simulate(levels_path: &str, world_filter: Option<usize>) -> ExitCode {
    let pack = match load_levels(levels_path) {
        Ok((p, _)) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let config = match load_config(DEFAULT_CONFIG_PATH) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let levels_to_run: Vec<LevelDef> = pack
        .levels
        .iter()
        .filter(|l| world_filter.is_none_or(|w| l.world == w))
        .cloned()
        .collect();

    if levels_to_run.is_empty() {
        eprintln!("No levels matched filter {world_filter:?}.");
        return ExitCode::FAILURE;
    }

    let summaries = simulate_levels(&config, &levels_to_run, &DEFAULT_SEEDS);
    write_csv(&summaries);
    if levels_to_run.len() <= MAX_LEVELS_IN_DETAIL_TABLE {
        print_level_table(&summaries);
        println!();
    }
    print_world_stats_table(&levels_to_run, Some(&summaries));
    println!();
    print_block_table(&summaries);
    println!();

    let mut problems = fairness_issues(&summaries);
    if let Err(e) = casual_loss_gate(&summaries) {
        problems.push(e);
    }
    problems.extend(campaign_curve_issues(&summaries));
    if problems.is_empty() {
        println!("Fairness: OK ({} levels).", summaries.len());
        ExitCode::SUCCESS
    } else {
        eprintln!("FAIL: {} fairness issue(s) detected:", problems.len());
        for p in &problems {
            eprintln!("  - {p}");
        }
        ExitCode::FAILURE
    }
}

fn cmd_stats(levels_path: &str, with_sim: bool) -> ExitCode {
    let (pack, parse_time) = match load_levels(levels_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Parsed {} levels from {levels_path} in {:.2} ms.",
        pack.levels.len(),
        parse_time.as_secs_f64() * MS_PER_SEC
    );
    let summaries = if with_sim {
        let config = match load_config(DEFAULT_CONFIG_PATH) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        Some(simulate_levels(&config, &pack.levels, &DEFAULT_SEEDS))
    } else {
        None
    };

    print_world_stats_table(&pack.levels, summaries.as_deref());
    ExitCode::SUCCESS
}

fn print_world_stats_table(levels: &[LevelDef], sims: Option<&[LevelSimSummary]>) {
    let stats = compute_world_stats(levels, sims);
    println!(
        "| World | Sectors | Avg Bricks | Tough % | Special % | Preset | Clear Rate | Median Time (s) | Losses/Run | SD Share |"
    );
    println!(
        "|------:|--------:|-----------:|--------:|----------:|:-------|-----------:|----------------:|-----------:|---------:|"
    );
    for st in stats {
        if st.per_preset.is_empty() {
            println!(
                "| {:>5} | {:>7} | {:>10.1} | {:>6.1}% | {:>8.1}% | - | - | - | - | - |",
                st.world,
                st.sector_count,
                st.avg_bricks,
                st.tough_ratio * 100.0,
                st.special_ratio * 100.0,
            );
        }
        for p in &st.per_preset {
            println!(
                "| {:>5} | {:>7} | {:>10.1} | {:>6.1}% | {:>8.1}% | {} | {:>9.1}% | {:>15} | {:>10.2} | {:>7.1}% |",
                st.world,
                st.sector_count,
                st.avg_bricks,
                st.tough_ratio * 100.0,
                st.special_ratio * 100.0,
                p.preset,
                p.clear_rate * 100.0,
                fmt_time(p.median_clear_time),
                p.losses_per_run,
                p.sudden_death_share * 100.0
            );
        }
    }
}

/// One sector the generator has to fill.
#[derive(Clone, Copy)]
struct SectorSlot {
    id: usize,
    world: usize,
    sector_in_world: usize,
    is_last: bool,
    planned_total: usize,
}

/// What the generator wants from one regular sector.
#[derive(Clone, Copy)]
struct PickRequest {
    slot: SectorSlot,
    /// Good-bot clear rate the sector should land near.
    desired: f32,
    /// A hard candidate would make a run of too many hard sectors.
    forbid_hard: bool,
    seed: u64,
}

/// Counters of one world (or one sector): how many candidates were generated, how many passed
/// every gate, and why the others were rejected.
#[derive(Default)]
struct Tally {
    generated: usize,
    passed: usize,
    rejections: BTreeMap<String, usize>,
}

impl Tally {
    fn reject(&mut self, kind: &str, detail: &str) {
        let text = detail.split_once("]: ").map_or(detail, |(_, rest)| rest);
        let mut key = format!("{kind}: ");
        let mut last_was_digit = false;
        for ch in text.chars() {
            if ch.is_ascii_digit() {
                if !last_was_digit {
                    key.push('#');
                }
                last_was_digit = true;
            } else {
                key.push(ch);
                last_was_digit = false;
            }
        }
        *self.rejections.entry(key).or_default() += 1;
    }

    fn merge(&mut self, other: &Tally) {
        self.generated += other.generated;
        self.passed += other.passed;
        for (key, count) in &other.rejections {
            *self.rejections.entry(key.clone()).or_default() += count;
        }
    }

    /// The `limit` most frequent rejection reasons, as `count x reason` lines.
    fn top_reasons(&self, limit: usize) -> Vec<String> {
        let mut list: Vec<(&String, &usize)> = self.rejections.iter().collect();
        list.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        list.into_iter()
            .take(limit)
            .map(|(reason, count)| format!("{count} x {reason}"))
            .collect()
    }

    fn pass_rate(&self) -> f32 {
        if self.generated == 0 {
            0.0
        } else {
            self.passed as f32 / self.generated as f32
        }
    }
}

/// A candidate that passed every gate.
struct Chosen {
    level: LevelDef,
    names: BTreeSet<String>,
    summary: LevelSimSummary,
    hardness: usize,
}

/// Searches candidates for one regular sector: builds batches of rule-valid candidates,
/// simulates each batch, and keeps the fair one nearest to the wanted clear rate. Never accepts
/// a candidate that fails a gate; fails when the attempt budget runs out.
fn pick_regular(
    config: &GameConfig,
    prefix: &[LevelDef],
    used_names: &BTreeSet<String>,
    pick: &PickRequest,
    tally: &mut Tally,
) -> Result<Chosen, String> {
    let slot = pick.slot;
    let base_hardness = hardness_for_rate(pick.desired);
    let mut local = Tally::default();
    let mut attempts: u64 = 0;
    let mut best: Option<(f32, Chosen)> = None;
    let mut batches = 0usize;
    while (best.is_none() || batches < GENERATE_MIN_BATCHES) && attempts < GENERATE_ATTEMPT_BUDGET {
        let mut batch: Vec<(LevelDef, BTreeSet<String>, usize)> = Vec::new();
        while batch.len() < GENERATE_BATCH_SIZE && attempts < GENERATE_ATTEMPT_BUDGET {
            let attempt = attempts;
            attempts += 1;
            local.generated += 1;
            let delta = ATTEMPT_HARDNESS_DELTAS[attempt as usize % ATTEMPT_HARDNESS_DELTAS.len()];
            let hardness =
                (base_hardness as isize + delta).clamp(0, MAX_HARDNESS as isize) as usize;
            let request = SectorRequest {
                id: slot.id,
                world: slot.world,
                sector_in_world: slot.sector_in_world,
                is_last: slot.is_last,
                seed: pick
                    .seed
                    .wrapping_add(attempt.wrapping_mul(GENERATE_ATTEMPT_SEED_STRIDE)),
                hardness,
            };
            let mut names = used_names.clone();
            let level = match generate_sector(&request, &mut names) {
                Ok(level) => level,
                Err(SectorError::Name(message)) => return Err(message),
                Err(SectorError::Layout(message)) => {
                    local.reject("layout", &message);
                    continue;
                }
            };
            let errors = validate_candidate(prefix, &level, slot.is_last, slot.planned_total);
            if let Some(first) = errors.first() {
                local.reject("rule", &first.message);
                continue;
            }
            batch.push((level, names, hardness));
        }
        if batch.is_empty() {
            break;
        }
        let levels: Vec<LevelDef> = batch.iter().map(|(level, _, _)| level.clone()).collect();
        let sims = simulate_levels(config, &levels, &DEFAULT_SEEDS);
        batches += 1;
        for ((level, names, hardness), summary) in batch.into_iter().zip(sims) {
            let issues = fairness_issues(std::slice::from_ref(&summary));
            if let Some(first) = issues.first() {
                local.reject("fairness", first);
                continue;
            }
            let rate = summary.good.clear_rate;
            if pick.forbid_hard && rate <= HARD_GOOD_CLEAR_RATE {
                local.reject(
                    "hard run",
                    "hard candidate would extend a run of hard sectors",
                );
                continue;
            }
            local.passed += 1;
            let score = (rate - pick.desired).abs();
            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score < *best_score)
            {
                best = Some((
                    score,
                    Chosen {
                        level,
                        names,
                        summary,
                        hardness,
                    },
                ));
            }
        }
    }
    tally.merge(&local);
    best.map(|(_, chosen)| chosen).ok_or_else(|| {
        format!(
            "no candidate for level {} (world {}, sector {}) passed every gate after {attempts} attempts; top rejections: {}",
            slot.id,
            slot.world,
            slot.sector_in_world,
            local.top_reasons(REPORTED_REJECTIONS).join("; ")
        )
    })
}

/// Good-bot clear rate a regular sector should land near: the target of its world (minus a
/// safety margin) shaped by the role of the sector, corrected by what the world has produced so
/// far so the world mean converges on the target.
fn desired_rate(world: usize, sector_in_world: usize, world_sum: f32, world_count: usize) -> f32 {
    let target = WORLD_GOOD_TARGET[(world - 1).min(WORLD_GOOD_TARGET.len() - 1)] - MEAN_MARGIN;
    let nominal = target + difficulty_shape(world, sector_in_world);
    let deficit = target * world_count as f32 - world_sum;
    (nominal + deficit / CORRECTION_HORIZON).clamp(DESIRED_RATE_MIN, DESIRED_RATE_MAX)
}

/// Builds the boss levels of the first `max_world` worlds and simulates them once: the arenas
/// are fixed, so their clear rates steer the sectors around them.
fn presimulate_bosses(
    config: &GameConfig,
    max_world: usize,
) -> Result<BTreeMap<usize, LevelSimSummary>, String> {
    let mut bosses = Vec::new();
    for world in 1..=max_world {
        for sector in BOSS_SECTORS {
            let request = SectorRequest {
                id: (world - 1) * SECTORS_PER_WORLD + sector,
                world,
                sector_in_world: sector,
                is_last: false,
                seed: 0,
                hardness: 0,
            };
            let level = generate_sector(&request, &mut BTreeSet::new())
                .map_err(|e| format!("boss arena world {world} sector {sector}: {e}"))?;
            bosses.push(level);
        }
    }
    let sims = simulate_levels(config, &bosses, &DEFAULT_SEEDS);
    let issues = fairness_issues(&sims);
    if !issues.is_empty() {
        return Err(format!(
            "{} boss arena(s) fail the fairness gate, redesign them in bosses.rs:\n  - {}",
            issues.len(),
            issues.join("\n  - ")
        ));
    }
    Ok(sims.into_iter().map(|s| (s.level_id, s)).collect())
}

fn cmd_generate(
    levels_path: &str,
    max_world: usize,
    seed: u64,
    out_path: &str,
    verbose: bool,
) -> ExitCode {
    match generate_pack(levels_path, max_world, seed, out_path, verbose) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("FAIL: {message}");
            ExitCode::FAILURE
        }
    }
}

fn generate_pack(
    levels_path: &str,
    max_world: usize,
    seed: u64,
    out_path: &str,
    verbose: bool,
) -> Result<(), String> {
    if max_world == 0 || max_world > MAX_GENERATED_WORLDS {
        return Err(format!("--worlds must be in 1..={MAX_GENERATED_WORLDS}"));
    }
    let (mut pack, _) = load_levels(levels_path)?;
    let config = load_config(DEFAULT_CONFIG_PATH)?;

    // Generation always restarts from the legacy reference levels (indices 0..LEGACY_LEVEL_COUNT).
    let target_total = max_world * SECTORS_PER_WORLD;
    pack.levels.truncate(LEGACY_LEVEL_COUNT.min(target_total));
    // In a pack with generated levels the legacy reference levels all belong to world 1.
    for level in &mut pack.levels {
        level.world = 1;
    }

    let boss_sims = presimulate_bosses(&config, max_world)?;
    let mut used_names: BTreeSet<String> = pack.levels.iter().map(|l| l.name.clone()).collect();
    // Good clear rate and summary of every generated level so far, bosses included, in id order.
    let mut rates: Vec<f32> = Vec::new();
    let mut summaries: Vec<LevelSimSummary> = Vec::new();
    let mut world_tally = Tally::default();
    let mut world_sum = 0.0f32;
    let mut world_count = 0usize;

    for idx in pack.levels.len()..target_total {
        let slot = SectorSlot {
            id: idx + 1,
            world: idx / SECTORS_PER_WORLD + 1,
            sector_in_world: idx % SECTORS_PER_WORLD + 1,
            is_last: idx + 1 == target_total,
            planned_total: (idx / SECTORS_PER_WORLD + 1) * SECTORS_PER_WORLD,
        };

        let (level, names, summary, hardness, desired) = if is_boss_sector(slot.sector_in_world) {
            let request = SectorRequest {
                id: slot.id,
                world: slot.world,
                sector_in_world: slot.sector_in_world,
                is_last: slot.is_last,
                seed,
                hardness: 0,
            };
            let mut names = used_names.clone();
            let level = generate_sector(&request, &mut names).map_err(|e| e.to_string())?;
            let errors = validate_candidate(&pack.levels, &level, slot.is_last, slot.planned_total);
            if let Some(first) = errors.first() {
                return Err(format!("boss {}: {first}", level.name));
            }
            let summary = boss_sims
                .get(&slot.id)
                .cloned()
                .ok_or_else(|| format!("no simulation for boss level {}", slot.id))?;
            (level, names, summary, 0, f32::NAN)
        } else {
            let prev_hard = rates.last().is_some_and(|&r| r <= HARD_GOOD_CLEAR_RATE);
            let prev_two_hard =
                prev_hard && rates.len() >= 2 && rates[rates.len() - 2] <= HARD_GOOD_CLEAR_RATE;
            let next_boss_hard = is_boss_sector(slot.sector_in_world + 1)
                && boss_sims
                    .get(&(slot.id + 1))
                    .is_some_and(|b| b.good.clear_rate <= HARD_GOOD_CLEAR_RATE);
            let desired = desired_rate(slot.world, slot.sector_in_world, world_sum, world_count);
            let chosen = pick_regular(
                &config,
                &pack.levels,
                &used_names,
                &PickRequest {
                    slot,
                    desired,
                    forbid_hard: prev_two_hard || (prev_hard && next_boss_hard),
                    seed,
                },
                &mut world_tally,
            )?;
            world_sum += chosen.summary.good.clear_rate;
            world_count += 1;
            (
                chosen.level,
                chosen.names,
                chosen.summary,
                chosen.hardness,
                desired,
            )
        };

        if verbose {
            println!(
                "  sector {:>3} {:<24} hardness {hardness} good {:.3} desired {desired:.3}{}",
                slot.sector_in_world,
                level.name,
                summary.good.clear_rate,
                set_piece_theme(slot.world, slot.sector_in_world)
                    .map_or(String::new(), |t| format!(" [set piece: {t}]")),
            );
        }
        rates.push(summary.good.clear_rate);
        summaries.push(summary);
        used_names = names;
        pack.levels.push(level);

        if slot.sector_in_world == SECTORS_PER_WORLD {
            let world = slot.world;
            let mean = world_mean_good_clear(world, &summaries).unwrap_or(0.0);
            println!(
                "World {world}: {} candidates, {} passed every gate ({:.1}%), mean good clear {:.1}% (target {:.1}%)",
                world_tally.generated,
                world_tally.passed,
                world_tally.pass_rate() * 100.0,
                mean * 100.0,
                WORLD_GOOD_TARGET[(world - 1).min(WORLD_GOOD_TARGET.len() - 1)] * 100.0
            );
            for reason in world_tally.top_reasons(REPORTED_REJECTIONS) {
                println!("    rejected: {reason}");
            }
            let issues = world_curve_issues(world, &summaries);
            if !issues.is_empty() {
                return Err(format!(
                    "the difficulty curve of world {world} is out of range:\n  - {}",
                    issues.join("\n  - ")
                ));
            }
            world_tally = Tally::default();
            world_sum = 0.0;
            world_count = 0;
        }
    }

    let curve = campaign_curve_issues(&summaries);
    if !curve.is_empty() {
        return Err(format!(
            "the campaign difficulty curve is out of range:\n  - {}",
            curve.join("\n  - ")
        ));
    }
    let errors = validate_pack(&pack.levels);
    if !errors.is_empty() {
        report_errors(&errors, "the generated pack");
        return Err("the generated pack fails validation".to_string());
    }

    let json = serde_json::to_string_pretty(&pack)
        .map(|s| format!("{s}\n"))
        .map_err(|e| format!("error serializing pack: {e}"))?;
    if let Some(dir) = Path::new(out_path).parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("error creating {}: {e}", dir.display()))?;
    }
    std::fs::write(out_path, json).map_err(|e| format!("error writing {out_path}: {e}"))?;

    println!(
        "Generated {} sectors up to world {max_world} into candidate file {out_path}. Run `write` to install it.",
        pack.levels.len()
    );
    Ok(())
}

/// Installs a candidate file as the shipped level pack, after validating it.
fn cmd_write(candidate_path: &str, levels_path: &str) -> ExitCode {
    let raw = match std::fs::read_to_string(candidate_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to read {candidate_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let pack = match serde_json::from_str::<LevelPack>(&raw) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: invalid JSON in {candidate_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let errors = validate_pack(&pack.levels);
    if !errors.is_empty() {
        report_errors(&errors, candidate_path);
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(levels_path, raw) {
        eprintln!("error writing {levels_path}: {e}");
        return ExitCode::FAILURE;
    }
    println!(
        "Wrote {} levels from {candidate_path} to {levels_path}.",
        pack.levels.len()
    );
    ExitCode::SUCCESS
}

/// Usage text printed on any command line error.
const USAGE: &str = "usage:
  kinetix-levelforge validate [--levels <path>]
  kinetix-levelforge simulate [--levels <path>] [--world <n>]
  kinetix-levelforge stats [--levels <path>] [--sim]
  kinetix-levelforge generate --worlds <1..=8> [--seed <u64>] [--levels <path>] [--out <path>] [--verbose]
  kinetix-levelforge write [--candidate <path>] [--levels <path>]
defaults: --levels assets/config/levels.json, --out and --candidate target/levelforge/candidate-levels.json,
          --seed is a decimal u64 (built-in default when omitted)";

/// Strictly parsed flags of one command.
#[derive(Debug, Default)]
struct Flags {
    values: BTreeMap<String, String>,
    switches: BTreeSet<String>,
}

impl Flags {
    fn value(&self, flag: &str) -> Option<&str> {
        self.values.get(flag).map(String::as_str)
    }

    fn has(&self, flag: &str) -> bool {
        self.switches.contains(flag)
    }

    /// Value of `flag` parsed as `T`; `Ok(None)` when absent, `Err` when unparsable.
    fn parsed<T: std::str::FromStr>(&self, flag: &str) -> Result<Option<T>, String> {
        match self.value(flag) {
            None => Ok(None),
            Some(raw) => raw
                .parse::<T>()
                .map(Some)
                .map_err(|_| format!("invalid value {raw:?} for {flag}")),
        }
    }
}

/// Parses `args` against the flags that take a value and the boolean switches of a command.
/// Unknown flags, positional arguments, repeated flags and a missing value are errors.
fn parse_flags(args: &[String], valued: &[&str], switches: &[&str]) -> Result<Flags, String> {
    let mut flags = Flags::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if switches.contains(&arg.as_str()) {
            if !flags.switches.insert(arg.clone()) {
                return Err(format!("repeated flag {arg}"));
            }
        } else if valued.contains(&arg.as_str()) {
            let value = iter
                .next()
                .filter(|v| !v.starts_with("--"))
                .ok_or_else(|| format!("missing value for {arg}"))?;
            if flags.values.insert(arg.clone(), value.clone()).is_some() {
                return Err(format!("repeated flag {arg}"));
            }
        } else {
            return Err(format!("unknown argument {arg:?}"));
        }
    }
    Ok(flags)
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("error: {message}\n{USAGE}");
    ExitCode::FAILURE
}

/// Absolute, lexically normalized form of a path; canonical when the file exists.
fn normalized_path(path: &str) -> std::path::PathBuf {
    use std::path::{Component, PathBuf};
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    let raw = std::path::Path::new(path);
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(raw))
            .unwrap_or_else(|_| raw.to_path_buf())
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// True when both arguments name the same file (existing or not yet created).
fn same_path(a: &str, b: &str) -> bool {
    normalized_path(a) == normalized_path(b)
}

fn run(cmd: &str, rest: &[String]) -> Result<ExitCode, String> {
    match cmd {
        "validate" => {
            let flags = parse_flags(rest, &["--levels"], &[])?;
            Ok(cmd_validate(
                flags.value("--levels").unwrap_or(DEFAULT_LEVELS_PATH),
            ))
        }
        "simulate" => {
            let flags = parse_flags(rest, &["--levels", "--world"], &[])?;
            let world_filter = flags.parsed::<usize>("--world")?;
            Ok(cmd_simulate(
                flags.value("--levels").unwrap_or(DEFAULT_LEVELS_PATH),
                world_filter,
            ))
        }
        "stats" => {
            let flags = parse_flags(rest, &["--levels"], &["--sim"])?;
            Ok(cmd_stats(
                flags.value("--levels").unwrap_or(DEFAULT_LEVELS_PATH),
                flags.has("--sim"),
            ))
        }
        "generate" => {
            let flags = parse_flags(
                rest,
                &["--worlds", "--seed", "--levels", "--out"],
                &["--verbose"],
            )?;
            let max_world = flags.parsed::<usize>("--worlds")?.ok_or_else(|| {
                format!("generate requires --worlds <1..={MAX_GENERATED_WORLDS}>")
            })?;
            let seed = flags.parsed::<u64>("--seed")?.unwrap_or(DEFAULT_SEED);
            let levels_path = flags.value("--levels").unwrap_or(DEFAULT_LEVELS_PATH);
            let out_path = flags.value("--out").unwrap_or(DEFAULT_CANDIDATE_FILE);
            if same_path(levels_path, out_path) {
                return Err(format!(
                    "--out {out_path:?} is the levels file itself; the candidate must be a separate file (use `write` to publish it)"
                ));
            }
            Ok(cmd_generate(
                levels_path,
                max_world,
                seed,
                out_path,
                flags.has("--verbose"),
            ))
        }
        "write" => {
            let flags = parse_flags(rest, &["--candidate", "--levels"], &[])?;
            Ok(cmd_write(
                flags.value("--candidate").unwrap_or(DEFAULT_CANDIDATE_FILE),
                flags.value("--levels").unwrap_or(DEFAULT_LEVELS_PATH),
            ))
        }
        other => Err(format!(
            "unknown subcommand {other:?}; expected validate | simulate | stats | generate | write"
        )),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((cmd, rest)) => (cmd.as_str(), rest),
        None => ("validate", &[][..]),
    };
    match run(cmd, rest) {
        Ok(code) => code,
        Err(message) => usage_error(&message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_detects_equivalent_spellings() {
        assert!(same_path("a/b.json", "a/./b.json"));
        assert!(same_path("a/x/../b.json", "a/b.json"));
        assert!(!same_path("a/b.json", "a/c.json"));
    }

    #[test]
    fn generate_refuses_out_equal_to_levels() {
        let args: Vec<String> = [
            "--worlds",
            "1",
            "--levels",
            "x/l.json",
            "--out",
            "x/./l.json",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert!(run("generate", &args).is_err());
    }

    const RAW_LEVELS: &str = include_str!("../../../assets/config/levels.json");

    #[test]
    fn shipped_levels_json_passes_all_validation_rules() {
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("valid levels.json");
        let errors = validate_pack(&pack.levels);
        assert!(
            errors.is_empty(),
            "levels.json validation failed: {errors:#?}"
        );
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_flags_reads_values_and_switches() {
        let flags = parse_flags(
            &args(&["--worlds", "3", "--sim"]),
            &["--worlds", "--seed"],
            &["--sim"],
        )
        .expect("valid flags");
        assert_eq!(flags.value("--worlds"), Some("3"));
        assert_eq!(flags.value("--seed"), None);
        assert!(flags.has("--sim"));
        assert_eq!(flags.parsed::<usize>("--worlds"), Ok(Some(3)));
    }

    #[test]
    fn parse_flags_rejects_unknown_missing_repeated_and_unparsable() {
        let valued = ["--worlds", "--seed"];
        assert!(parse_flags(&args(&["--bogus"]), &valued, &[]).is_err());
        assert!(parse_flags(&args(&["positional"]), &valued, &[]).is_err());
        assert!(parse_flags(&args(&["--worlds"]), &valued, &[]).is_err());
        assert!(parse_flags(&args(&["--worlds", "--seed", "1"]), &valued, &[]).is_err());
        assert!(parse_flags(&args(&["--worlds", "1", "--worlds", "2"]), &valued, &[]).is_err());
        let flags = parse_flags(&args(&["--seed", "abc"]), &valued, &[]).expect("shape ok");
        assert!(flags.parsed::<u64>("--seed").is_err());
    }

    #[test]
    fn commands_reject_bad_command_lines() {
        assert!(run("generate", &[]).is_err());
        assert!(run("generate", &args(&["--worlds", "x"])).is_err());
        assert!(run("generate", &args(&["--worlds", "1", "--seed", "x"])).is_err());
        assert!(run("simulate", &args(&["--world", "x"])).is_err());
        assert!(run("validate", &args(&["--levels"])).is_err());
        assert!(run("nope", &[]).is_err());
    }
}
