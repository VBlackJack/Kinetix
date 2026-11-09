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

//! Data the application supplies to the menu each frame, and the input/output
//! types of the menu state machine.

use kinetix_core::i18n::Language;

/// Identifier of a saved library level (matches the storage layer's record id).
pub type LibraryId = u64;

/// One sector line of a world.
#[derive(Debug, Clone, PartialEq)]
pub struct SectorRow {
    /// Absolute zero-based sector index, passed back in `MenuAction::StartSector`.
    pub index: usize,
    /// Already localised text shown on the row.
    pub label: String,
    pub unlocked: bool,
}

/// One chapter (world) line.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldRow {
    /// Already localised world name.
    pub name: String,
    pub unlocked: bool,
    pub cleared: usize,
    pub total: usize,
    pub sectors: Vec<SectorRow>,
    /// Offset inside `sectors` that is focused when the world is opened.
    pub focus_offset: usize,
}

/// One saved level of the player's library.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    pub id: LibraryId,
    pub name: String,
}

/// Everything the menu needs to know about the application.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuModel {
    pub worlds: Vec<WorldRow>,
    /// Names of the satisfaction pack levels.
    pub satisfaction_levels: Vec<String>,
    pub library: Vec<LibraryEntry>,
    pub library_max: usize,
    /// True when the application can quit (native builds).
    pub quit_supported: bool,
    pub language: Language,
    /// Already localised name of the active renderer.
    pub renderer_name: String,
    pub sfx_on: bool,
    pub music_on: bool,
    /// Best score of the saved campaign progress, shown on the Home page.
    pub high_score: u64,
}

impl MenuModel {
    /// An empty model: no worlds, no levels, defaults for every setting.
    pub fn empty() -> Self {
        Self {
            worlds: Vec::new(),
            satisfaction_levels: Vec::new(),
            library: Vec::new(),
            library_max: 0,
            quit_supported: false,
            language: Language::En,
            renderer_name: String::new(),
            sfx_on: true,
            music_on: true,
            high_score: 0,
        }
    }
}

/// Per-frame input. Mouse and touch both arrive as the pointer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MenuInput {
    /// Pointer position in arena coordinates; `None` when there is no pointer.
    /// On the release frame it must be the position at release.
    pub pointer: Option<(f32, f32)>,
    pub pointer_pressed: bool,
    pub pointer_released: bool,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub page_up: bool,
    pub page_down: bool,
    pub home: bool,
    pub end: bool,
    pub enter: bool,
    pub escape: bool,
}

/// What the menu asks the application to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    None,
    StartSector(usize),
    StartSatisfaction(usize),
    OpenEditor,
    PlayLibrary(LibraryId),
    DeleteLibrary(LibraryId),
    SetLanguage(Language),
    /// Preview the next renderer immediately; not persisted yet.
    CycleRenderer,
    /// The renderer choice must now be saved (page left).
    PersistRenderer,
    ToggleSfx,
    ToggleMusic,
    Resume,
    Restart,
    QuitToTitle,
    QuitApp,
}
