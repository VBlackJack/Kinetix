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

//! Play session helpers: starting, restarting and advancing a run, and keeping saved progress
//! out of reach of developer mode and non-campaign sources.

use crate::dev::{DevState, ProgressGuard};
use crate::tuning::{HIGH_SCORE_SAVE_INTERVAL_SEC, LIBRARY_NAME_PLACEHOLDER};
use kinetix_app::library::{sanitize_name, Library, LibraryEntry, LibraryError, SaveError};
use kinetix_app::menu::{MenuAction, PageId};
use kinetix_app::persistence::{high_score_save_due, save_progress};
use kinetix_app::play_source::{progress_frozen, PlaySource};
use kinetix_core::config::LevelDef;
use kinetix_core::level_code::{parse_level_code, LevelCodeError};
use kinetix_core::physics::{GameWorld, SessionStatus};

/// State of the current play session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Session {
    /// Where the board being played comes from.
    pub(crate) source: PlaySource,
    /// Share code of the board a test play started from the editor with: restarting an editor
    /// test imports it again instead of loading a campaign sector.
    pub(crate) editor_snapshot: Option<String>,
}

impl Session {
    /// A session playing the main campaign.
    pub(crate) fn campaign() -> Self {
        Self {
            source: PlaySource::Campaign,
            editor_snapshot: None,
        }
    }
}

/// Enters the level editor: the board edited from here on belongs to an editor test, whatever
/// was being played before, so it can never reach the campaign progress. Fresh paddle and ball.
pub(crate) fn enter_editor(session: &mut Session, world: &mut GameWorld) {
    session.source = PlaySource::EditorTest;
    world.reset_paddle_and_ball();
    world.status = SessionStatus::Playing;
}

/// Starts a test play of the editor board: the board is remembered so a restart replays it.
pub(crate) fn start_editor_test(session: &mut Session, world: &mut GameWorld) {
    session.source = PlaySource::EditorTest;
    session.editor_snapshot = Some(world.export_level_code());
    world.reset_paddle_and_ball();
    world.status = SessionStatus::Playing;
}

/// Starts a fresh run from the title menu: the only transition that ends a developer taint,
/// because it is the only one that checks the sector against the saved unlock.
pub(crate) fn start_run_from_title(
    world: &mut GameWorld,
    dev: &mut DevState,
    levels: &[LevelDef],
    sector_idx: usize,
    source: PlaySource,
) {
    world.lives = world.config.gameplay.initial_lives;
    world.score = 0;
    world.load_pack_level(sector_idx, levels, source.pack_rules());
    dev.level_loaded_normally();
}

/// Restarts the current sector (R key or game-over restart). It never clears a taint.
pub(crate) fn restart_level(
    world: &mut GameWorld,
    dev: &mut DevState,
    levels: &[LevelDef],
    source: PlaySource,
) {
    debug_assert!(!dev.active || dev.tainted);
    world.lives = world.config.gameplay.initial_lives;
    world.score = 0;
    world.load_pack_level(world.current_level_idx, levels, source.pack_rules());
}

/// Campaign sector list for a source that plays it; satisfaction levels for the satisfaction
/// source. Library and editor boards have no level list.
pub(crate) fn levels_for<'a>(
    source: PlaySource,
    campaign: &'a [LevelDef],
    satisfaction: &'a [LevelDef],
) -> &'a [LevelDef] {
    match source {
        PlaySource::Satisfaction => satisfaction,
        PlaySource::Campaign | PlaySource::Library(_) | PlaySource::EditorTest => campaign,
    }
}

/// Starts satisfaction level `idx` as a fresh run. It does not touch the developer taint (only a
/// campaign start from the title does) and returns false, leaving everything as it was, when
/// the index is out of range.
pub(crate) fn start_satisfaction(
    session: &mut Session,
    world: &mut GameWorld,
    satisfaction: &[LevelDef],
    idx: usize,
) -> bool {
    if idx >= satisfaction.len() {
        return false;
    }
    session.source = PlaySource::Satisfaction;
    world.lives = world.config.gameplay.initial_lives;
    world.score = 0;
    world.load_pack_level(idx, satisfaction, PlaySource::Satisfaction.pack_rules());
    true
}

/// Starts a saved library level as a fresh run on a custom board. On a code that no longer
/// imports, the world and the session are left untouched.
pub(crate) fn start_library_level(
    session: &mut Session,
    world: &mut GameWorld,
    entry: &LibraryEntry,
) -> Result<(), LevelCodeError> {
    world.import_level_code(&entry.code)?;
    session.source = PlaySource::Library(entry.id);
    world.lives = world.config.gameplay.initial_lives;
    world.score = 0;
    Ok(())
}

/// What restarting the current board did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestartOutcome {
    /// The board was reloaded and play goes on.
    Restarted,
    /// There is nothing to restart: back to the title screen, opened on this page.
    ToTitle(PageId),
}

/// Restarts the current board of the session: the same satisfaction level, the same library
/// code, the editor test snapshot, or the current sector. Score and lives reset; a taint is never
/// cleared. A library entry deleted in the meantime sends the player back to the library page.
pub(crate) fn restart_current(
    session: &Session,
    world: &mut GameWorld,
    dev: &mut DevState,
    campaign: &[LevelDef],
    satisfaction: &[LevelDef],
    library: &Library,
) -> RestartOutcome {
    match session.source {
        PlaySource::Library(id) => {
            let Some(entry) = library.get(id) else {
                return RestartOutcome::ToTitle(PageId::Library);
            };
            reload_custom_board(world, &entry.code);
        }
        PlaySource::EditorTest => {
            if let Some(code) = &session.editor_snapshot {
                reload_custom_board(world, code);
            }
        }
        PlaySource::Campaign | PlaySource::Satisfaction => restart_level(
            world,
            dev,
            levels_for(session.source, campaign, satisfaction),
            session.source,
        ),
    }
    RestartOutcome::Restarted
}

