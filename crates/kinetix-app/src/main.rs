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

mod audio;
mod persistence;
mod tuning;
mod vfx;

use audio::{
    pointer_lock_active, prompt_level_code, set_document_lang, set_pointer_bounds,
    set_pointer_lock, set_resume_mask, share_level_code, SoundBank,
};
use kinetix_core::config::{GameConfig, LevelDef, LevelPack, RenderStyle, ThemeDef, ThemeRegistry};
use kinetix_core::i18n::{I18nCatalog, Language};
use kinetix_core::level_code::LevelCodeError;
use kinetix_core::physics::{BrickKind, GameEvent, GameWorld, SessionStatus};
use macroquad::prelude::*;
use persistence::{high_score_save_due, load_progress, save_progress};
use std::collections::BTreeSet;
use tuning::*;
use vfx::{
    additive_material, cap_oldest, draw_modern_capsule_brick, draw_powerup_capsule,
    draw_ribbon_trail, draw_rounded_rect, fit_font_size, nebula_material, DebrisShard, Starfield,
    WarpGrid, BADGE_FONT_SIZE_MAX, BADGE_FONT_SIZE_MIN,
};

const RAW_GAME_CONFIG: &str = include_str!("../../../assets/config/game_config.json");
const RAW_THEMES: &str = include_str!("../../../assets/config/themes.json");
const RAW_LEVELS: &str = include_str!("../../../assets/config/levels.json");
const RAW_EN: &str = include_str!("../../../assets/locales/en.json");
const RAW_FR: &str = include_str!("../../../assets/locales/fr.json");
const RAW_FONT: &[u8] = include_bytes!("../../../assets/fonts/ui_font.ttf");

/// Malus power-ups and error popups.
const MALUS_COLOR: [u8; 4] = [255, 65, 75, 255];
/// Bonus power-ups (popups and HUD toast).
const BONUS_COLOR: [u8; 4] = [30, 250, 165, 255];
/// Colour of the bars around the arena when the window is not 16:9.
const LETTERBOX_COLOR: Color = BLACK;

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    life: f32,
    max_life: f32,
    color: Color,
    gravity: f32,
}

struct Shockwave {
    x: f32,
    y: f32,
    radius: f32,
    max_radius: f32,
    life: f32,
    max_life: f32,
    color: Color,
}

struct LightningArc {
    from: (f32, f32),
    to: (f32, f32),
    life: f32,
}

struct FloatingPopup {
    x: f32,
    y: f32,
    text: String,
    life: f32,
    color: Color,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppScreen {
    TitleMenu,
    InGame,
    Editor,
}

fn rgba(c: [u8; 4]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], c[3])
}

/// Fits an arena of `arena_w` x `arena_h` into a `sw` x `sh` window, keeping the aspect ratio.
/// Returns the uniform scale and the offsets of the arena inside the window.
fn letterbox(sw: f32, sh: f32, arena_w: f32, arena_h: f32) -> (f32, f32, f32) {
    let scale = (sw / arena_w).min(sh / arena_h).max(f32::EPSILON);
    (
        scale,
        (sw - arena_w * scale) * 0.5,
        (sh - arena_h * scale) * 0.5,
    )
}

/// Maps a window position (logical pixels) to arena coordinates.
fn screen_to_arena(mx: f32, my: f32, scale: f32, ox: f32, oy: f32) -> (f32, f32) {
    ((mx - ox) / scale, (my - oy) / scale)
}

/// Window rectangle expressed in arena units: the arena extended symmetrically to the window aspect.
fn backdrop_rect(sw: f32, sh: f32, arena_w: f32, arena_h: f32) -> Rect {
    let (scale, ox, oy) = letterbox(sw, sh, arena_w, arena_h);
    Rect::new(-ox / scale, -oy / scale, sw / scale, sh / scale)
}

/// Physical-pixel viewport rectangle of the arena for `Camera2D::viewport`.
fn arena_viewport(
    scale: f32,
    ox: f32,
    oy: f32,
    arena_w: f32,
    arena_h: f32,
    dpi: f32,
) -> (i32, i32, i32, i32) {
    (
        (ox * dpi).round() as i32,
        (oy * dpi).round() as i32,
        (arena_w * scale * dpi).round() as i32,
        (arena_h * scale * dpi).round() as i32,
    )
}

fn error_popup(i18n: &I18nCatalog, err: &LevelCodeError) -> FloatingPopup {
    FloatingPopup {
        x: POPUP_X,
        y: POPUP_Y,
        text: i18n.t(err.i18n_key()).to_string(),
        life: POPUP_LIFE_SEC,
        color: rgba(MALUS_COLOR),
    }
}

/// Keys that jump straight to the n-th world of the title menu (Key1 is the first world).
const WORLD_SELECT_KEYS: [KeyCode; 9] = [
    KeyCode::Key1,
    KeyCode::Key2,
    KeyCode::Key3,
    KeyCode::Key4,
    KeyCode::Key5,
    KeyCode::Key6,
    KeyCode::Key7,
    KeyCode::Key8,
    KeyCode::Key9,
];

fn rand_in(range: (f32, f32)) -> f32 {
    rand::gen_range(range.0, range.1)
}

/// Worlds in campaign order as `(world id, index of the first level)`; a new entry starts
/// whenever the world id changes along the level list.
fn build_worlds(levels: &[LevelDef]) -> Vec<(usize, usize)> {
    let mut worlds: Vec<(usize, usize)> = Vec::new();
    for (idx, level) in levels.iter().enumerate() {
        if worlds.last().map(|&(id, _)| id) != Some(level.world) {
            worlds.push((level.world, idx));
        }
    }
    worlds
}

/// Position in `worlds` of the world owning level `level_idx` (`fallback` when unknown).
fn world_pos_for_level(
    worlds: &[(usize, usize)],
    levels: &[LevelDef],
    level_idx: usize,
    fallback: usize,
) -> usize {
    levels
        .get(level_idx)
        .and_then(|level| {
            worlds
                .iter()
                .rposition(|&(id, first)| id == level.world && first <= level_idx)
        })
        .unwrap_or(fallback)
}

/// A sector can be started when its index is not above the highest unlocked one.
fn is_sector_unlocked(sector_idx: usize, max_unlocked: usize) -> bool {
    sector_idx <= max_unlocked
}

/// A world is playable once its first sector is unlocked.
fn is_world_unlocked(first_level_idx: usize, max_unlocked: usize) -> bool {
    is_sector_unlocked(first_level_idx, max_unlocked)
}

/// Keeps a menu sector selection on an existing, unlocked sector.
fn clamp_sector(selected: usize, max_unlocked: usize, last_sector: usize) -> usize {
    selected.min(max_unlocked).min(last_sector)
}

/// Position of the next unlocked world after `from` in the given direction, wrapping around and
/// skipping locked worlds; `from` itself when no other world is unlocked.
fn step_unlocked_world(
    worlds: &[(usize, usize)],
    from: usize,
    forward: bool,
    max_unlocked: usize,
) -> usize {
    let count = worlds.len();
    for step in 1..count {
        let pos = if forward {
            (from + step) % count
        } else {
            (from + count - step) % count
        };
        if is_world_unlocked(worlds[pos].1, max_unlocked) {
            return pos;
        }
    }
    from
}

/// Tracks the last persisted high score and save time to throttle automatic saves.
struct SaveTracker {
    last_saved_high_score: u64,
    last_save_sec: f64,
}

impl SaveTracker {
    fn new(saved_high_score: u64) -> Self {
        Self {
            last_saved_high_score: saved_high_score,
            last_save_sec: f64::NEG_INFINITY,
        }
    }

    /// Saves unconditionally and records the saved high score and time.
    fn persist(&mut self, world: &GameWorld, theme_idx: usize, lang_fr: bool, now_sec: f64) {
        save_progress(
            world.high_score,
            world.max_unlocked_level,
            theme_idx,
            lang_fr,
        );
        self.last_saved_high_score = world.high_score;
        self.last_save_sec = now_sec;
    }

    /// Saves only when a new high score is due according to the throttle.
    fn persist_if_due(&mut self, world: &GameWorld, theme_idx: usize, lang_fr: bool, now_sec: f64) {
        if high_score_save_due(
            world.high_score,
            self.last_saved_high_score,
            now_sec,
            self.last_save_sec,
            HIGH_SCORE_SAVE_INTERVAL_SEC,
        ) {
            self.persist(world, theme_idx, lang_fr, now_sec);
        }
    }
}

/// Replaces the count placeholder of a locale string with the number of worlds.
fn substitute_count(template: &str, count: usize) -> String {
    template.replace(COUNT_PLACEHOLDER, &count.to_string())
}

/// Text of a brick score popup: the points, plus the combo multiplier when it exceeds 1.
fn format_score_popup(points: u64, multiplier: u32) -> String {
    if multiplier > POPUP_MULTIPLIER_HIDDEN_MAX {
        format!(
            "{POPUP_POINTS_PREFIX}{points}{POPUP_MULTIPLIER_OPEN}{multiplier}{POPUP_MULTIPLIER_CLOSE}"
        )
    } else {
        format!("{POPUP_POINTS_PREFIX}{points}")
    }
}

/// Geometry of the title menu world list, derived from the number of worlds.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WorldRowLayout {
    step: f32,
    pill_h: f32,
    text_dy: f32,
    text_size: u16,
}

/// Fits `count` world rows between `MENU_WORLD_LIST_Y` and `MENU_WORLD_LIST_BOTTOM_Y`. While
/// they fit at full size the rows keep their reference look; past that the pills shrink down to
/// `MENU_WORLD_PILL_MIN_H` and the gaps absorb the remainder.
fn world_row_layout(count: usize) -> WorldRowLayout {
    let count = count.min(WORLD_SELECT_KEYS.len());
    let n = count.max(1) as f32;
    let available = MENU_WORLD_LIST_BOTTOM_Y - MENU_WORLD_LIST_Y;
    let full_needed = n * MENU_WORLD_PILL_H + (n - 1.0) * MENU_WORLD_ROW_GAP;
    let pill_h = if full_needed <= available {
        MENU_WORLD_PILL_H
    } else {
        let scale = available / full_needed;
        (MENU_WORLD_PILL_H * scale).clamp(MENU_WORLD_PILL_MIN_H, MENU_WORLD_PILL_H)
    };
    let gap = if count <= 1 {
        MENU_WORLD_ROW_GAP
    } else {
        ((available - n * pill_h) / (n - 1.0)).clamp(0.0, MENU_WORLD_ROW_GAP)
    };
    let ratio = pill_h / MENU_WORLD_PILL_H;
    let text_size = ((f32::from(MENU_WORLD_TEXT_SIZE) * ratio).round() as u16)
        .clamp(MENU_WORLD_TEXT_SIZE_FLOOR, MENU_WORLD_TEXT_SIZE);
    WorldRowLayout {
        step: pill_h + gap,
        pill_h,
        text_dy: MENU_WORLD_TEXT_DY * ratio,
        text_size,
    }
}

/// Seeds the virtual cursor X when the grab starts. A session that is already running keeps the
/// paddle where it is (the accumulated raw mouse position has drifted from the window); only a
/// session starting from the menu seeds from the real cursor.
fn seed_virtual_cursor_x(
    session_live: bool,
    paddle_screen_x: f32,
    mouse_x: f32,
    min_x: f32,
    max_x: f32,
) -> f32 {
    let source = if session_live {
        paddle_screen_x
    } else {
        mouse_x
    };
    source.clamp(min_x, max_x)
}

/// True when a single-frame raw delta is larger than the whole on-screen arena: the device
/// reports absolute coordinates (remote desktop, VM integration, tablet) and the grab is unusable.
fn absolute_input_detected(delta: f32, extent: f32) -> bool {
    delta.abs() > extent
}

/// True when the window lost focus while the game is being played, so it must pause.
#[cfg(not(target_arch = "wasm32"))]
fn focus_loss_pauses(in_game: bool, playing: bool, focus_lost: bool) -> bool {
    in_game && playing && focus_lost
}

/// Records window focus loss events forwarded by macroquad (native builds).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct FocusWatcher {
    lost: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl miniquad::EventHandler for FocusWatcher {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn window_minimized_event(&mut self) {
        self.lost = true;
    }
}

/// Moves the virtual cursor X by a raw mouse delta, clamped to the on-screen arena extent.
fn advance_virtual_cursor_x(current: f32, delta: f32, min_x: f32, max_x: f32) -> f32 {
    (current + delta).clamp(min_x, max_x)
}

