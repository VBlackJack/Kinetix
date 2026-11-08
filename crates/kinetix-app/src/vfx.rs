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

use kinetix_core::physics::PowerUpKind;
use macroquad::prelude::*;
use miniquad::{BlendFactor, BlendState, Equation, UniformDesc, UniformType};

pub fn additive_material() -> Material {
    let vertex_shader = "#version 100
    attribute vec3 position;
    attribute vec2 texcoord;
    attribute vec4 color0;
    varying lowp vec2 uv;
    varying lowp vec4 color;
    uniform mat4 Model;
    uniform mat4 Projection;
    void main() {
        gl_Position = Projection * Model * vec4(position, 1);
        color = color0 / 255.0;
        uv = texcoord;
    }";

    let fragment_shader = "#version 100
    varying lowp vec4 color;
    varying lowp vec2 uv;
    uniform sampler2D Texture;
    void main() {
        gl_FragColor = vec4(color.rgb * color.a, color.a) * texture2D(Texture, uv);
    }";

    load_material(
        ShaderSource::Glsl {
            vertex: vertex_shader,
            fragment: fragment_shader,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::One,
                    BlendFactor::One,
                )),
                alpha_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::One,
                    BlendFactor::One,
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("Failed to compile additive blend shader")
}

pub fn nebula_material() -> Material {
    let vertex_shader = "#version 100
    attribute vec3 position;
    attribute vec2 texcoord;
    attribute vec4 color0;
    varying highp vec2 uv;
    varying lowp vec4 color;
    uniform mat4 Model;
    uniform mat4 Projection;
    void main() {
        gl_Position = Projection * Model * vec4(position, 1);
        color = color0 / 255.0;
        uv = texcoord;
    }";

    let fragment_shader = "#version 100
    precision highp float;
    varying highp vec2 uv;
    varying lowp vec4 color;
    uniform vec4 u_params; // x: time, y: paddle_norm_x, z: style_id, w: pulse
    uniform vec4 u_col_top;
    uniform vec4 u_col_bot;
    uniform vec4 u_col_acc;
    uniform vec4 u_rect; // xy: arena-relative origin of the drawn rect, zw: its arena-relative size

    float hash(vec2 p) {
        return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
    }

    float noise(vec2 p) {
        vec2 i = floor(p);
        vec2 f = fract(p);
        vec2 u = f * f * (3.0 - 2.0 * f);
        return mix(
            mix(hash(i + vec2(0.0, 0.0)), hash(i + vec2(1.0, 0.0)), u.x),
            mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x),
            u.y
        );
    }

    float fbm(vec2 p) {
        float v = 0.0;
        float a = 0.5;
        for (int i = 0; i < 4; i++) {
            v += a * noise(p);
            p = p * 2.03 + vec2(1.7, 9.2);
            a *= 0.5;
        }
        return v;
    }

    void main() {
        // Arena-relative coordinates: identical to `uv` inside the arena, continuous outside it.
        vec2 auv = u_rect.xy + uv * u_rect.zw;
        float t = u_params.x * 0.18;
        vec2 p = auv * vec2(3.2, 1.8);
        p.x += (u_params.y - 0.5) * 0.25;

        float n1 = fbm(p + vec2(t, -t * 0.6));
        float n2 = fbm(p * 1.6 - vec2(t * 0.8, t * 0.4) + n1 * 1.4);
        float cloud = smoothstep(0.22, 0.85, n2);

        vec3 base = mix(u_col_top.rgb, u_col_bot.rgb, clamp(auv.y, 0.0, 1.0));
        vec3 aurora = u_col_acc.rgb * cloud * (0.28 + u_params.w * 0.35);

        // Subtle radial spotlight around center-top
        float d = length((auv - vec2(0.5, 0.38)) * vec2(1.3, 1.0));
        float vig = smoothstep(1.15, 0.15, d);

        vec3 final_col = (base + aurora) * (0.65 + 0.35 * vig);
        gl_FragColor = vec4(final_col, 1.0);
    }";

    load_material(
        ShaderSource::Glsl {
            vertex: vertex_shader,
            fragment: fragment_shader,
        },
        MaterialParams {
            uniforms: vec![
                UniformDesc::new("u_params", UniformType::Float4),
                UniformDesc::new("u_col_top", UniformType::Float4),
                UniformDesc::new("u_col_bot", UniformType::Float4),
                UniformDesc::new("u_col_acc", UniformType::Float4),
                UniformDesc::new("u_rect", UniformType::Float4),
            ],
            ..Default::default()
        },
    )
    .expect("Failed to compile nebula shader")
}

