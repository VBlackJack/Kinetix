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

//! Named layout anchors and game-feel tuning values. Layout values are in arena units
//! (the 1920x1080 reference space); text inside a panel is positioned as `PANEL_X + offset`.

use macroquad::prelude::KeyCode;

// ---- Window ----
pub const WINDOW_TITLE: &str = "KINETIX";
pub const WINDOW_WIDTH: i32 = 1920;
pub const WINDOW_HEIGHT: i32 = 1080;
pub const WINDOW_SAMPLE_COUNT: i32 = 8;
/// Frame time clamp, in seconds.
pub const MAX_FRAME_DT: f32 = 0.033;
pub const MIN_RENDER_W: f32 = 640.0;
pub const MIN_RENDER_H: f32 = 360.0;
pub const MS_PER_SEC: f64 = 1000.0;

// ---- Background ----
/// Colour of the bars around the arena when the window is not 16:9 is black; this is the dim
/// opacity drawn over the backdrop outside the arena viewport.
pub const LETTERBOX_DIM_ALPHA: f32 = 0.35;
/// Starfield extent relative to the arena on each axis (covers extreme window aspect ratios).
pub const STARFIELD_EXTENT_FACTOR: f32 = 3.5;
/// Star count for the arena area alone.
pub const STARFIELD_BASE_COUNT: usize = 260;
/// Warp grid cell size.
pub const WARP_GRID_STEP: f32 = 48.0;

// ---- VFX caps: the oldest entries are dropped past these limits ----
pub const MAX_PARTICLES: usize = 2048;
pub const MAX_SHARDS: usize = 256;
pub const MAX_SHOCKWAVES: usize = 32;
pub const MAX_ARCS: usize = 32;
pub const MAX_POPUPS: usize = 48;
pub const MAX_EVENTS: usize = 64;

// ---- Persistence ----
/// Minimum delay between two automatic saves triggered by a new high score.
pub const HIGH_SCORE_SAVE_INTERVAL_SEC: f64 = 5.0;

// ---- Score popups ----
pub const POPUP_POINTS_PREFIX: &str = "+";
pub const POPUP_MULTIPLIER_OPEN: &str = " (x";
pub const POPUP_MULTIPLIER_CLOSE: &str = ")";
/// Combo multiplier at or below which the popup shows the points only.
pub const POPUP_MULTIPLIER_HIDDEN_MAX: u32 = 1;

// ---- Menu placeholders ----
/// Placeholders replaced by the first and last sector ids of a world in `menu.sector_range`.
pub const FIRST_PLACEHOLDER: &str = "{first}";
pub const LAST_PLACEHOLDER: &str = "{last}";

// ---- Game feel ----
/// Multiplier applied to the raw mouse delta while the native cursor grab is active.
pub const NATIVE_RAW_MOUSE_SENSITIVITY: f32 = 1.0;
/// Keyboard paddle speed, in arena units per second.
pub const KEYBOARD_PADDLE_SPEED: f32 = 1350.0;
/// Upper bound of the screen shake amplitude. Also the margin the backdrop extends past the
/// window on each side so the shaking camera never reveals the clear colour.
pub const SCREEN_SHAKE_MAX: f32 = 24.0;
pub const SCREEN_SHAKE_MINOR_MAX: f32 = 16.0;
pub const SCREEN_SHAKE_DECAY_PER_SEC: f32 = 28.0;
pub const SHAKE_BRICK_DESTROYED: f32 = 4.2;
pub const SHAKE_EXPLOSION: f32 = 14.0;
pub const SHAKE_LIGHTNING: f32 = 7.0;
pub const CHROMATIC_DECAY_PER_SEC: f32 = 3.2;
pub const CHROMATIC_EXPLOSION: f32 = 1.0;
pub const CHROMATIC_LIGHTNING: f32 = 0.65;
/// Hit-stop duration after an explosion, and the time scale applied while it lasts.
pub const HIT_STOP_SEC: f32 = 0.045;
pub const HIT_STOP_TIME_SCALE: f32 = 0.15;