/// Confines the OS cursor to the window on native builds. The web build uses its own pointer
/// lock (see `set_pointer_lock`); macroquad's grab would request a second one there.
fn set_native_cursor_grab(grab: bool) {
    #[cfg(not(target_arch = "wasm32"))]
    set_cursor_grab(grab);
    #[cfg(target_arch = "wasm32")]
    let _ = grab;
}

/// Input gestures that will make the pointer lock wanted from the given state (see
/// `lock_wanted` in the main loop); the page uses it to request the lock inside the gesture.
fn resume_mask_for(screen: AppScreen, status: SessionStatus) -> u32 {
    match screen {
        AppScreen::TitleMenu => {
            let base = RESUME_CLICK | RESUME_SPACE | RESUME_ENTER | RESUME_CHEAT_MASK;
            // Tab from the title enters the game and wants the lock when the world is playing.
            if status == SessionStatus::Playing {
                base | RESUME_TAB
            } else {
                base
            }
        }
        AppScreen::Editor => RESUME_F4 | RESUME_CHEAT_MASK,
        AppScreen::InGame => match status {
            SessionStatus::Playing => 0,
            SessionStatus::Paused => RESUME_P | RESUME_R | RESUME_CHEAT_MASK,
            SessionStatus::Victory | SessionStatus::GameOver => {
                RESUME_CLICK | RESUME_SPACE | RESUME_R | RESUME_CHEAT_MASK
            }
        },
    }
}

/// Fills the glyph atlas with every glyph and size the UI uses, before the first frame, so the
/// atlas is never grown (and its texture recreated) in the middle of a frame.
fn prewarm_font(font: &Font, raw_locales: &[&str], levels: &[LevelDef], titles: &[&str]) {
    let mut chars: BTreeSet<char> = Font::ascii_character_list()
        .into_iter()
        .filter(|c| !c.is_control())
        .collect();
    let printable = |c: &char| !c.is_control();
    for raw in raw_locales {
        chars.extend(raw.chars().filter(printable));
    }
    for level in levels {
        chars.extend(level.name.chars().filter(printable));
    }
    let chars: Vec<char> = chars.into_iter().collect();

    // Glyphs are cached per physical pixel size (font size times the DPI scale, rounded up).
    let dpi = miniquad::window::dpi_scale();
    let physical = |size: u16| (size as f32 * dpi).ceil() as u16;
    let sizes: BTreeSet<u16> = UI_FONT_SIZES
        .iter()
        .copied()
        .chain(BADGE_FONT_SIZE_MIN..=BADGE_FONT_SIZE_MAX)
        .chain(MENU_WORLD_TEXT_SIZE_FLOOR..=MENU_WORLD_TEXT_SIZE)
        .map(physical)
        .collect();
    for size in sizes {
        font.populate_font_cache(&chars, size);
    }

    let title_chars: Vec<char> = titles
        .iter()
        .flat_map(|t| t.chars())
        .filter(printable)
        .collect::<BTreeSet<char>>()
        .into_iter()
        .collect();
    font.populate_font_cache(&title_chars, physical(MENU_TITLE_SIZE));
}

struct HudContext<'a> {
    world: &'a GameWorld,
    levels: &'a LevelPack,
    theme: &'a ThemeDef,
    i18n: &'a I18nCatalog,
    lang: Language,
    font: &'a Font,
    editor_mode: bool,
    rt_w: u32,
    rt_h: u32,
}

fn txt(font: &Font, text: &str, x: f32, y: f32, size: u16, color: Color) {
    draw_text_ex(
        text,
        x + 2.0,
        y + 2.0,
        TextParams {
            font: Some(font),
            font_size: size,
            color: Color::new(0.0, 0.0, 0.0, 0.65 * color.a),
            ..Default::default()
        },
    );
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: Some(font),
            font_size: size,
            color,
            ..Default::default()
        },
    );
}

