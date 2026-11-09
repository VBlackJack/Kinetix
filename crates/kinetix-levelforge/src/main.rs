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

pub mod generate;
pub mod simulate;
pub mod validate;

use generate::generate_sector;
use kinetix_core::config::{GameConfig, LevelDef, LevelPack};
use simulate::{
    casual_loss_gate, compute_world_stats, fairness_issues, simulate_levels, LevelSimSummary,
    DEFAULT_SEEDS, PRESETS,
};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};
use validate::{
    check_reachability, validate_pack, LEGACY_LEVEL_COUNT, MAX_WORLDS, SECTORS_PER_WORLD,
};

const DEFAULT_LEVELS_PATH: &str = "assets/config/levels.json";
const DEFAULT_CONFIG_PATH: &str = "assets/config/game_config.json";
const DEFAULT_OUT_DIR: &str = "target/levelforge";
const DEFAULT_CSV_FILE: &str = "target/levelforge/simulation.csv";
const DEFAULT_CANDIDATE_FILE: &str = "target/levelforge/candidate-levels.json";
const DEFAULT_SEED: u64 = 0x2026_1108_CAFE_BABE;
const DEFAULT_WORLDS: usize = 8;
/// Candidates tried per sector before `generate` gives up (no forced acceptance).
const GENERATE_ATTEMPT_BUDGET: u64 = 25;
/// Seed stride between two attempts of the same sector.
const GENERATE_ATTEMPT_SEED_STRIDE: u64 = 7919;
/// The per-level table is printed only for runs up to this many levels.
const MAX_LEVELS_IN_DETAIL_TABLE: usize = 60;
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

/// Why a generated candidate is not acceptable (empty means accepted).
fn candidate_issues(config: &GameConfig, candidate: &LevelDef) -> Vec<String> {
    if let Err(e) = check_reachability(&candidate.rows) {
        return vec![e];
    }
    let sims = simulate_levels(config, std::slice::from_ref(candidate), &DEFAULT_SEEDS);
    fairness_issues(&sims)
}

fn cmd_generate(levels_path: &str, max_world: usize, seed: u64, out_path: &str) -> ExitCode {
    if max_world == 0 || max_world > MAX_WORLDS {
        eprintln!("error: --worlds must be in 1..={MAX_WORLDS}");
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

    let mut used_names: BTreeSet<String> = pack.levels.iter().map(|l| l.name.clone()).collect();

    for idx in pack.levels.len()..target_total {
        let id = idx + 1;
        let world = idx / SECTORS_PER_WORLD + 1;
        let sector_in_world = idx % SECTORS_PER_WORLD + 1;
        let is_last = idx + 1 == target_total;

        let mut last_issues = Vec::new();
        let mut accepted = None;
        for attempt in 0..GENERATE_ATTEMPT_BUDGET {
            let mut candidate_names = used_names.clone();
            let candidate = generate_sector(
                id,
                world,
                sector_in_world,
                is_last,
                seed.wrapping_add(attempt * GENERATE_ATTEMPT_SEED_STRIDE),
                &mut candidate_names,
            );
            let issues = candidate_issues(&config, &candidate);
            if issues.is_empty() {
                used_names = candidate_names;
                accepted = Some(candidate);
                break;
            }
            last_issues = issues;
        }
        let Some(level) = accepted else {
            eprintln!(
                "FAIL: no candidate for level {id} (world {world}, sector {sector_in_world}) passed after {GENERATE_ATTEMPT_BUDGET} attempts. Last attempt:"
            );
            for issue in &last_issues {
                eprintln!("  - {issue}");
            }
            return ExitCode::FAILURE;
        };
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

/// Value following `flag` in `args`, if present.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("validate");

    match cmd {
        "validate" => {
            let path = args
                .get(1)
                .map(String::as_str)
                .unwrap_or(DEFAULT_LEVELS_PATH);
            cmd_validate(path)
        }
        "simulate" => {
            let world_filter = flag_value(&args, "--world").and_then(|v| v.parse::<usize>().ok());
            cmd_simulate(DEFAULT_LEVELS_PATH, world_filter)
        }
        "stats" => {
            let with_sim = args.iter().any(|a| a == "--sim");
            cmd_stats(DEFAULT_LEVELS_PATH, with_sim)
        }
        "generate" => {
            let max_world = flag_value(&args, "--worlds")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(DEFAULT_WORLDS);
            let seed = flag_value(&args, "--seed")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(DEFAULT_SEED);
            let out = flag_value(&args, "--out").unwrap_or(DEFAULT_CANDIDATE_FILE);
            cmd_generate(DEFAULT_LEVELS_PATH, max_world, seed, out)
        }
        "write" => {
            let candidate = flag_value(&args, "--candidate").unwrap_or(DEFAULT_CANDIDATE_FILE);
            let levels = flag_value(&args, "--levels").unwrap_or(DEFAULT_LEVELS_PATH);
            cmd_write(candidate, levels)
        }
        other => {
            eprintln!(
                "Unknown subcommand: {other}. Expected: validate | simulate | stats | generate | write"
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn flag_value_reads_the_following_argument() {
        let args: Vec<String> = ["generate", "--worlds", "3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(flag_value(&args, "--worlds"), Some("3"));
        assert_eq!(flag_value(&args, "--seed"), None);
    }
}
