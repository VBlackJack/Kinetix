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

//! Neon panel rendering of the menu, consistent with the original title screen.

use kinetix_core::config::ThemeDef;
use kinetix_core::i18n::I18nCatalog;
use macroquad::prelude::*;

use super::model::MenuModel;
use super::state::Menu;
use super::widgets::{
    fit_to_sizes, Align, Role, Tone, UiRect, Widget, HEADER_H, HEADER_INSET_X, HEADER_INSET_Y,
    HEADER_RADIUS, PANEL_BORDER, PANEL_H, PANEL_RADIUS, PANEL_W, PANEL_X, PANEL_Y,
};

const DIM_COLOR: [u8; 4] = [5, 4, 15, 215];
const PANEL_COLOR: [u8; 4] = [12, 14, 30, 245];
const IDLE_PILL_COLOR: [u8; 4] = [24, 28, 52, 200];
const HEADER_BAND_ALPHA: f32 = 0.05;
const FOCUS_FILL_ALPHA: f32 = 0.25;
const FOCUS_BORDER: f32 = 3.0;
const PILL_RADIUS: f32 = 14.0;
const DISABLED_ALPHA: f32 = 0.4;
const SHADOW_OFFSET: f32 = 2.0;
const SHADOW_ALPHA: f32 = 0.65;
const PILL_PAD_X: f32 = 30.0;
const PILL_VALUE_GAP: f32 = 24.0;
/// Baseline offset below the vertical centre of a pill, as a fraction of the font size.
const BASELINE_FACTOR: f32 = 0.35;
const TEXT_MAX_W: f32 = PANEL_W - 2.0 * HEADER_INSET_X;
const TEXT_SPACING: f32 = 1.0;

fn rgba(c: [u8; 4]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], c[3])
}

fn with_alpha(c: Color, alpha: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * alpha)
}

fn fill_rounded(x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
    let r = radius.min(w * 0.5).min(h * 0.5);
    draw_rectangle(x + r, y, w - 2.0 * r, h, color);
    draw_rectangle(x, y + r, r, h - 2.0 * r, color);
    draw_rectangle(x + w - r, y + r, r, h - 2.0 * r, color);
    draw_circle(x + r, y + r, r, color);
    draw_circle(x + w - r, y + r, r, color);
    draw_circle(x + r, y + h - r, r, color);
    draw_circle(x + w - r, y + h - r, r, color);
}

fn text_width(font: &Font, text: &str, size: u16) -> f32 {
    measure_text(text, Some(font), size, TEXT_SPACING).width
}

fn draw_shadowed(font: &Font, text: &str, x: f32, y: f32, size: u16, color: Color) {
    let params = |color: Color| TextParams {
        font: Some(font),
        font_size: size,
        color,
        ..Default::default()
    };
    draw_text_ex(
        text,
        x + SHADOW_OFFSET,
        y + SHADOW_OFFSET,
        params(Color::new(0.0, 0.0, 0.0, SHADOW_ALPHA * color.a)),
    );
    draw_text_ex(text, x, y, params(color));
}

/// Where and how a fitted text is drawn.
struct FitSpec {
    x: f32,
    y: f32,
    preferred: u16,
    limit: f32,
    align: Align,
    color: Color,
}

/// Draws `text` shrunk (to a menu size) to fit `spec.limit`. Returns the drawn width.
fn draw_fitted(font: &Font, text: &str, spec: FitSpec) -> f32 {
    let size = fit_to_sizes(|s| text_width(font, text, s), spec.preferred, spec.limit);
    let width = text_width(font, text, size);
    let left = match spec.align {
        Align::Left => spec.x,
        Align::Center => spec.x - width * 0.5,
        Align::Right => spec.x - width,
    };
    draw_shadowed(font, text, left, spec.y, size, spec.color);
    width
}