// ---- Particle recipes ----
pub const BRICK_PARTICLES_HIT: usize = 12;
pub const BRICK_PARTICLES_DESTROYED: usize = 30;
pub const BRICK_PARTICLE_LIFE: f32 = 0.52;
pub const BRICK_PARTICLE_SPEED: (f32, f32) = (120.0, 520.0);
pub const BRICK_PARTICLE_SIZE: (f32, f32) = (3.0, 8.0);
pub const BRICK_PARTICLE_ANGLE_JITTER: f32 = 0.25;
pub const GRAVITY_INDUSTRIAL: f32 = 580.0;
pub const GRAVITY_DEFAULT: f32 = 140.0;
pub const SHARDS_PER_BRICK: usize = 6;
pub const SHARD_LIFE: f32 = 0.78;
pub const SHARD_GRAVITY: f32 = 650.0;
pub const SHARD_SPREAD_X: f32 = 26.0;
pub const SHARD_SPREAD_Y: f32 = 10.0;
pub const SHARD_SPEED_X: f32 = 240.0;
pub const SHARD_SPEED_Y: (f32, f32) = (-280.0, 80.0);
pub const SHARD_WIDTH: (f32, f32) = (12.0, 22.0);
pub const SHARD_SPIN: f32 = 12.0;
pub const EXPLOSION_PARTICLES: usize = 65;
pub const EXPLOSION_PARTICLE_LIFE: f32 = 0.65;
pub const EXPLOSION_PARTICLE_SPEED: (f32, f32) = (160.0, 680.0);
pub const EXPLOSION_PARTICLE_SIZE: (f32, f32) = (4.5, 10.0);
pub const EXPLOSION_GRAVITY: f32 = 190.0;
pub const EXPLOSION_WAVE_START_RADIUS: f32 = 10.0;
pub const EXPLOSION_WAVE_RADIUS_FACTOR: f32 = 1.4;
pub const EXPLOSION_WAVE_LIFE: f32 = 0.40;
pub const PADDLE_PARTICLES: usize = 14;
pub const PADDLE_PARTICLE_LIFE: f32 = 0.28;
pub const PADDLE_PARTICLE_SPEED: (f32, f32) = (80.0, 260.0);
pub const PADDLE_PARTICLE_SIZE: f32 = 4.5;
pub const WALL_PARTICLES: usize = 8;
pub const WALL_PARTICLE_LIFE: f32 = 0.22;
pub const WALL_PARTICLE_SPEED: (f32, f32) = (50.0, 180.0);
pub const WALL_PARTICLE_SIZE: f32 = 3.5;
pub const LASER_PARTICLE_LIFE: f32 = 0.12;
pub const LASER_PARTICLE_SIZE: f32 = 5.5;
pub const LASER_PARTICLE_VY: f32 = -120.0;
pub const LIGHTNING_ARC_LIFE: f32 = 0.25;
pub const LIGHTNING_SEGMENTS: usize = 6;
pub const LIGHTNING_JITTER: f32 = 22.0;
pub const PORTAL_WAVE_START_RADIUS: f32 = 6.0;
/// (max radius, life in seconds) of the shockwave at the portal entry and exit.
pub const PORTAL_WAVE_FROM: (f32, f32) = (75.0, 0.32);
pub const PORTAL_WAVE_TO: (f32, f32) = (85.0, 0.36);
pub const POWERUP_WAVE_START_RADIUS: f32 = 8.0;
pub const POWERUP_WAVE_RADIUS: f32 = 105.0;
pub const POWERUP_WAVE_LIFE: f32 = 0.35;

