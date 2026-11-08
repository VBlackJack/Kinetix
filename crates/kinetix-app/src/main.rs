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
mod vfx;

use audio::{
    load_progress, pointer_lock_active, prompt_level_code, save_progress, set_document_lang,
    set_pointer_bounds, set_pointer_lock, share_level_code, SoundBank,
};
use kinetix_core::config::{GameConfig, LevelPack, RenderStyle, ThemeDef, ThemeRegistry};
use kinetix_core::i18n::{I18nCatalog, Language};
use kinetix_core::level_code::LevelCodeError;
use kinetix_core::physics::{BrickKind, GameEvent, GameWorld, SessionStatus};
use macroquad::prelude::*;
use vfx::{
    additive_material, draw_modern_capsule_brick, draw_powerup_capsule, draw_ribbon_trail,
    draw_rounded_rect, nebula_material, DebrisShard, Starfield, WarpGrid,
};

const RAW_GAME_CONFIG: &str = include_str!("../../../assets/config/game_config.json");
const RAW_THEMES: &str = include_str!("../../../assets/config/themes.json");
const RAW_LEVELS: &str = include_str!("../../../assets/config/levels.json");
const RAW_EN: &str = include_str!("../../../assets/locales/en.json");
const RAW_FR: &str = include_str!("../../../assets/locales/fr.json");
const RAW_FONT: &[u8] = include_bytes!("../../../assets/fonts/ui_font.ttf");