pub fn draw_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32, col: Color) {
    let r = r.min(w * 0.5).min(h * 0.5);
    draw_rectangle(x + r, y, w - 2.0 * r, h, col);
    draw_rectangle(x, y + r, r, h - 2.0 * r, col);
    draw_rectangle(x + w - r, y + r, r, h - 2.0 * r, col);
    draw_circle(x + r, y + r, r, col);
    draw_circle(x + w - r, y + r, r, col);
    draw_circle(x + r, y + h - r, r, col);
    draw_circle(x + w - r, y + h - r, r, col);
}

pub fn draw_modern_capsule_brick(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    col: Color,
    border_col: Color,
    glow_alpha: f32,
) {
    let r = (h * 0.28).min(7.0);
    // Soft drop shadow
    draw_rounded_rect(
        x + 2.0,
        y + 3.0,
        w,
        h,
        r,
        Color::new(0.0, 0.0, 0.0, 0.38 * col.a),
    );
    // Subtle outer neon aura
    if glow_alpha > 0.01 {
        draw_rounded_rect(
            x - 2.5,
            y - 2.5,
            w + 5.0,
            h + 5.0,
            r + 2.0,
            Color::new(col.r, col.g, col.b, glow_alpha * 0.65 * col.a),
        );
    }
    // Thin crisp rim (1.5px instead of 2.5px blocky border)
    draw_rounded_rect(
        x,
        y,
        w,
        h,
        r,
        Color::new(
            border_col.r,
            border_col.g,
            border_col.b,
            0.75 * border_col.a,
        ),
    );
    // Rich saturated body
    draw_rounded_rect(
        x + 1.5,
        y + 1.5,
        w - 3.0,
        h - 3.0,
        (r - 1.0).max(2.0),
        Color::new(col.r * 0.72, col.g * 0.72, col.b * 0.72, 0.96 * col.a),
    );
    // Upper luminous gradient half
    draw_rounded_rect(
        x + 2.5,
        y + 2.5,
        w - 5.0,
        h * 0.46,
        (r - 1.5).max(1.5),
        Color::new(col.r, col.g, col.b, 0.92 * col.a),
    );
    // Crisp 2px top glass highlight
    draw_rectangle(
        x + r + 2.0,
        y + 3.0,
        (w - 2.0 * r - 4.0).max(2.0),
        2.0,
        Color::new(1.0, 1.0, 1.0, 0.38 * col.a),
    );
}

