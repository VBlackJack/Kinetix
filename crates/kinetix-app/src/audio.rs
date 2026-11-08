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
    fn kinetix_save_state(high_score: u32, unlocked: u32, theme: u32, lang: u32);
    fn kinetix_load_packed() -> u32;
    fn kinetix_load_highscore() -> u32;
    fn kinetix_share_level(code_ptr: *const u8, code_len: u32, msg_ptr: *const u8, msg_len: u32);
    fn kinetix_prompt_level(
        msg_ptr: *const u8,
        msg_len: u32,
        buf_ptr: *mut u8,
        max_len: u32,
    ) -> u32;
    fn kinetix_set_lang(lang_fr: u32);
    fn kinetix_set_pointer_lock(enabled: u32);
    fn kinetix_set_pointer_bounds(left_css_px: f32, right_css_px: f32);
    fn kinetix_pointer_lock_active() -> u32;
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

#[allow(unused_variables)]
pub fn save_progress(high_score: u64, unlocked: usize, theme: usize, lang_fr: bool) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_save_state(
            high_score.min(u32::MAX as u64) as u32,
            unlocked as u32,
            theme as u32,
            u32::from(lang_fr),
        );
    }
}

#[cfg(target_arch = "wasm32")]
pub fn load_progress() -> (u64, usize, usize, bool) {
    unsafe {
        let hs = kinetix_load_highscore() as u64;
        let packed = kinetix_load_packed();
        let unlocked = (packed & 0xFF) as usize;
        let theme = ((packed >> 8) & 0xFF) as usize;
        let lang_fr = ((packed >> 16) & 0xFF) != 0;
        (hs, unlocked, theme, lang_fr)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_progress() -> (u64, usize, usize, bool) {
    (0, 0, 0, true)
}

/// Shows the level code to the player (web: clipboard + prompt dialog titled with `message`).
#[allow(unused_variables)]
pub fn share_level_code(code: &str, message: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        kinetix_share_level(
            code.as_ptr(),
            code.len() as u32,
            message.as_ptr(),
            message.len() as u32,
        );
    }
}

/// Asks the player for a level code. `Ok(None)` means cancelled or empty (or not supported).
#[cfg(target_arch = "wasm32")]
pub fn prompt_level_code(message: &str) -> Result<Option<String>, LevelCodeError> {
    let mut buf = vec![0u8; LEVEL_CODE_MAX_LEN];
    let len = unsafe {
        kinetix_prompt_level(
            message.as_ptr(),
            message.len() as u32,
            buf.as_mut_ptr(),
            buf.len() as u32,
        )
    } as usize;
    if len == 0 {
        return Ok(None);
    }
    if len > buf.len() {
        return Err(LevelCodeError::TooLong {
            len: buf.len() + 1,
            max: LEVEL_CODE_MAX_LEN,
        });
    }
    buf.truncate(len);
    String::from_utf8(buf)
        .map(Some)
        .map_err(|_| LevelCodeError::Empty)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn prompt_level_code(_message: &str) -> Result<Option<String>, LevelCodeError> {
    Ok(None)
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

/// True while the canvas holds the pointer lock (always true on native).
#[cfg(target_arch = "wasm32")]
pub fn pointer_lock_active() -> bool {
    unsafe { kinetix_pointer_lock_active() != 0 }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn pointer_lock_active() -> bool {
    true
}
