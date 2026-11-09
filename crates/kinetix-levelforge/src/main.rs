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

//! CLI tool for validating, simulating, generating, and inspecting the KINETIX level campaign.

pub mod generate;
pub mod simulate;
pub mod validate;

use generate::generate_sector;
use kinetix_core::config::{GameConfig, LevelPack};
use simulate::{compute_world_stats, simulate_level, BOT_MAX_TIME_SEC, DEFAULT_SEEDS};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;
use validate::validate_pack;

const DEFAULT_LEVELS_PATH: &str = "assets/config/levels.json";
const DEFAULT_CONFIG_PATH: &str = "assets/config/game_config.json";
const DEFAULT_CSV_DIR: &str = "target/levelforge";
const DEFAULT_CSV_FILE: &str = "target/levelforge/simulation.csv";
const SECTORS_PER_WORLD: usize = 100;
const DEFAULT_SEED: u64 = 0x2026_1108_CAFE_BABE;

fn load_levels(path: &str) -> Result<LevelPack, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    serde_json::from_str::<LevelPack>(&raw).map_err(|e| format!("invalid JSON in {path}: {e}"))
}

fn load_config(path: &str) -> Result<GameConfig, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    serde_json::from_str::<GameConfig>(&raw).map_err(|e| format!("invalid JSON in {path}: {e}"))
}

fn cmd_validate(levels_path: &str) -> ExitCode {
    let pack = match load_levels(levels_path) {
        Ok(p) => p,
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
        eprintln!(
            "FAIL: {} validation error(s) in {levels_path}:",
            errors.len()
        );
        for err in &errors {
            eprintln!("  - {err}");
        }
        ExitCode::FAILURE
    }
}

fn cmd_simulate(levels_path: &str, world_filter: Option<usize>) -> ExitCode {
    let pack = match load_levels(levels_path) {
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

    let levels_to_run: Vec<_> = pack
        .levels
        .iter()
        .filter(|l| world_filter.is_none_or(|w| l.world == w))
        .cloned()
        .collect();

    if levels_to_run.is_empty() {
        eprintln!("No levels matched filter {:?}.", world_filter);
        return ExitCode::FAILURE;
    }

    let mut summaries = Vec::with_capacity(levels_to_run.len());
    let mut unfair = Vec::new();
    let sudden_death = config.gameplay.sudden_death_time_sec;

    for lvl in &levels_to_run {
        let sum = simulate_level(&config, lvl, &DEFAULT_SEEDS, BOT_MAX_TIME_SEC);
        let is_set_piece_or_boss = lvl.boss || lvl.id.is_multiple_of(10) || lvl.id <= 15;
        if sum.clear_count == 0 {
            unfair.push(format!(
                "level {} ({}) cleared on 0/{} seeds",
                lvl.id,
                lvl.name,
                DEFAULT_SEEDS.len()
            ));
        } else if !is_set_piece_or_boss && sum.median_clear_time > sudden_death {
            unfair.push(format!(
                "level {} ({}) median clear time {:.1}s exceeds sudden_death ({:.1}s)",
                lvl.id, lvl.name, sum.median_clear_time, sudden_death
            ));
        }
        summaries.push(sum);
    }

    if let Err(e) = std::fs::create_dir_all(DEFAULT_CSV_DIR) {
        eprintln!("warning: could not create {DEFAULT_CSV_DIR}: {e}");
    } else {
        let mut csv = String::from(
            "level_id,world,name,boss,clear_count,seed_count,median_clear_time_s,avg_balls_lost,avg_powerups\n",
        );
        for s in &summaries {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{:.2},{:.2},{:.2}\n",
                s.level_id,
                s.world,
                s.name,
                s.boss,
                s.clear_count,
                s.runs.len(),
                s.median_clear_time,
                s.avg_balls_lost,
                s.avg_powerups
            ));
        }
        let _ = std::fs::write(DEFAULT_CSV_FILE, csv);
    }

    print_world_stats_table(&levels_to_run, Some(&summaries));

    if !unfair.is_empty() {
        eprintln!("WARNING: {} fairness issue(s) detected:", unfair.len());
        for u in &unfair {
            eprintln!("  - {u}");
        }
    }
    ExitCode::SUCCESS
}