// ---- Popups ----
pub const POPUP_X: f32 = 680.0;
pub const POPUP_Y: f32 = 540.0;
pub const POPUP_LIFE_SEC: f32 = 1.8;
pub const POPUP_RISE_SPEED: f32 = 55.0;
pub const POPUP_FONT_SIZE: u16 = 26;
/// Life used to normalise the popup fade-out: longer popups stay opaque until they near the end.
pub const POPUP_FADE_REF_SEC: f32 = 0.70;
pub const BRICK_POPUP_LIFE: f32 = 0.70;
pub const BRICK_POPUP_DX: f32 = -24.0;
pub const PORTAL_POPUP_LIFE: f32 = 0.75;
pub const PORTAL_POPUP_DX: f32 = -40.0;
pub const PORTAL_POPUP_DY: f32 = -16.0;
pub const POWERUP_POPUP_LIFE: f32 = 1.15;
pub const POWERUP_POPUP_DX: f32 = -120.0;
pub const POWERUP_POPUP_DY: f32 = -28.0;
/// Horizontal clamp of power-up popups so long labels stay on screen.
pub const POPUP_MIN_X: f32 = 80.0;
pub const POPUP_MAX_X: f32 = 1560.0;

// ---- Title menu ----
pub const MENU_TITLE_SIZE: u16 = 96;

// ---- HUD ----
pub const HUD_BAR_X: f32 = 48.0;
pub const HUD_BAR_Y: f32 = 8.0;
pub const HUD_BAR_W: f32 = 1824.0;
pub const HUD_BAR_H: f32 = 58.0;
pub const HUD_BAR_RADIUS: f32 = 12.0;
pub const HUD_BAR_INSET: f32 = 1.5;
pub const HUD_BAR_BORDER_ALPHA: f32 = 0.40;
pub const HUD_BAR_COLOR: [u8; 4] = [10, 12, 26, 225];
/// Left text margin inside the bar.
pub const HUD_TEXT_DX: f32 = 24.0;
pub const HUD_TOP_TEXT_Y: f32 = 34.0;
pub const HUD_TOP_TEXT_SIZE: u16 = 21;
pub const HUD_SUB_TEXT_Y: f32 = 56.0;
pub const HUD_SUB_TEXT_SIZE: u16 = 16;
/// Right-hand block (fps badge and vortex gauge), as offsets from the bar's left edge.
pub const HUD_RIGHT_DX: f32 = 1507.0;
pub const HUD_PERF_Y: f32 = 32.0;
pub const HUD_VORTEX_LABEL_Y: f32 = 56.0;
pub const HUD_RIGHT_TEXT_SIZE: u16 = 17;
pub const HUD_VORTEX_BAR_DX: f32 = 1602.0;
pub const HUD_VORTEX_BAR_Y: f32 = 42.0;
pub const HUD_VORTEX_BAR_W: f32 = 190.0;
pub const HUD_VORTEX_BAR_H: f32 = 12.0;
pub const HUD_VORTEX_BAR_MIN_W: f32 = 10.0;
pub const HUD_VORTEX_BAR_COLOR: [u8; 4] = [28, 32, 52, 240];
/// Free space kept between the sub bar text and the right-hand block.
pub const HUD_SUB_BAR_GAP: f32 = 16.0;
/// Widest the sub bar may be before its font shrinks (it ends where the right-hand block starts).
pub const HUD_SUB_BAR_MAX_W: f32 = HUD_RIGHT_DX - HUD_TEXT_DX - HUD_SUB_BAR_GAP;
pub const HUD_SUB_BAR_SEPARATOR: &str = "    ";
pub const HUD_BANNER_X: f32 = 72.0;
pub const HUD_BANNER_Y: f32 = 138.0;
pub const HUD_BANNER_SIZE: u16 = 28;
pub const HUD_SUDDEN_DEATH_Y: f32 = 136.0;
pub const HUD_SUDDEN_DEATH_SIZE: u16 = 26;
pub const HUD_FOOTER_Y: f32 = 1058.0;
pub const HUD_HINT_X: f32 = 380.0;
pub const HUD_HINT_Y: f32 = 1060.0;
pub const HUD_FOOTER_SIZE: u16 = 22;
/// Arena width the editor help line is laid out in (the footer is centred on `HUD_BANNER_X`).
pub const HUD_FOOTER_ARENA_W: f32 = 1920.0;
/// Widest the editor help line may be: the arena width minus the banner margin on both sides.
pub const HUD_FOOTER_MAX_W: f32 = HUD_FOOTER_ARENA_W - 2.0 * HUD_BANNER_X;
/// Separator between the sector label, id and name on the title menu sector line.
pub const MENU_SECTOR_ID_PREFIX: &str = " #";
pub const MENU_SECTOR_NAME_SEPARATOR: &str = " - ";
pub const TOAST_Y: f32 = 82.0;
pub const TOAST_H: f32 = 38.0;
pub const TOAST_RADIUS: f32 = 10.0;
pub const TOAST_BORDER: f32 = 2.5;
pub const TOAST_PAD_X: f32 = 22.0;
pub const TOAST_TEXT_DY: f32 = 27.0;
pub const TOAST_FONT_SIZE: u16 = 24;
pub const TOAST_BG_COLOR: [u8; 4] = [8, 12, 24, 240];
pub const OVERLAY_X: f32 = 460.0;
pub const OVERLAY_Y: f32 = 410.0;
pub const OVERLAY_W: f32 = 1000.0;
pub const OVERLAY_H: f32 = 250.0;
pub const OVERLAY_RADIUS: f32 = 24.0;
pub const OVERLAY_BORDER: f32 = 4.0;
pub const OVERLAY_COLOR: [u8; 4] = [10, 12, 26, 242];
pub const OVERLAY_TEXT_DX: f32 = 70.0;
pub const OVERLAY_TITLE_DY: f32 = 105.0;
pub const OVERLAY_TITLE_SIZE: u16 = 48;
pub const OVERLAY_SUB_DY: f32 = 175.0;
pub const OVERLAY_SUB_SIZE: u16 = 28;
/// Shield glow opacity at full charge.
pub const SHIELD_MAX_ALPHA: f32 = 0.85;
/// Boss label and HP bar.
pub const BOSS_LABEL_SIZE: u16 = 28;