fn draw_widget(
    widget: &Widget,
    focused: bool,
    i18n: &I18nCatalog,
    font: &Font,
    primary: Color,
    accent: Color,
) {
    let UiRect { x, y, w, h } = widget.rect;
    let alpha = if widget.enabled { 1.0 } else { DISABLED_ALPHA };
    if focused {
        fill_rounded(x, y, w, h, PILL_RADIUS, with_alpha(accent, alpha));
        fill_rounded(
            x + FOCUS_BORDER,
            y + FOCUS_BORDER,
            w - 2.0 * FOCUS_BORDER,
            h - 2.0 * FOCUS_BORDER,
            PILL_RADIUS - FOCUS_BORDER,
            rgba(IDLE_PILL_COLOR),
        );
        fill_rounded(
            x + FOCUS_BORDER,
            y + FOCUS_BORDER,
            w - 2.0 * FOCUS_BORDER,
            h - 2.0 * FOCUS_BORDER,
            PILL_RADIUS - FOCUS_BORDER,
            with_alpha(accent, FOCUS_FILL_ALPHA * alpha),
        );
    } else {
        fill_rounded(x, y, w, h, PILL_RADIUS, rgba(IDLE_PILL_COLOR));
    }
    let color = with_alpha(if focused { accent } else { primary }, alpha);
    let baseline = y + h * 0.5 + widget.size as f32 * BASELINE_FACTOR;
    let label = widget.label.resolve(i18n);
    let inner_w = w - 2.0 * PILL_PAD_X;
    let value_w = match &widget.value {
        Some(value) => {
            let text = value.resolve(i18n);
            draw_fitted(
                font,
                &text,
                FitSpec {
                    x: x + w - PILL_PAD_X,
                    y: baseline,
                    preferred: widget.size,
                    limit: inner_w * 0.5,
                    align: Align::Right,
                    color: with_alpha(accent, alpha),
                },
            ) + PILL_VALUE_GAP
        }
        None => 0.0,
    };
    let centered = widget.value.is_none() && matches!(widget.role, Role::Extra | Role::Back);
    let spec = if centered {
        FitSpec {
            x: x + w * 0.5,
            y: baseline,
            preferred: widget.size,
            limit: inner_w,
            align: Align::Center,
            color,
        }
    } else {
        FitSpec {
            x: x + PILL_PAD_X,
            y: baseline,
            preferred: widget.size,
            limit: inner_w - value_w,
            align: Align::Left,
            color,
        }
    };
    draw_fitted(font, &label, spec);
}

/// Draws the current menu page over the arena.
pub fn draw_menu(
    menu: &Menu,
    model: &MenuModel,
    theme: &ThemeDef,
    i18n: &I18nCatalog,
    font: &Font,
    arena_w: f32,
    arena_h: f32,
) {
    let primary = rgba(theme.text_primary);
    let accent = rgba(theme.text_accent);
    let border = rgba(theme.border_color);
    draw_rectangle(0.0, 0.0, arena_w, arena_h, rgba(DIM_COLOR));
    fill_rounded(PANEL_X, PANEL_Y, PANEL_W, PANEL_H, PANEL_RADIUS, border);
    fill_rounded(
        PANEL_X + PANEL_BORDER,
        PANEL_Y + PANEL_BORDER,
        PANEL_W - 2.0 * PANEL_BORDER,
        PANEL_H - 2.0 * PANEL_BORDER,
        PANEL_RADIUS - PANEL_BORDER,
        rgba(PANEL_COLOR),
    );
    let layout = menu.layout(model);
    if !layout.is_home {
        fill_rounded(
            PANEL_X + HEADER_INSET_X,
            PANEL_Y + HEADER_INSET_Y,
            PANEL_W - 2.0 * HEADER_INSET_X,
            HEADER_H,
            HEADER_RADIUS,
            Color::new(1.0, 1.0, 1.0, HEADER_BAND_ALPHA),
        );
    }
    for item in &layout.texts {
        let color = match item.tone {
            Tone::Primary => primary,
            Tone::Accent => accent,
        };
        let text = item.label.resolve(i18n);
        let spec = FitSpec {
            x: item.x,
            y: item.y,
            preferred: item.size,
            limit: TEXT_MAX_W,
            align: item.align,
            color,
        };
        draw_fitted(font, &text, spec);
    }
    for (index, widget) in layout.widgets.iter().enumerate() {
        draw_widget(widget, index == menu.focus(), i18n, font, primary, accent);
    }
}