fn window_conf() -> Conf {
    Conf {
        window_title: WINDOW_TITLE.to_owned(),
        window_width: WINDOW_WIDTH,
        window_height: WINDOW_HEIGHT,
        window_resizable: true,
        high_dpi: true,
        sample_count: WINDOW_SAMPLE_COUNT,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let config: GameConfig =
        serde_json::from_str(RAW_GAME_CONFIG).expect("Invalid game_config.json");
    let theme_reg: ThemeRegistry = serde_json::from_str(RAW_THEMES).expect("Invalid themes.json");
    let level_pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("Invalid levels.json");
    let catalog_en = I18nCatalog::from_json_str(RAW_EN).expect("Invalid en.json");
    let catalog_fr = I18nCatalog::from_json_str(RAW_FR).expect("Invalid fr.json");
    let ui_font = load_ttf_font_from_bytes(RAW_FONT).expect("Failed to load embedded UI font");
    prewarm_font(
        &ui_font,
        &[RAW_EN, RAW_FR],
        &level_pack.levels,
        &[catalog_en.t("app.title"), catalog_fr.t("app.title")],
    );

    let add_mat = additive_material();
    let neb_mat = nebula_material();

    let starfield = Starfield::new(
        config.arena.width,
        config.arena.height,
        STARFIELD_BASE_COUNT,
        STARFIELD_EXTENT_FACTOR,
    );
    let mut warp_grid = WarpGrid::new(
        config.arena.wall_side,
        config.arena.wall_top,
        config.arena.width - config.arena.wall_side,
        config.arena.height,
        WARP_GRID_STEP,
    );

    let (saved_hs, saved_unlocked, saved_theme, saved_fr) = load_progress();
    let mut sfx = SoundBank::init().await;
    let mut lang = if saved_fr { Language::Fr } else { Language::En };
    set_document_lang(lang == Language::Fr);
    let mut theme_idx: usize = saved_theme % theme_reg.themes.len();
    let mut world = GameWorld::new(config.clone(), &level_pack.levels);
    let seed = (miniquad::date::now() * MS_PER_SEC) as u64;
    world.reseed(seed);
    macroquad::rand::srand(seed);
    world.high_score = saved_hs;
    world.max_unlocked_level = saved_unlocked.min(level_pack.levels.len().saturating_sub(1));

    #[cfg(not(target_arch = "wasm32"))]
    prevent_quit();
    let mut save_tracker = SaveTracker::new(saved_hs);
    let mut last_mouse_x: f32 = 0.0;
    let mut virtual_cursor_x: f32 = 0.0;
    // True once a grab has run in the current game session (reset when leaving the game screen).
    let mut grab_session_live = false;
    // Set when the pointer reports absolute coordinates: the grab is dropped for the process.
    let mut grab_disabled = false;
    #[cfg(not(target_arch = "wasm32"))]
    let focus_subscriber = macroquad::input::utils::register_input_subscriber();
    #[cfg(not(target_arch = "wasm32"))]
    let mut focus_watcher = FocusWatcher::default();

    let mut screen = AppScreen::TitleMenu;
    let worlds = build_worlds(&level_pack.levels);
    let mut selected_world_pos: usize = 0;
    let mut selected_sector_idx: usize = 0;

    let mut events: Vec<GameEvent> = Vec::with_capacity(MAX_EVENTS);
    let mut particles: Vec<Particle> = Vec::with_capacity(MAX_PARTICLES);
    let mut shards: Vec<DebrisShard> = Vec::with_capacity(MAX_SHARDS);
    let mut shockwaves: Vec<Shockwave> = Vec::with_capacity(MAX_SHOCKWAVES);
    let mut arcs: Vec<LightningArc> = Vec::with_capacity(MAX_ARCS);
    let mut popups: Vec<FloatingPopup> = Vec::with_capacity(MAX_POPUPS);
    let mut screen_shake: f32 = 0.0;
    let mut chromatic_pulse: f32 = 0.0;
    let mut hit_stop_timer: f32 = 0.0;

    let mut last_lock_wanted = false;
    let mut last_resume_mask: Option<u32> = None;
    let mut lock_was_active = false;
    let mut last_pointer_bounds: Option<(f32, f32)> = None;
    let mut editor_brush = BrickKind::Standard {
        hp: 1,
        max_hp: 1,
        color_idx: 0,
    };

    loop {
        let now_sec = get_time();
        let time = now_sec as f32;
        #[cfg(not(target_arch = "wasm32"))]
        if is_quit_requested() {
            set_native_cursor_grab(false);
            save_tracker.persist(&world, theme_idx, lang == Language::Fr, now_sec);
            break;
        }
        let raw_dt = get_frame_time().min(MAX_FRAME_DT);
        if hit_stop_timer > 0.0 {
            hit_stop_timer = (hit_stop_timer - raw_dt).max(0.0);
        }
        let dt = if hit_stop_timer > 0.0 {
            raw_dt * HIT_STOP_TIME_SCALE
        } else {
            raw_dt
        };

        let i18n = match lang {
            Language::En => &catalog_en,
            Language::Fr => &catalog_fr,
        };
        let theme = &theme_reg.themes[theme_idx % theme_reg.themes.len()];

        sfx.update_music(
            world.boss.is_some(),
            world.combo,
            screen == AppScreen::TitleMenu,
        );

        // Global hotkeys
        if is_key_pressed(KeyCode::F1) {
            theme_idx = (theme_idx + 1) % theme_reg.themes.len();
            save_tracker.persist(&world, theme_idx, lang == Language::Fr, now_sec);
        }
        if is_key_pressed(KeyCode::F2) {
            lang = lang.next();
            set_document_lang(lang == Language::Fr);
            save_tracker.persist(&world, theme_idx, lang == Language::Fr, now_sec);
        }
        if LEVEL_CHEAT_ENABLED && is_key_pressed(KeyCode::F3) {
            let next_lvl = (world.current_level_idx + 1) % level_pack.levels.len();
            world.load_level(next_lvl, &level_pack.levels);
            screen = AppScreen::InGame;
        }
        if is_key_pressed(KeyCode::F4) {
            screen = match screen {
                AppScreen::Editor => AppScreen::InGame,
                _ => AppScreen::Editor,
            };
            world.reset_paddle_and_ball();
            world.status = SessionStatus::Playing;
        }
        if is_key_pressed(KeyCode::Tab) {
            screen = match screen {
                AppScreen::TitleMenu => AppScreen::InGame,
                _ => AppScreen::TitleMenu,
            };
        }
        if is_key_pressed(KeyCode::M) {
            sfx.muted = !sfx.muted;
        }
        if is_key_pressed(KeyCode::B) {
            sfx.music_enabled = !sfx.music_enabled;
        }

        let rt_w = (screen_width() * miniquad::window::dpi_scale()).max(MIN_RENDER_W) as u32;
        let rt_h = (screen_height() * miniquad::window::dpi_scale()).max(MIN_RENDER_H) as u32;

        let arena_w = world.config.arena.width;
        let arena_h = world.config.arena.height;
        let (view_scale, view_ox, view_oy) =
            letterbox(screen_width(), screen_height(), arena_w, arena_h);
        let (mx, my) = mouse_position();
        // With the native cursor grab, `mouse_position` accumulates raw deltas without bound: the
        // paddle follows a virtual X clamped to the arena instead. The web build is untouched.
        let cursor_min_x = view_ox;
        let cursor_max_x = view_ox + arena_w * view_scale;
        let mut native_grab_active =
            cfg!(not(target_arch = "wasm32")) && last_lock_wanted && !grab_disabled;
        if native_grab_active {
            let delta = (mx - last_mouse_x) * NATIVE_RAW_MOUSE_SENSITIVITY;
            if absolute_input_detected(delta, cursor_max_x - cursor_min_x) {
                grab_disabled = true;
                native_grab_active = false;
                set_native_cursor_grab(false);
            } else {
                virtual_cursor_x =
                    advance_virtual_cursor_x(virtual_cursor_x, delta, cursor_min_x, cursor_max_x);
            }
        }
        last_mouse_x = mx;
        let paddle_screen_x = if native_grab_active {
            virtual_cursor_x
        } else {
            mx
        };
        let (world_mx, world_my) = screen_to_arena(mx, my, view_scale, view_ox, view_oy);
        let (paddle_mx, _) = screen_to_arena(paddle_screen_x, my, view_scale, view_ox, view_oy);

        // The browser can release the lock on its own (Esc, Alt-Tab, focus loss): pause then.
        // Only an active -> inactive transition counts (lock_was_active is false during the async
        // gap after a request), and only while the game is still being played.
        if lock_was_active
            && !pointer_lock_active()
            && screen == AppScreen::InGame
            && world.status == SessionStatus::Playing
        {
            world.status = SessionStatus::Paused;
        }

        // Keep the browser-side virtual cursor inside the letterboxed arena.
        let bounds = (view_ox, view_ox + arena_w * view_scale);
        if last_pointer_bounds != Some(bounds) {
            set_pointer_bounds(bounds.0, bounds.1);
            last_pointer_bounds = Some(bounds);
        }

        // Native builds have no pointer-lock release: pause when the window loses focus so the
        // grab is dropped and the cursor comes back.
        #[cfg(not(target_arch = "wasm32"))]
        {
            macroquad::input::utils::repeat_all_miniquad_input(
                &mut focus_watcher,
                focus_subscriber,
            );
            let focus_lost = std::mem::take(&mut focus_watcher.lost);
            if focus_loss_pauses(
                screen == AppScreen::InGame,
                world.status == SessionStatus::Playing,
                focus_lost,
            ) {
                world.status = SessionStatus::Paused;
            }
        }
        if screen != AppScreen::InGame {
            grab_session_live = false;
        }

        match screen {
            AppScreen::TitleMenu => {
                let max_unlocked = world.max_unlocked_level;
                if !worlds.is_empty() {
                    for (n, key) in WORLD_SELECT_KEYS.iter().enumerate() {
                        if is_key_pressed(*key) {
                            if let Some(&(_, first_idx)) = worlds.get(n) {
                                if is_world_unlocked(first_idx, max_unlocked) {
                                    selected_world_pos = n;
                                    selected_sector_idx = first_idx;
                                }
                            }
                        }
                    }
                    if is_key_pressed(KeyCode::Left) {
                        selected_world_pos =
                            step_unlocked_world(&worlds, selected_world_pos, false, max_unlocked);
                        selected_sector_idx = worlds[selected_world_pos].1;
                    }
                    if is_key_pressed(KeyCode::Right) {
                        selected_world_pos =
                            step_unlocked_world(&worlds, selected_world_pos, true, max_unlocked);
                        selected_sector_idx = worlds[selected_world_pos].1;
                    }
                }
                let last_sector = level_pack.levels.len().saturating_sub(1);
                if is_key_pressed(KeyCode::Up) {
                    selected_sector_idx =
                        clamp_sector(selected_sector_idx + 1, max_unlocked, last_sector);
                    selected_world_pos = world_pos_for_level(
                        &worlds,
                        &level_pack.levels,
                        selected_sector_idx,
                        selected_world_pos,
                    );
                }
                if is_key_pressed(KeyCode::Down) {
                    selected_sector_idx = selected_sector_idx.saturating_sub(1);
                    selected_world_pos = world_pos_for_level(
                        &worlds,
                        &level_pack.levels,
                        selected_sector_idx,
                        selected_world_pos,
                    );
                }
                selected_sector_idx = clamp_sector(selected_sector_idx, max_unlocked, last_sector);

                if (is_key_pressed(KeyCode::Enter)
                    || is_key_pressed(KeyCode::Space)
                    || is_mouse_button_pressed(MouseButton::Left))
                    && is_sector_unlocked(selected_sector_idx, max_unlocked)
                {
                    world.lives = world.config.gameplay.initial_lives;
                    world.score = 0;
                    world.load_level(selected_sector_idx, &level_pack.levels);
                    screen = AppScreen::InGame;
                }
            }
            AppScreen::Editor => {
                for (idx, key) in [
                    KeyCode::Key1,
                    KeyCode::Key2,
                    KeyCode::Key3,
                    KeyCode::Key4,
                    KeyCode::Key5,
                    KeyCode::Key6,
                ]
                .iter()
                .enumerate()
                {
                    if is_key_pressed(*key) {
                        let hp = if idx >= 4 { 2 } else { 1 };
                        editor_brush = BrickKind::Standard {
                            hp,
                            max_hp: hp,
                            color_idx: idx,
                        };
                    }
                }
                if is_key_pressed(KeyCode::E) {
                    editor_brush = BrickKind::Explosive;
                }
                if is_key_pressed(KeyCode::H) || is_key_pressed(KeyCode::GraveAccent) {
                    editor_brush = BrickKind::Indestructible;
                }
                if is_key_pressed(KeyCode::T) {
                    editor_brush = BrickKind::Portal;
                }
                if is_key_pressed(KeyCode::G) {
                    editor_brush = BrickKind::Ghost { revealed: false };
                }
                if is_key_pressed(KeyCode::V) {
                    editor_brush = BrickKind::Moving {
                        hp: 2,
                        color_idx: 3,
                    };
                }
                if is_key_pressed(KeyCode::C) {
                    world.clear_bricks();
                }
                if is_key_pressed(KeyCode::X) {
                    let code = world.export_level_code();
                    share_level_code(&code, i18n.t("editor.share_prompt"));
                    popups.push(FloatingPopup {
                        x: POPUP_X,
                        y: POPUP_Y,
                        text: i18n.t("editor.exported").to_string(),
                        life: POPUP_LIFE_SEC,
                        color: rgba(theme.text_accent),
                    });
                }
                if is_key_pressed(KeyCode::I) {
                    match prompt_level_code(i18n.t("editor.paste_prompt")) {
                        Ok(Some(code)) => match world.import_level_code(&code) {
                            Ok(()) => popups.push(FloatingPopup {
                                x: POPUP_X,
                                y: POPUP_Y,
                                text: i18n.t("editor.imported").to_string(),
                                life: POPUP_LIFE_SEC,
                                color: rgba(theme.text_accent),
                            }),
                            Err(err) => popups.push(error_popup(i18n, &err)),
                        },
                        Ok(None) => {}
                        Err(err) => popups.push(error_popup(i18n, &err)),
                    }
                }
                if is_mouse_button_down(MouseButton::Left) {
                    world.paint_brick_at(world_mx, world_my, Some(editor_brush));
                } else if is_mouse_button_down(MouseButton::Right) {
                    world.paint_brick_at(world_mx, world_my, None);
                }
            }
            AppScreen::InGame => {
                // P toggles pause; Escape only pauses (the browser also uses it to release the lock).
                let toggle_pause = is_key_pressed(KeyCode::P);
                let pause_only = is_key_pressed(KeyCode::Escape);
                if toggle_pause || pause_only {
                    world.status = match world.status {
                        SessionStatus::Playing => SessionStatus::Paused,
                        SessionStatus::Paused if toggle_pause => SessionStatus::Playing,
                        other => other,
                    };
                }
                if is_key_pressed(KeyCode::R) {
                    let cur = world.current_level_idx;
                    world.lives = world.config.gameplay.initial_lives;
                    world.score = 0;
                    world.load_level(cur, &level_pack.levels);
                }

                world.set_paddle_target(paddle_mx);

                let kb_speed = KEYBOARD_PADDLE_SPEED * dt;
                if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) || is_key_down(KeyCode::Q)
                {
                    world.set_paddle_target(world.paddle_x - kb_speed);
                }
                if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
                    world.set_paddle_target(world.paddle_x + kb_speed);
                }

                let vortex_input = is_mouse_button_down(MouseButton::Right)
                    || is_key_down(KeyCode::LeftShift)
                    || is_key_down(KeyCode::RightShift);
                world.set_vortex(vortex_input);
                if world.vortex_active {
                    warp_grid.apply_impulse(
                        world.paddle_x,
                        world.config.arena.paddle_y,
                        280.0,
                        -540.0 * dt,
                    );
                }

                let action_triggered =
                    is_mouse_button_pressed(MouseButton::Left) || is_key_pressed(KeyCode::Space);
                if action_triggered {
                    match world.status {
                        SessionStatus::Playing => world.launch_or_fire(&mut events),
                        SessionStatus::Victory if !world.is_campaign_board() => {
                            // A custom board has no next sector: go back to the editor.
                            screen = AppScreen::Editor;
                            world.reset_paddle_and_ball();
                            world.status = SessionStatus::Playing;
                        }
                        SessionStatus::Victory => {
                            let next_lvl = (world.current_level_idx + 1) % level_pack.levels.len();
                            world.load_level(next_lvl, &level_pack.levels);
                            save_tracker.persist(&world, theme_idx, lang == Language::Fr, now_sec);
                        }
                        SessionStatus::GameOver => {
                            save_tracker.persist(&world, theme_idx, lang == Language::Fr, now_sec);
                            world.lives = world.config.gameplay.initial_lives;
                            world.score = 0;
                            world.load_level(world.current_level_idx, &level_pack.levels);
                        }
                        SessionStatus::Paused => {}
                    }
                }

                let prev_status = world.status;
                world.update(dt, &mut events);
                if prev_status == SessionStatus::Playing && world.status != SessionStatus::Playing {
                    save_tracker.persist(&world, theme_idx, lang == Language::Fr, now_sec);
                }
            }
        }

        // A new high score is persisted without waiting for pause / game over, throttled.
        save_tracker.persist_if_due(&world, theme_idx, lang == Language::Fr, now_sec);

        // Lock the pointer exactly while the player is actually playing; evaluated after input and
        // world update so a start / resume takes effect in the same frame.
        let lock_wanted = screen == AppScreen::InGame && world.status == SessionStatus::Playing;
        if lock_wanted != last_lock_wanted {
            set_pointer_lock(lock_wanted);
            show_mouse(!lock_wanted);
            set_native_cursor_grab(lock_wanted && !grab_disabled);
            if lock_wanted {
                let paddle_screen = view_ox + world.paddle_x * view_scale;
                virtual_cursor_x = seed_virtual_cursor_x(
                    grab_session_live,
                    paddle_screen,
                    mx,
                    cursor_min_x,
                    cursor_max_x,
                );
                grab_session_live = true;
            }
            last_lock_wanted = lock_wanted;
        }
        lock_was_active = lock_wanted && pointer_lock_active();

        // Tell the page which gestures will make the lock wanted next, so it can request the
        // lock synchronously inside the input handler (required by Firefox).
        let resume_mask = resume_mask_for(screen, world.status);
        if last_resume_mask != Some(resume_mask) {
            set_resume_mask(resume_mask);
            last_resume_mask = Some(resume_mask);
        }

        for ball in &world.balls {
            if ball.stuck_offset.is_none() {
                warp_grid.apply_impulse(ball.x, ball.y, 95.0, 120.0 * dt);
            }
        }

        // Process visual & audio events
        for ev in events.drain(..) {
            match ev {
                GameEvent::BrickHit {
                    x,
                    y,
                    color_idx,
                    destroyed,
                    points,
                    multiplier,
                } => {
                    sfx.play_brick(world.combo);
                    warp_grid.apply_impulse(x, y, 165.0, if destroyed { 320.0 } else { 140.0 });
                    let base_col = rgba(theme.brick_palette[color_idx % theme.brick_palette.len()]);
                    let count = if destroyed {
                        BRICK_PARTICLES_DESTROYED
                    } else {
                        BRICK_PARTICLES_HIT
                    };
                    let gravity = if theme.style == RenderStyle::IndustrialMetal {
                        GRAVITY_INDUSTRIAL
                    } else {
                        GRAVITY_DEFAULT
                    };
                    for i in 0..count {
                        let angle = (i as f32 / count as f32) * std::f32::consts::TAU
                            + rand::gen_range(
                                -BRICK_PARTICLE_ANGLE_JITTER,
                                BRICK_PARTICLE_ANGLE_JITTER,
                            );
                        let spd = rand_in(BRICK_PARTICLE_SPEED);
                        particles.push(Particle {
                            x,
                            y,
                            vx: angle.cos() * spd,
                            vy: angle.sin() * spd,
                            size: rand_in(BRICK_PARTICLE_SIZE),
                            life: BRICK_PARTICLE_LIFE,
                            max_life: BRICK_PARTICLE_LIFE,
                            color: base_col,
                            gravity,
                        });
                    }
                    if destroyed {
                        screen_shake =
                            (screen_shake + SHAKE_BRICK_DESTROYED).min(SCREEN_SHAKE_MINOR_MAX);
                        for _ in 0..SHARDS_PER_BRICK {
                            shards.push(DebrisShard {
                                x: x + rand::gen_range(-SHARD_SPREAD_X, SHARD_SPREAD_X),
                                y: y + rand::gen_range(-SHARD_SPREAD_Y, SHARD_SPREAD_Y),
                                vx: rand::gen_range(-SHARD_SPEED_X, SHARD_SPEED_X),
                                vy: rand_in(SHARD_SPEED_Y),
                                w: rand_in(SHARD_WIDTH),
                                angle: rand::gen_range(0.0, std::f32::consts::TAU),
                                v_angle: rand::gen_range(-SHARD_SPIN, SHARD_SPIN),
                                life: SHARD_LIFE,
                                max_life: SHARD_LIFE,
                                color: base_col,
                            });
                        }
                        popups.push(FloatingPopup {
                            x: x + BRICK_POPUP_DX,
                            y,
                            text: format_score_popup(points, multiplier),
                            life: BRICK_POPUP_LIFE,
                            color: base_col,
                        });
                    }
                }
                GameEvent::Explosion { x, y, radius } => {
                    sfx.play_explosion();
                    warp_grid.apply_impulse(x, y, radius * 2.2, 920.0);
                    screen_shake = (screen_shake + SHAKE_EXPLOSION).min(SCREEN_SHAKE_MAX);
                    chromatic_pulse = CHROMATIC_EXPLOSION;
                    hit_stop_timer = HIT_STOP_SEC;
                    shockwaves.push(Shockwave {
                        x,
                        y,
                        radius: EXPLOSION_WAVE_START_RADIUS,
                        max_radius: radius * EXPLOSION_WAVE_RADIUS_FACTOR,
                        life: EXPLOSION_WAVE_LIFE,
                        max_life: EXPLOSION_WAVE_LIFE,
                        color: rgba(theme.fireball_glow),
                    });
                    for _ in 0..EXPLOSION_PARTICLES {
                        let angle = rand::gen_range(0.0, std::f32::consts::TAU);
                        let spd = rand_in(EXPLOSION_PARTICLE_SPEED);
                        particles.push(Particle {
                            x,
                            y,
                            vx: angle.cos() * spd,
                            vy: angle.sin() * spd,
                            size: rand_in(EXPLOSION_PARTICLE_SIZE),
                            life: EXPLOSION_PARTICLE_LIFE,
                            max_life: EXPLOSION_PARTICLE_LIFE,
                            color: rgba(theme.fireball_glow),
                            gravity: EXPLOSION_GRAVITY,
                        });
                    }
                }
                GameEvent::LightningBolt { from, to } => {
                    sfx.play_lightning();
                    warp_grid.apply_impulse(to.0, to.1, 180.0, 520.0);
                    screen_shake = (screen_shake + SHAKE_LIGHTNING).min(SCREEN_SHAKE_MINOR_MAX);
                    chromatic_pulse = CHROMATIC_LIGHTNING;
                    arcs.push(LightningArc {
                        from,
                        to,
                        life: LIGHTNING_ARC_LIFE,
                    });
                }
                GameEvent::PortalWarp { from, to } => {
                    sfx.play_portal();
                    warp_grid.apply_impulse(from.0, from.1, 170.0, -420.0);
                    warp_grid.apply_impulse(to.0, to.1, 190.0, 520.0);
                    shockwaves.push(Shockwave {
                        x: from.0,
                        y: from.1,
                        radius: PORTAL_WAVE_START_RADIUS,
                        max_radius: PORTAL_WAVE_FROM.0,
                        life: PORTAL_WAVE_FROM.1,
                        max_life: PORTAL_WAVE_FROM.1,
                        color: rgba(theme.lightning_color),
                    });
                    shockwaves.push(Shockwave {
                        x: to.0,
                        y: to.1,
                        radius: PORTAL_WAVE_START_RADIUS,
                        max_radius: PORTAL_WAVE_TO.0,
                        life: PORTAL_WAVE_TO.1,
                        max_life: PORTAL_WAVE_TO.1,
                        color: rgba(theme.paddle_accent),
                    });
                    popups.push(FloatingPopup {
                        x: to.0 + PORTAL_POPUP_DX,
                        y: to.1 + PORTAL_POPUP_DY,
                        text: i18n.t("status.portal_warp").to_string(),
                        life: PORTAL_POPUP_LIFE,
                        color: rgba(theme.lightning_color),
                    });
                }
                GameEvent::PaddleHit { x, y } => {
                    sfx.play_paddle();
                    warp_grid.apply_impulse(x, y, 130.0, 260.0);
                    for _ in 0..PADDLE_PARTICLES {
                        let angle = rand::gen_range(0.0, std::f32::consts::TAU);
                        let spd = rand_in(PADDLE_PARTICLE_SPEED);
                        particles.push(Particle {
                            x,
                            y,
                            vx: angle.cos() * spd,
                            vy: angle.sin() * spd,
                            size: PADDLE_PARTICLE_SIZE,
                            life: PADDLE_PARTICLE_LIFE,
                            max_life: PADDLE_PARTICLE_LIFE,
                            color: rgba(theme.paddle_core),
                            gravity: 0.0,
                        });
                    }
                }
                GameEvent::WallHit { x, y } => {
                    sfx.play_wall();
                    warp_grid.apply_impulse(x, y, 100.0, 180.0);
                    for _ in 0..WALL_PARTICLES {
                        let angle = rand::gen_range(0.0, std::f32::consts::TAU);
                        let spd = rand_in(WALL_PARTICLE_SPEED);
                        particles.push(Particle {
                            x,
                            y,
                            vx: angle.cos() * spd,
                            vy: angle.sin() * spd,
                            size: WALL_PARTICLE_SIZE,
                            life: WALL_PARTICLE_LIFE,
                            max_life: WALL_PARTICLE_LIFE,
                            color: rgba(theme.border_color),
                            gravity: 0.0,
                        });
                    }
                }
                GameEvent::PowerUpCollected { kind, x, y } => {
                    sfx.play_powerup();
                    warp_grid.apply_impulse(x, y, 150.0, 360.0);
                    let pop_col = rgba(if kind.is_malus() {
                        MALUS_COLOR
                    } else {
                        BONUS_COLOR
                    });
                    shockwaves.push(Shockwave {
                        x,
                        y,
                        radius: POWERUP_WAVE_START_RADIUS,
                        max_radius: POWERUP_WAVE_RADIUS,
                        life: POWERUP_WAVE_LIFE,
                        max_life: POWERUP_WAVE_LIFE,
                        color: pop_col,
                    });
                    let label = i18n.t(kind.i18n_key()).to_string();
                    popups.push(FloatingPopup {
                        x: (x + POWERUP_POPUP_DX).clamp(POPUP_MIN_X, POPUP_MAX_X),
                        y: y + POWERUP_POPUP_DY,
                        text: label,
                        life: POWERUP_POPUP_LIFE,
                        color: pop_col,
                    });
                }
                GameEvent::LaserFired { x, y } => {
                    sfx.play_laser();
                    particles.push(Particle {
                        x,
                        y,
                        vx: 0.0,
                        vy: LASER_PARTICLE_VY,
                        size: LASER_PARTICLE_SIZE,
                        life: LASER_PARTICLE_LIFE,
                        max_life: LASER_PARTICLE_LIFE,
                        color: rgba(theme.paddle_accent),
                        gravity: 0.0,
                    });
                }
            }
        }
        cap_oldest(&mut particles, MAX_PARTICLES);
        cap_oldest(&mut shards, MAX_SHARDS);
        cap_oldest(&mut shockwaves, MAX_SHOCKWAVES);
        cap_oldest(&mut arcs, MAX_ARCS);
        cap_oldest(&mut popups, MAX_POPUPS);

        warp_grid.update(raw_dt);
        if screen_shake > 0.0 {
            screen_shake = (screen_shake - raw_dt * SCREEN_SHAKE_DECAY_PER_SEC).max(0.0);
        }
        if chromatic_pulse > 0.0 {
            chromatic_pulse = (chromatic_pulse - raw_dt * CHROMATIC_DECAY_PER_SEC).max(0.0);
        }
        particles.retain_mut(|p| {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += p.gravity * dt;
            p.life -= raw_dt;
            p.life > 0.0
        });
        shards.retain_mut(|s| {
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.vy += SHARD_GRAVITY * dt;
            s.angle += s.v_angle * dt;
            s.life -= raw_dt;
            s.life > 0.0
        });
        shockwaves.retain_mut(|s| {
            s.life -= raw_dt;
            let progress = 1.0 - (s.life / s.max_life).clamp(0.0, 1.0);
            s.radius = s.max_radius * progress;
            s.life > 0.0
        });
        arcs.retain_mut(|a| {
            a.life -= raw_dt;
            a.life > 0.0
        });
        popups.retain_mut(|pop| {
            pop.y -= POPUP_RISE_SPEED * raw_dt;
            pop.life -= raw_dt;
            pop.life > 0.0
        });

        let shake_x = if screen_shake > 0.0 {
            rand::gen_range(-screen_shake, screen_shake)
        } else {
            0.0
        };
        let shake_y = if screen_shake > 0.0 {
            rand::gen_range(-screen_shake, screen_shake)
        } else {
            0.0
        };

        // Backdrop pass: nebula and stars fill the whole window, bars included.
        let backdrop = backdrop_rect(screen_width(), screen_height(), arena_w, arena_h);
        let shake_target = vec2(arena_w * 0.5 + shake_x, arena_h * 0.5 + shake_y);
        set_default_camera();
        clear_background(LETTERBOX_COLOR);
        set_camera(&Camera2D {
            zoom: vec2(2.0 / backdrop.w, 2.0 / backdrop.h),
            target: shake_target,
            ..Default::default()
        });
        // Drawn rect inflated by the shake amplitude so the shaking camera never shows the clear colour.
        let backdrop_drawn = Rect::new(
            backdrop.x - SCREEN_SHAKE_MAX,
            backdrop.y - SCREEN_SHAKE_MAX,
            backdrop.w + 2.0 * SCREEN_SHAKE_MAX,
            backdrop.h + 2.0 * SCREEN_SHAKE_MAX,
        );
        draw_backdrop(&world, theme, &neb_mat, &starfield, backdrop_drawn, time);

        // Dim the bars (screen space) so the playfield stays readable.
        set_default_camera();
        let (sw, sh) = (screen_width(), screen_height());
        let dim = Color::new(0.0, 0.0, 0.0, LETTERBOX_DIM_ALPHA);
        draw_rectangle(0.0, 0.0, view_ox, sh, dim);
        draw_rectangle(sw - view_ox, 0.0, view_ox, sh, dim);
        draw_rectangle(view_ox, 0.0, sw - view_ox * 2.0, view_oy, dim);
        draw_rectangle(view_ox, sh - view_oy, sw - view_ox * 2.0, view_oy, dim);

        // Arena pass: gameplay rendered into the centred 16:9 viewport.
        set_camera(&Camera2D {
            zoom: vec2(2.0 / arena_w, 2.0 / arena_h),
            target: shake_target,
            viewport: Some(arena_viewport(
                view_scale,
                view_ox,
                view_oy,
                arena_w,
                arena_h,
                miniquad::window::dpi_scale(),
            )),
            ..Default::default()
        });

        draw_arena_background(&world, theme, &warp_grid, chromatic_pulse);
        draw_bricks(&world, theme, &ui_font, time);
        draw_boss_and_hazards(&world, theme, i18n, &ui_font);
        draw_paddle_and_lasers(&world, theme, time);

        gl_use_material(&add_mat);
        draw_balls(&world, theme, chromatic_pulse);
        draw_additive_fx(&particles, &shards, &shockwaves, &arcs, theme);
        gl_use_default_material();

        // Draw capsules above additive explosions/glows so icons stay 100% crisp and readable
        draw_capsules(&world, i18n, &ui_font, time);
        draw_popups(&popups, &ui_font);
        draw_hud(&HudContext {
            world: &world,
            levels: &level_pack,
            theme,
            i18n,
            lang,
            font: &ui_font,
            editor_mode: screen == AppScreen::Editor,
            rt_w,
            rt_h,
        });

        if screen == AppScreen::TitleMenu {
            draw_title_menu(
                &world,
                &level_pack,
                theme,
                i18n,
                &ui_font,
                &MenuSelection {
                    worlds: &worlds,
                    world_pos: selected_world_pos,
                    sector_idx: selected_sector_idx,
                },
            );
        }

        next_frame().await
    }
}