// ---- Font atlas pre-warm ----
/// Every UI font size used by `txt` and `measure_text` (badge shrink sizes are added separately,
/// and the title size is only warmed for the title string).
pub const UI_FONT_SIZES: [u16; 13] = [16, 17, 21, 22, 23, 24, 26, 28, 29, 30, 32, 34, 48];

// ---- Pointer lock resume gestures (must match RESUME_* in web/index.html) ----
pub const RESUME_CLICK: u32 = 1;
pub const RESUME_SPACE: u32 = 2;
pub const RESUME_ENTER: u32 = 4;
pub const RESUME_P: u32 = 8;
pub const RESUME_R: u32 = 16;
pub const RESUME_F3: u32 = 32;
pub const RESUME_F4: u32 = 64;
pub const RESUME_TAB: u32 = 128;
/// Debug-only "next sector" cheat (F3); release builds neither bind the key nor advertise it.
pub const LEVEL_CHEAT_ENABLED: bool = cfg!(debug_assertions);
/// Resume gestures contributed by the F3 cheat: none when the cheat is compiled out.
pub const RESUME_CHEAT_MASK: u32 = if LEVEL_CHEAT_ENABLED { RESUME_F3 } else { 0 };

// ---- Special brick drawing ----
/// Decorative glyphs drawn on moving, bumper and regenerating bricks (not translatable text).
pub const MOVING_GLYPH: &str = "<>";
pub const BUMPER_GLYPH: &str = ">>";
pub const REGEN_GLYPH: &str = "+";
/// Glyph offsets from the brick centre (x) and shared baseline offset (y).
pub const MOVING_GLYPH_DX: f32 = -12.0;
pub const BUMPER_GLYPH_DX: f32 = -12.0;
pub const REGEN_GLYPH_DX: f32 = -7.0;
pub const BRICK_GLYPH_DY: f32 = 8.0;
/// Font sizes of the brick glyphs.
pub const BRICK_GLYPH_SIZE: u16 = 22;
pub const REGEN_GLYPH_SIZE: u16 = 24;
/// Bumper pulsing ring: base radius, pulse amplitude, pulse rate (rad/s) and line thickness.
pub const BUMPER_RING_RADIUS: f32 = 9.0;
pub const BUMPER_RING_PULSE_AMP: f32 = 3.5;
pub const BUMPER_RING_PULSE_RATE: f32 = 11.0;
pub const BUMPER_RING_THICKNESS: f32 = 2.5;

