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

//! Hidden developer mode state: sector number entry, power-up selection and the guard that
//! keeps tested levels from ever touching the saved progress.

use crate::tuning::DEV_SECTOR_DIGITS_MAX;
use kinetix_core::physics::PowerUpKind;

/// Session-only developer mode state. Never persisted.
#[derive(Default)]
pub struct DevState {
    /// True while the developer mode is switched on.
    pub active: bool,
    /// True once the mode was used on the current level; sticky until a level is loaded normally.
    pub tainted: bool,
    /// Digits typed so far for a sector jump.
    pub buffer: String,
    /// Index into `PowerUpKind::all()` of the power-up selected for granting.
    pub selected_powerup: usize,
    /// When true, losing the last ball costs no life.
    pub invulnerable: bool,
    /// Whether the key help panel is shown.
    pub help_visible: bool,
}

impl DevState {
    /// Flips the mode. Turning it on taints the current level; turning it off clears the
    /// transient entry state but leaves the taint in place.
    pub fn toggle(&mut self) {
        self.active = !self.active;
        self.buffer.clear();
        if self.active {
            self.tainted = true;
        } else {
            self.invulnerable = false;
        }
    }

    /// Called when a level is loaded through a normal (non-dev) route.
    pub fn level_loaded_normally(&mut self) {
        if !self.active {
            self.tainted = false;
        }
    }

    /// The currently selected power-up.
    pub fn selected_kind(&self) -> PowerUpKind {
        let all = PowerUpKind::all();
        all[self.selected_powerup % all.len()]
    }

    /// Moves the power-up selection, wrapping around.
    pub fn cycle_powerup(&mut self, forward: bool) {
        self.selected_powerup =
            cycle_index(self.selected_powerup, PowerUpKind::all().len(), forward);
    }
}

/// Steps `current` by one inside `0..len`, wrapping at both ends.
pub fn cycle_index(current: usize, len: usize, forward: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if forward {
        (current + 1) % len
    } else {
        (current + len - 1) % len
    }
}

/// Appends a digit to the sector buffer; ignored once the buffer is full.
pub fn push_sector_digit(buffer: &mut String, digit: char) {
    if digit.is_ascii_digit() && buffer.len() < DEV_SECTOR_DIGITS_MAX {
        buffer.push(digit);
    }
}

/// Resolves a typed 1-based sector number into a level index, clamped to `0..level_count`.
/// Returns `None` for an empty buffer or an empty level list.
pub fn resolve_sector(buffer: &str, level_count: usize) -> Option<usize> {
    if buffer.is_empty() || level_count == 0 {
        return None;
    }
    let number: usize = buffer.parse().ok()?;
    Some(number.clamp(1, level_count) - 1)
}

/// Previous or next level index around `current`, clamped to the level list.
pub fn step_level(current: usize, level_count: usize, forward: bool) -> usize {
    let last = level_count.saturating_sub(1);
    if forward {
        (current + 1).min(last)
    } else {
        current.saturating_sub(1)
    }
}

/// Remembers the genuine saved progress so a tested level can never change it.
pub struct ProgressGuard {
    high_score: u64,
    unlocked: usize,
}

impl ProgressGuard {
    pub fn new(high_score: u64, unlocked: usize) -> Self {
        Self {
            high_score,
            unlocked,
        }
    }

    /// While `tainted`, overwrites the live values with the genuine ones; otherwise adopts the
    /// live values as the new genuine ones.
    pub fn sync(&mut self, tainted: bool, high_score: &mut u64, unlocked: &mut usize) {
        if tainted {
            *high_score = self.high_score;
            *unlocked = self.unlocked;
        } else {
            self.high_score = *high_score;
            self.unlocked = *unlocked;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_buffer_accepts_only_digits_up_to_the_limit() {
        let mut buffer = String::new();
        push_sector_digit(&mut buffer, 'x');
        assert!(buffer.is_empty());
        for _ in 0..DEV_SECTOR_DIGITS_MAX + 3 {
            push_sector_digit(&mut buffer, '7');
        }
        assert_eq!(buffer.len(), DEV_SECTOR_DIGITS_MAX);
    }

    #[test]
    fn sector_number_is_clamped_to_the_level_list() {
        assert_eq!(resolve_sector("", 800), None);
        assert_eq!(resolve_sector("12", 0), None);
        assert_eq!(resolve_sector("1", 800), Some(0));
        assert_eq!(resolve_sector("0", 800), Some(0));
        assert_eq!(resolve_sector("000", 800), Some(0));
        assert_eq!(resolve_sector("800", 800), Some(799));
        assert_eq!(resolve_sector("999", 800), Some(799));
        assert_eq!(resolve_sector("42", 800), Some(41));
    }

    #[test]
    fn level_stepping_stops_at_both_ends() {
        assert_eq!(step_level(0, 5, false), 0);
        assert_eq!(step_level(4, 5, true), 4);
        assert_eq!(step_level(2, 5, true), 3);
        assert_eq!(step_level(2, 5, false), 1);
        assert_eq!(step_level(0, 0, true), 0);
    }

    #[test]
    fn powerup_selection_wraps_both_ways() {
        let len = PowerUpKind::all().len();
        assert_eq!(cycle_index(len - 1, len, true), 0);
        assert_eq!(cycle_index(0, len, false), len - 1);
        assert_eq!(cycle_index(3, len, true), 4);
        assert_eq!(cycle_index(0, 0, true), 0);
        let mut dev = DevState::default();
        dev.cycle_powerup(false);
        assert_eq!(dev.selected_kind(), PowerUpKind::all()[len - 1]);
        dev.cycle_powerup(true);
        assert_eq!(dev.selected_kind(), PowerUpKind::all()[0]);
    }

    #[test]
    fn toggle_taints_and_only_a_normal_load_clears_it() {
        let mut dev = DevState::default();
        dev.toggle();
        assert!(dev.active && dev.tainted);
        dev.level_loaded_normally();
        assert!(dev.tainted, "a load while active stays tainted");
        dev.toggle();
        assert!(!dev.active && dev.tainted, "taint survives switching off");
        dev.level_loaded_normally();
        assert!(!dev.tainted);
    }

    #[test]
    fn tainted_progress_is_restored_and_clean_progress_is_adopted() {
        let mut guard = ProgressGuard::new(100, 3);
        // A genuine win moves the baseline.
        let (mut hs, mut unlocked) = (150, 4);
        guard.sync(false, &mut hs, &mut unlocked);
        assert_eq!((hs, unlocked), (150, 4));
        // A dev win is rolled back.
        (hs, unlocked) = (900, 9);
        guard.sync(true, &mut hs, &mut unlocked);
        assert_eq!((hs, unlocked), (150, 4));
    }
}
