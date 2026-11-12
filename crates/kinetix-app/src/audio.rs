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

use kinetix_core::level_code::LevelCodeError;
#[cfg(target_arch = "wasm32")]
use kinetix_core::level_code::LEVEL_CODE_MAX_LEN;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn kinetix_synth_sfx(kind: u32, param: u32);
    fn kinetix_set_music(enabled: u32, intensity: u32);
    fn kinetix_share_level(
        code_ptr: *const u8,
        code_len: u32,
        msg_ptr: *const u8,
        msg_len: u32,
        copy_ptr: *const u8,
        copy_len: u32,
        close_ptr: *const u8,
        close_len: u32,
    );
    fn kinetix_request_import_level(
        msg_ptr: *const u8,
        msg_len: u32,
        ph_ptr: *const u8,
        ph_len: u32,
        import_ptr: *const u8,
        import_len: u32,
        close_ptr: *const u8,
        close_len: u32,
    );
    fn kinetix_poll_imported_level(buf_ptr: *mut u8, max_len: u32) -> i32;
    fn kinetix_set_lang(lang_fr: u32);
    fn kinetix_set_pointer_lock(enabled: u32);
    fn kinetix_set_pointer_bounds(left_css_px: f32, right_css_px: f32);
    fn kinetix_sync_pointer_x(css_x: f32);
    fn kinetix_pointer_lock_active() -> u32;
    fn kinetix_set_resume_mask(mask: u32);
    fn kinetix_is_touch_device() -> u32;
}

pub struct SoundBank {
    pub muted: bool,
    pub music_enabled: bool,
}

impl SoundBank {
    pub async fn init() -> Self {
        Self {
            muted: false,
            music_enabled: true,
        }
    }

    #[allow(unused_variables)]
    fn emit(&self, kind: u32, param: u32) {
        #[cfg(target_arch = "wasm32")]
        if !self.muted {
            unsafe {
                kinetix_synth_sfx(kind, param);
            }
        }
    }

    #[allow(unused_variables)]
    pub fn update_music(&self, boss_active: bool, combo: u32, in_menu: bool) {
        let enabled = u32::from(self.music_enabled && !self.muted);
        let intensity = if in_menu {
            0
        } else if boss_active {
            3
        } else if combo >= 6 {
            2
        } else {
            1
        };
        #[cfg(target_arch = "wasm32")]
        unsafe {
            kinetix_set_music(enabled, intensity);
        }
    }

    pub fn play_paddle(&self) {
        self.emit(1, 0);
    }
    pub fn play_wall(&self) {
        self.emit(2, 0);
    }
    pub fn play_brick(&self, combo: u32) {
        self.emit(3, combo);
    }
    pub fn play_explosion(&self) {
        self.emit(4, 0);
    }
    pub fn play_powerup(&self) {
        self.emit(5, 0);
    }
    pub fn play_laser(&self) {
        self.emit(6, 0);
    }
    pub fn play_lightning(&self) {
        self.emit(7, 0);
    }
    pub fn play_portal(&self) {
        self.emit(8, 0);
    }
}

/// Shows the level code to the player (web: clipboard + non-blocking DOM modal).
#[allow(unused_variables)]
pub fn share_level_code(code: &str, message: &str, copy_label: &str, close_label: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_share_level(
            code.as_ptr(),
            code.len() as u32,
            message.as_ptr(),
            message.len() as u32,
            copy_label.as_ptr(),
            copy_label.len() as u32,
            close_label.as_ptr(),
            close_label.len() as u32,
        );
    }
}

/// Opens the non-blocking DOM level import modal on the web build (no-op natively).
#[allow(unused_variables)]
pub fn request_import_level_code(
    message: &str,
    placeholder: &str,
    import_label: &str,
    close_label: &str,
) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_request_import_level(
            message.as_ptr(),
            message.len() as u32,
            placeholder.as_ptr(),
            placeholder.len() as u32,
            import_label.as_ptr(),
            import_label.len() as u32,
            close_label.as_ptr(),
            close_label.len() as u32,
        );
    }
}

/// Polls whether the DOM level import modal submitted a code.
#[cfg(target_arch = "wasm32")]
pub fn poll_imported_level_code() -> Option<Result<String, LevelCodeError>> {
    let mut buf = vec![0u8; LEVEL_CODE_MAX_LEN];
    let len = unsafe { kinetix_poll_imported_level(buf.as_mut_ptr(), buf.len() as u32) };
    if len <= 0 {
        return None;
    }
    let len = len as usize;
    if len > buf.len() {
        return Some(Err(LevelCodeError::TooLong {
            len: buf.len() + 1,
            max: LEVEL_CODE_MAX_LEN,
        }));
    }
    buf.truncate(len);
    Some(String::from_utf8(buf).map_err(|_| LevelCodeError::Empty))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn poll_imported_level_code() -> Option<Result<String, LevelCodeError>> {
    None
}

/// Keeps the document `lang` attribute in sync with the UI language (web only).
#[allow(unused_variables)]
pub fn set_document_lang(lang_fr: bool) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_set_lang(u32::from(lang_fr));
    }
}

/// Requests or releases the browser pointer lock (web only).
#[allow(unused_variables)]
pub fn set_pointer_lock(enabled: bool) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_set_pointer_lock(u32::from(enabled));
    }
}

/// Sets the horizontal extent of the arena on screen, in CSS pixels relative to the canvas (web only).
#[allow(unused_variables)]
pub fn set_pointer_bounds(left: f32, right: f32) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_set_pointer_bounds(left, right);
    }
}

/// Synchronises the browser-side virtual cursor X after keyboard or touch movement (web only).
#[allow(unused_variables)]
pub fn sync_pointer_x(css_x: f32) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_sync_pointer_x(css_x);
    }
}

/// True while the canvas holds the pointer lock (always true on native).
#[cfg(target_arch = "wasm32")]
pub fn pointer_lock_active() -> bool {
    unsafe { kinetix_pointer_lock_active() != 0 }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn pointer_lock_active() -> bool {
    true
}

/// Tells the page which input gestures will make the pointer lock wanted next, so it can request
/// the lock synchronously inside the gesture (web only; no-op natively).
#[allow(unused_variables)]
pub fn set_resume_mask(mask: u32) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_set_resume_mask(mask);
    }
}

/// True when the browser reports a coarse pointer or touch usage (false natively).
#[cfg(target_arch = "wasm32")]
pub fn is_touch_device() -> bool {
    unsafe { kinetix_is_touch_device() != 0 }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_touch_device() -> bool {
    false
}