/// Title menu selection state: the world list and the highlighted world / start sector.
struct MenuSelection<'a> {
    worlds: &'a [(usize, usize)],
    world_pos: usize,
    sector_idx: usize,
}

fn draw_title_menu(
    world: &GameWorld,
    levels: &LevelPack,
    theme: &ThemeDef,
    i18n: &I18nCatalog,
    font: &Font,
    selection: &MenuSelection,
) {
    let MenuSelection {
        worlds,
        world_pos: selected_world_pos,
        sector_idx: selected_sector_idx,
    } = *selection;
    let w = world.config.arena.width;
    let h = world.config.arena.height;
    draw_rectangle(0.0, 0.0, w, h, rgba(MENU_DIM_COLOR));
    let pri = rgba(theme.text_primary);
    let acc = rgba(theme.text_accent);
    let border = rgba(theme.border_color);

    // Modern Glassmorphic Modal Panel
    draw_rounded_rect(
        MENU_PANEL_X,
        MENU_PANEL_Y,
        MENU_PANEL_W,
        MENU_PANEL_H,
        MENU_PANEL_RADIUS,
        border,
    );
    draw_rounded_rect(
        MENU_PANEL_X + MENU_PANEL_BORDER,
        MENU_PANEL_Y + MENU_PANEL_BORDER,
        MENU_PANEL_W - 2.0 * MENU_PANEL_BORDER,
        MENU_PANEL_H - 2.0 * MENU_PANEL_BORDER,
        MENU_PANEL_RADIUS - MENU_PANEL_BORDER,
        rgba(MENU_PANEL_COLOR),
    );
    draw_rounded_rect(
        MENU_PANEL_X + MENU_HEADER_INSET_X,
        MENU_PANEL_Y + MENU_HEADER_INSET_Y,
        MENU_PANEL_W - 2.0 * MENU_HEADER_INSET_X,
        MENU_HEADER_H,
        MENU_HEADER_RADIUS,
        Color::new(1.0, 1.0, 1.0, 0.05),
    );

    txt(
        font,
        i18n.t("app.title"),
        MENU_PANEL_X + MENU_TITLE_DX,
        MENU_TITLE_Y,
        MENU_TITLE_SIZE,
        acc,
    );
    txt(
        font,
        i18n.t("app.subtitle"),
        MENU_PANEL_X + MENU_SUBTITLE_DX,
        MENU_SUBTITLE_Y,
        MENU_SUBTITLE_SIZE,
        pri,
    );

    draw_rounded_rect(
        MENU_PANEL_X + MENU_BEST_PILL_DX,
        MENU_BEST_PILL_Y,
        MENU_BEST_PILL_W,
        MENU_BEST_PILL_H,
        MENU_BEST_PILL_RADIUS,
        Color::new(acc.r, acc.g, acc.b, MENU_BEST_PILL_ALPHA),
    );
    let best_line = format!("{}: {:07}", i18n.t("menu.best_score"), world.high_score);
    txt(
        font,
        &best_line,
        MENU_PANEL_X + MENU_BEST_TEXT_DX,
        MENU_BEST_TEXT_Y,
        MENU_BEST_TEXT_SIZE,
        acc,
    );

    let campaigns_line = substitute_count(
        i18n.t("menu.campaigns"),
        worlds.len().clamp(1, WORLD_SELECT_KEYS.len()),
    );
    txt(
        font,
        &campaigns_line,
        MENU_PANEL_X + MENU_LABEL_DX,
        MENU_CAMPAIGNS_Y,
        MENU_LABEL_SIZE,
        pri,
    );
    let row_layout = world_row_layout(worlds.len());
    for (pos, &(world_id, first_idx)) in worlds.iter().take(WORLD_SELECT_KEYS.len()).enumerate() {
        let unlocked = is_world_unlocked(first_idx, world.max_unlocked_level);
        let y = MENU_WORLD_LIST_Y + pos as f32 * row_layout.step;
        let is_sel = selected_world_pos == pos;
        let pill_bg = if is_sel {
            Color::new(acc.r, acc.g, acc.b, MENU_PILL_SELECTED_ALPHA)
        } else {
            rgba(MENU_PILL_IDLE_COLOR)
        };
        draw_rounded_rect(
            MENU_PANEL_X + MENU_WORLD_PILL_DX,
            y,
            MENU_WORLD_PILL_W,
            row_layout.pill_h,
            MENU_WORLD_PILL_RADIUS.min(row_layout.pill_h * 0.5),
            pill_bg,
        );
        let base_col = if is_sel { acc } else { pri };
        let col = if unlocked {
            base_col
        } else {
            Color::new(base_col.r, base_col.g, base_col.b, MENU_LOCKED_ALPHA)
        };
        let prefix = if is_sel {
            MENU_SELECTED_PREFIX
        } else {
            MENU_UNSELECTED_PREFIX
        };
        let label_key = format!("campaigns.c{world_id}");
        let row_text = format!("{}{}", prefix, i18n.t(&label_key));
        let text_x = MENU_PANEL_X + MENU_WORLD_TEXT_DX;
        let pill_right = MENU_PANEL_X + MENU_WORLD_PILL_DX + MENU_WORLD_PILL_W;
        // A locked row reserves room for its label on the right and shrinks the name to fit.
        let lock_label = i18n.t("menu.locked");
        let lock_w = if unlocked {
            0.0
        } else {
            measure_text(lock_label, Some(font), row_layout.text_size, 1.0).width + MENU_LOCK_GAP
        };
        let text_limit = pill_right - MENU_LOCK_LABEL_PAD - lock_w - text_x;
        let row_size = fit_font_size(
            |size| measure_text(&row_text, Some(font), size, 1.0).width,
            row_layout.text_size,
            MENU_WORLD_TEXT_SIZE_MIN.min(row_layout.text_size),
            text_limit,
        );
        txt(
            font,
            &row_text,
            text_x,
            y + row_layout.text_dy,
            row_size,
            col,
        );
        if !unlocked {
            let label_w = lock_w - MENU_LOCK_GAP;
            txt(
                font,
                lock_label,
                pill_right - MENU_LOCK_LABEL_PAD - label_w,
                y + row_layout.text_dy,
                row_layout.text_size,
                col,
            );
        }
    }

    let sec_def = &levels.levels[selected_sector_idx % levels.levels.len()];
    let sec_line = format!(
        "{} #{} - {}",
        i18n.t("menu.sector_select"),
        sec_def.id,
        sec_def.name
    );
    txt(
        font,
        &sec_line,
        MENU_PANEL_X + MENU_LABEL_DX,
        MENU_SECTOR_Y,
        MENU_SECTOR_SIZE,
        acc,
    );

    // Glowing CTA Pill Button
    draw_rounded_rect(
        MENU_CTA_X,
        MENU_CTA_Y,
        MENU_CTA_W,
        MENU_CTA_H,
        MENU_CTA_RADIUS,
        acc,
    );
    draw_rounded_rect(
        MENU_CTA_X + MENU_CTA_BORDER,
        MENU_CTA_Y + MENU_CTA_BORDER,
        MENU_CTA_W - 2.0 * MENU_CTA_BORDER,
        MENU_CTA_H - 2.0 * MENU_CTA_BORDER,
        MENU_CTA_RADIUS - MENU_CTA_BORDER,
        rgba(MENU_CTA_COLOR),
    );
    txt(
        font,
        i18n.t("menu.play"),
        MENU_CTA_X + MENU_CTA_TEXT_DX,
        MENU_CTA_TEXT_Y,
        MENU_CTA_TEXT_SIZE,
        WHITE,
    );

    let footer = [
        i18n.t("menu.editor_btn"),
        i18n.t("menu.theme_btn"),
        i18n.t("menu.lang_btn"),
        i18n.t("menu.music_btn"),
    ]
    .join(MENU_FOOTER_SEPARATOR);
    txt(
        font,
        &footer,
        MENU_PANEL_X + MENU_FOOTER_DX,
        MENU_FOOTER_Y,
        MENU_FOOTER_SIZE,
        pri,
    );
}