fn cmd_stats(levels_path: &str, with_sim: bool) -> ExitCode {
    let pack = match load_levels(levels_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let summaries = if with_sim {
        let config = match load_config(DEFAULT_CONFIG_PATH) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        Some(
            pack.levels
                .iter()
                .map(|l| simulate_level(&config, l, &DEFAULT_SEEDS, BOT_MAX_TIME_SEC))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    print_world_stats_table(&pack.levels, summaries.as_deref());
    ExitCode::SUCCESS
}

fn print_world_stats_table(
    levels: &[kinetix_core::config::LevelDef],
    sims: Option<&[simulate::LevelSimSummary]>,
) {
    let stats = compute_world_stats(levels, sims);
    println!(
        "| World | Sectors | Avg Bricks | Tough % | Special % | Clear Rate | Median Time (s) | Avg Losses |"
    );
    println!(
        "|------:|--------:|-----------:|--------:|----------:|-----------:|----------------:|-----------:|"
    );
    for st in stats {
        println!(
            "| {:>5} | {:>7} | {:>10.1} | {:>6.1}% | {:>8.1}% | {:>9.1}% | {:>15.2} | {:>10.2} |",
            st.world,
            st.sector_count,
            st.avg_bricks,
            st.tough_ratio * 100.0,
            st.special_ratio * 100.0,
            st.clear_rate * 100.0,
            st.median_clear_time,
            st.avg_losses
        );
    }
}

fn cmd_generate(levels_path: &str, max_world: usize, seed: u64) -> ExitCode {
    let mut pack = match load_levels(levels_path) {
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

    // Preserve existing levels up to index 14 (or existing count if extending).
    let target_total = max_world * SECTORS_PER_WORLD;
    if pack.levels.len() > target_total {
        pack.levels.truncate(target_total);
    }

    let mut used_names: BTreeSet<String> = pack.levels.iter().map(|l| l.name.clone()).collect();
    let sudden_death = config.gameplay.sudden_death_time_sec;

    let start_idx = pack.levels.len();
    for idx in start_idx..target_total {
        let id = idx + 1;
        let world = (idx / SECTORS_PER_WORLD) + 1;
        let sector_in_world = (idx % SECTORS_PER_WORLD) + 1;
        let is_set_piece_or_boss =
            sector_in_world == 50 || sector_in_world == 100 || sector_in_world.is_multiple_of(10);

        let mut attempt = 0u64;
        let chosen = loop {
            let mut candidate_names = used_names.clone();
            let candidate = generate_sector(
                id,
                world,
                sector_in_world,
                seed.wrapping_add(attempt * 7919),
                &mut candidate_names,
            );
            let sim = simulate_level(&config, &candidate, &DEFAULT_SEEDS, BOT_MAX_TIME_SEC);
            let fair_time = is_set_piece_or_boss || sim.median_clear_time <= sudden_death * 0.95;
            if sim.clear_count > 0 && fair_time {
                used_names = candidate_names;
                break candidate;
            }
            attempt += 1;
            if attempt > 25 {
                used_names = candidate_names;
                break candidate;
            }
        };
        pack.levels.push(chosen);
    }

    let errors = validate_pack(&pack.levels);
    if !errors.is_empty() {
        eprintln!(
            "FAIL: generated pack has {} validation error(s):",
            errors.len()
        );
        for err in &errors {
            eprintln!("  - {err}");
        }
        return ExitCode::FAILURE;
    }

    let json = match serde_json::to_string_pretty(&pack) {
        Ok(s) => format!("{s}\n"),
        Err(e) => {
            eprintln!("error serializing pack: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = std::fs::write(Path::new(levels_path), json) {
        eprintln!("error writing {levels_path}: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "Generated campaign up to world {max_world} ({} total sectors) in {levels_path}.",
        pack.levels.len()
    );
    ExitCode::SUCCESS
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
            let mut world_filter = None;
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--world" {
                    if let Some(w) = args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
                        world_filter = Some(w);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            cmd_simulate(DEFAULT_LEVELS_PATH, world_filter)
        }
        "stats" => {
            let with_sim = args.iter().any(|a| a == "--sim");
            cmd_stats(DEFAULT_LEVELS_PATH, with_sim)
        }
        "generate" => {
            let mut max_world = 8usize;
            let mut seed = DEFAULT_SEED;
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--worlds" {
                    if let Some(w) = args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
                        max_world = w;
                    }
                    i += 2;
                } else if args[i] == "--seed" {
                    if let Some(s) = args.get(i + 1).and_then(|v| v.parse::<u64>().ok()) {
                        seed = s;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            cmd_generate(DEFAULT_LEVELS_PATH, max_world, seed)
        }
        other => {
            eprintln!(
                "Unknown subcommand: {other}. Expected: validate | simulate | stats | generate"
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
    fn shipped_levels_json_passes_all_validation_rules_and_parses_fast() {
        let start = std::time::Instant::now();
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("valid levels.json");
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 50,
            "levels.json parse took {} ms (limit 50 ms)",
            elapsed.as_millis()
        );
        let errors = validate_pack(&pack.levels);
        assert!(
            errors.is_empty(),
            "levels.json validation failed: {errors:#?}"
        );
    }
}