// ---- Hidden developer mode ----
/// Letter key of the Ctrl+Shift toggle chord.
pub const DEV_TOGGLE_KEY: KeyCode = KeyCode::D;
/// Key that shows or hides the developer key help.
pub const DEV_HELP_KEY: KeyCode = KeyCode::F9;
/// Keys that select the previous / next power-up.
pub const DEV_POWERUP_PREV_KEYS: [KeyCode; 2] = [KeyCode::LeftBracket, KeyCode::Up];
pub const DEV_POWERUP_NEXT_KEYS: [KeyCode; 2] = [KeyCode::RightBracket, KeyCode::Down];
/// Applies the selected power-up immediately, as a real pickup would.
pub const DEV_APPLY_KEY: KeyCode = KeyCode::G;
/// Drops the selected power-up as a falling capsule above the paddle.
pub const DEV_DROP_KEY: KeyCode = KeyCode::H;
/// Toggles the invulnerable ball loss.
pub const DEV_INVULNERABLE_KEY: KeyCode = KeyCode::L;
/// Clears the level instantly (counts as a developer win).
pub const DEV_CLEAR_KEY: KeyCode = KeyCode::K;
/// Level navigation while the mode is on (outside the title menu).
pub const DEV_PREV_LEVEL_KEY: KeyCode = KeyCode::PageUp;
pub const DEV_NEXT_LEVEL_KEY: KeyCode = KeyCode::PageDown;
/// Digit keys of the sector number entry, with the character each one types.
pub const DEV_DIGIT_KEYS: [(KeyCode, char); 10] = [
    (KeyCode::Key0, '0'),
    (KeyCode::Key1, '1'),
    (KeyCode::Key2, '2'),
    (KeyCode::Key3, '3'),
    (KeyCode::Key4, '4'),
    (KeyCode::Key5, '5'),
    (KeyCode::Key6, '6'),
    (KeyCode::Key7, '7'),
    (KeyCode::Key8, '8'),
    (KeyCode::Key9, '9'),
];
/// Maximum number of digits accepted in the sector number entry.
pub const DEV_SECTOR_DIGITS_MAX: usize = 4;
/// Placeholders of the developer locale strings.
pub const DEV_NAME_PLACEHOLDER: &str = "{name}";
pub const DEV_VALUE_PLACEHOLDER: &str = "{value}";
pub const DEV_MAX_PLACEHOLDER: &str = "{max}";
/// Vertical distance above the paddle at which a dropped capsule appears.
pub const DEV_DROP_OFFSET_Y: f32 = 360.0;
/// Developer panel layout (arena units): position, width, text sizes and line height.
pub const DEV_PANEL_X: f32 = 48.0;
pub const DEV_PANEL_Y: f32 = 150.0;
pub const DEV_PANEL_W: f32 = 820.0;
pub const DEV_PANEL_PAD: f32 = 16.0;
pub const DEV_PANEL_RADIUS: f32 = 10.0;
pub const DEV_PANEL_COLOR: [u8; 4] = [8, 10, 22, 215];
pub const DEV_BADGE_SIZE: u16 = 30;
pub const DEV_TEXT_SIZE: u16 = 22;
pub const DEV_TEXT_SIZE_MIN: u16 = 14;
pub const DEV_LINE_H: f32 = 30.0;
/// Widest developer text line allowed inside the panel.
pub const DEV_TEXT_MAX_W: f32 = DEV_PANEL_W - 2.0 * DEV_PANEL_PAD;
/// Locale keys of the help lines, in display order.
pub const DEV_HELP_KEYS: [&str; 7] = [
    "dev.help.level",
    "dev.help.step",
    "dev.help.powerup_select",
    "dev.help.powerup_apply",
    "dev.help.helpers",
    "dev.help.panel",
    "dev.help.toggle",
];