fn draw_backdrop(
    world: &GameWorld,
    theme: &ThemeDef,
    neb_mat: &Material,
    starfield: &Starfield,
    view: Rect,
    time: f32,
) {
    let w = world.config.arena.width;
    let h = world.config.arena.height;
    let top = rgba(theme.bg_top);
    let bot = rgba(theme.bg_bottom);
    let acc = rgba(theme.paddle_accent);

    // GPU Procedural Nebula / Aurora Shader pass
    neb_mat.set_uniform(
        "u_params",
        vec4(
            time,
            world.paddle_x / w,
            0.0,
            (world.combo as f32 / 12.0).min(1.0),
        ),
    );
    neb_mat.set_uniform("u_col_top", vec4(top.r, top.g, top.b, 1.0));
    neb_mat.set_uniform("u_col_bot", vec4(bot.r, bot.g, bot.b, 1.0));
    neb_mat.set_uniform("u_col_acc", vec4(acc.r, acc.g, acc.b, 1.0));
    neb_mat.set_uniform(
        "u_rect",
        vec4(view.x / w, view.y / h, view.w / w, view.h / h),
    );

    gl_use_material(neb_mat);
    draw_rectangle(view.x, view.y, view.w, view.h, WHITE);
    gl_use_default_material();

    starfield.draw(world.paddle_x, w, view, time, acc);
}

fn draw_arena_background(
    world: &GameWorld,
    theme: &ThemeDef,
    warp_grid: &WarpGrid,
    chromatic_pulse: f32,
) {
    let w = world.config.arena.width;
    let h = world.config.arena.height;
    let acc = rgba(theme.paddle_accent);

    warp_grid.draw(rgba(theme.grid_color), acc);

    if chromatic_pulse > 0.01 {
        let shift = chromatic_pulse * 6.0;
        draw_rectangle_lines(
            world.config.arena.wall_side - shift,
            world.config.arena.wall_top,
            w - world.config.arena.wall_side * 2.0,
            h - world.config.arena.wall_top,
            2.0,
            Color::new(1.0, 0.1, 0.4, chromatic_pulse * 0.45),
        );
    }

    // Modern Glowing Arena Frame
    let border = rgba(theme.border_color);
    let left = world.config.arena.wall_side;
    let right = w - world.config.arena.wall_side;
    let top_y = world.config.arena.wall_top;
    draw_line(
        left,
        top_y,
        right,
        top_y,
        10.0,
        Color::new(border.r, border.g, border.b, 0.22),
    );
    draw_line(
        left,
        top_y,
        left,
        h,
        10.0,
        Color::new(border.r, border.g, border.b, 0.22),
    );
    draw_line(
        right,
        top_y,
        right,
        h,
        10.0,
        Color::new(border.r, border.g, border.b, 0.22),
    );
    draw_line(left, top_y, right, top_y, 3.0, border);
    draw_line(left, top_y, left, h, 3.0, border);
    draw_line(right, top_y, right, h, 3.0, border);

    if world.shield_hits > 0 {
        let sy = world.shield_y();
        let shield_col = rgba(theme.paddle_core);
        let alpha = (world.shield_hits as f32 / world.config.gameplay.shield_hits.max(1) as f32)
            * SHIELD_MAX_ALPHA;
        draw_line(
            left + 4.0,
            sy,
            right - 4.0,
            sy,
            12.0,
            Color::new(shield_col.r, shield_col.g, shield_col.b, alpha * 0.32),
        );
        draw_line(left + 4.0, sy, right - 4.0, sy, 3.5, WHITE);
    }
}

fn draw_boss_and_hazards(world: &GameWorld, theme: &ThemeDef, i18n: &I18nCatalog, font: &Font) {
    if let Some(boss) = &world.boss {
        let base = rgba(theme.paddle_accent);
        let col = Color::new(
            (base.r + boss.hit_flash).min(1.0),
            (base.g + boss.hit_flash).min(1.0),
            (base.b + boss.hit_flash).min(1.0),
            1.0,
        );
        draw_modern_capsule_brick(boss.x, boss.y, boss.w, boss.h, col, WHITE, 0.45);

        let hp_ratio = boss.hp as f32 / boss.max_hp.max(1) as f32;
        draw_rounded_rect(
            boss.x,
            boss.y - 20.0,
            boss.w,
            10.0,
            5.0,
            Color::from_rgba(30, 32, 48, 230),
        );
        draw_rounded_rect(boss.x, boss.y - 20.0, boss.w * hp_ratio, 10.0, 5.0, col);
        txt(
            font,
            i18n.t("hud.boss"),
            boss.x + 52.0,
            boss.y + 42.0,
            BOSS_LABEL_SIZE,
            WHITE,
        );
    }

    let haz_col = rgba(theme.fireball_glow);
    for h in &world.hazards {
        draw_circle(
            h.x,
            h.y,
            h.radius * 2.0,
            Color::new(haz_col.r, haz_col.g, haz_col.b, 0.35),
        );
        draw_circle(h.x, h.y, h.radius, haz_col);
        draw_circle(h.x, h.y, h.radius * 0.45, WHITE);
    }
}