/// Imports `code` again with fresh lives and score; a code that no longer imports changes nothing.
fn reload_custom_board(world: &mut GameWorld, code: &str) {
    if world.import_level_code(code).is_ok() {
        world.lives = world.config.gameplay.initial_lives;
        world.score = 0;
    }
}

/// Where the game goes after a victory is acknowledged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VictoryOutcome {
    /// The next board is loaded and play goes on.
    NextLevel,
    /// Back to the title screen, opened on this page.
    ToTitle(PageId),
    /// Back to the level editor.
    ToEditor,
}

/// Applies a victory according to the source: the campaign moves to the next sector, the
/// satisfaction pack to its next level (title when it was the last), a library level returns to
/// the library page and an editor test to the editor. A campaign sector is never loaded while
/// the source is satisfaction or library.
pub(crate) fn advance_after_victory(
    session: &Session,
    world: &mut GameWorld,
    dev: &mut DevState,
    campaign: &[LevelDef],
    satisfaction: &[LevelDef],
) -> VictoryOutcome {
    match session.source {
        PlaySource::Campaign if world.is_campaign_board() => {
            next_sector(world, dev, campaign, PlaySource::Campaign);
            VictoryOutcome::NextLevel
        }
        PlaySource::Campaign | PlaySource::EditorTest => VictoryOutcome::ToEditor,
        PlaySource::Satisfaction => {
            let next = world.current_level_idx + 1;
            if next < satisfaction.len() {
                world.load_pack_level(next, satisfaction, PlaySource::Satisfaction.pack_rules());
                VictoryOutcome::NextLevel
            } else {
                VictoryOutcome::ToTitle(PageId::Satisfaction)
            }
        }
        PlaySource::Library(_) => VictoryOutcome::ToTitle(PageId::Library),
    }
}

/// True when a victory on the current board goes on to another board (no title or editor
/// screen in between): the campaign sector list, or the satisfaction pack before its last level.
pub(crate) fn victory_continues(
    source: PlaySource,
    world: &GameWorld,
    satisfaction_len: usize,
) -> bool {
    match source {
        PlaySource::Campaign => world.is_campaign_board(),
        PlaySource::Satisfaction => world.current_level_idx + 1 < satisfaction_len,
        PlaySource::Library(_) | PlaySource::EditorTest => false,
    }
}

/// Number and name shown by the HUD for the board being played. A test play from the editor shows
/// `editor_label`, never the name of a campaign sector.
pub(crate) fn level_label<'a>(
    source: PlaySource,
    current_idx: usize,
    campaign: &'a [LevelDef],
    satisfaction: &'a [LevelDef],
    library: &'a Library,
    editor_label: &'a str,
) -> (usize, &'a str) {
    match source {
        PlaySource::EditorTest => (1, editor_label),
        PlaySource::Library(id) => library
            .entries
            .iter()
            .position(|entry| entry.id == id)
            .map_or((1, ""), |pos| (pos + 1, library.entries[pos].name.as_str())),
        PlaySource::Satisfaction | PlaySource::Campaign => {
            let levels = levels_for(source, campaign, satisfaction);
            (
                current_idx + 1,
                levels.get(current_idx).map_or("", |l| l.name.as_str()),
            )
        }
    }
}

/// What a library change did, for the toast that follows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LibraryOutcome {
    Saved,
    Full,
    InvalidBoard(LevelCodeError),
    Deleted,
    Missing,
    /// The change was made in memory but the storage refused it.
    SaveFailed,
}

impl LibraryOutcome {
    /// Locale key of the toast shown for this outcome, if any.
    pub(crate) fn toast_key(&self) -> Option<&'static str> {
        match self {
            Self::Saved => Some("menu.library.toast.saved"),
            Self::Full => Some("menu.library.toast.full"),
            Self::InvalidBoard(err) => Some(err.i18n_key()),
            Self::Deleted => Some("menu.library.toast.deleted"),
            Self::Missing => None,
            Self::SaveFailed => Some("storage.toast.save_failed"),
        }
    }

    /// True for outcomes shown as a warning rather than a confirmation.
    pub(crate) fn is_failure(&self) -> bool {
        matches!(self, Self::Full | Self::InvalidBoard(_) | Self::SaveFailed)
    }
}

/// Name given to a level saved from the editor: the locale template with its number filled in,
/// reduced to printable ASCII plus the letters of the template itself (all of which are
/// preloaded in the glyph atlas).
pub(crate) fn default_library_name(template: &str, id: u32) -> String {
    let name = template.replace(LIBRARY_NAME_PLACEHOLDER, &id.to_string());
    sanitize_name(&name, |c| {
        c == ' ' || c.is_ascii_graphic() || (c.is_alphabetic() && template.contains(c))
    })
}

