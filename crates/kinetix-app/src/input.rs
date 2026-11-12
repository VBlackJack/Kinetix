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

//! Pointer capture, drift protection, and touch-deck gesture handling.

use crate::tuning::*;
use macroquad::prelude::*;

/// Seeds the virtual cursor X when the grab starts. A session that is already running keeps the
/// paddle where it is (the accumulated raw mouse position has drifted from the window); only a
/// session starting from the menu seeds from the real cursor.
pub fn seed_virtual_cursor_x(
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
pub fn absolute_input_detected(delta: f32, extent: f32) -> bool {
    delta.abs() > extent
}

/// Moves the virtual cursor X by a raw mouse delta, clamped to the on-screen arena extent.
pub fn advance_virtual_cursor_x(current: f32, delta: f32, min_x: f32, max_x: f32) -> f32 {
    (current + delta).clamp(min_x, max_x)
}

/// Confines the OS cursor to the window on native builds. The web build uses its own pointer
/// lock (see `set_pointer_lock`); macroquad's grab would request a second one there.
pub fn set_native_cursor_grab(grab: bool) {
    #[cfg(all(not(target_arch = "wasm32"), not(test)))]
    set_cursor_grab(grab);
    #[cfg(any(target_arch = "wasm32", test))]
    let _ = grab;
}

/// Tracks native OS cursor capture, swallowing post-grab warps and falling back safely on absolute spikes.
#[derive(Debug, Default)]
pub struct NativeGrabState {
    pub last_mouse_x: f32,
    pub virtual_cursor_x: f32,
    pub grab_session_live: bool,
    pub grab_disabled: bool,
    pub grab_just_engaged: bool,
    pub await_clean_pointer: bool,
    pub last_paddle_screen_x: Option<f32>,
}

impl NativeGrabState {
    /// Resolves the logical screen X controlling the paddle for this frame, plus whether the pointer moved.
    pub fn resolve_pointer_x(
        &mut self,
        mx: f32,
        sw: f32,
        cursor_min_x: f32,
        cursor_max_x: f32,
        native_grab_active: bool,
    ) -> (f32, bool) {
        let (screen_x, moved) = if native_grab_active {
            if self.grab_just_engaged {
                self.grab_just_engaged = false;
                self.last_mouse_x = mx;
                (self.virtual_cursor_x, false)
            } else {
                let delta = (mx - self.last_mouse_x) * NATIVE_RAW_MOUSE_SENSITIVITY;
                self.last_mouse_x = mx;
                if absolute_input_detected(delta, cursor_max_x - cursor_min_x) {
                    self.grab_disabled = true;
                    self.await_clean_pointer = true;
                    set_native_cursor_grab(false);
                    (self.virtual_cursor_x, false)
                } else {
                    self.virtual_cursor_x = advance_virtual_cursor_x(
                        self.virtual_cursor_x,
                        delta,
                        cursor_min_x,
                        cursor_max_x,
                    );
                    let moved = match self.last_paddle_screen_x {
                        Some(prev) => (self.virtual_cursor_x - prev).abs() > POINTER_MOVE_EPSILON,
                        None => true,
                    };
                    (self.virtual_cursor_x, moved)
                }
            }
        } else {
            let clean = mx.is_finite() && mx >= 0.0 && mx <= sw;
            let raw_moved = (mx - self.last_mouse_x).abs() > POINTER_MOVE_EPSILON;
            self.last_mouse_x = mx;
            if self.await_clean_pointer {
                if clean && raw_moved {
                    self.await_clean_pointer = false;
                    (mx, true)
                } else {
                    (self.virtual_cursor_x, false)
                }
            } else {
                (mx, self.last_paddle_screen_x.is_none() || raw_moved)
            }
        };

        self.last_paddle_screen_x = Some(screen_x);
        (screen_x, moved)
    }

    /// Synchronises the virtual cursor after keyboard or touch movement so pointer focus does not snap back.
    pub fn sync_from_paddle(
        &mut self,
        paddle_screen_x: f32,
        min_x: f32,
        max_x: f32,
        last_mouse_x: f32,
    ) {
        let clamped = paddle_screen_x.clamp(min_x, max_x);
        self.virtual_cursor_x = clamped;
        self.last_paddle_screen_x = Some(clamped);
        self.last_mouse_x = last_mouse_x;
    }

    pub fn update_lock_state(
        &mut self,
        lock_wanted: bool,
        last_lock_wanted: &mut bool,
        paddle_screen_x: f32,
        mx: f32,
        cursor_min_x: f32,
        cursor_max_x: f32,
    ) {
        if lock_wanted != *last_lock_wanted {
            crate::audio::set_pointer_lock(lock_wanted);
            show_mouse(!lock_wanted);
            let enable_native = lock_wanted && !self.grab_disabled;
            set_native_cursor_grab(enable_native);
            if lock_wanted {
                self.virtual_cursor_x = seed_virtual_cursor_x(
                    self.grab_session_live,
                    paddle_screen_x,
                    mx,
                    cursor_min_x,
                    cursor_max_x,
                );
                self.grab_session_live = true;
                self.grab_just_engaged = cfg!(not(target_arch = "wasm32")) && enable_native;
                self.last_mouse_x = mx;
                self.last_paddle_screen_x = Some(self.virtual_cursor_x);
            } else {
                self.grab_just_engaged = false;
            }
            *last_lock_wanted = lock_wanted;
        }
    }
}

/// Touch point in logical screen coordinates (CSS pixels on web).
#[derive(Debug, Clone, Copy)]
pub struct LogicalTouch {
    pub id: u64,
    pub phase: TouchPhase,
    pub x: f32,
    pub y: f32,
}

/// Collects macroquad `touches()` converted from physical pixels into logical screen pixels.
pub fn logical_touches() -> Vec<LogicalTouch> {
    let dpi = miniquad::window::dpi_scale().max(f32::EPSILON);
    touches()
        .into_iter()
        .map(|t| LogicalTouch {
            id: t.id,
            phase: t.phase,
            x: t.position.x / dpi,
            y: t.position.y / dpi,
        })
        .collect()
}

/// Interactive touch deck geometry in logical screen pixels.
#[derive(Debug, Clone, Copy)]
pub struct TouchDeckLayout {
    pub deck_rect: Rect,
    pub launch_btn: Rect,
    pub vortex_btn: Rect,
    pub pause_btn: Rect,
}

pub fn touch_deck_layout(sw: f32, sh: f32, arena_bottom_y: f32) -> TouchDeckLayout {
    let bottom_space = (sh - arena_bottom_y).max(0.0);
    let deck_h = if bottom_space >= TOUCH_DECK_MIN_H {
        (bottom_space - TOUCH_UI_PAD * 2.0).min(TOUCH_DECK_MAX_H)
    } else {
        TOUCH_BTN_H + TOUCH_UI_PAD * 2.0
    };
    let deck_y = sh - deck_h - TOUCH_UI_PAD;
    let total_w = (sw - TOUCH_UI_PAD * 2.0).max(120.0);
    let btn_w = ((total_w - TOUCH_BTN_GAP * 2.0) * 0.24).clamp(64.0, 148.0);
    let btn_h = (deck_h - TOUCH_UI_PAD * 2.0).clamp(38.0, 64.0);
    let btn_y = deck_y + (deck_h - btn_h) * 0.5;
    let right_edge = sw - TOUCH_UI_PAD;
    let pause_btn = Rect::new(right_edge - btn_w * 0.72, btn_y, btn_w * 0.72, btn_h);
    let vortex_btn = Rect::new(pause_btn.x - TOUCH_BTN_GAP - btn_w, btn_y, btn_w, btn_h);
    let launch_btn = Rect::new(vortex_btn.x - TOUCH_BTN_GAP - btn_w, btn_y, btn_w, btn_h);
    let pad_w = (launch_btn.x - TOUCH_UI_PAD - TOUCH_BTN_GAP).max(80.0);
    let deck_rect = Rect::new(TOUCH_UI_PAD, deck_y, pad_w, deck_h);
    TouchDeckLayout {
        deck_rect,
        launch_btn,
        vortex_btn,
        pause_btn,
    }
}

/// Output produced by processing multi-touch deck gestures in a single frame.
#[derive(Debug, Default, Clone, Copy)]
pub struct TouchFrameResult {
    pub paddle_delta_screen_x: f32,
    pub launch_pressed: bool,
    pub vortex_held: bool,
    pub pause_pressed: bool,
    pub touch_paddle_moved: bool,
}

#[derive(Debug, Default)]
pub struct TouchDeckState {
    pub drag_touch_id: Option<u64>,
    pub last_drag_x: f32,
    pub touch_seen: bool,
}

impl TouchDeckState {
    pub fn update(
        &mut self,
        touches: &[LogicalTouch],
        layout: &TouchDeckLayout,
        arena_rect: Rect,
    ) -> TouchFrameResult {
        let mut res = TouchFrameResult::default();
        if !touches.is_empty() {
            self.touch_seen = true;
        }
        for &t in touches {
            let pt = vec2(t.x, t.y);
            let in_launch = layout.launch_btn.contains(pt);
            let in_vortex = layout.vortex_btn.contains(pt);
            let in_pause = layout.pause_btn.contains(pt);
            let on_button = in_launch || in_vortex || in_pause;
            match t.phase {
                TouchPhase::Started => {
                    if in_pause {
                        res.pause_pressed = true;
                    } else if in_launch {
                        res.launch_pressed = true;
                    } else if in_vortex {
                        res.vortex_held = true;
                    } else if self.drag_touch_id.is_none()
                        && (layout.deck_rect.contains(pt) || arena_rect.contains(pt))
                    {
                        self.drag_touch_id = Some(t.id);
                        self.last_drag_x = t.x;
                    }
                }
                TouchPhase::Stationary => {
                    if in_vortex {
                        res.vortex_held = true;
                    }
                }
                TouchPhase::Moved => {
                    if in_vortex {
                        res.vortex_held = true;
                    }
                    if self.drag_touch_id == Some(t.id) {
                        let dx = (t.x - self.last_drag_x) * TOUCH_DRAG_GAIN;
                        self.last_drag_x = t.x;
                        if dx.abs() > POINTER_MOVE_EPSILON {
                            res.paddle_delta_screen_x += dx;
                            res.touch_paddle_moved = true;
                        }
                    } else if self.drag_touch_id.is_none() && !on_button {
                        self.drag_touch_id = Some(t.id);
                        self.last_drag_x = t.x;
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    if self.drag_touch_id == Some(t.id) {
                        self.drag_touch_id = None;
                    }
                }
            }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_grab_swallows_initial_engage_delta_and_holds_virtual_x_on_absolute_spike() {
        let mut grab = NativeGrabState {
            virtual_cursor_x: 500.0,
            grab_just_engaged: true,
            last_mouse_x: 200.0,
            ..Default::default()
        };

        let (x0, _) = grab.resolve_pointer_x(1920.0, 1280.0, 0.0, 1280.0, true);
        assert_eq!(x0, 500.0);
        assert!(!grab.grab_just_engaged);

        let (x1, moved1) = grab.resolve_pointer_x(1930.0, 1280.0, 0.0, 1280.0, true);
        assert_eq!(x1, 510.0);
        assert!(moved1);

        let (x2, _) = grab.resolve_pointer_x(10000.0, 1280.0, 0.0, 1280.0, true);
        assert_eq!(x2, 510.0);
        assert!(grab.grab_disabled);
        assert!(grab.await_clean_pointer);

        let (x3, _) = grab.resolve_pointer_x(10000.0, 1280.0, 0.0, 1280.0, false);
        assert_eq!(x3, 510.0);

        let (x4, moved4) = grab.resolve_pointer_x(640.0, 1280.0, 0.0, 1280.0, false);
        assert_eq!(x4, 640.0);
        assert!(moved4);
        assert!(!grab.await_clean_pointer);
    }

    #[test]
    fn pointer_moved_flag_is_false_when_cursor_is_stationary() {
        let mut grab = NativeGrabState::default();
        let (x1, moved1) = grab.resolve_pointer_x(600.0, 1920.0, 0.0, 1920.0, false);
        assert_eq!(x1, 600.0);
        assert!(moved1);
        let (x2, moved2) = grab.resolve_pointer_x(600.0, 1920.0, 0.0, 1920.0, false);
        assert_eq!(x2, 600.0);
        assert!(!moved2);
    }

    #[test]
    fn touch_deck_supports_relative_drag_and_multitouch_buttons() {
        let layout = touch_deck_layout(390.0, 844.0, 520.0);
        let arena = Rect::new(0.0, 300.0, 390.0, 220.0);
        let mut state = TouchDeckState::default();

        let start = [LogicalTouch {
            id: 1,
            phase: TouchPhase::Started,
            x: layout.deck_rect.x + 20.0,
            y: layout.deck_rect.y + 20.0,
        }];
        let r1 = state.update(&start, &layout, arena);
        assert!(!r1.touch_paddle_moved);
        assert!(state.touch_seen);

        let swipe_and_tap = [
            LogicalTouch {
                id: 1,
                phase: TouchPhase::Moved,
                x: layout.deck_rect.x + 60.0,
                y: layout.deck_rect.y + 20.0,
            },
            LogicalTouch {
                id: 2,
                phase: TouchPhase::Started,
                x: layout.launch_btn.x + 10.0,
                y: layout.launch_btn.y + 10.0,
            },
            LogicalTouch {
                id: 3,
                phase: TouchPhase::Stationary,
                x: layout.vortex_btn.x + 10.0,
                y: layout.vortex_btn.y + 10.0,
            },
        ];
        let r2 = state.update(&swipe_and_tap, &layout, arena);
        assert!(r2.touch_paddle_moved);
        assert!((r2.paddle_delta_screen_x - 40.0 * TOUCH_DRAG_GAIN).abs() < 0.01);
        assert!(r2.launch_pressed);
        assert!(r2.vortex_held);
    }
}
