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
/// Text colours tried on the accent-filled focused widget: the panel background and white.
const FOCUS_TEXT_DARK: [u8; 4] = [12, 14, 30, 255];
const FOCUS_TEXT_LIGHT: [u8; 4] = [255, 255, 255, 255];
/// Linearisation of an sRGB channel (WCAG relative luminance).
const SRGB_THRESHOLD: f32 = 0.039_28;
const SRGB_LINEAR_DIVISOR: f32 = 12.92;
const SRGB_OFFSET: f32 = 0.055;
const SRGB_SCALE: f32 = 1.055;
const SRGB_GAMMA: f32 = 2.4;
const LUMINANCE_WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// Offset added to both luminances in the WCAG contrast ratio.
const CONTRAST_OFFSET: f32 = 0.05;
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

fn linear_channel(value: u8) -> f32 {
    let c = f32::from(value) / f32::from(u8::MAX);
    if c <= SRGB_THRESHOLD {
        c / SRGB_LINEAR_DIVISOR
    } else {
        ((c + SRGB_OFFSET) / SRGB_SCALE).powf(SRGB_GAMMA)
    }
}

fn luminance(c: [u8; 4]) -> f32 {
    LUMINANCE_WEIGHTS[0] * linear_channel(c[0])
        + LUMINANCE_WEIGHTS[1] * linear_channel(c[1])
        + LUMINANCE_WEIGHTS[2] * linear_channel(c[2])
}

/// WCAG contrast ratio between two colours (always at least 1).
pub(crate) fn contrast_ratio(a: [u8; 4], b: [u8; 4]) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + CONTRAST_OFFSET) / (la.min(lb) + CONTRAST_OFFSET)
}

/// Text colour drawn on a widget filled with `fill`: whichever of the panel background and white
/// contrasts more.
pub(crate) fn focus_text_color(fill: [u8; 4]) -> [u8; 4] {
    if contrast_ratio(fill, FOCUS_TEXT_DARK) >= contrast_ratio(fill, FOCUS_TEXT_LIGHT) {
        FOCUS_TEXT_DARK
    } else {
        FOCUS_TEXT_LIGHT
    }
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

/// Theme colours a widget is drawn with.
#[derive(Clone, Copy)]
struct WidgetColors {
    primary: Color,
    accent: Color,
    /// Text on the accent-filled focused widget.
    focus_text: Color,
}

fn draw_widget(
    widget: &Widget,
    focused: bool,
    i18n: &I18nCatalog,
    font: &Font,
    colors: WidgetColors,
) {
    let UiRect { x, y, w, h } = widget.rect;
    let alpha = if widget.enabled { 1.0 } else { DISABLED_ALPHA };
    let WidgetColors {
        primary,
        accent,
        focus_text,
    } = colors;
    if focused {
        fill_rounded(x, y, w, h, PILL_RADIUS, with_alpha(accent, alpha));
    } else {
        fill_rounded(x, y, w, h, PILL_RADIUS, rgba(IDLE_PILL_COLOR));
    }
    let color = with_alpha(if focused { focus_text } else { primary }, alpha);
    let value_color = if focused { focus_text } else { accent };
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
                    color: with_alpha(value_color, alpha),
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
    let colors = WidgetColors {
        primary,
        accent,
        focus_text: rgba(focus_text_color(theme.text_accent)),
    };
    for (index, widget) in layout.widgets.iter().enumerate() {
        draw_widget(widget, index == menu.focus(), i18n, font, colors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinetix_core::config::ThemeRegistry;

    const RAW_THEMES: &str = include_str!("../../../../assets/config/themes.json");
    /// Minimum contrast ratio of the focused text against the accent fill (WCAG AA for the
    /// large text the menu uses is 3; this keeps a margin above it).
    const MIN_FOCUS_CONTRAST: f32 = 4.0;

    #[test]
    fn contrast_ratio_is_symmetric_and_bounded() {
        let black = [0, 0, 0, 255];
        let white = [255, 255, 255, 255];
        assert!((contrast_ratio(black, white) - 21.0).abs() < 0.01);
        assert_eq!(contrast_ratio(black, white), contrast_ratio(white, black));
        assert!((contrast_ratio(white, white) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn focus_text_picks_the_more_readable_of_the_two_colours() {
        assert_eq!(focus_text_color([255, 255, 80, 255]), FOCUS_TEXT_DARK);
        assert_eq!(focus_text_color([20, 30, 140, 255]), FOCUS_TEXT_LIGHT);
    }

    #[test]
    fn focused_text_is_readable_on_every_theme_accent() {
        let registry: ThemeRegistry = serde_json::from_str(RAW_THEMES).unwrap();
        assert!(!registry.themes.is_empty());
        for theme in &registry.themes {
            let text = focus_text_color(theme.text_accent);
            let ratio = contrast_ratio(theme.text_accent, text);
            assert!(
                ratio >= MIN_FOCUS_CONTRAST,
                "{} focus contrast {ratio}",
                theme.name_key
            );
        }
    }
}