struct GridPoint {
    base_x: f32,
    base_y: f32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

pub struct WarpGrid {
    cols: usize,
    rows: usize,
    points: Vec<GridPoint>,
}

impl WarpGrid {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32, step: f32) -> Self {
        let cols = (((right - left) / step).round() as usize) + 1;
        let rows = (((bottom - top) / step).round() as usize) + 1;
        let mut points = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            let by = top + r as f32 * step;
            for c in 0..cols {
                let bx = left + c as f32 * step;
                points.push(GridPoint {
                    base_x: bx,
                    base_y: by,
                    x: bx,
                    y: by,
                    vx: 0.0,
                    vy: 0.0,
                });
            }
        }
        Self { cols, rows, points }
    }

    pub fn apply_impulse(&mut self, cx: f32, cy: f32, radius: f32, strength: f32) {
        let r_sq = radius * radius;
        for (idx, p) in self.points.iter_mut().enumerate() {
            let r = idx / self.cols;
            let c = idx % self.cols;
            if r == 0 || r == self.rows - 1 || c == 0 || c == self.cols - 1 {
                continue;
            }
            let dx = p.x - cx;
            let dy = p.y - cy;
            let d_sq = dx * dx + dy * dy;
            if d_sq < r_sq && d_sq > 1.0 {
                let d = d_sq.sqrt();
                let factor = (1.0 - d / radius).powi(2) * strength;
                p.vx += (dx / d) * factor;
                p.vy += (dy / d) * factor;
            }
        }
    }

    pub fn update(&mut self, dt: f32) {
        let stiffness = 42.0;
        let damping = 0.88f32.powf(dt * 60.0);
        for p in &mut self.points {
            let fx = (p.base_x - p.x) * stiffness;
            let fy = (p.base_y - p.y) * stiffness;
            p.vx = (p.vx + fx * dt) * damping;
            p.vy = (p.vy + fy * dt) * damping;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }
    }

    pub fn draw(&self, base_col: Color, accent_col: Color) {
        for r in 0..self.rows {
            for c in 0..self.cols {
                let idx = r * self.cols + c;
                let p = &self.points[idx];
                let disp = ((p.x - p.base_x).powi(2) + (p.y - p.base_y).powi(2)).sqrt();
                let strain = (disp / 24.0).clamp(0.0, 1.0);
                let col = Color::new(
                    base_col.r + (accent_col.r - base_col.r) * strain,
                    base_col.g + (accent_col.g - base_col.g) * strain,
                    base_col.b + (accent_col.b - base_col.b) * strain,
                    (base_col.a + strain * 0.55).min(0.85),
                );
                let thick = 1.0 + strain * 1.6;
                if c + 1 < self.cols {
                    let right = &self.points[idx + 1];
                    draw_line(p.x, p.y, right.x, right.y, thick, col);
                }
                if r + 1 < self.rows {
                    let down = &self.points[idx + self.cols];
                    draw_line(p.x, p.y, down.x, down.y, thick, col);
                }
                if strain > 0.08 {
                    draw_circle(p.x, p.y, 2.2 + strain * 2.0, col);
                }
            }
        }
    }
}

pub struct Starfield {
    stars: Vec<(f32, f32, f32, f32)>,
    origin_x: f32,
    origin_y: f32,
    span_w: f32,
    span_h: f32,
}

impl Starfield {
    /// Generates stars over the arena scaled by `extent_factor` on each axis and centred on it.
    /// `base_count` is the star count for the arena area alone, so density per area is constant.
    pub fn new(width: f32, height: f32, base_count: usize, extent_factor: f32) -> Self {
        let span_w = width * extent_factor;
        let span_h = height * extent_factor;
        let origin_x = (width - span_w) * 0.5;
        let origin_y = (height - span_h) * 0.5;
        let count = (base_count as f32 * extent_factor * extent_factor).round() as usize;
        let mut stars = Vec::with_capacity(count);
        let mut seed = 0x1337_BEEFu32;
        for _ in 0..count {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let x = origin_x + (seed % (span_w as u32)) as f32;
            seed ^= seed << 13;
            let y = origin_y + (seed % (span_h as u32)) as f32;
            let depth = 0.15 + ((seed & 0xFF) as f32 / 255.0) * 0.85;
            let size = 1.0 + depth * 2.2;
            stars.push((x, y, depth, size));
        }
        Self {
            stars,
            origin_x,
            origin_y,
            span_w,
            span_h,
        }
    }

