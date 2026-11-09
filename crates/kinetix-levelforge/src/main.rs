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

use generate::generate_sector;
use kinetix_core::config::{GameConfig, LevelDef, LevelPack};
use simulate::{
    block_stats, casual_loss_gate, compute_world_stats, fairness_issues, simulate_levels,
    LevelSimSummary, DEFAULT_SEEDS, PRESETS,
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
/// Candidates tried per sector before `generate` gives up (no forced acceptance).
const GENERATE_ATTEMPT_BUDGET: u64 = 25;
/// Seed stride between two attempts of the same sector.
const GENERATE_ATTEMPT_SEED_STRIDE: u64 = 7919;
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

/// Why a generated candidate is not acceptable (empty means accepted): the per-level rules of
/// the pack validation (reachability, portals, density, name, similarity, asymmetry share
/// feasibility) applied to `prefix` plus the candidate, then the bot fairness bands.
fn candidate_issues(
    config: &GameConfig,
    prefix: &[LevelDef],
    candidate: &LevelDef,
    is_last: bool,
    planned_total: usize,
) -> Vec<String> {
    let rule_errors = validate_candidate(prefix, candidate, is_last, planned_total);
    if !rule_errors.is_empty() {
        return rule_errors.iter().map(ToString::to_string).collect();
    }
    let sims = simulate_levels(config, std::slice::from_ref(candidate), &DEFAULT_SEEDS);
    fairness_issues(&sims)
}

fn cmd_generate(levels_path: &str, max_world: usize, seed: u64, out_path: &str) -> ExitCode {
    if max_world == 0 || max_world > MAX_GENERATED_WORLDS {
        eprintln!("error: --worlds must be in 1..={MAX_GENERATED_WORLDS}");
        return ExitCode::FAILURE;
    }
    let (mut pack, _) = match load_levels(levels_path) {
        Ok(p) => p,
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

    // Generation always restarts from the legacy reference levels (indices 0..LEGACY_LEVEL_COUNT).
    let target_total = max_world * SECTORS_PER_WORLD;
    pack.levels.truncate(LEGACY_LEVEL_COUNT.min(target_total));
    // In a pack with generated levels the legacy reference levels all belong to world 1.
    for level in &mut pack.levels {
        level.world = 1;
    }

    let mut used_names: BTreeSet<String> = pack.levels.iter().map(|l| l.name.clone()).collect();
    let legacy_sims = simulate_levels(&config, &pack.levels, &DEFAULT_SEEDS);
    let mut good_rates: Vec<f32> = legacy_sims.iter().map(|s| s.good.clear_rate).collect();

    const HARD_RATE_THRESHOLD: f32 = 0.375;
    const EARLY_ACCEPT_TOLERANCE: f32 = 0.13;
    const MAX_FAIR_CANDIDATES_TO_SCORE: usize = 6;
    const ATTEMPT_LIMIT: u64 = GENERATE_ATTEMPT_BUDGET * 2;

    for idx in pack.levels.len()..target_total {
        let id = idx + 1;
        let world = idx / SECTORS_PER_WORLD + 1;
        let sector_in_world = idx % SECTORS_PER_WORLD + 1;
        let is_last = idx + 1 == target_total;
        let world_planned_total = world * SECTORS_PER_WORLD;

        let prev_one_hard = good_rates.last().is_some_and(|&r| r <= HARD_RATE_THRESHOLD);
        let prev_two_hard = good_rates.len() >= 2
            && good_rates[good_rates.len() - 1] <= HARD_RATE_THRESHOLD
            && good_rates[good_rates.len() - 2] <= HARD_RATE_THRESHOLD;

        // Compute the previous 10-sector block mean within the same world (or prologue for W1 S11-20)
        // so we can steer candidates toward a smooth non-increasing curve.
        let block_idx = (sector_in_world - 1) / simulate::SECTORS_PER_BLOCK;
        let prev_block_mean = if world == 1 && block_idx == 1 {
            Some(good_rates[..LEGACY_LEVEL_COUNT].iter().sum::<f32>() / (LEGACY_LEVEL_COUNT as f32))
        } else if block_idx > 0 {
            let prev_start =
                (world - 1) * SECTORS_PER_WORLD + (block_idx - 1) * simulate::SECTORS_PER_BLOCK;
            let prev_start = prev_start.max(LEGACY_LEVEL_COUNT);
            let prev_end =
                (world - 1) * SECTORS_PER_WORLD + block_idx * simulate::SECTORS_PER_BLOCK;
            if prev_end > prev_start && prev_end <= good_rates.len() {
                let slice = &good_rates[prev_start..prev_end];
                Some(slice.iter().sum::<f32>() / (slice.len() as f32))
            } else {
                None
            }
        } else {
            None
        };

        let cur_block_start = ((world - 1) * SECTORS_PER_WORLD
            + block_idx * simulate::SECTORS_PER_BLOCK)
            .max(LEGACY_LEVEL_COUNT);
        let cur_slice = &good_rates[cur_block_start..idx];
        let base_target = generate::target_good_clear_rate(world, sector_in_world);
        let block_goal = match prev_block_mean {
            Some(prev_m) => base_target.min((prev_m - 0.015).max(0.44)),
            None => base_target,
        };
        let desired_rate = if cur_slice.is_empty() {
            block_goal
        } else {
            let sum_so_far: f32 = cur_slice.iter().sum();
            let n_so_far = cur_slice.len() as f32;
            ((block_goal * (n_so_far + 1.0)) - sum_so_far).clamp(0.42, 0.95)
        };

        let mut last_issues = Vec::new();
        let mut best: Option<(f32, f32, LevelDef, BTreeSet<String>)> = None;
        let mut fair_seen = 0usize;

        for attempt in 0..ATTEMPT_LIMIT {
            let mut candidate_names = used_names.clone();
            let candidate = match generate_sector(
                id,
                world,
                sector_in_world,
                is_last,
                seed.wrapping_add(attempt * GENERATE_ATTEMPT_SEED_STRIDE),
                &mut candidate_names,
            ) {
                Ok(c) => c,
                Err(e) => {
                    // Names do not depend on the seed, so retrying cannot help.
                    eprintln!("FAIL: level {id}: {e}");
                    return ExitCode::FAILURE;
                }
            };

            let issues = candidate_issues(
                &config,
                &pack.levels,
                &candidate,
                is_last,
                world_planned_total,
            );
            if !issues.is_empty() {
                last_issues = issues;
                continue;
            }

            let sims = simulate_levels(&config, std::slice::from_ref(&candidate), &DEFAULT_SEEDS);
            let rate = sims[0].good.clear_rate;
            if candidate.boss {
                best = Some((0.0, rate, candidate, candidate_names));
                break;
            }

            fair_seen += 1;
            let hard_penalty = if rate <= HARD_RATE_THRESHOLD && prev_two_hard {
                10.0
            } else if rate <= HARD_RATE_THRESHOLD && prev_one_hard {
                1.5
            } else {
                0.0
            };
            let score = (rate - desired_rate).abs() + hard_penalty;
            let is_better = best
                .as_ref()
                .is_none_or(|(best_s, _, _, _)| score < *best_s);
            if is_better {
                best = Some((score, rate, candidate, candidate_names));
            }
            if hard_penalty == 0.0 && (rate - desired_rate).abs() <= EARLY_ACCEPT_TOLERANCE {
                break;
            }
            if fair_seen >= MAX_FAIR_CANDIDATES_TO_SCORE {
                break;
            }
        }
        let Some((_, chosen_rate, level, chosen_names)) = best else {
            eprintln!(
                "FAIL: no candidate for level {id} (world {world}, sector {sector_in_world}) passed after {ATTEMPT_LIMIT} attempts. Last attempt:"
            );
            for issue in &last_issues {
                eprintln!("  - {issue}");
            }
            return ExitCode::FAILURE;
        };
        used_names = chosen_names;
        good_rates.push(chosen_rate);
        pack.levels.push(level);
    }

    let errors = validate_pack(&pack.levels);
    if !errors.is_empty() {
        report_errors(&errors, "the generated pack");
        return ExitCode::FAILURE;
    }

    let json = match serde_json::to_string_pretty(&pack) {
        Ok(s) => format!("{s}\n"),
        Err(e) => {
            eprintln!("error serializing pack: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(dir) = Path::new(out_path).parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("error creating {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = std::fs::write(out_path, json) {
        eprintln!("error writing {out_path}: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "Generated {} sectors up to world {max_world} into candidate file {out_path}. Run `write` to install it.",
        pack.levels.len()
    );
    ExitCode::SUCCESS
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
  kinetix-levelforge generate --worlds <1..=8> [--seed <u64>] [--levels <path>] [--out <path>]
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
            let flags = parse_flags(rest, &["--worlds", "--seed", "--levels", "--out"], &[])?;
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
            Ok(cmd_generate(levels_path, max_world, seed, out_path))
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