/// Saves the board `code` as a new library entry named from `name_template`, then writes the
/// library through `persist`. A code that does not import is rejected before the library is
/// touched.
pub(crate) fn save_board_to_library(
    library: &mut Library,
    code: String,
    name_template: &str,
    persist: impl FnOnce(&Library) -> Result<(), SaveError>,
) -> LibraryOutcome {
    if let Err(err) = parse_level_code(&code) {
        return LibraryOutcome::InvalidBoard(err);
    }
    let name = default_library_name(name_template, library.next_id);
    match library.add(name, code) {
        Err(LibraryError::Full) => LibraryOutcome::Full,
        Ok(_) => match persist(library) {
            Ok(()) => LibraryOutcome::Saved,
            Err(_) => LibraryOutcome::SaveFailed,
        },
    }
}

/// Deletes library entry `id`, then writes the library through `persist`.
pub(crate) fn delete_library_level(
    library: &mut Library,
    id: u32,
    persist: impl FnOnce(&Library) -> Result<(), SaveError>,
) -> LibraryOutcome {
    if !library.delete(id) {
        return LibraryOutcome::Missing;
    }
    match persist(library) {
        Ok(()) => LibraryOutcome::Deleted,
        Err(_) => LibraryOutcome::SaveFailed,
    }
}

/// What a pause menu action changed outside the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PauseOutcome {
    /// Nothing for the pause flow to do (settings actions are handled by the caller).
    None,
    /// The run goes on (resumed or restarted).
    Resumed,
    /// The player left for the title screen; the world is left exactly as it is.
    ToTitle,
    /// The board cannot be restarted any more: the title screen opens on this page.
    ToTitlePage(PageId),
}

/// Applies the pause menu actions that do not reload a board. Resume puts the world back to
/// playing; Quit to menu leaves the world, the progress and the developer taint untouched, like
/// the Tab key does. Restart belongs to `apply_session_pause_action`.
fn apply_pause_action(action: MenuAction, world: &mut GameWorld) -> PauseOutcome {
    match action {
        MenuAction::Resume => {
            world.status = SessionStatus::Playing;
            PauseOutcome::Resumed
        }
        MenuAction::QuitToTitle => PauseOutcome::ToTitle,
        _ => PauseOutcome::None,
    }
}

/// Applies a pause menu action to the session of any source: Restart reloads the board of the
/// source (never a campaign sector for satisfaction, library or editor), the rest is
/// `apply_pause_action`.
pub(crate) fn apply_session_pause_action(
    action: MenuAction,
    session: &Session,
    world: &mut GameWorld,
    dev: &mut DevState,
    campaign: &[LevelDef],
    satisfaction: &[LevelDef],
    library: &Library,
) -> PauseOutcome {
    if action == MenuAction::Restart {
        return match restart_current(session, world, dev, campaign, satisfaction, library) {
            RestartOutcome::Restarted => {
                world.status = SessionStatus::Playing;
                PauseOutcome::Resumed
            }
            RestartOutcome::ToTitle(page) => PauseOutcome::ToTitlePage(page),
        };
    }
    apply_pause_action(action, world)
}

/// Moves on to the sector after a win. The score carries over and a taint is never cleared.
pub(crate) fn next_sector(
    world: &mut GameWorld,
    dev: &mut DevState,
    levels: &[LevelDef],
    source: PlaySource,
) {
    debug_assert!(!dev.active || dev.tainted);
    let next_lvl = (world.current_level_idx + 1) % levels.len();
    world.load_pack_level(next_lvl, levels, source.pack_rules());
}

/// Keeps the saved progress out of reach of developer mode and of boards that are not the
/// campaign: while the level is tainted, the source is not the campaign, or the board in play is
/// not a campaign sector (a custom board under a campaign source), the live high score and
/// unlock marker are rolled back to the genuine ones.
pub(crate) fn sync_progress(
    guard: &mut ProgressGuard,
    dev: &DevState,
    world: &mut GameWorld,
    source: PlaySource,
) {
    guard.sync(
        progress_frozen(source, dev.tainted) || !world.is_campaign_board(),
        &mut world.high_score,
        &mut world.max_unlocked_level,
    );
}

/// Tracks the last persisted high score and save time to throttle automatic saves.
pub(crate) struct SaveTracker {
    last_saved_high_score: u64,
    last_save_sec: f64,
}

impl SaveTracker {
    pub(crate) fn new(saved_high_score: u64) -> Self {
        Self {
            last_saved_high_score: saved_high_score,
            last_save_sec: f64::NEG_INFINITY,
        }
    }

    /// Saves unconditionally and records the saved high score and time.
    pub(crate) fn persist(
        &mut self,
        world: &GameWorld,
        theme_idx: usize,
        lang_fr: bool,
        now_sec: f64,
    ) {
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
    pub(crate) fn persist_if_due(
        &mut self,
        world: &GameWorld,
        theme_idx: usize,
        lang_fr: bool,
        now_sec: f64,
    ) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RAW_GAME_CONFIG, RAW_LEVELS};
    use kinetix_core::config::{GameConfig, LevelPack};
    use kinetix_core::physics::SessionStatus;