fn draw_bricks(world: &GameWorld, theme: &ThemeDef, font: &Font, time: f32) {
    let sweep_x = ((time * 340.0) % 2400.0) - 240.0;

    for brick in &world.bricks {
        let (base_col, alpha_mul) = match brick.kind {
            BrickKind::Standard { color_idx, .. } | BrickKind::Moving { color_idx, .. } => (
                rgba(theme.brick_palette[color_idx % theme.brick_palette.len()]),
                1.0,
            ),
            BrickKind::Explosive => (rgba(theme.fireball_glow), 1.0),
            BrickKind::Indestructible => (Color::from_rgba(145, 155, 175, 255), 1.0),
            BrickKind::Portal => (rgba(theme.lightning_color), 1.0),
            BrickKind::Ghost { revealed } => (
                rgba(theme.paddle_accent),
                if revealed { 0.94 } else { 0.24 },
            ),
        };

        let (bx, by) = brick.center();
        let mut light_boost = 0.0f32;
        for ball in &world.balls {
            let d_sq = (ball.x - bx).powi(2) + (ball.y - by).powi(2);
            if d_sq < 260.0 * 260.0 {
                let factor = (1.0 - d_sq.sqrt() / 260.0) * 0.38;
                light_boost = light_boost.max(factor);
            }
        }

        let spec_dist = (bx + by * 0.5 - sweep_x).abs();
        let spec_boost = if spec_dist < 65.0 {
            (1.0 - spec_dist / 65.0) * 0.25
        } else {
            0.0
        };

        let boost = (brick.hit_flash + light_boost + spec_boost).min(1.0);
        let col = Color::new(
            (base_col.r + boost).min(1.0),
            (base_col.g + boost).min(1.0),
            (base_col.b + boost).min(1.0),
            alpha_mul,
        );

        let pop = brick.hit_flash * 4.5;
        let rx = brick.x - pop * 0.5;
        let ry = brick.y - pop * 0.5;
        let rw = brick.w + pop;
        let rh = brick.h + pop;

        let rim = match theme.style {
            RenderStyle::NeonGlow | RenderStyle::WireframeHolo | RenderStyle::OutrunGold => {
                Color::new(
                    (col.r + 0.25).min(1.0),
                    (col.g + 0.25).min(1.0),
                    (col.b + 0.25).min(1.0),
                    alpha_mul,
                )
            }
            RenderStyle::IndustrialMetal | RenderStyle::VolcanicCore => {
                Color::from_rgba(45, 50, 62, (255.0 * alpha_mul) as u8)
            }
            RenderStyle::BeveledClassic | RenderStyle::CrystalFacet | RenderStyle::CryoGlass => {
                Color::from_rgba(245, 248, 255, (235.0 * alpha_mul) as u8)
            }
        };

        match theme.style {
            RenderStyle::WireframeHolo => {
                // Wireframe Hologram style: translucent grid body + glowing corner nodes
                draw_rounded_rect(
                    rx,
                    ry,
                    rw,
                    rh,
                    8.0,
                    Color::new(col.r * 0.12, col.g * 0.22, col.b * 0.15, 0.72 * alpha_mul),
                );
                draw_rectangle_lines(rx, ry, rw, rh, 2.5, col);
                let scan_y = ry + ((time * 40.0 + bx * 0.2) % rh.max(1.0));
                draw_line(
                    rx + 3.0,
                    scan_y,
                    rx + rw - 3.0,
                    scan_y,
                    1.5,
                    Color::new(1.0, 1.0, 1.0, 0.55 * alpha_mul),
                );
                draw_circle(rx + 4.0, ry + 4.0, 3.0, WHITE);
                draw_circle(rx + rw - 4.0, ry + 4.0, 3.0, WHITE);
                draw_circle(rx + 4.0, ry + rh - 4.0, 3.0, WHITE);
                draw_circle(rx + rw - 4.0, ry + rh - 4.0, 3.0, WHITE);
            }
            RenderStyle::CrystalFacet => {
                // Cut gemstone with inner diamond facet lines
                draw_modern_capsule_brick(rx, ry, rw, rh, col, rim, 0.32);
                let il = rx + 14.0;
                let ir = rx + rw - 14.0;
                let it = ry + 8.0;
                let ib = ry + rh - 8.0;
                let facet_c = Color::new(1.0, 1.0, 1.0, 0.55 * alpha_mul);
                draw_rectangle_lines(il, it, ir - il, ib - it, 1.8, facet_c);
                draw_line(rx + 4.0, ry + 4.0, il, it, 1.8, facet_c);
                draw_line(rx + rw - 4.0, ry + 4.0, ir, it, 1.8, facet_c);
                draw_line(rx + 4.0, ry + rh - 4.0, il, ib, 1.8, facet_c);
                draw_line(rx + rw - 4.0, ry + rh - 4.0, ir, ib, 1.8, facet_c);
            }
            RenderStyle::VolcanicCore => {
                // Dark obsidian crust with pulsing molten magma veins
                let obsidian = Color::from_rgba(26, 18, 20, (245.0 * alpha_mul) as u8);
                draw_rounded_rect(rx, ry, rw, rh, 9.0, col);
                draw_rounded_rect(rx + 3.0, ry + 3.0, rw - 6.0, rh - 6.0, 7.0, obsidian);
                let magma_pulse = 0.65 + 0.35 * (time * 5.0 + bx * 0.05).sin();
                let lava = Color::new(col.r, col.g * magma_pulse, 0.1, alpha_mul);
                draw_line(rx + 8.0, by, bx - 6.0, ry + 7.0, 3.0, lava);
                draw_line(bx - 6.0, ry + 7.0, bx + 8.0, ry + rh - 7.0, 3.0, lava);
                draw_line(bx + 8.0, ry + rh - 7.0, rx + rw - 8.0, by, 3.0, lava);
            }
            RenderStyle::CryoGlass => {
                // Frosted ice capsule with diagonal specular frost slashes
                draw_modern_capsule_brick(rx, ry, rw, rh, col, rim, 0.28);
                let frost = Color::new(1.0, 1.0, 1.0, 0.55 * alpha_mul);
                draw_line(rx + 18.0, ry + rh - 5.0, rx + 34.0, ry + 5.0, 3.0, frost);
                draw_line(rx + 30.0, ry + rh - 5.0, rx + 46.0, ry + 5.0, 1.8, frost);
            }
            RenderStyle::OutrunGold => {
                // 80s Synth Sunset horizontal chrome louver bars
                draw_modern_capsule_brick(rx, ry, rw, rh, col, rim, 0.36);
                let stripe = Color::from_rgba(28, 8, 42, (190.0 * alpha_mul) as u8);
                draw_rectangle(rx + 6.0, ry + rh * 0.52, rw - 12.0, 3.5, stripe);
                draw_rectangle(rx + 6.0, ry + rh * 0.72, rw - 12.0, 4.5, stripe);
            }
            _ => {
                draw_modern_capsule_brick(
                    rx,
                    ry,
                    rw,
                    rh,
                    col,
                    rim,
                    if theme.style == RenderStyle::NeonGlow {
                        0.28 + light_boost * 0.45
                    } else {
                        0.12
                    },
                );
            }
        }

        if theme.style == RenderStyle::IndustrialMetal {
            let rivet_col = Color::from_rgba(235, 240, 250, (220.0 * alpha_mul) as u8);
            draw_circle(rx + 8.0, ry + 8.0, 2.5, rivet_col);
            draw_circle(rx + rw - 8.0, ry + 8.0, 2.5, rivet_col);
            draw_circle(rx + 8.0, ry + rh - 8.0, 2.5, rivet_col);
            draw_circle(rx + rw - 8.0, ry + rh - 8.0, 2.5, rivet_col);
        }

        match brick.kind {
            BrickKind::Explosive => {
                let pulse = (time * 9.0).sin() * 0.5 + 0.5;
                draw_circle(bx, by, 8.0 + pulse * 4.0, Color::new(1.0, 0.92, 0.35, 0.9));
                draw_circle(bx, by, 4.0, WHITE);
            }
            BrickKind::Portal => {
                let r = 10.0 + (time * 6.0).sin() * 3.0;
                draw_circle_lines(bx, by, r, 3.0, WHITE);
                draw_circle(bx, by, 4.5, rgba(theme.paddle_accent));
            }
            BrickKind::Moving { hp, .. } => {
                txt(font, "<>", bx - 12.0, by + 8.0, 22, WHITE);
                if hp == 1 {
                    draw_crack(rx, ry, rw, rh);
                }
            }
            BrickKind::Standard { hp, max_hp, .. } if max_hp > 1 => {
                if hp > 1 {
                    draw_rounded_rect(
                        rx + 6.0,
                        ry + 6.0,
                        rw - 12.0,
                        rh - 12.0,
                        6.0,
                        Color::new(1.0, 1.0, 1.0, 0.25),
                    );
                } else {
                    draw_crack(rx, ry, rw, rh);
                }
            }
            _ => {}
        }
    }
}

fn draw_crack(x: f32, y: f32, w: f32, h: f32) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    draw_line(x + w * 0.25, y + 3.0, cx - 4.0, cy, 2.5, WHITE);
    draw_line(cx - 4.0, cy, cx + 6.0, cy + 4.0, 2.5, WHITE);
    draw_line(cx + 6.0, cy + 4.0, x + w * 0.72, y + h - 3.0, 2.5, WHITE);
}

fn draw_paddle_and_lasers(world: &GameWorld, theme: &ThemeDef, time: f32) {
    let px = world.paddle_x - world.paddle_width * 0.5;
    let py = world.config.arena.paddle_y - world.config.paddle.height * 0.5;
    let pw = world.paddle_width;
    let ph = world.config.paddle.height;

    let core = rgba(theme.paddle_core);
    let accent = rgba(theme.paddle_accent);

    if world.vortex_active {
        let pulse = (time * 14.0).sin() * 10.0;
        let vortex_col = Color::new(accent.r, accent.g, accent.b, 0.35);
        draw_circle_lines(world.paddle_x, py, 70.0 + pulse, 3.0, vortex_col);
        draw_circle_lines(world.paddle_x, py, 130.0 - pulse, 2.0, vortex_col);
    }

    // Sleek twin plasma thrusters
    let flame_h = 12.0 + (time * 34.0).sin().abs() * 10.0;
    draw_triangle(
        vec2(px + 28.0, py + ph - 2.0),
        vec2(px + 46.0, py + ph - 2.0),
        vec2(px + 37.0, py + ph + flame_h),
        accent,
    );
    draw_triangle(
        vec2(px + pw - 46.0, py + ph - 2.0),
        vec2(px + pw - 28.0, py + ph - 2.0),
        vec2(px + pw - 37.0, py + ph + flame_h),
        accent,
    );

    // Modern Curved Spacecraft Paddle Chassis
    draw_modern_capsule_brick(px, py, pw, ph, core, accent, 0.35);
    // Glowing central energy core on paddle
    draw_circle(world.paddle_x, py + ph * 0.5, ph * 0.32, WHITE);
    draw_circle_lines(world.paddle_x, py + ph * 0.5, ph * 0.45, 2.0, accent);

    if world.laser_active {
        draw_rounded_rect(px + 14.0, py - 10.0, 8.0, 12.0, 3.0, accent);
        draw_rounded_rect(px + pw - 22.0, py - 10.0, 8.0, 12.0, 3.0, accent);
    }

    for bolt in &world.lasers {
        draw_rounded_rect(
            bolt.x - 6.0,
            bolt.y - 28.0,
            12.0,
            30.0,
            5.0,
            Color::new(accent.r, accent.g, accent.b, 0.42),
        );
        draw_rounded_rect(bolt.x - 2.5, bolt.y - 24.0, 5.0, 24.0, 2.5, WHITE);
    }
}

fn draw_balls(world: &GameWorld, theme: &ThemeDef, chromatic_pulse: f32) {
    for ball in &world.balls {
        let glow_col = if ball.fireball {
            rgba(theme.fireball_glow)
        } else {
            rgba(theme.ball_glow)
        };

        draw_ribbon_trail(&ball.trail, ball.radius, glow_col);

        if chromatic_pulse > 0.05 {
            let shift = chromatic_pulse * 6.0;
            draw_circle(
                ball.x - shift,
                ball.y,
                ball.radius,
                Color::new(1.0, 0.1, 0.3, 0.55),
            );
            draw_circle(
                ball.x + shift,
                ball.y,
                ball.radius,
                Color::new(0.1, 0.9, 1.0, 0.55),
            );
        }

        draw_circle(
            ball.x,
            ball.y,
            ball.radius * 3.4,
            Color::new(glow_col.r, glow_col.g, glow_col.b, 0.18),
        );
        draw_circle(
            ball.x,
            ball.y,
            ball.radius * 1.9,
            Color::new(glow_col.r, glow_col.g, glow_col.b, 0.48),
        );
        draw_circle(ball.x, ball.y, ball.radius, rgba(theme.ball_core));
    }
}

fn draw_capsules(world: &GameWorld, i18n: &I18nCatalog, font: &Font, time: f32) {
    for cap in &world.capsules {
        let badge = i18n.t(cap.kind.badge_key());
        draw_powerup_capsule(cap.x, cap.y, cap.radius, cap.kind, badge, time, Some(font));
    }
}