/// Malus power-ups and error popups.
const MALUS_COLOR: [u8; 4] = [255, 65, 75, 255];
/// Colour of the bars around the arena when the window is not 16:9.
const LETTERBOX_COLOR: Color = BLACK;
/// Opacity of the black overlay drawn over the backdrop outside the arena viewport.
const LETTERBOX_DIM_ALPHA: f32 = 0.35;
/// Starfield extent relative to the arena on each axis (covers extreme window aspect ratios).
const STARFIELD_EXTENT_FACTOR: f32 = 3.0;
/// Star count for the arena area alone.
const STARFIELD_BASE_COUNT: usize = 260;
const POPUP_X: f32 = 680.0;
const POPUP_Y: f32 = 540.0;
const POPUP_LIFE_SEC: f32 = 1.8;

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
        window_title: "KINETIX".to_owned(),
        window_width: 1920,
        window_height: 1080,
        window_resizable: true,
        high_dpi: true,
        sample_count: 8,
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
        48.0,
    );

    let (saved_hs, saved_unlocked, saved_theme, saved_fr) = load_progress();
    let mut sfx = SoundBank::init().await;
    let mut lang = if saved_fr { Language::Fr } else { Language::En };
    set_document_lang(lang == Language::Fr);
    let mut theme_idx: usize = saved_theme % theme_reg.themes.len();
    let mut world = GameWorld::new(config.clone(), &level_pack.levels);
    world.high_score = saved_hs;
    world.max_unlocked_level = saved_unlocked.min(level_pack.levels.len().saturating_sub(1));

    let mut screen = AppScreen::TitleMenu;
    let mut selected_world: usize = 1;
    let mut selected_sector_idx: usize = 0;

    let mut events: Vec<GameEvent> = Vec::with_capacity(64);
    let mut particles: Vec<Particle> = Vec::with_capacity(2048);
    let mut shards: Vec<DebrisShard> = Vec::with_capacity(256);
    let mut shockwaves: Vec<Shockwave> = Vec::with_capacity(32);
    let mut arcs: Vec<LightningArc> = Vec::with_capacity(32);
    let mut popups: Vec<FloatingPopup> = Vec::with_capacity(32);
    let mut screen_shake: f32 = 0.0;
    let mut chromatic_pulse: f32 = 0.0;
    let mut hit_stop_timer: f32 = 0.0;

    let mut last_lock_wanted = false;
    let mut lock_was_active = false;
    let mut last_pointer_bounds: Option<(f32, f32)> = None;
    let mut editor_brush = BrickKind::Standard {
        hp: 1,
        max_hp: 1,
        color_idx: 0,
    };

    loop {
        let raw_dt = get_frame_time().min(0.033);
        let time = get_time() as f32;
        if hit_stop_timer > 0.0 {
            hit_stop_timer = (hit_stop_timer - raw_dt).max(0.0);
        }
        let dt = if hit_stop_timer > 0.0 {
            raw_dt * 0.15
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
            save_progress(
                world.high_score,
                world.max_unlocked_level,
                theme_idx,
                lang == Language::Fr,
            );
        }
        if is_key_pressed(KeyCode::F2) {
            lang = lang.next();
            set_document_lang(lang == Language::Fr);
            save_progress(
                world.high_score,
                world.max_unlocked_level,
                theme_idx,
                lang == Language::Fr,
            );
        }
        if is_key_pressed(KeyCode::F3) {
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

        let rt_w = (screen_width() * miniquad::window::dpi_scale()).max(640.0) as u32;
        let rt_h = (screen_height() * miniquad::window::dpi_scale()).max(360.0) as u32;

        let arena_w = world.config.arena.width;
        let arena_h = world.config.arena.height;
        let (view_scale, view_ox, view_oy) =
            letterbox(screen_width(), screen_height(), arena_w, arena_h);
        let (mx, my) = mouse_position();
        let (world_mx, world_my) = screen_to_arena(mx, my, view_scale, view_ox, view_oy);

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

        match screen {
            AppScreen::TitleMenu => {
                if is_key_pressed(KeyCode::Key1) {
                    selected_world = 1;
                    selected_sector_idx = 0;
                } else if is_key_pressed(KeyCode::Key2) {
                    selected_world = 2;
                    selected_sector_idx = 5;
                } else if is_key_pressed(KeyCode::Key3) {
                    selected_world = 3;
                    selected_sector_idx = 10;
                }
                if is_key_pressed(KeyCode::Left) {
                    selected_world = if selected_world <= 1 {
                        3
                    } else {
                        selected_world - 1
                    };
                    selected_sector_idx = (selected_world - 1) * 5;
                }
                if is_key_pressed(KeyCode::Right) {
                    selected_world = if selected_world >= 3 {
                        1
                    } else {
                        selected_world + 1
                    };
                    selected_sector_idx = (selected_world - 1) * 5;
                }
                if is_key_pressed(KeyCode::Up) {
                    selected_sector_idx =
                        (selected_sector_idx + 1).min(level_pack.levels.len() - 1);
                    selected_world = level_pack.levels[selected_sector_idx].world;
                }
                if is_key_pressed(KeyCode::Down) {
                    selected_sector_idx = selected_sector_idx.saturating_sub(1);
                    selected_world = level_pack.levels[selected_sector_idx].world;
                }

                if is_key_pressed(KeyCode::Enter)
                    || is_key_pressed(KeyCode::Space)
                    || is_mouse_button_pressed(MouseButton::Left)
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
                    world.bricks.clear();
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

                world.set_paddle_target(world_mx);

                let kb_speed = 1350.0 * dt;
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
                        SessionStatus::Victory => {
                            let next_lvl = (world.current_level_idx + 1) % level_pack.levels.len();
                            world.load_level(next_lvl, &level_pack.levels);
                            save_progress(
                                world.high_score,
                                world.max_unlocked_level,
                                theme_idx,
                                lang == Language::Fr,
                            );
                        }
                        SessionStatus::GameOver => {
                            save_progress(
                                world.high_score,
                                world.max_unlocked_level,
                                theme_idx,
                                lang == Language::Fr,
                            );
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
                    save_progress(
                        world.high_score,
                        world.max_unlocked_level,
                        theme_idx,
                        lang == Language::Fr,
                    );
                }
            }
        }

        // Lock the pointer exactly while the player is actually playing; evaluated after input and
        // world update so a start / resume takes effect in the same frame.
        let lock_wanted = screen == AppScreen::InGame && world.status == SessionStatus::Playing;
        if lock_wanted != last_lock_wanted {
            set_pointer_lock(lock_wanted);
            show_mouse(!lock_wanted);
            last_lock_wanted = lock_wanted;
        }
        lock_was_active = lock_wanted && pointer_lock_active();

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
                } => {
                    sfx.play_brick(world.combo);
                    warp_grid.apply_impulse(x, y, 165.0, if destroyed { 320.0 } else { 140.0 });
                    let base_col = rgba(theme.brick_palette[color_idx % theme.brick_palette.len()]);
                    let count = if destroyed { 30 } else { 12 };
                    let gravity = if theme.style == RenderStyle::IndustrialMetal {
                        580.0
                    } else {
                        140.0
                    };
                    for i in 0..count {
                        let angle = (i as f32 / count as f32) * std::f32::consts::TAU
                            + rand::gen_range(-0.25, 0.25);
                        let spd = rand::gen_range(120.0, 520.0);
                        particles.push(Particle {
                            x,
                            y,
                            vx: angle.cos() * spd,
                            vy: angle.sin() * spd,
                            size: rand::gen_range(3.0, 8.0),
                            life: 0.52,
                            max_life: 0.52,
                            color: base_col,
                            gravity,
                        });
                    }
                    if destroyed {
                        screen_shake = (screen_shake + 4.2).min(16.0);
                        for _ in 0..6 {
                            shards.push(DebrisShard {
                                x: x + rand::gen_range(-26.0, 26.0),
                                y: y + rand::gen_range(-10.0, 10.0),
                                vx: rand::gen_range(-240.0, 240.0),
                                vy: rand::gen_range(-280.0, 80.0),
                                w: rand::gen_range(12.0, 22.0),
                                angle: rand::gen_range(0.0, std::f32::consts::TAU),
                                v_angle: rand::gen_range(-12.0, 12.0),
                                life: 0.78,
                                max_life: 0.78,
                                color: base_col,
                            });
                        }
                        let mult = (1 + world.combo / 4).min(8);
                        let pts = world.config.gameplay.base_brick_score * mult as u64;
                        popups.push(FloatingPopup {
                            x: x - 24.0,
                            y,
                            text: if mult > 1 {
                                format!("+{} (x{})", pts, mult)
                            } else {
                                format!("+{}", pts)
                            },
                            life: 0.70,
                            color: base_col,
                        });
                    }
                }
                GameEvent::Explosion { x, y, radius } => {
                    sfx.play_explosion();
                    warp_grid.apply_impulse(x, y, radius * 2.2, 920.0);
                    screen_shake = (screen_shake + 14.0).min(24.0);
                    chromatic_pulse = 1.0;
                    hit_stop_timer = 0.045;
                    shockwaves.push(Shockwave {
                        x,
                        y,
                        radius: 10.0,
                        max_radius: radius * 1.4,
                        life: 0.40,
                        color: rgba(theme.fireball_glow),
                    });
                    for _ in 0..65 {
                        let angle = rand::gen_range(0.0, std::f32::consts::TAU);
                        let spd = rand::gen_range(160.0, 680.0);
                        particles.push(Particle {
                            x,
                            y,
                            vx: angle.cos() * spd,
                            vy: angle.sin() * spd,
                            size: rand::gen_range(4.5, 10.0),
                            life: 0.65,
                            max_life: 0.65,
                            color: rgba(theme.fireball_glow),
                            gravity: 190.0,
                        });
                    }
                }
                GameEvent::LightningBolt { from, to } => {
                    sfx.play_lightning();
                    warp_grid.apply_impulse(to.0, to.1, 180.0, 520.0);
                    screen_shake = (screen_shake + 7.0).min(16.0);
                    chromatic_pulse = 0.65;
                    arcs.push(LightningArc {
                        from,
                        to,
                        life: 0.25,
                    });
                }
                GameEvent::PortalWarp { from, to } => {
                    sfx.play_portal();
                    warp_grid.apply_impulse(from.0, from.1, 170.0, -420.0);
                    warp_grid.apply_impulse(to.0, to.1, 190.0, 520.0);
                    shockwaves.push(Shockwave {
                        x: from.0,
                        y: from.1,
                        radius: 6.0,
                        max_radius: 75.0,
                        life: 0.32,
                        color: rgba(theme.lightning_color),
                    });
                    shockwaves.push(Shockwave {
                        x: to.0,
                        y: to.1,
                        radius: 6.0,
                        max_radius: 85.0,
                        life: 0.36,
                        color: rgba(theme.paddle_accent),
                    });
                    popups.push(FloatingPopup {
                        x: to.0 - 40.0,
                        y: to.1 - 16.0,
                        text: i18n.t("status.portal_warp").to_string(),
                        life: 0.75,
                        color: rgba(theme.lightning_color),
                    });
                }
                GameEvent::PaddleHit { x, y } => {
                    sfx.play_paddle();
                    warp_grid.apply_impulse(x, y, 130.0, 260.0);
                    for _ in 0..14 {
                        let angle = rand::gen_range(0.0, std::f32::consts::TAU);
                        let spd = rand::gen_range(80.0, 260.0);
                        particles.push(Particle {
                            x,
                            y,
                            vx: angle.cos() * spd,
                            vy: angle.sin() * spd,
                            size: 4.5,
                            life: 0.28,
                            max_life: 0.28,
                            color: rgba(theme.paddle_core),
                            gravity: 0.0,
                        });
                    }
                }
                GameEvent::WallHit { x, y } => {
                    sfx.play_wall();
                    warp_grid.apply_impulse(x, y, 100.0, 180.0);
                    for _ in 0..8 {
                        let angle = rand::gen_range(0.0, std::f32::consts::TAU);
                        let spd = rand::gen_range(50.0, 180.0);
                        particles.push(Particle {
                            x,
                            y,
                            vx: angle.cos() * spd,
                            vy: angle.sin() * spd,
                            size: 3.5,
                            life: 0.22,
                            max_life: 0.22,
                            color: rgba(theme.border_color),
                            gravity: 0.0,
                        });
                    }
                }
                GameEvent::PowerUpCollected { kind, x, y } => {
                    sfx.play_powerup();
                    warp_grid.apply_impulse(x, y, 150.0, 360.0);
                    let pop_col = if kind.is_malus() {
                        rgba(MALUS_COLOR)
                    } else {
                        Color::from_rgba(40, 255, 165, 255)
                    };
                    shockwaves.push(Shockwave {
                        x,
                        y,
                        radius: 8.0,
                        max_radius: 105.0,
                        life: 0.35,
                        color: pop_col,
                    });
                    let label = i18n.t(kind.i18n_key()).to_string();
                    popups.push(FloatingPopup {
                        x: (x - 120.0).clamp(80.0, 1560.0),
                        y: y - 28.0,
                        text: label,
                        life: 1.15,
                        color: pop_col,
                    });
                }
                GameEvent::LaserFired { x, y } => {
                    sfx.play_laser();
                    particles.push(Particle {
                        x,
                        y,
                        vx: 0.0,
                        vy: -120.0,
                        size: 5.5,
                        life: 0.12,
                        max_life: 0.12,
                        color: rgba(theme.paddle_accent),
                        gravity: 0.0,
                    });
                }
            }
        }

        warp_grid.update(raw_dt);
        if screen_shake > 0.0 {
            screen_shake = (screen_shake - raw_dt * 28.0).max(0.0);
        }
        if chromatic_pulse > 0.0 {
            chromatic_pulse = (chromatic_pulse - raw_dt * 3.2).max(0.0);
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
            s.vy += 650.0 * dt;
            s.angle += s.v_angle * dt;
            s.life -= raw_dt;
            s.life > 0.0
        });
        shockwaves.retain_mut(|s| {
            s.life -= raw_dt;
            let progress = 1.0 - (s.life / 0.40).clamp(0.0, 1.0);
            s.radius = s.max_radius * progress;
            s.life > 0.0
        });
        arcs.retain_mut(|a| {
            a.life -= raw_dt;
            a.life > 0.0
        });
        popups.retain_mut(|pop| {
            pop.y -= 55.0 * raw_dt;
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
        draw_backdrop(&world, theme, &neb_mat, &starfield, backdrop, time);

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
                selected_world,
                selected_sector_idx,
            );
        }

        next_frame().await
    }
}