    /// Draws the stars that fall inside `view` (arena units), which may extend past the arena.
    pub fn draw(&self, paddle_x: f32, arena_w: f32, view: Rect, time: f32, accent: Color) {
        let offset_x = (paddle_x - arena_w * 0.5) * -0.08;
        for (i, &(bx, by, depth, size)) in self.stars.iter().enumerate() {
            let x = self.origin_x + (bx - self.origin_x + offset_x * depth).rem_euclid(self.span_w);
            let y =
                self.origin_y + (by - self.origin_y + time * 18.0 * depth).rem_euclid(self.span_h);
            if !view.contains(vec2(x, y)) {
                continue;
            }
            let twinkle = 0.35 + 0.35 * ((time * 3.0 + i as f32).sin());
            let c = Color::new(
                accent.r * 0.6 + 0.4,
                accent.g * 0.6 + 0.4,
                accent.b * 0.6 + 0.4,
                depth * twinkle,
            );
            draw_circle(x, y, size * 1.25, c);
        }
    }
}

pub struct DebrisShard {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub w: f32,
    pub angle: f32,
    pub v_angle: f32,
    pub life: f32,
    pub max_life: f32,
    pub color: Color,
}

pub fn draw_ribbon_trail(points: &[(f32, f32)], max_radius: f32, color: Color) {
    if points.len() < 2 {
        return;
    }
    let n = points.len();
    for i in 0..(n - 1) {
        let (x1, y1) = points[i];
        let (x2, y2) = points[i + 1];
        let t1 = i as f32 / n as f32;
        let t2 = (i + 1) as f32 / n as f32;
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        let nx = -dy / len;
        let ny = dx / len;
        let r1 = max_radius * t1 * 0.95;
        let r2 = max_radius * t2 * 0.95;
        let c = Color::new(color.r, color.g, color.b, t2 * 0.55);

        let p1 = vec2(x1 + nx * r1, y1 + ny * r1);
        let p2 = vec2(x1 - nx * r1, y1 - ny * r1);
        let p3 = vec2(x2 + nx * r2, y2 + ny * r2);
        let p4 = vec2(x2 - nx * r2, y2 - ny * r2);

        draw_triangle(p1, p2, p3, c);
        draw_triangle(p2, p4, p3, c);
    }
}