    #[test]
    fn a_developer_win_never_unlocks_or_scores_into_the_saved_progress() {
        let config: GameConfig = serde_json::from_str(RAW_GAME_CONFIG).expect("config");
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("levels");
        assert!(pack.levels.len() > 2);
        let mut world = GameWorld::new(config, &pack.levels);
        world.high_score = 1;
        let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
        let mut dev = DevState::default();
        dev.toggle();

        world.load_level(0, &pack.levels);
        world.apply_powerup(kinetix_core::physics::PowerUpKind::ExtraLife);
        world.bricks.clear();
        world.boss = None;
        let mut events = Vec::new();
        world.update(0.016, &mut events);
        assert_eq!(world.status, SessionStatus::Victory);
        assert!(world.max_unlocked_level > 0, "the core unlocks on a win");
        assert!(world.high_score > 1);

        sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
        assert_eq!(world.max_unlocked_level, 0);
        assert_eq!(world.high_score, 1);

        // Switching the mode off keeps the level tainted until a normal load.
        dev.toggle();
        sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
        assert_eq!(world.max_unlocked_level, 0);
        dev.level_loaded_normally();
        world.load_level(0, &pack.levels);
        world.bricks.clear();
        world.boss = None;
        world.update(0.016, &mut events);
        sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
        assert_eq!(world.max_unlocked_level, 1, "a genuine win still unlocks");
    }

    fn dev_test_world() -> (GameWorld, LevelPack) {
        let config: GameConfig = serde_json::from_str(RAW_GAME_CONFIG).expect("config");
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("levels");
        let mut world = GameWorld::new(config, &pack.levels);
        world.high_score = 1;
        (world, pack)
    }

    fn win_current_level(world: &mut GameWorld) {
        world.bricks.clear();
        world.boss = None;
        world.update(0.016, &mut Vec::new());
    }

    #[test]
    fn dev_score_then_next_sector_leaves_the_high_score_unchanged() {
        let (mut world, pack) = dev_test_world();
        let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
        let mut dev = DevState::default();
        start_run_from_title(&mut world, &mut dev, &pack.levels, 0, PlaySource::Campaign);
        dev.toggle();
        world.apply_powerup(kinetix_core::physics::PowerUpKind::ExtraLife);
        dev.toggle();
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
        next_sector(&mut world, &mut dev, &pack.levels, PlaySource::Campaign);
        assert!(world.score > 1, "the dev score carries over");
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
        assert!(dev.tainted);
        assert_eq!(world.high_score, 1);
        assert_eq!(world.max_unlocked_level, 0);
    }

    #[test]
    fn dev_jump_to_a_locked_sector_stays_locked_after_restart_or_game_over() {
        for game_over_restart in [false, true] {
            let (mut world, pack) = dev_test_world();
            let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
            let mut dev = DevState::default();
            start_run_from_title(&mut world, &mut dev, &pack.levels, 0, PlaySource::Campaign);
            dev.toggle();
            world.load_level(5, &pack.levels);
            dev.toggle();
            if game_over_restart {
                world.status = SessionStatus::GameOver;
            }
            restart_level(&mut world, &mut dev, &pack.levels, PlaySource::Campaign);
            win_current_level(&mut world);
            assert_eq!(world.max_unlocked_level, 6, "the core alone would unlock");
            sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
            assert_eq!(world.max_unlocked_level, 0);
        }
    }

    #[test]
    fn only_a_title_start_clears_the_taint_and_then_wins_count() {
        let (mut world, pack) = dev_test_world();
        let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
        let mut dev = DevState::default();
        dev.toggle();
        dev.toggle();
        restart_level(&mut world, &mut dev, &pack.levels, PlaySource::Campaign);
        next_sector(&mut world, &mut dev, &pack.levels, PlaySource::Campaign);
        assert!(dev.tainted);
        start_run_from_title(&mut world, &mut dev, &pack.levels, 0, PlaySource::Campaign);
        assert!(!dev.tainted);
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
        assert_eq!(world.max_unlocked_level, 1);
    }

