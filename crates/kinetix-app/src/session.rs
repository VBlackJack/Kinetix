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
use crate::tuning::HIGH_SCORE_SAVE_INTERVAL_SEC;
use kinetix_app::menu::MenuAction;
use kinetix_app::persistence::{high_score_save_due, save_progress};
use kinetix_app::play_source::{progress_frozen, PlaySource};
use kinetix_core::config::LevelDef;
use kinetix_core::physics::{GameWorld, SessionStatus};

/// State of the current play session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Session {
    /// Where the board being played comes from.
    pub(crate) source: PlaySource,
}

impl Session {
    /// A session playing the main campaign.
    pub(crate) fn campaign() -> Self {
        Self {
            source: PlaySource::Campaign,
        }
    }
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

/// What a pause menu action changed outside the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PauseOutcome {
    /// Nothing for the pause flow to do (settings actions are handled by the caller).
    None,
    /// The run goes on (resumed or restarted).
    Resumed,
    /// The player left for the title screen; the world is left exactly as it is.
    ToTitle,
}

/// Applies a pause menu action to the session. Resume and Restart put the world back to
/// playing; Quit to menu leaves the world, the progress and the developer taint untouched, like
/// the Tab key does.
pub(crate) fn apply_pause_action(
    action: MenuAction,
    world: &mut GameWorld,
    dev: &mut DevState,
    levels: &[LevelDef],
    source: PlaySource,
) -> PauseOutcome {
    match action {
        MenuAction::Resume => {
            world.status = SessionStatus::Playing;
            PauseOutcome::Resumed
        }
        MenuAction::Restart => {
            restart_level(world, dev, levels, source);
            PauseOutcome::Resumed
        }
        MenuAction::QuitToTitle => PauseOutcome::ToTitle,
        _ => PauseOutcome::None,
    }
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

/// Keeps the saved progress out of reach of developer mode: while the current level is tainted,
/// the live high score and unlock marker are rolled back to the genuine ones.
pub(crate) fn sync_progress(
    guard: &mut ProgressGuard,
    dev: &DevState,
    world: &mut GameWorld,
    source: PlaySource,
) {
    guard.sync(
        progress_frozen(source, dev.tainted),
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
        let (mut world, pack) = dev_test_world();
        let mut guard = ProgressGuard::new(world.high_score, world.max_unlocked_level);
        let dev = DevState::default();
        start_run_from_title(
            &mut world,
            &mut DevState::default(),
            &pack.levels,
            0,
            PlaySource::Satisfaction,
        );
        assert!(!world.is_campaign_board());
        win_current_level(&mut world);
        sync_progress(&mut guard, &dev, &mut world, PlaySource::Satisfaction);
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
        let (mut world, pack) = paused_world(&mut dev);
        world.score = 700;
        let outcome = apply_pause_action(
            MenuAction::Resume,
            &mut world,
            &mut dev,
            &pack.levels,
            PlaySource::Campaign,
        );
        assert_eq!(outcome, PauseOutcome::Resumed);
        assert_eq!(world.status, SessionStatus::Playing);
        assert_eq!(world.score, 700, "resume keeps the score");
    }

    #[test]
    fn restart_reloads_the_current_sector_with_a_fresh_score() {
        let mut dev = DevState::default();
        let (mut world, pack) = paused_world(&mut dev);
        world.score = 700;
        let outcome = apply_pause_action(
            MenuAction::Restart,
            &mut world,
            &mut dev,
            &pack.levels,
            PlaySource::Campaign,
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
        apply_pause_action(
            MenuAction::Restart,
            &mut world,
            &mut dev,
            &pack.levels,
            PlaySource::Campaign,
        );
        assert!(dev.tainted);
    }

    #[test]
    fn quit_to_menu_leaves_the_world_and_the_taint_alone() {
        let mut dev = DevState::default();
        let (mut world, pack) = paused_world(&mut dev);
        dev.toggle();
        dev.toggle();
        world.score = 700;
        let outcome = apply_pause_action(
            MenuAction::QuitToTitle,
            &mut world,
            &mut dev,
            &pack.levels,
            PlaySource::Campaign,
        );
        assert_eq!(outcome, PauseOutcome::ToTitle);
        assert_eq!(world.status, SessionStatus::Paused);
        assert_eq!(world.score, 700);
        assert_eq!(world.current_level_idx, 2);
        assert!(dev.tainted);
    }

    #[test]
    fn settings_and_unrelated_actions_are_left_to_the_caller() {
        let mut dev = DevState::default();
        let (mut world, pack) = paused_world(&mut dev);
        for action in [
            MenuAction::None,
            MenuAction::ToggleSfx,
            MenuAction::CycleRenderer,
            MenuAction::PersistRenderer,
            MenuAction::StartSector(0),
            MenuAction::QuitApp,
        ] {
            let outcome = apply_pause_action(
                action,
                &mut world,
                &mut dev,
                &pack.levels,
                PlaySource::Campaign,
            );
            assert_eq!(outcome, PauseOutcome::None);
            assert_eq!(world.status, SessionStatus::Paused);
        }
    }
}