fn draw_additive_fx(
    particles: &[Particle],
    shards: &[DebrisShard],
    shockwaves: &[Shockwave],
    arcs: &[LightningArc],
    theme: &ThemeDef,
) {
    for p in particles {
        let alpha = (p.life / p.max_life).clamp(0.0, 1.0);
        let c = Color::new(p.color.r, p.color.g, p.color.b, alpha * 0.88);
        let tail_x = p.x - p.vx * 0.03;
        let tail_y = p.y - p.vy * 0.03;
        draw_line(p.x, p.y, tail_x, tail_y, p.size * 0.8, c);
        draw_circle(p.x, p.y, p.size * 0.45, WHITE);
    }

    for s in shards {
        let alpha = (s.life / s.max_life).clamp(0.0, 1.0);
        let c = Color::new(s.color.r, s.color.g, s.color.b, alpha * 0.78);
        draw_poly(s.x, s.y, 4, s.w * 0.65, s.angle.to_degrees(), c);
    }

    for s in shockwaves {
        let alpha = (s.life / s.max_life).clamp(0.0, 1.0);
        let c = Color::new(s.color.r, s.color.g, s.color.b, alpha * 0.88);
        draw_circle_lines(s.x, s.y, s.radius, 5.5, c);
        draw_circle_lines(
            s.x,
            s.y,
            s.radius * 0.92,
            2.0,
            Color::new(1.0, 1.0, 1.0, alpha * 0.65),
        );
    }

    let bolt_col = rgba(theme.lightning_color);
    for arc in arcs {
        let (x1, y1) = arc.from;
        let (x2, y2) = arc.to;
        let mut prev = (x1, y1);
        for seg in 1..=LIGHTNING_SEGMENTS {
            let t = seg as f32 / LIGHTNING_SEGMENTS as f32;
            let jitter = if seg < LIGHTNING_SEGMENTS {
                LIGHTNING_JITTER
            } else {
                0.0
            };
            let nx = x1 + (x2 - x1) * t + rand::gen_range(-jitter, jitter);
            let ny = y1 + (y2 - y1) * t + rand::gen_range(-jitter, jitter);
            draw_line(
                prev.0,
                prev.1,
                nx,
                ny,
                8.0,
                Color::new(bolt_col.r, bolt_col.g, bolt_col.b, 0.38),
            );
            draw_line(prev.0, prev.1, nx, ny, 2.8, WHITE);
            prev = (nx, ny);
        }
    }
}

fn draw_popups(popups: &[FloatingPopup], font: &Font) {
    for pop in popups {
        let alpha = (pop.life / POPUP_FADE_REF_SEC).clamp(0.0, 1.0);
        let c = Color::new(pop.color.r, pop.color.g, pop.color.b, alpha);
        txt(font, &pop.text, pop.x, pop.y, POPUP_FONT_SIZE, c);
    }
}

fn draw_hud(ctx: &HudContext) {
    let HudContext {
        world,
        levels,
        theme,
        i18n,
        lang,
        font,
        editor_mode,
        rt_w,
        rt_h,
    } = *ctx;
    let pri = rgba(theme.text_primary);
    let acc = rgba(theme.text_accent);
    let border = rgba(theme.border_color);
    let lvl_name = levels
        .levels
        .get(world.current_level_idx)
        .map(|l| l.name.as_str())
        .unwrap_or("");

    // Sleek Compact Top Dashboard Bar
    draw_rounded_rect(
        HUD_BAR_X,
        HUD_BAR_Y,
        HUD_BAR_W,
        HUD_BAR_H,
        HUD_BAR_RADIUS,
        Color::new(border.r, border.g, border.b, HUD_BAR_BORDER_ALPHA),
    );
    draw_rounded_rect(
        HUD_BAR_X + HUD_BAR_INSET,
        HUD_BAR_Y + HUD_BAR_INSET,
        HUD_BAR_W - 2.0 * HUD_BAR_INSET,
        HUD_BAR_H - 2.0 * HUD_BAR_INSET,
        HUD_BAR_RADIUS - HUD_BAR_INSET,
        rgba(HUD_BAR_COLOR),
    );
    let text_x = HUD_BAR_X + HUD_TEXT_DX;
    let right_x = HUD_BAR_X + HUD_RIGHT_DX;

    let top_bar = format!(
        "{}   |   {}: {:07} ({}: {:07})   |   {}: {}   |   {}: x{}   |   {} {}: {}",
        i18n.t("app.title"),
        i18n.t("hud.score"),
        world.score,
        i18n.t("hud.high_score"),
        world.high_score,
        i18n.t("hud.lives"),
        world.lives,
        i18n.t("hud.combo"),
        world.combo.max(1),
        i18n.t("hud.level"),
        world.current_level_idx + 1,
        lvl_name
    );
    txt(
        font,
        &top_bar,
        text_x,
        HUD_TOP_TEXT_Y,
        HUD_TOP_TEXT_SIZE,
        pri,
    );

    let perf_badge = format!("{}x{} px | {} {}", rt_w, rt_h, get_fps(), i18n.t("hud.fps"));
    txt(
        font,
        &perf_badge,
        right_x,
        HUD_PERF_Y,
        HUD_RIGHT_TEXT_SIZE,
        acc,
    );

    let v_ratio = (world.vortex_energy / world.config.gameplay.vortex_max_energy).clamp(0.0, 1.0);
    txt(
        font,
        i18n.t("hud.vortex"),
        right_x,
        HUD_VORTEX_LABEL_Y,
        HUD_RIGHT_TEXT_SIZE,
        pri,
    );
    let vortex_x = HUD_BAR_X + HUD_VORTEX_BAR_DX;
    draw_rounded_rect(
        vortex_x,
        HUD_VORTEX_BAR_Y,
        HUD_VORTEX_BAR_W,
        HUD_VORTEX_BAR_H,
        HUD_VORTEX_BAR_H * 0.5,
        rgba(HUD_VORTEX_BAR_COLOR),
    );
    draw_rounded_rect(
        vortex_x,
        HUD_VORTEX_BAR_Y,
        (HUD_VORTEX_BAR_W * v_ratio).max(HUD_VORTEX_BAR_MIN_W),
        HUD_VORTEX_BAR_H,
        HUD_VORTEX_BAR_H * 0.5,
        acc,
    );

    // Shrinks (down to the badge minimum) instead of running under the right-hand block.
    let sub_bar = build_sub_bar(i18n, theme, lang);
    let sub_size = fit_font_size(
        |size| measure_text(&sub_bar, Some(font), size, 1.0).width,
        HUD_SUB_TEXT_SIZE,
        BADGE_FONT_SIZE_MIN,
        HUD_SUB_BAR_MAX_W,
    );
    txt(font, &sub_bar, text_x, HUD_SUB_TEXT_Y, sub_size, acc);

    if editor_mode {
        txt(
            font,
            i18n.t("editor.banner"),
            HUD_BANNER_X,
            HUD_BANNER_Y,
            HUD_BANNER_SIZE,
            acc,
        );
        txt(
            font,
            i18n.t("editor.instructions"),
            HUD_BANNER_X,
            HUD_FOOTER_Y,
            HUD_FOOTER_SIZE,
            pri,
        );
        return;
    }

    if world.level_elapsed >= world.config.gameplay.sudden_death_time_sec {
        txt(
            font,
            i18n.t("status.sudden_death"),
            HUD_BANNER_X,
            HUD_SUDDEN_DEATH_Y,
            HUD_SUDDEN_DEATH_SIZE,
            acc,
        );
    }

    if world.last_powerup_timer > 0.0 {
        if let Some(kind) = world.last_powerup {
            let msg = i18n.t(kind.i18n_key());
            let toast_col = rgba(if kind.is_malus() {
                MALUS_COLOR
            } else {
                BONUS_COLOR
            });
            let dims = measure_text(msg, Some(font), TOAST_FONT_SIZE, 1.0);
            let bw = dims.width + 2.0 * TOAST_PAD_X;
            let bx = world.config.arena.width * 0.5 - bw * 0.5;
            draw_rounded_rect(bx, TOAST_Y, bw, TOAST_H, TOAST_RADIUS, toast_col);
            draw_rounded_rect(
                bx + TOAST_BORDER,
                TOAST_Y + TOAST_BORDER,
                bw - 2.0 * TOAST_BORDER,
                TOAST_H - 2.0 * TOAST_BORDER,
                TOAST_RADIUS - TOAST_BORDER,
                rgba(TOAST_BG_COLOR),
            );
            txt(
                font,
                msg,
                bx + TOAST_PAD_X,
                TOAST_Y + TOAST_TEXT_DY,
                TOAST_FONT_SIZE,
                toast_col,
            );
        }
    }
    if world.balls.iter().any(|b| b.stuck_offset.is_some()) {
        txt(
            font,
            i18n.t("controls.launch_hint"),
            HUD_HINT_X,
            HUD_HINT_Y,
            HUD_FOOTER_SIZE,
            pri,
        );
    } else if world.laser_active {
        txt(
            font,
            i18n.t("controls.laser_hint"),
            HUD_HINT_X,
            HUD_HINT_Y,
            HUD_FOOTER_SIZE,
            acc,
        );
    }

    match world.status {
        SessionStatus::Paused => {
            draw_overlay_box(
                font,
                i18n.t("status.paused"),
                i18n.t("status.paused_sub"),
                pri,
                acc,
            );
        }
        SessionStatus::Victory => {
            draw_overlay_box(
                font,
                i18n.t("status.victory"),
                i18n.t("status.victory_sub"),
                acc,
                pri,
            );
        }
        SessionStatus::GameOver => {
            draw_overlay_box(
                font,
                i18n.t("status.game_over"),
                i18n.t("status.game_over_sub"),
                acc,
                pri,
            );
        }
        SessionStatus::Playing => {}
    }
}

