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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArenaConfig {
    pub width: f32,
    pub height: f32,
    pub wall_top: f32,
    pub wall_side: f32,
    pub paddle_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaddleConfig {
    pub default_width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub height: f32,
    pub width_step: f32,
    pub max_bounce_angle_deg: f32,
    pub laser_cooldown_sec: f32,
    pub laser_speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BallConfig {
    pub default_radius: f32,
    pub mega_radius: f32,
    pub default_speed: f32,
    pub min_speed: f32,
    pub max_speed: f32,
    pub speed_step_on_hit: f32,
    pub substeps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameplayConfig {
    pub initial_lives: u32,
    pub powerup_drop_chance: f32,
    pub powerup_fall_speed: f32,
    pub powerup_radius: f32,
    pub combo_timeout_sec: f32,
    pub base_brick_score: u64,
    pub explosion_radius: f32,
    pub lightning_targets: usize,
    pub vortex_pull_strength: f32,
    pub vortex_max_energy: f32,
    pub vortex_drain_rate: f32,
    pub vortex_recharge_rate: f32,
    pub sudden_death_time_sec: f32,
    pub boss_base_hp: u32,
    pub boss_speed: f32,
    pub boss_fire_interval_sec: f32,
    pub boss_hazard_speed: f32,
    pub sudden_death_strike_interval_sec: f32,
    pub portal_cooldown_sec: f32,
    pub powerup_spawn_cooldown_sec: f32,
    pub powerup_min_spawn_distance: f32,
    pub powerup_wall_margin: f32,
    pub powerup_toast_sec: f32,
    pub slow_factor: f32,
    pub fast_factor: f32,
    pub shield_hits: u8,
    pub boss_hp_per_tier: u32,
    pub boss_speed_per_tier: f32,
    pub boss_kill_score_mult: u64,
    pub combo_step: u32,
    pub combo_max_mult: u32,
    pub multiball_spread_rad: f32,
    pub powerup_score_mult: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub arena: ArenaConfig,
    pub paddle: PaddleConfig,
    pub ball: BallConfig,
    pub gameplay: GameplayConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderStyle {
    NeonGlow,
    IndustrialMetal,
    BeveledClassic,
    WireframeHolo,
    CrystalFacet,
    VolcanicCore,
    CryoGlass,
    OutrunGold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeDef {
    pub id: String,
    pub name_key: String,
    pub style: RenderStyle,
    pub bg_top: [u8; 4],
    pub bg_bottom: [u8; 4],
    pub grid_color: [u8; 4],
    pub border_color: [u8; 4],
    pub paddle_core: [u8; 4],
    pub paddle_accent: [u8; 4],
    pub ball_core: [u8; 4],
    pub ball_glow: [u8; 4],
    pub fireball_glow: [u8; 4],
    pub lightning_color: [u8; 4],
    pub text_primary: [u8; 4],
    pub text_accent: [u8; 4],
    pub brick_palette: Vec<[u8; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeRegistry {
    pub themes: Vec<ThemeDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDef {
    pub id: usize,
    #[serde(default = "default_world")]
    pub world: usize,
    pub name: String,
    #[serde(default)]
    pub boss: bool,
    pub rows: Vec<String>,
}

fn default_world() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelPack {
    pub levels: Vec<LevelDef>,
}