fn draw_title_menu(
    world: &GameWorld,
    levels: &LevelPack,
    theme: &ThemeDef,
    i18n: &I18nCatalog,
    font: &Font,
    selected_world: usize,
    selected_sector_idx: usize,
) {
    let w = world.config.arena.width;
    let h = world.config.arena.height;
    draw_rectangle(0.0, 0.0, w, h, Color::from_rgba(5, 4, 15, 215));
    let pri = rgba(theme.text_primary);
    let acc = rgba(theme.text_accent);
    let border = rgba(theme.border_color);

    // Modern Glassmorphic Modal Panel
    draw_rounded_rect(310.0, 110.0, 1300.0, 860.0, 26.0, border);
    draw_rounded_rect(
        314.0,
        114.0,
        1292.0,
        852.0,
        24.0,
        Color::from_rgba(12, 14, 30, 245),
    );
    draw_rounded_rect(
        340.0,
        126.0,
        1240.0,
        90.0,
        16.0,
        Color::new(1.0, 1.0, 1.0, 0.05),
    );

    txt(font, i18n.t("app.title"), 745.0, 235.0, 96, acc);
    txt(font, i18n.t("app.subtitle"), 635.0, 290.0, 32, pri);

    draw_rounded_rect(
        680.0,
        325.0,
        560.0,
        56.0,
        14.0,
        Color::new(acc.r, acc.g, acc.b, 0.16),
    );
    let best_line = format!("{}: {:07}", i18n.t("menu.best_score"), world.high_score);
    txt(font, &best_line, 760.0, 364.0, 34, acc);

    txt(font, i18n.t("menu.campaigns"), 560.0, 445.0, 28, pri);
    for (idx, w_key) in ["campaigns.c1", "campaigns.c2", "campaigns.c3"]
        .iter()
        .enumerate()
    {
        let w_num = idx + 1;
        let y = 475.0 + idx as f32 * 68.0;
        let is_sel = selected_world == w_num;
        let pill_bg = if is_sel {
            Color::new(acc.r, acc.g, acc.b, 0.25)
        } else {
            Color::from_rgba(24, 28, 52, 200)
        };
        draw_rounded_rect(510.0, y, 900.0, 54.0, 14.0, pill_bg);
        let col = if is_sel { acc } else { pri };
        let prefix = if is_sel { ">>  " } else { "    " };
        txt(
            font,
            &format!("{}{}", prefix, i18n.t(w_key)),
            540.0,
            y + 37.0,
            30,
            col,
        );
    }

    let sec_def = &levels.levels[selected_sector_idx % levels.levels.len()];
    let sec_line = format!(
        "{} #{} - {}",
        i18n.t("menu.sector_select"),
        sec_def.id,
        sec_def.name
    );
    txt(font, &sec_line, 560.0, 735.0, 32, acc);

    // Glowing CTA Pill Button
    draw_rounded_rect(470.0, 775.0, 980.0, 78.0, 20.0, acc);
    draw_rounded_rect(
        474.0,
        779.0,
        972.0,
        70.0,
        18.0,
        Color::from_rgba(22, 16, 48, 240),
    );
    txt(font, i18n.t("menu.play"), 515.0, 825.0, 29, WHITE);

    let footer = format!(
        "{}   |   {}   |   {}   |   {}",
        i18n.t("menu.editor_btn"),
        i18n.t("menu.theme_btn"),
        i18n.t("menu.lang_btn"),
        i18n.t("menu.music_btn")
    );
    txt(font, &footer, 395.0, 920.0, 23, pri);
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
        let alpha = (world.shield_hits as f32 / 3.0) * 0.85;
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
            28,
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
        let alpha = (s.life / 0.40).clamp(0.0, 1.0);
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
        for seg in 1..=6 {
            let t = seg as f32 / 6.0;
            let jitter = if seg < 6 { 22.0 } else { 0.0 };
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
        let alpha = (pop.life / 0.70).clamp(0.0, 1.0);
        let c = Color::new(pop.color.r, pop.color.g, pop.color.b, alpha);
        txt(font, &pop.text, pop.x, pop.y, 26, c);
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
        48.0,
        8.0,
        1824.0,
        58.0,
        12.0,
        Color::new(border.r, border.g, border.b, 0.40),
    );
    draw_rounded_rect(
        49.5,
        9.5,
        1821.0,
        55.0,
        11.0,
        Color::from_rgba(10, 12, 26, 225),
    );

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
    txt(font, &top_bar, 72.0, 34.0, 21, pri);

    let perf_badge = format!("{}x{} px | {} {}", rt_w, rt_h, get_fps(), i18n.t("hud.fps"));
    txt(font, &perf_badge, 1555.0, 32.0, 17, acc);

    let v_ratio = (world.vortex_energy / world.config.gameplay.vortex_max_energy).clamp(0.0, 1.0);
    txt(font, i18n.t("hud.vortex"), 1555.0, 56.0, 17, pri);
    draw_rounded_rect(
        1650.0,
        42.0,
        190.0,
        12.0,
        6.0,
        Color::from_rgba(28, 32, 52, 240),
    );
    draw_rounded_rect(1650.0, 42.0, (190.0 * v_ratio).max(10.0), 12.0, 6.0, acc);

    let sub_bar = format!(
        "{}: {}    {}    {}    {}    {}    {}    [{}: {}]",
        i18n.t("hud.theme"),
        i18n.t(&theme.name_key),
        i18n.t("controls.theme_hint"),
        i18n.t("controls.lang_hint"),
        i18n.t("controls.level_hint"),
        i18n.t("controls.editor_hint"),
        i18n.t("controls.mute_hint"),
        i18n.t("hud.lang"),
        lang.code()
    );
    txt(font, &sub_bar, 72.0, 56.0, 16, acc);

    if editor_mode {
        txt(font, i18n.t("editor.banner"), 72.0, 138.0, 28, acc);
        txt(font, i18n.t("editor.instructions"), 72.0, 1058.0, 22, pri);
        return;
    }

    if world.level_elapsed >= world.config.gameplay.sudden_death_time_sec {
        txt(font, i18n.t("status.sudden_death"), 72.0, 136.0, 26, acc);
    }

    if world.last_powerup_timer > 0.0 {
        if let Some(key) = world.last_powerup_key {
            let msg = i18n.t(key);
            let is_malus = key.ends_with("shrink") || key.ends_with("fast");
            let toast_col = if is_malus {
                rgba(MALUS_COLOR)
            } else {
                Color::from_rgba(30, 250, 165, 255)
            };
            let dims = measure_text(msg, Some(font), 24, 1.0);
            let bw = dims.width + 44.0;
            let bx = 960.0 - bw * 0.5;
            draw_rounded_rect(bx, 82.0, bw, 38.0, 10.0, toast_col);
            draw_rounded_rect(
                bx + 2.5,
                84.5,
                bw - 5.0,
                33.0,
                8.0,
                Color::from_rgba(8, 12, 24, 240),
            );
            txt(font, msg, bx + 22.0, 109.0, 24, toast_col);
        }
    }
    if world.balls.iter().any(|b| b.stuck_offset.is_some()) {
        txt(font, i18n.t("controls.launch_hint"), 380.0, 1060.0, 22, pri);
    } else if world.laser_active {
        txt(font, i18n.t("controls.laser_hint"), 380.0, 1060.0, 22, acc);
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

fn draw_overlay_box(font: &Font, title: &str, subtitle: &str, title_col: Color, sub_col: Color) {
    draw_rounded_rect(460.0, 410.0, 1000.0, 250.0, 24.0, title_col);
    draw_rounded_rect(
        464.0,
        414.0,
        992.0,
        242.0,
        22.0,
        Color::from_rgba(10, 12, 26, 242),
    );
    txt(font, title, 530.0, 515.0, 48, title_col);
    txt(font, subtitle, 530.0, 585.0, 28, sub_col);
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