/// Second HUD line: current renderer, the hotkey reminders and the active language.
fn build_sub_bar(i18n: &I18nCatalog, theme: &ThemeDef, lang: Language) -> String {
    let level_hint = LEVEL_CHEAT_ENABLED.then(|| i18n.t("controls.level_hint").to_string());
    [
        Some(format!(
            "{}: {}",
            i18n.t("hud.theme"),
            i18n.t(&theme.name_key)
        )),
        Some(i18n.t("controls.theme_hint").to_string()),
        Some(i18n.t("controls.lang_hint").to_string()),
        level_hint,
        Some(i18n.t("controls.editor_hint").to_string()),
        Some(i18n.t("controls.mute_hint").to_string()),
        Some(i18n.t("controls.pause_hint").to_string()),
        Some(i18n.t("controls.restart_hint").to_string()),
        Some(format!("[{}: {}]", i18n.t("hud.lang"), lang.code())),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(HUD_SUB_BAR_SEPARATOR)
}

fn draw_overlay_box(font: &Font, title: &str, subtitle: &str, title_col: Color, sub_col: Color) {
    draw_rounded_rect(
        OVERLAY_X,
        OVERLAY_Y,
        OVERLAY_W,
        OVERLAY_H,
        OVERLAY_RADIUS,
        title_col,
    );
    draw_rounded_rect(
        OVERLAY_X + OVERLAY_BORDER,
        OVERLAY_Y + OVERLAY_BORDER,
        OVERLAY_W - 2.0 * OVERLAY_BORDER,
        OVERLAY_H - 2.0 * OVERLAY_BORDER,
        OVERLAY_RADIUS - OVERLAY_BORDER,
        rgba(OVERLAY_COLOR),
    );
    txt(
        font,
        title,
        OVERLAY_X + OVERLAY_TEXT_DX,
        OVERLAY_Y + OVERLAY_TITLE_DY,
        OVERLAY_TITLE_SIZE,
        title_col,
    );
    txt(
        font,
        subtitle,
        OVERLAY_X + OVERLAY_TEXT_DX,
        OVERLAY_Y + OVERLAY_SUB_DY,
        OVERLAY_SUB_SIZE,
        sub_col,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_input_is_detected_by_oversized_deltas() {
        assert!(!absolute_input_detected(120.0, 1000.0));
        assert!(!absolute_input_detected(-1000.0, 1000.0));
        assert!(absolute_input_detected(1500.0, 1000.0));
        assert!(absolute_input_detected(-1500.0, 1000.0));
    }

    #[test]
    fn focus_loss_pauses_only_while_playing_in_game() {
        assert!(focus_loss_pauses(true, true, true));
        assert!(!focus_loss_pauses(true, true, false));
        assert!(!focus_loss_pauses(true, false, true));
        assert!(!focus_loss_pauses(false, true, true));
    }

    #[test]
    fn nine_world_rows_fit_and_extra_worlds_are_capped() {
        let l = world_row_layout(WORLD_SELECT_KEYS.len());
        let last_bottom =
            MENU_WORLD_LIST_Y + (WORLD_SELECT_KEYS.len() - 1) as f32 * l.step + l.pill_h;
        assert!(last_bottom <= MENU_WORLD_LIST_BOTTOM_Y + 0.01);
        assert!(l.text_size <= l.pill_h as u16);
        assert_eq!(world_row_layout(40), l);
    }

    #[test]
    fn count_placeholder_is_substituted() {
        assert_eq!(substitute_count("keys [1-{count}]", 5), "keys [1-5]");
        assert_eq!(substitute_count("no placeholder", 5), "no placeholder");
    }

    #[test]
    fn locale_campaign_strings_use_the_placeholder() {
        for raw in [RAW_EN, RAW_FR] {
            let catalog = I18nCatalog::from_json_str(raw).unwrap();
            assert!(catalog.t("menu.campaigns").contains(COUNT_PLACEHOLDER));
        }
    }

    #[test]
    fn score_popup_shows_multiplier_only_above_one() {
        assert_eq!(format_score_popup(150, 1), "+150");
        assert_eq!(format_score_popup(300, 2), "+300 (x2)");
    }

    #[test]
    fn world_rows_keep_reference_look_for_three_worlds() {
        let l = world_row_layout(3);
        assert_eq!(l.pill_h, MENU_WORLD_PILL_H);
        assert_eq!(l.step, MENU_WORLD_PILL_H + MENU_WORLD_ROW_GAP);
        assert_eq!(l.text_size, MENU_WORLD_TEXT_SIZE);
        assert_eq!(l.text_dy, MENU_WORLD_TEXT_DY);
    }

    #[test]
    fn world_rows_fit_above_the_sector_line() {
        for count in 1..=WORLD_SELECT_KEYS.len() {
            let l = world_row_layout(count);
            let last_bottom = MENU_WORLD_LIST_Y + (count - 1) as f32 * l.step + l.pill_h;
            assert!(
                last_bottom <= MENU_WORLD_LIST_BOTTOM_Y + 0.01,
                "count {count}"
            );
            assert!(last_bottom < MENU_SECTOR_Y);
            assert!(l.pill_h >= MENU_WORLD_PILL_MIN_H);
            assert!(l.text_size >= MENU_WORLD_TEXT_SIZE_FLOOR);
        }
        assert!(world_row_layout(6).pill_h < MENU_WORLD_PILL_H);
        assert_eq!(world_row_layout(0), world_row_layout(1));
    }

    #[test]
    fn virtual_cursor_seed_and_advance_are_clamped() {
        // New session: seeded from the mouse. Running session: kept at the paddle.
        assert_eq!(
            seed_virtual_cursor_x(false, 700.0, 500.0, 100.0, 900.0),
            500.0
        );
        assert_eq!(
            seed_virtual_cursor_x(false, 700.0, -40.0, 100.0, 900.0),
            100.0
        );
        assert_eq!(
            seed_virtual_cursor_x(false, 700.0, 5000.0, 100.0, 900.0),
            900.0
        );
        assert_eq!(
            seed_virtual_cursor_x(true, 700.0, 5000.0, 100.0, 900.0),
            700.0
        );
        assert_eq!(
            seed_virtual_cursor_x(true, 50.0, 500.0, 100.0, 900.0),
            100.0
        );
        assert_eq!(advance_virtual_cursor_x(500.0, 30.0, 100.0, 900.0), 530.0);
        assert_eq!(
            advance_virtual_cursor_x(500.0, -9999.0, 100.0, 900.0),
            100.0
        );
        assert_eq!(advance_virtual_cursor_x(500.0, 9999.0, 100.0, 900.0), 900.0);
    }

    const ARENA_W: f32 = 1920.0;
    const ARENA_H: f32 = 1080.0;
    const EPS: f32 = 1e-3;

    #[test]
    fn letterbox_is_identity_at_native_size() {
        let (s, ox, oy) = letterbox(ARENA_W, ARENA_H, ARENA_W, ARENA_H);
        assert!((s - 1.0).abs() < EPS && ox.abs() < EPS && oy.abs() < EPS);
    }

    #[test]
    fn letterbox_pillarboxes_wide_windows() {
        let (s, ox, oy) = letterbox(2560.0, 1080.0, ARENA_W, ARENA_H);
        assert!((s - 1.0).abs() < EPS);
        assert!((ox - 320.0).abs() < EPS && oy.abs() < EPS);
    }

    #[test]
    fn letterbox_bars_on_tall_windows() {
        let (s, ox, oy) = letterbox(960.0, 960.0, ARENA_W, ARENA_H);
        assert!((s - 0.5).abs() < EPS);
        assert!(ox.abs() < EPS && (oy - 210.0).abs() < EPS);
    }

    #[test]
    fn backdrop_rect_equals_arena_at_16_9() {
        let r = backdrop_rect(1280.0, 720.0, ARENA_W, ARENA_H);
        assert!(r.x.abs() < 1e-3 && r.y.abs() < 1e-3);
        assert!((r.w - ARENA_W).abs() < 1e-2 && (r.h - ARENA_H).abs() < 1e-2);
    }

    #[test]
    fn backdrop_rect_extends_y_on_tall_windows() {
        let r = backdrop_rect(960.0, 960.0, ARENA_W, ARENA_H);
        assert!(r.x.abs() < 1e-3 && (r.w - ARENA_W).abs() < 1e-2);
        assert!(r.y < 0.0);
        assert!((r.y + r.h - (ARENA_H - r.y)).abs() < 1e-2);
    }

    #[test]
    fn backdrop_rect_extends_x_on_wide_windows() {
        let r = backdrop_rect(2560.0, 1080.0, ARENA_W, ARENA_H);
        assert!(r.y.abs() < 1e-3 && (r.h - ARENA_H).abs() < 1e-2);
        assert!(r.x < 0.0);
        assert!((r.x + r.w - (ARENA_W - r.x)).abs() < 1e-2);
    }

    #[test]
    fn screen_to_arena_maps_corners_and_center() {
        let (s, ox, oy) = letterbox(960.0, 960.0, ARENA_W, ARENA_H);
        let (x0, y0) = screen_to_arena(ox, oy, s, ox, oy);
        assert!(x0.abs() < EPS && y0.abs() < EPS);
        let (x1, y1) = screen_to_arena(480.0, 480.0, s, ox, oy);
        assert!((x1 - ARENA_W * 0.5).abs() < EPS && (y1 - ARENA_H * 0.5).abs() < EPS);
        let (x2, y2) = screen_to_arena(960.0 - ox, 960.0 - oy, s, ox, oy);
        assert!((x2 - ARENA_W).abs() < EPS && (y2 - ARENA_H).abs() < EPS);
    }

    #[test]
    fn viewport_is_in_physical_pixels() {
        let (s, ox, oy) = letterbox(1000.0, 1000.0, ARENA_W, ARENA_H);
        let (vx, vy, vw, vh) = arena_viewport(s, ox, oy, ARENA_W, ARENA_H, 2.0);
        assert_eq!(vx, 0);
        assert_eq!(vw, 2000);
        assert_eq!(vh, (1000.0f32 * 1080.0 / 1920.0 * 2.0).round() as i32);
        assert_eq!(
            vy,
            ((1000.0f32 - 1000.0 * 1080.0 / 1920.0) * 0.5 * 2.0).round() as i32
        );
    }

    fn level(id: usize, world: usize) -> LevelDef {
        LevelDef {
            id,
            world,
            name: String::new(),
            boss: false,
            rows: Vec::new(),
        }
    }

    #[test]
    fn worlds_are_derived_from_the_level_list() {
        let levels = [
            level(1, 1),
            level(2, 1),
            level(3, 2),
            level(4, 2),
            level(5, 5),
        ];
        assert_eq!(build_worlds(&levels), vec![(1, 0), (2, 2), (5, 4)]);
        assert!(build_worlds(&[]).is_empty());
    }

    #[test]
    fn sector_navigation_syncs_the_world_position() {
        let levels = [level(1, 1), level(2, 1), level(3, 2), level(4, 2)];
        let worlds = build_worlds(&levels);
        assert_eq!(world_pos_for_level(&worlds, &levels, 1, 9), 0);
        assert_eq!(world_pos_for_level(&worlds, &levels, 2, 9), 1);
        assert_eq!(world_pos_for_level(&worlds, &levels, 99, 9), 9);
    }

    #[test]
    fn fresh_profile_can_only_start_the_first_sector() {
        let max_unlocked = 0;
        assert!(is_sector_unlocked(0, max_unlocked));
        assert!(!is_sector_unlocked(1, max_unlocked));
        assert_eq!(clamp_sector(9, max_unlocked, 14), 0);
    }

    #[test]
    fn sector_clamp_respects_both_bounds() {
        assert_eq!(clamp_sector(3, 5, 14), 3);
        assert_eq!(clamp_sector(6, 5, 14), 5);
        assert_eq!(clamp_sector(20, 30, 14), 14);
        assert_eq!(clamp_sector(0, 0, 0), 0);
    }

    #[test]
    fn world_unlock_boundary_is_its_first_sector() {
        assert!(is_world_unlocked(5, 5));
        assert!(!is_world_unlocked(5, 4));
        assert!(is_world_unlocked(0, 0));
    }

    #[test]
    fn world_navigation_skips_locked_worlds() {
        let worlds = [(1, 0), (2, 5), (3, 10)];
        // Only world 1 unlocked: nothing else to move to.
        assert_eq!(step_unlocked_world(&worlds, 0, true, 4), 0);
        assert_eq!(step_unlocked_world(&worlds, 0, false, 4), 0);
        // Worlds 1 and 2 unlocked: wraps between them and never reaches world 3.
        assert_eq!(step_unlocked_world(&worlds, 0, true, 5), 1);
        assert_eq!(step_unlocked_world(&worlds, 1, true, 9), 0);
        assert_eq!(step_unlocked_world(&worlds, 0, false, 9), 1);
        // Everything unlocked: plain wrap-around.
        assert_eq!(step_unlocked_world(&worlds, 2, true, 14), 0);
        assert_eq!(step_unlocked_world(&worlds, 0, false, 14), 2);
        assert_eq!(step_unlocked_world(&[], 0, true, 0), 0);
    }

    #[test]
    fn resume_mask_matches_the_inputs_that_start_play() {
        assert_eq!(
            resume_mask_for(AppScreen::InGame, SessionStatus::Playing),
            0
        );
        let paused = resume_mask_for(AppScreen::InGame, SessionStatus::Paused);
        assert_eq!(paused, RESUME_P | RESUME_R | RESUME_CHEAT_MASK);
        let over = resume_mask_for(AppScreen::InGame, SessionStatus::GameOver);
        assert_eq!(
            over,
            resume_mask_for(AppScreen::InGame, SessionStatus::Victory)
        );
        assert_eq!(over & RESUME_P, 0);
        assert_ne!(
            resume_mask_for(AppScreen::TitleMenu, SessionStatus::Playing) & RESUME_CLICK,
            0
        );
        assert_ne!(
            resume_mask_for(AppScreen::TitleMenu, SessionStatus::Playing) & RESUME_TAB,
            0
        );
        assert_eq!(
            resume_mask_for(AppScreen::TitleMenu, SessionStatus::Paused) & RESUME_TAB,
            0
        );
        assert_eq!(
            resume_mask_for(AppScreen::Editor, SessionStatus::Playing),
            RESUME_F4 | RESUME_CHEAT_MASK
        );
    }

    fn text_width(face: &ttf_parser::Face, text: &str, size: f32) -> f32 {
        let scale = size / face.units_per_em() as f32;
        text.chars()
            .filter_map(|c| face.glyph_index(c))
            .filter_map(|g| face.glyph_hor_advance(g))
            .map(|a| a as f32 * scale)
            .sum()
    }

    #[test]
    fn hud_sub_bar_fits_beside_the_right_hand_block() {
        let face = ttf_parser::Face::parse(RAW_FONT, 0).expect("valid font");
        let themes: ThemeRegistry = serde_json::from_str(RAW_THEMES).expect("themes");
        let limit = HUD_SUB_BAR_MAX_W;
        for (code, raw, lang) in [("en", RAW_EN, Language::En), ("fr", RAW_FR, Language::Fr)] {
            let i18n = I18nCatalog::from_json_str(raw).expect("catalog");
            for theme in &themes.themes {
                let line = build_sub_bar(&i18n, theme, lang);
                let width = text_width(&face, &line, HUD_SUB_TEXT_SIZE as f32);
                assert!(
                    width <= limit,
                    "{code}/{}: sub bar is {width:.0}px, limit {limit:.0}px",
                    theme.name_key
                );
            }
        }
    }
}