pub fn draw_powerup_capsule(
    x: f32,
    y: f32,
    r: f32,
    kind: PowerUpKind,
    badge_label: &str,
    time: f32,
    font: Option<&Font>,
) {
    let is_bad = kind.is_malus();
    let (rim_col, bg_col, icon_col) = match kind {
        PowerUpKind::Shrink | PowerUpKind::Fast => (
            Color::from_rgba(255, 42, 55, 255),
            Color::from_rgba(62, 10, 16, 248),
            Color::from_rgba(255, 210, 75, 255),
        ),
        PowerUpKind::Fireball | PowerUpKind::MegaBall | PowerUpKind::MultiBall => (
            Color::from_rgba(255, 150, 20, 255),
            Color::from_rgba(52, 24, 6, 248),
            Color::from_rgba(255, 225, 95, 255),
        ),
        PowerUpKind::Laser | PowerUpKind::Lightning => (
            Color::from_rgba(180, 95, 255, 255),
            Color::from_rgba(32, 14, 58, 248),
            Color::from_rgba(235, 195, 255, 255),
        ),
        PowerUpKind::ExtraLife => (
            Color::from_rgba(255, 65, 165, 255),
            Color::from_rgba(56, 12, 38, 248),
            Color::from_rgba(255, 165, 218, 255),
        ),
        _ => (
            Color::from_rgba(20, 245, 155, 255),
            Color::from_rgba(8, 46, 32, 248),
            Color::from_rgba(140, 255, 210, 255),
        ),
    };

    let pulse = 1.0 + (time * if is_bad { 12.0 } else { 5.5 }).sin() * 0.10;
    let w = r * 2.85;
    let h = r * 2.22;
    let bx = x - w * 0.5;
    let by = y - h * 0.5;

    // Outer recognition aura (Danger rotating diamond for Malus, soft neon halo for Bonus)
    if is_bad {
        draw_poly_lines(
            x,
            y,
            4,
            r * 1.48 * pulse,
            time * 85.0,
            3.0,
            Color::new(rim_col.r, rim_col.g, rim_col.b, 0.85),
        );
        // Top corner danger spikes
        draw_triangle(
            vec2(bx - 4.0, by - 4.0),
            vec2(bx + 10.0, by - 4.0),
            vec2(bx - 4.0, by + 10.0),
            icon_col,
        );
        draw_triangle(
            vec2(bx + w + 4.0, by - 4.0),
            vec2(bx + w - 10.0, by - 4.0),
            vec2(bx + w + 4.0, by + 10.0),
            icon_col,
        );
    } else {
        draw_rounded_rect(
            bx - 5.0 * pulse,
            by - 5.0 * pulse,
            w + 10.0 * pulse,
            h + 10.0 * pulse,
            15.0,
            Color::new(rim_col.r, rim_col.g, rim_col.b, 0.24),
        );
    }

    // Main Card Chassis
    draw_rounded_rect(bx, by, w, h, 12.0, rim_col);
    draw_rounded_rect(bx + 3.0, by + 3.0, w - 6.0, h - 6.0, 10.0, bg_col);

    // Top zone: Dark circular high-contrast medallion for the pictogram
    let iy = by + (h - 22.0) * 0.5 + 1.0;
    let med_r = (h - 28.0) * 0.48;
    draw_circle(x, iy, med_r, Color::from_rgba(6, 9, 18, 240));
    draw_circle_lines(
        x,
        iy,
        med_r,
        2.0,
        Color::new(rim_col.r, rim_col.g, rim_col.b, 0.65),
    );

    // Unmistakable scaled vector icon inside the top medallion
    let s = r / 24.0;
    match kind {
        PowerUpKind::Fireball => {
            draw_triangle(
                vec2(x, iy - 14.5 * s),
                vec2(x - 11.5 * s, iy + 6.0 * s),
                vec2(x + 11.5 * s, iy + 6.0 * s),
                Color::from_rgba(255, 70, 15, 255),
            );
            draw_triangle(
                vec2(x - 6.0 * s, iy - 10.0 * s),
                vec2(x - 12.0 * s, iy + 5.0 * s),
                vec2(x + 2.0 * s, iy + 5.0 * s),
                icon_col,
            );
            draw_triangle(
                vec2(x + 6.0 * s, iy - 10.0 * s),
                vec2(x - 2.0 * s, iy + 5.0 * s),
                vec2(x + 12.0 * s, iy + 5.0 * s),
                icon_col,
            );
            draw_circle(x, iy + 3.5 * s, 8.5 * s, icon_col);
            draw_circle(x, iy + 4.5 * s, 5.0 * s, WHITE);
        }
        PowerUpKind::MegaBall => {
            draw_circle_lines(x, iy, 13.5 * s, 2.8 * s, icon_col);
            draw_circle(x, iy, 8.8 * s, WHITE);
            draw_circle(x - 2.5 * s, iy - 2.5 * s, 3.0 * s, rim_col);
            // 4 outward expansion spikes
            draw_line(x - 15.5 * s, iy, x - 10.5 * s, iy, 2.8 * s, icon_col);
            draw_line(x + 10.5 * s, iy, x + 15.5 * s, iy, 2.8 * s, icon_col);
            draw_line(x, iy - 15.5 * s, x, iy - 10.5 * s, 2.8 * s, icon_col);
            draw_line(x, iy + 10.5 * s, x, iy + 15.5 * s, 2.8 * s, icon_col);
        }
        PowerUpKind::MultiBall => {
            draw_line(x, iy - 8.5 * s, x - 9.5 * s, iy + 6.5 * s, 2.0 * s, rim_col);
            draw_line(x, iy - 8.5 * s, x + 9.5 * s, iy + 6.5 * s, 2.0 * s, rim_col);
            draw_line(
                x - 9.5 * s,
                iy + 6.5 * s,
                x + 9.5 * s,
                iy + 6.5 * s,
                2.0 * s,
                rim_col,
            );
            draw_circle(x, iy - 8.5 * s, 5.8 * s, WHITE);
            draw_circle(x - 9.5 * s, iy + 6.5 * s, 5.8 * s, icon_col);
            draw_circle(x + 9.5 * s, iy + 6.5 * s, 5.8 * s, icon_col);
        }
        PowerUpKind::Expand => {
            draw_rounded_rect(x - 8.0 * s, iy - 3.5 * s, 16.0 * s, 7.0 * s, 3.0 * s, WHITE);
            draw_triangle(
                vec2(x - 16.5 * s, iy),
                vec2(x - 9.0 * s, iy - 7.5 * s),
                vec2(x - 9.0 * s, iy + 7.5 * s),
                icon_col,
            );
            draw_triangle(
                vec2(x + 16.5 * s, iy),
                vec2(x + 9.0 * s, iy - 7.5 * s),
                vec2(x + 9.0 * s, iy + 7.5 * s),
                icon_col,
            );
        }
        PowerUpKind::Shrink => {
            draw_triangle(
                vec2(x - 3.5 * s, iy),
                vec2(x - 15.0 * s, iy - 8.0 * s),
                vec2(x - 15.0 * s, iy + 8.0 * s),
                rim_col,
            );
            draw_triangle(
                vec2(x + 3.5 * s, iy),
                vec2(x + 15.0 * s, iy - 8.0 * s),
                vec2(x + 15.0 * s, iy + 8.0 * s),
                rim_col,
            );
            draw_rectangle(x - 2.2 * s, iy - 8.5 * s, 4.4 * s, 17.0 * s, icon_col);
        }
        PowerUpKind::Magnet => {
            draw_line(
                x - 9.0 * s,
                iy - 9.0 * s,
                x - 9.0 * s,
                iy + 6.5 * s,
                4.8 * s,
                rim_col,
            );
            draw_line(
                x + 9.0 * s,
                iy - 9.0 * s,
                x + 9.0 * s,
                iy + 6.5 * s,
                4.8 * s,
                rim_col,
            );
            draw_line(
                x - 9.0 * s,
                iy + 6.5 * s,
                x + 9.0 * s,
                iy + 6.5 * s,
                4.8 * s,
                rim_col,
            );
            draw_rectangle(x - 11.4 * s, iy - 10.5 * s, 5.2 * s, 5.5 * s, WHITE);
            draw_rectangle(x + 6.2 * s, iy - 10.5 * s, 5.2 * s, 5.5 * s, WHITE);
            draw_line(
                x - 4.0 * s,
                iy - 8.0 * s,
                x,
                iy - 12.0 * s,
                2.0 * s,
                icon_col,
            );
            draw_line(
                x,
                iy - 12.0 * s,
                x + 4.0 * s,
                iy - 8.0 * s,
                2.0 * s,
                icon_col,
            );
        }
        PowerUpKind::Laser => {
            draw_rounded_rect(
                x - 12.0 * s,
                iy + 5.0 * s,
                24.0 * s,
                5.5 * s,
                2.5 * s,
                icon_col,
            );
            draw_rectangle(x - 10.5 * s, iy - 1.5 * s, 5.0 * s, 7.5 * s, icon_col);
            draw_rectangle(x + 5.5 * s, iy - 1.5 * s, 5.0 * s, 7.5 * s, icon_col);
            draw_line(
                x - 8.0 * s,
                iy - 2.0 * s,
                x - 8.0 * s,
                iy - 13.0 * s,
                3.4 * s,
                WHITE,
            );
            draw_line(
                x + 8.0 * s,
                iy - 2.0 * s,
                x + 8.0 * s,
                iy - 13.0 * s,
                3.4 * s,
                WHITE,
            );
        }
        PowerUpKind::Lightning => {
            draw_line(x + 5.0 * s, iy - 13.0 * s, x - 7.0 * s, iy, 4.0 * s, WHITE);
            draw_line(x - 7.0 * s, iy, x + 6.0 * s, iy, 4.0 * s, icon_col);
            draw_line(x + 6.0 * s, iy, x - 6.0 * s, iy + 13.0 * s, 4.0 * s, WHITE);
        }
        PowerUpKind::Slow => {
            draw_line(
                x - 10.0 * s,
                iy - 11.0 * s,
                x + 10.0 * s,
                iy - 11.0 * s,
                3.0 * s,
                WHITE,
            );
            draw_line(
                x - 10.0 * s,
                iy + 11.0 * s,
                x + 10.0 * s,
                iy + 11.0 * s,
                3.0 * s,
                WHITE,
            );
            draw_triangle(
                vec2(x - 9.0 * s, iy - 10.0 * s),
                vec2(x + 9.0 * s, iy - 10.0 * s),
                vec2(x, iy),
                icon_col,
            );
            draw_triangle(
                vec2(x, iy),
                vec2(x - 9.0 * s, iy + 10.0 * s),
                vec2(x + 9.0 * s, iy + 10.0 * s),
                WHITE,
            );
        }
        PowerUpKind::Fast => {
            for i in 0..3 {
                let ox = (i as f32 - 1.0) * 8.0 * s;
                let col = if i == 2 { WHITE } else { icon_col };
                draw_line(
                    x + ox - 4.0 * s,
                    iy - 8.5 * s,
                    x + ox + 4.0 * s,
                    iy,
                    3.6 * s,
                    col,
                );
                draw_line(
                    x + ox + 4.0 * s,
                    iy,
                    x + ox - 4.0 * s,
                    iy + 8.5 * s,
                    3.6 * s,
                    col,
                );
            }
        }
        PowerUpKind::ExtraLife => {
            draw_circle(x - 5.8 * s, iy - 4.2 * s, 6.6 * s, rim_col);
            draw_circle(x + 5.8 * s, iy - 4.2 * s, 6.6 * s, rim_col);
            draw_triangle(
                vec2(x - 12.0 * s, iy - 1.8 * s),
                vec2(x + 12.0 * s, iy - 1.8 * s),
                vec2(x, iy + 11.8 * s),
                rim_col,
            );
            draw_line(
                x - 4.5 * s,
                iy - 1.0 * s,
                x + 4.5 * s,
                iy - 1.0 * s,
                2.8 * s,
                WHITE,
            );
            draw_line(x, iy - 5.5 * s, x, iy + 3.5 * s, 2.8 * s, WHITE);
        }
        PowerUpKind::Shield => {
            draw_line(
                x - 14.0 * s,
                iy + 8.0 * s,
                x + 14.0 * s,
                iy + 8.0 * s,
                4.2 * s,
                icon_col,
            );
            draw_line(x - 11.0 * s, iy + 2.0 * s, x, iy - 10.0 * s, 3.5 * s, WHITE);
            draw_line(x, iy - 10.0 * s, x + 11.0 * s, iy + 2.0 * s, 3.5 * s, WHITE);
            draw_line(
                x - 11.0 * s,
                iy + 2.0 * s,
                x + 11.0 * s,
                iy + 2.0 * s,
                2.5 * s,
                icon_col,
            );
        }
    }

    // Bottom zone: Solid colored pill banner with centered localized label
    let banner_h = 20.0;
    let banner_y = by + h - banner_h - 2.5;
    draw_rounded_rect(bx + 3.0, banner_y, w - 6.0, banner_h, 7.0, rim_col);

    let font_size = 16u16;
    let dims = measure_text(badge_label, font, font_size, 1.0);
    let tx = x - dims.width * 0.5;
    let ty = banner_y + banner_h - 4.5;
    // Subtle dark text shadow for crisp contrast on bright banner
    draw_text_ex(
        badge_label,
        tx + 1.0,
        ty + 1.0,
        TextParams {
            font,
            font_size,
            color: Color::from_rgba(0, 0, 0, 210),
            ..Default::default()
        },
    );
    draw_text_ex(
        badge_label,
        tx,
        ty,
        TextParams {
            font,
            font_size,
            color: WHITE,
            ..Default::default()
        },
    );
}