    #[test]
    fn a_non_campaign_source_never_writes_progress_even_without_a_taint() {
        let (mut world, _) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
        let dev = DevState::default();
        let mut session = Session::campaign();
        assert!(start_satisfaction(
            &mut session,
            &mut world,
            &satisfaction,
            0
        ));
        assert!(!world.is_campaign_board());
        world.high_score = 9_999;
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, session.source);
        assert_eq!(world.max_unlocked_level, 0);
        assert_eq!(world.high_score, 1);
    }

    fn paused_world(dev: &mut DevState) -> (GameWorld, LevelPack) {
        let (mut world, pack) = dev_test_world();
        start_run_from_title(&mut world, dev, &pack.levels, 2, PlaySource::Campaign);
        world.status = SessionStatus::Paused;
        (world, pack)
    }

    #[test]
    fn resume_puts_a_paused_world_back_to_playing() {
        let mut dev = DevState::default();
        let (mut world, _) = paused_world(&mut dev);
        world.score = 700;
        let outcome = apply_pause_action(MenuAction::Resume, &mut world);
        assert_eq!(outcome, PauseOutcome::Resumed);
        assert_eq!(world.status, SessionStatus::Playing);
        assert_eq!(world.score, 700, "resume keeps the score");
    }

    #[test]
    fn restart_reloads_the_current_sector_with_a_fresh_score() {
        let mut dev = DevState::default();
        let (mut world, pack) = paused_world(&mut dev);
        world.score = 700;
        let outcome = apply_session_pause_action(
            MenuAction::Restart,
            &Session::campaign(),
            &mut world,
            &mut dev,
            &pack.levels,
            &satisfaction_levels(),
            &Library::default(),
        );
        assert_eq!(outcome, PauseOutcome::Resumed);
        assert_eq!(world.status, SessionStatus::Playing);
        assert_eq!(world.score, 0);
        assert_eq!(world.current_level_idx, 2);
    }

    #[test]
    fn restart_from_the_pause_menu_keeps_the_developer_taint() {
        let mut dev = DevState::default();
        let (mut world, pack) = paused_world(&mut dev);
        dev.toggle();
        dev.toggle();
        assert!(dev.tainted);
        apply_session_pause_action(
            MenuAction::Restart,
            &Session::campaign(),
            &mut world,
            &mut dev,
            &pack.levels,
            &satisfaction_levels(),
            &Library::default(),
        );
        assert!(dev.tainted);
    }

    #[test]
    fn quit_to_menu_leaves_the_world_and_the_taint_alone() {
        let mut dev = DevState::default();
        let (mut world, _) = paused_world(&mut dev);
        dev.toggle();
        dev.toggle();
        world.score = 700;
        let outcome = apply_pause_action(MenuAction::QuitToTitle, &mut world);
        assert_eq!(outcome, PauseOutcome::ToTitle);
        assert_eq!(world.status, SessionStatus::Paused);
        assert_eq!(world.score, 700);
        assert_eq!(world.current_level_idx, 2);
        assert!(dev.tainted);
    }

    #[test]
    fn settings_and_unrelated_actions_are_left_to_the_caller() {
        let mut dev = DevState::default();
        let (mut world, _) = paused_world(&mut dev);
        for action in [
            MenuAction::None,
            MenuAction::ToggleSfx,
            MenuAction::CycleRenderer,
            MenuAction::PersistRenderer,
            MenuAction::StartSector(0),
            MenuAction::QuitApp,
        ] {
            let outcome = apply_pause_action(action, &mut world);
            assert_eq!(outcome, PauseOutcome::None);
            assert_eq!(world.status, SessionStatus::Paused);
        }
    }

    fn satisfaction_levels() -> Vec<LevelDef> {
        kinetix_app::satisfaction::satisfaction_pack().levels
    }

    fn library_with_current_board(world: &GameWorld) -> (Library, u32) {
        let mut library = Library::default();
        let id = library
            .add("LEVEL 1".to_owned(), world.export_level_code())
            .expect("room in the library");
        (library, id)
    }

    #[test]
    fn a_satisfaction_start_resets_the_run_and_keeps_the_taint_alone() {
        let (mut world, _) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut session = Session::campaign();
        world.lives = 0;
        world.score = 900;
        let mut dev = DevState::default();
        dev.toggle();
        dev.toggle();
        assert!(dev.tainted);

        assert!(start_satisfaction(
            &mut session,
            &mut world,
            &satisfaction,
            2
        ));
        assert_eq!(session.source, PlaySource::Satisfaction);
        assert_eq!(world.current_level_idx, 2);
        assert_eq!(world.lives, world.config.gameplay.initial_lives);
        assert_eq!(world.score, 0);
        assert!(!world.is_campaign_board());
        assert!(dev.tainted, "only a campaign title start clears the taint");
    }

    #[test]
    fn a_satisfaction_start_out_of_range_changes_nothing() {
        let (mut world, _) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut session = Session::campaign();
        world.score = 900;
        assert!(!start_satisfaction(
            &mut session,
            &mut world,
            &satisfaction,
            satisfaction.len()
        ));
        assert_eq!(session.source, PlaySource::Campaign);
        assert_eq!(world.score, 900);
    }

    #[test]
    fn a_satisfaction_win_never_changes_the_saved_high_score_or_unlock() {
        let (mut world, _) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut session = Session::campaign();
        let saved_high_score = world.high_score;
        let saved_unlocked = world.max_unlocked_level;
        let mut guard = ProgressGuard::new(saved_high_score, saved_unlocked);
        let dev = DevState::default();
        assert!(start_satisfaction(
            &mut session,
            &mut world,
            &satisfaction,
            0
        ));
        world.score = 50_000;
        world.high_score = 50_000;
        win_current_level(&mut world);
        assert_eq!(world.status, SessionStatus::Victory);

        sync_progress(&mut guard, &dev, &mut world, session.source);
        assert_eq!(world.high_score, saved_high_score);
        assert_eq!(world.max_unlocked_level, saved_unlocked);
        assert!(
            !high_score_save_due(
                world.high_score,
                saved_high_score,
                f64::MAX,
                f64::NEG_INFINITY,
                HIGH_SCORE_SAVE_INTERVAL_SEC
            ),
            "nothing new to save after a satisfaction win"
        );
    }

    #[test]
    fn a_satisfaction_victory_moves_through_the_pack_and_ends_on_its_page() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut session = Session::campaign();
        let mut dev = DevState::default();
        assert!(start_satisfaction(
            &mut session,
            &mut world,
            &satisfaction,
            0
        ));
        for expected in 1..satisfaction.len() {
            win_current_level(&mut world);
            let outcome = advance_after_victory(
                &session,
                &mut world,
                &mut dev,
                &campaign.levels,
                &satisfaction,
            );
            assert_eq!(outcome, VictoryOutcome::NextLevel);
            assert_eq!(world.current_level_idx, expected);
            assert!(!world.is_campaign_board(), "never a campaign sector");
            assert_eq!(world.status, SessionStatus::Playing);
        }
        win_current_level(&mut world);
        let outcome = advance_after_victory(
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
        );
        assert_eq!(outcome, VictoryOutcome::ToTitle(PageId::Satisfaction));
    }

    #[test]
    fn restart_and_game_over_replay_the_same_satisfaction_level() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let library = Library::default();
        let mut session = Session::campaign();
        let mut dev = DevState::default();
        assert!(start_satisfaction(
            &mut session,
            &mut world,
            &satisfaction,
            3
        ));
        let fresh_bricks = world.bricks.len();
        world.bricks.truncate(1);
        world.score = 400;
        world.status = SessionStatus::GameOver;

        restart_current(
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
            &library,
        );
        assert_eq!(world.current_level_idx, 3);
        assert_eq!(world.bricks.len(), fresh_bricks);
        assert_eq!(world.score, 0);
        assert!(!world.is_campaign_board());
        assert_eq!(world.status, SessionStatus::Playing);
    }

    #[test]
    fn the_pause_restart_follows_the_source() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let library = Library::default();
        let mut session = Session::campaign();
        let mut dev = DevState::default();
        assert!(start_satisfaction(
            &mut session,
            &mut world,
            &satisfaction,
            1
        ));
        world.status = SessionStatus::Paused;
        world.score = 300;
        let outcome = apply_session_pause_action(
            MenuAction::Restart,
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
            &library,
        );
        assert_eq!(outcome, PauseOutcome::Resumed);
        assert_eq!(world.current_level_idx, 1);
        assert!(!world.is_campaign_board());
        assert_eq!(world.score, 0);
        assert_eq!(world.status, SessionStatus::Playing);

        // The other pause actions keep their behavior.
        world.status = SessionStatus::Paused;
        let outcome = apply_session_pause_action(
            MenuAction::QuitToTitle,
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
            &library,
        );
        assert_eq!(outcome, PauseOutcome::ToTitle);
    }

    #[test]
    fn a_library_level_plays_on_a_custom_board_and_a_win_returns_to_the_library() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut session = Session::campaign();
        let mut dev = DevState::default();
        let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
        world.load_pack_level(0, &satisfaction, PlaySource::Satisfaction.pack_rules());
        let (library, id) = library_with_current_board(&world);
        let entry = library.get(id).expect("entry").clone();
        world.score = 700;

        start_library_level(&mut session, &mut world, &entry).expect("valid code");
        assert_eq!(session.source, PlaySource::Library(id));
        assert!(!world.is_campaign_board());
        assert_eq!(world.score, 0);
        assert_eq!(world.lives, world.config.gameplay.initial_lives);

        world.score = 50_000;
        world.high_score = 50_000;
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, session.source);
        assert_eq!(world.high_score, 1, "a library win saves no high score");
        assert_eq!(world.max_unlocked_level, 0, "a library win unlocks nothing");
        let outcome = advance_after_victory(
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
        );
        assert_eq!(outcome, VictoryOutcome::ToTitle(PageId::Library));
    }

    #[test]
    fn restarting_a_library_level_imports_the_same_code_again() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut session = Session::campaign();
        let mut dev = DevState::default();
        world.load_pack_level(1, &satisfaction, PlaySource::Satisfaction.pack_rules());
        let (library, id) = library_with_current_board(&world);
        let entry = library.get(id).expect("entry").clone();
        start_library_level(&mut session, &mut world, &entry).expect("valid code");
        let fresh_bricks = world.bricks.len();
        world.bricks.truncate(1);
        world.score = 400;

        restart_current(
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
            &library,
        );
        assert_eq!(world.bricks.len(), fresh_bricks);
        assert_eq!(world.score, 0);
        assert!(!world.is_campaign_board());
    }

    #[test]
    fn a_library_entry_that_no_longer_imports_leaves_the_session_alone() {
        let (mut world, _) = dev_test_world();
        let mut session = Session::campaign();
        let entry = LibraryEntry {
            id: 9,
            name: "BAD".to_owned(),
            code: "KTX1:nonsense".to_owned(),
        };
        world.score = 123;
        assert!(start_library_level(&mut session, &mut world, &entry).is_err());
        assert_eq!(session.source, PlaySource::Campaign);
        assert_eq!(world.score, 123);
    }

    #[test]
    fn an_editor_test_and_a_campaign_with_a_custom_board_go_back_to_the_editor() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut dev = DevState::default();
        let editor = Session {
            source: PlaySource::EditorTest,
            editor_snapshot: None,
        };
        let outcome = advance_after_victory(
            &editor,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
        );
        assert_eq!(outcome, VictoryOutcome::ToEditor);

        let session = Session::campaign();
        world.clear_bricks();
        let outcome = advance_after_victory(
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
        );
        assert_eq!(outcome, VictoryOutcome::ToEditor);

        start_run_from_title(&mut world, &mut dev, &campaign.levels, 0, session.source);
        let outcome = advance_after_victory(
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
        );
        assert_eq!(outcome, VictoryOutcome::NextLevel);
        assert_eq!(world.current_level_idx, 1);
        assert!(world.is_campaign_board());
    }

    #[test]
    fn the_hud_label_names_the_board_of_the_source() {
        let (_, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut library = Library::default();
        library
            .add("FIRST".to_owned(), "KTX1:".to_owned())
            .expect("room");
        let second = library
            .add("SECOND".to_owned(), "KTX1:".to_owned())
            .expect("room");

        let (number, name) = level_label(
            PlaySource::Satisfaction,
            2,
            &campaign.levels,
            &satisfaction,
            &library,
            EDITOR_LABEL,
        );
        assert_eq!((number, name), (3, satisfaction[2].name.as_str()));
        let (number, name) = level_label(
            PlaySource::Campaign,
            0,
            &campaign.levels,
            &satisfaction,
            &library,
            EDITOR_LABEL,
        );
        assert_eq!((number, name), (1, campaign.levels[0].name.as_str()));
        let (number, name) = level_label(
            PlaySource::Library(second),
            0,
            &campaign.levels,
            &satisfaction,
            &library,
            EDITOR_LABEL,
        );
        assert_eq!((number, name), (2, "SECOND"));
        let (number, name) = level_label(
            PlaySource::Library(999),
            0,
            &campaign.levels,
            &satisfaction,
            &library,
            EDITOR_LABEL,
        );
        assert_eq!((number, name), (1, ""));
        let (number, name) = level_label(
            PlaySource::EditorTest,
            7,
            &campaign.levels,
            &satisfaction,
            &library,
            EDITOR_LABEL,
        );
        assert_eq!(
            (number, name),
            (1, EDITOR_LABEL),
            "an editor test never shows a campaign sector name"
        );
    }

    const EDITOR_LABEL: &str = "EDITOR";
    const NAME_TEMPLATE: &str = "LEVEL {n}";

    #[test]
    fn a_saved_board_is_named_from_the_template_and_written_once() {
        let (world, _) = dev_test_world();
        let mut library = Library::default();
        let mut writes = 0;
        let outcome = save_board_to_library(
            &mut library,
            world.export_level_code(),
            NAME_TEMPLATE,
            |_| {
                writes += 1;
                Ok(())
            },
        );
        assert_eq!(outcome, LibraryOutcome::Saved);
        assert_eq!(writes, 1);
        assert_eq!(library.entries.len(), 1);
        assert_eq!(library.entries[0].name, "LEVEL 1");
        assert_eq!(outcome.toast_key(), Some("menu.library.toast.saved"));
        assert!(!outcome.is_failure());

        let outcome = save_board_to_library(
            &mut library,
            world.export_level_code(),
            NAME_TEMPLATE,
            |_| Ok(()),
        );
        assert_eq!(outcome, LibraryOutcome::Saved);
        assert_eq!(library.entries[1].name, "LEVEL 2");
    }

    #[test]
    fn an_empty_board_is_rejected_before_the_library_is_touched() {
        let (mut world, _) = dev_test_world();
        world.clear_bricks();
        let mut library = Library::default();
        let outcome = save_board_to_library(
            &mut library,
            world.export_level_code(),
            NAME_TEMPLATE,
            |_| panic!("nothing to write"),
        );
        assert!(matches!(outcome, LibraryOutcome::InvalidBoard(_)));
        assert!(outcome.is_failure());
        assert!(library.entries.is_empty());
        assert_eq!(library.next_id, Library::default().next_id);
    }

    #[test]
    fn a_full_library_refuses_a_new_level_without_writing() {
        let (world, _) = dev_test_world();
        let mut library = Library::default();
        for _ in 0..kinetix_app::library::MAX_LIBRARY_LEVELS {
            library
                .add("X".to_owned(), world.export_level_code())
                .expect("room");
        }
        let outcome = save_board_to_library(
            &mut library,
            world.export_level_code(),
            NAME_TEMPLATE,
            |_| panic!("nothing to write"),
        );
        assert_eq!(outcome, LibraryOutcome::Full);
        assert_eq!(outcome.toast_key(), Some("menu.library.toast.full"));
        assert_eq!(
            library.entries.len(),
            kinetix_app::library::MAX_LIBRARY_LEVELS
        );
    }

    #[test]
    fn a_refused_write_is_reported_as_a_failed_save() {
        let (world, _) = dev_test_world();
        let mut library = Library::default();
        let outcome = save_board_to_library(
            &mut library,
            world.export_level_code(),
            NAME_TEMPLATE,
            |_| Err(SaveError::Failed),
        );
        assert_eq!(outcome, LibraryOutcome::SaveFailed);
        assert_eq!(outcome.toast_key(), Some("storage.toast.save_failed"));
        assert!(outcome.is_failure());

        let id = library.entries[0].id;
        let outcome = delete_library_level(&mut library, id, |_| Err(SaveError::Unavailable));
        assert_eq!(outcome, LibraryOutcome::SaveFailed);
    }

    #[test]
    fn deleting_a_level_saves_the_library_and_a_missing_id_does_nothing() {
        let (world, _) = dev_test_world();
        let mut library = Library::default();
        let id = library
            .add("X".to_owned(), world.export_level_code())
            .expect("room");
        let outcome = delete_library_level(&mut library, id + 1, |_| panic!("nothing to write"));
        assert_eq!(outcome, LibraryOutcome::Missing);
        assert_eq!(outcome.toast_key(), None);
        assert_eq!(library.entries.len(), 1);

        let mut writes = 0;
        let outcome = delete_library_level(&mut library, id, |saved| {
            writes += 1;
            assert!(saved.entries.is_empty());
            Ok(())
        });
        assert_eq!(outcome, LibraryOutcome::Deleted);
        assert_eq!(writes, 1);
        assert_eq!(outcome.toast_key(), Some("menu.library.toast.deleted"));
    }

    #[test]
    fn default_names_keep_only_printable_characters() {
        assert_eq!(default_library_name("LEVEL {n}", 12), "LEVEL 12");
        assert_eq!(default_library_name("NIVEAU {n}", 3), "NIVEAU 3");
        assert_eq!(
            default_library_name("LEVEL\u{7} {n}\u{202e}", 4),
            "LEVEL 4",
            "control and direction characters are dropped"
        );
        assert_eq!(default_library_name("N\u{e9}o {n}", 5), "N\u{e9}o 5");
    }

    #[test]
    fn a_custom_board_edited_from_a_campaign_run_never_saves_progress() {
        let (mut world, pack) = dev_test_world();
        let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
        let mut dev = DevState::default();
        let mut session = Session::campaign();
        start_run_from_title(&mut world, &mut dev, &pack.levels, 0, session.source);
        let code = world.export_level_code();

        // F4 into the editor, import a board, F4 back into test play.
        enter_editor(&mut session, &mut world);
        assert_eq!(session.source, PlaySource::EditorTest);
        world.import_level_code(&code).expect("valid code");
        start_editor_test(&mut session, &mut world);
        world.high_score = 5_000;
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, session.source);
        assert_eq!(world.high_score, 1, "the editor test saved no high score");
        assert_eq!(world.max_unlocked_level, 0, "and unlocked nothing");
    }

    #[test]
    fn a_custom_board_under_a_campaign_source_is_frozen_on_its_own() {
        let (mut world, pack) = dev_test_world();
        let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
        let dev = DevState::default();
        start_run_from_title(
            &mut world,
            &mut DevState::default(),
            &pack.levels,
            0,
            PlaySource::Campaign,
        );
        let code = world.export_level_code();
        world.import_level_code(&code).expect("valid code");
        assert!(!world.is_campaign_board());
        world.high_score = 5_000;
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
        assert_eq!(world.high_score, 1);
        assert_eq!(world.max_unlocked_level, 0);

        // A campaign sector started afterwards counts again.
        start_run_from_title(
            &mut world,
            &mut DevState::default(),
            &pack.levels,
            0,
            PlaySource::Campaign,
        );
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, PlaySource::Campaign);
        assert_eq!(world.max_unlocked_level, 1);
    }

    #[test]
    fn restarting_an_editor_test_replays_its_snapshot_not_a_campaign_sector() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let library = Library::default();
        let mut dev = DevState::default();
        let mut session = Session::campaign();
        world.load_pack_level(2, &satisfaction, PlaySource::Satisfaction.pack_rules());
        let snapshot_bricks = world.bricks.len();
        start_editor_test(&mut session, &mut world);
        assert!(session.editor_snapshot.is_some());
        world.bricks.truncate(1);
        world.score = 400;
        world.status = SessionStatus::GameOver;

        let outcome = restart_current(
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
            &library,
        );
        assert_eq!(outcome, RestartOutcome::Restarted);
        assert_eq!(world.bricks.len(), snapshot_bricks);
        assert_eq!(world.score, 0);
        assert!(!world.is_campaign_board());
        assert_eq!(world.status, SessionStatus::Playing);
    }

    #[test]
    fn the_pause_restart_of_an_editor_test_replays_the_snapshot() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut dev = DevState::default();
        let mut session = Session::campaign();
        world.load_pack_level(1, &satisfaction, PlaySource::Satisfaction.pack_rules());
        let snapshot_bricks = world.bricks.len();
        start_editor_test(&mut session, &mut world);
        world.bricks.truncate(1);
        world.status = SessionStatus::Paused;
        let outcome = apply_session_pause_action(
            MenuAction::Restart,
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
            &Library::default(),
        );
        assert_eq!(outcome, PauseOutcome::Resumed);
        assert_eq!(world.bricks.len(), snapshot_bricks);
        assert!(!world.is_campaign_board());
    }

    #[test]
    fn restarting_a_deleted_library_level_returns_to_the_library_page() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut dev = DevState::default();
        world.load_pack_level(0, &satisfaction, PlaySource::Satisfaction.pack_rules());
        let (mut library, id) = library_with_current_board(&world);
        let entry = library.get(id).expect("entry").clone();
        let mut session = Session::campaign();
        start_library_level(&mut session, &mut world, &entry).expect("valid code");
        assert!(library.delete(id));
        world.score = 300;

        let outcome = restart_current(
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
            &library,
        );
        assert_eq!(outcome, RestartOutcome::ToTitle(PageId::Library));
        assert_eq!(world.score, 300, "nothing was reloaded");

        world.status = SessionStatus::Paused;
        let outcome = apply_session_pause_action(
            MenuAction::Restart,
            &session,
            &mut world,
            &mut dev,
            &campaign.levels,
            &satisfaction,
            &library,
        );
        assert_eq!(outcome, PauseOutcome::ToTitlePage(PageId::Library));
        assert_eq!(world.status, SessionStatus::Paused);
    }

    #[test]
    fn victory_continues_matches_what_advancing_does() {
        let (mut world, campaign) = dev_test_world();
        let satisfaction = satisfaction_levels();
        let mut dev = DevState::default();
        let sources = [
            PlaySource::Campaign,
            PlaySource::Satisfaction,
            PlaySource::Library(1),
            PlaySource::EditorTest,
        ];
        for source in sources {
            for idx in [0, satisfaction.len() - 1] {
                match source {
                    PlaySource::Campaign => {
                        world.load_pack_level(idx, &campaign.levels, source.pack_rules())
                    }
                    PlaySource::Satisfaction => {
                        world.load_pack_level(idx, &satisfaction, source.pack_rules())
                    }
                    _ => world
                        .import_level_code(&world.export_level_code())
                        .expect("valid code"),
                }
                let expected = victory_continues(source, &world, satisfaction.len());
                let session = Session {
                    source,
                    editor_snapshot: None,
                };
                let outcome = advance_after_victory(
                    &session,
                    &mut world,
                    &mut dev,
                    &campaign.levels,
                    &satisfaction,
                );
                assert_eq!(
                    expected,
                    outcome == VictoryOutcome::NextLevel,
                    "{source:?} at {idx}"
                );
            }
        }
    }
}
