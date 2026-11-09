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

//! Player progress persistence: browser storage on the web, a JSON file on native builds.

/// Decides whether a new high score must be saved now: it has to exceed the last saved value
/// and at least `interval_sec` must have elapsed since the previous save (pure, testable).
pub fn high_score_save_due(
    high_score: u64,
    last_saved: u64,
    now_sec: f64,
    last_save_sec: f64,
    interval_sec: f64,
) -> bool {
    high_score > last_saved && now_sec - last_save_sec >= interval_sec
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn kinetix_save_state(high_score: u32, unlocked: u32, theme: u32, lang: u32);
    fn kinetix_load_packed() -> u32;
    fn kinetix_load_highscore() -> u32;
    fn kinetix_save_settings(packed: u32);
    fn kinetix_load_settings() -> u32;
}

/// Player audio preferences, persisted on both platforms. Both default to on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub sfx_on: bool,
    pub music_on: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sfx_on: true,
            music_on: true,
        }
    }
}

/// Bit flags of the settings word exchanged with the web storage bindings.
#[cfg(any(target_arch = "wasm32", test))]
const WEB_SETTINGS_SFX_BIT: u32 = 0b01;
#[cfg(any(target_arch = "wasm32", test))]
const WEB_SETTINGS_MUSIC_BIT: u32 = 0b10;

/// Packs the settings into the web settings word (pure, testable).
#[cfg(any(target_arch = "wasm32", test))]
pub fn pack_web_settings(settings: Settings) -> u32 {
    (if settings.sfx_on {
        WEB_SETTINGS_SFX_BIT
    } else {
        0
    }) | (if settings.music_on {
        WEB_SETTINGS_MUSIC_BIT
    } else {
        0
    })
}

/// Unpacks the web settings word (pure, testable).
#[cfg(any(target_arch = "wasm32", test))]
pub fn unpack_web_settings(packed: u32) -> Settings {
    Settings {
        sfx_on: packed & WEB_SETTINGS_SFX_BIT != 0,
        music_on: packed & WEB_SETTINGS_MUSIC_BIT != 0,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn load_settings() -> Settings {
    unpack_web_settings(unsafe { kinetix_load_settings() })
}

#[cfg(target_arch = "wasm32")]
pub fn save_settings(settings: Settings) {
    unsafe {
        kinetix_save_settings(pack_web_settings(settings));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_settings() -> Settings {
    native::progress_path()
        .map(|path| native::current_settings(&path))
        .unwrap_or_default()
}

/// Persists the audio settings, keeping the rest of the progress file as it is.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_settings(settings: Settings) {
    let Some(path) = native::progress_path() else {
        return;
    };
    if let Err(_err) = native::save_settings_session(&path, settings) {
        #[cfg(debug_assertions)]
        eprintln!("settings save failed: {_err}");
    }
}

#[cfg(target_arch = "wasm32")]
pub fn save_progress(high_score: u64, unlocked: usize, theme: usize, lang_fr: bool) {
    unsafe {
        kinetix_save_state(
            high_score.min(u32::MAX as u64) as u32,
            unlocked as u32,
            theme as u32,
            u32::from(lang_fr),
        );
    }
}

/// Bit masks and shifts for the packed web state word returned by `kinetix_load_packed`.
#[cfg(any(target_arch = "wasm32", test))]
const WEB_UNLOCKED_MASK: u32 = 0xFFFF;
#[cfg(any(target_arch = "wasm32", test))]
const WEB_THEME_SHIFT: u32 = 16;
#[cfg(any(target_arch = "wasm32", test))]
const WEB_THEME_MASK: u32 = 0xFF;
#[cfg(any(target_arch = "wasm32", test))]
const WEB_LANG_SHIFT: u32 = 24;
#[cfg(any(target_arch = "wasm32", test))]
const WEB_LANG_MASK: u32 = 0x1;

/// Unpacks the 32-bit state word returned by `kinetix_load_packed` (pure, testable).
/// Bits 0..16: unlocked sector index; bits 16..24: theme index; bit 24: French locale flag.
#[cfg(any(target_arch = "wasm32", test))]
pub fn unpack_web_state(packed: u32) -> (usize, usize, bool) {
    let unlocked = (packed & WEB_UNLOCKED_MASK) as usize;
    let theme = ((packed >> WEB_THEME_SHIFT) & WEB_THEME_MASK) as usize;
    let lang_fr = ((packed >> WEB_LANG_SHIFT) & WEB_LANG_MASK) != 0;
    (unlocked, theme, lang_fr)
}

#[cfg(target_arch = "wasm32")]
pub fn load_progress() -> (u64, usize, usize, bool) {
    unsafe {
        let hs = kinetix_load_highscore() as u64;
        let (unlocked, theme, lang_fr) = unpack_web_state(kinetix_load_packed());
        (hs, unlocked, theme, lang_fr)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_progress(high_score: u64, unlocked: usize, theme: usize, lang_fr: bool) {
    let Some(path) = native::progress_path() else {
        return;
    };
    let state = native::ProgressState {
        high_score,
        unlocked,
        theme,
        lang_fr,
    };
    // Persistence must never crash the game; failures are reported in debug builds only.
    if let Err(_err) = native::save_progress_session(&path, state) {
        #[cfg(debug_assertions)]
        eprintln!("progress save failed: {_err}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_progress() -> (u64, usize, usize, bool) {
    let progress = native::progress_path()
        .and_then(|path| match native::read_progress(&path) {
            native::ReadOutcome::Valid(progress) => Some(progress),
            _ => None,
        })
        .unwrap_or_default();
    (
        progress.high_score,
        progress.unlocked,
        progress.theme,
        progress.lang_fr,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use serde::{Deserialize, Serialize};
    use std::ffi::OsStr;
    use std::io::{ErrorKind, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::Duration;

    pub const PROGRESS_VERSION: u32 = 1;
    const PROGRESS_FILE_NAME: &str = "progress.json";
    const PROGRESS_BACKUP_FILE_NAME: &str = "progress.json.bak";
    const PROGRESS_TMP_SUFFIX: &str = ".tmp";
    const APP_DIR_NAME_CAPITALIZED: &str = "Kinetix";
    const APP_DIR_NAME_UNIX: &str = "kinetix";
    const OS_WINDOWS: &str = "windows";
    const OS_MACOS: &str = "macos";
    const MACOS_SUPPORT_DIRS: [&str; 2] = ["Library", "Application Support"];
    const UNIX_DATA_DIRS: [&str; 2] = [".local", "share"];
    const ENV_APPDATA: &str = "APPDATA";
    const ENV_HOME: &str = "HOME";
    const ENV_XDG_DATA_HOME: &str = "XDG_DATA_HOME";
    const READ_ATTEMPTS: usize = 2;
    const RENAME_ATTEMPTS: usize = 3;
    const RENAME_BACKOFF: Duration = Duration::from_millis(10);

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(default)]
    pub struct Progress {
        pub version: u32,
        pub high_score: u64,
        pub unlocked: usize,
        pub theme: usize,
        pub lang_fr: bool,
        /// Sound effects enabled (additive field: absent in older files, defaults to on).
        pub sfx_on: bool,
        /// Music enabled (additive field: absent in older files, defaults to on).
        pub music_on: bool,
    }

    impl Default for Progress {
        fn default() -> Self {
            Self {
                version: PROGRESS_VERSION,
                high_score: 0,
                unlocked: 0,
                theme: 0,
                lang_fr: false,
                sfx_on: true,
                music_on: true,
            }
        }
    }

    /// Audio settings stored in the progress file at `path` (defaults when unreadable).
    pub fn current_settings(path: &Path) -> super::Settings {
        match read_progress(path) {
            ReadOutcome::Valid(progress) => super::Settings {
                sfx_on: progress.sfx_on,
                music_on: progress.music_on,
            },
            _ => super::Settings::default(),
        }
    }

    /// Result of reading the progress file.
    #[derive(Debug, PartialEq, Eq)]
    pub enum ReadOutcome {
        Missing,
        Valid(Progress),
        /// Corrupt JSON or incompatible version: the file must not be overwritten before it is
        /// moved aside to `.bak`.
        Unreadable,
        /// The file exists but could not be read (I/O failure): its content is unknown, so
        /// nothing may be written over it and no field may be filled from defaults.
        IoFailed,
    }

    /// The progress fields a game save supplies; the audio settings come from the file.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProgressState {
        pub high_score: u64,
        pub unlocked: usize,
        pub theme: usize,
        pub lang_fr: bool,
    }

    fn read_failed() -> std::io::Error {
        std::io::Error::other("progress file could not be read")
    }

    /// Per-session write state: the original file is preserved at most once.
    #[derive(Debug, Default)]
    pub struct SaveState {
        prepared: bool,
        disabled: bool,
    }

    static SESSION: Mutex<SaveState> = Mutex::new(SaveState {
        prepared: false,
        disabled: false,
    });

    fn non_empty(value: Option<&OsStr>) -> Option<&OsStr> {
        value.filter(|v| !v.is_empty())
    }

    fn absolute(value: Option<&OsStr>) -> Option<PathBuf> {
        non_empty(value)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    }

    /// Resolves the application data directory from injected values (pure, testable).
    /// Base directories must be absolute; a relative `XDG_DATA_HOME` falls back to `HOME`.
    pub fn data_dir_from(
        os: &str,
        appdata: Option<&OsStr>,
        xdg_data_home: Option<&OsStr>,
        home: Option<&OsStr>,
    ) -> Option<PathBuf> {
        let (base, app_dir) = match os {
            OS_WINDOWS => (absolute(appdata)?, APP_DIR_NAME_CAPITALIZED),
            OS_MACOS => {
                let mut base = absolute(home)?;
                base.extend(MACOS_SUPPORT_DIRS);
                (base, APP_DIR_NAME_CAPITALIZED)
            }
            _ => {
                let base = match absolute(xdg_data_home) {
                    Some(xdg) => xdg,
                    None => {
                        let mut base = absolute(home)?;
                        base.extend(UNIX_DATA_DIRS);
                        base
                    }
                };
                (base, APP_DIR_NAME_UNIX)
            }
        };
        Some(base.join(app_dir))
    }

    /// Resolves the progress file path from injected values (pure, testable).
    #[cfg(test)]
    pub fn progress_path_from(
        os: &str,
        appdata: Option<&OsStr>,
        xdg_data_home: Option<&OsStr>,
        home: Option<&OsStr>,
    ) -> Option<PathBuf> {
        data_dir_from(os, appdata, xdg_data_home, home).map(|dir| dir.join(PROGRESS_FILE_NAME))
    }

    /// Resolves the data directory for the running platform; `None` disables persistence.
    pub fn data_dir() -> Option<PathBuf> {
        let appdata = std::env::var_os(ENV_APPDATA);
        let xdg = std::env::var_os(ENV_XDG_DATA_HOME);
        let home = std::env::var_os(ENV_HOME);
        data_dir_from(
            std::env::consts::OS,
            appdata.as_deref(),
            xdg.as_deref(),
            home.as_deref(),
        )
    }

    /// Resolves the progress file path for the running platform; `None` disables persistence.
    pub fn progress_path() -> Option<PathBuf> {
        data_dir().map(|dir| dir.join(PROGRESS_FILE_NAME))
    }

    /// Reads the file, retrying once on an I/O error other than NotFound.
    pub fn read_progress(path: &Path) -> ReadOutcome {
        let mut text = None;
        for _ in 0..READ_ATTEMPTS {
            match std::fs::read_to_string(path) {
                Ok(t) => {
                    text = Some(t);
                    break;
                }
                Err(err) if err.kind() == ErrorKind::NotFound => return ReadOutcome::Missing,
                Err(_) => {}
            }
        }
        let Some(text) = text else {
            return ReadOutcome::IoFailed;
        };
        match serde_json::from_str::<Progress>(&text) {
            Ok(progress) if progress.version == PROGRESS_VERSION => ReadOutcome::Valid(progress),
            _ => ReadOutcome::Unreadable,
        }
    }

    pub fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
        let mut result = std::fs::rename(from, to);
        for _ in 1..RENAME_ATTEMPTS {
            if result.is_ok() {
                break;
            }
            std::thread::sleep(RENAME_BACKOFF);
            result = std::fs::rename(from, to);
        }
        result
    }

    /// Writes `text` to a unique temporary sibling, syncs it, then renames over the target.
    pub fn write_text_atomic(path: &Path, text: &str) -> std::io::Result<()> {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = dir.join(format!(
            "{file_name}.{}{PROGRESS_TMP_SUFFIX}",
            std::process::id()
        ));
        let result = (|| {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            drop(file);
            rename_with_retry(&tmp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    /// Serializes the progress and writes it atomically.
    pub fn write_progress(path: &Path, progress: &Progress) -> std::io::Result<()> {
        let json = serde_json::to_string(progress).map_err(std::io::Error::other)?;
        write_text_atomic(path, &json)
    }

    /// Before the first write of a session, moves an unreadable file aside to `.bak`. A file
    /// that could not even be read (I/O failure) is left alone and the session stays unprepared,
    /// so the next save looks at it again.
    fn prepare_first_write(path: &Path, state: &mut SaveState) {
        if state.prepared {
            return;
        }
        match read_progress(path) {
            ReadOutcome::IoFailed => return,
            ReadOutcome::Unreadable => {}
            ReadOutcome::Missing | ReadOutcome::Valid(_) => {
                state.prepared = true;
                return;
            }
        }
        state.prepared = true;
        let backup = path.with_file_name(PROGRESS_BACKUP_FILE_NAME);
        let _ = std::fs::remove_file(&backup);
        if rename_with_retry(path, &backup).is_err() {
            state.disabled = true;
        }
    }

    /// Saves with session safeguards: backup of an unreadable file, then monotonic merge. A file
    /// that cannot be read at all is never overwritten: the save fails and a later one retries.
    pub fn save_with(
        path: &Path,
        progress: &Progress,
        state: &mut SaveState,
    ) -> std::io::Result<()> {
        prepare_first_write(path, state);
        if state.disabled {
            return Ok(());
        }
        let mut merged = progress.clone();
        match read_progress(path) {
            ReadOutcome::Valid(current) => {
                merged.high_score = merged.high_score.max(current.high_score);
                merged.unlocked = merged.unlocked.max(current.unlocked);
            }
            ReadOutcome::IoFailed => return Err(read_failed()),
            ReadOutcome::Missing | ReadOutcome::Unreadable => {}
        }
        write_progress(path, &merged)
    }

    /// Saves the game progress keeping the audio settings of the file. The settings of a corrupt
    /// file fall back to the defaults (the file is moved aside first); when the file cannot be
    /// read at all nothing is written.
    pub fn save_progress_with(
        path: &Path,
        game: ProgressState,
        state: &mut SaveState,
    ) -> std::io::Result<()> {
        let settings = match read_progress(path) {
            ReadOutcome::Valid(current) => super::Settings {
                sfx_on: current.sfx_on,
                music_on: current.music_on,
            },
            ReadOutcome::Missing | ReadOutcome::Unreadable => super::Settings::default(),
            ReadOutcome::IoFailed => return Err(read_failed()),
        };
        let progress = Progress {
            version: PROGRESS_VERSION,
            high_score: game.high_score,
            unlocked: game.unlocked,
            theme: game.theme,
            lang_fr: game.lang_fr,
            sfx_on: settings.sfx_on,
            music_on: settings.music_on,
        };
        save_with(path, &progress, state)
    }

    /// Saves the audio settings keeping the game progress of the file. A missing or corrupt file
    /// starts from default progress; when the file cannot be read at all nothing is written.
    pub fn save_settings_with(
        path: &Path,
        settings: super::Settings,
        state: &mut SaveState,
    ) -> std::io::Result<()> {
        let mut progress = match read_progress(path) {
            ReadOutcome::Valid(current) => current,
            ReadOutcome::Missing | ReadOutcome::Unreadable => Progress::default(),
            ReadOutcome::IoFailed => return Err(read_failed()),
        };
        progress.sfx_on = settings.sfx_on;
        progress.music_on = settings.music_on;
        save_with(path, &progress, state)
    }

    pub fn save_progress_session(path: &Path, game: ProgressState) -> std::io::Result<()> {
        let mut state = SESSION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        save_progress_with(path, game, &mut state)
    }

    pub fn save_settings_session(path: &Path, settings: super::Settings) -> std::io::Result<()> {
        let mut state = SESSION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        save_settings_with(path, settings, &mut state)
    }

    pub fn save_session(path: &Path, progress: &Progress) -> std::io::Result<()> {
        let mut state = SESSION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        save_with(path, progress, &mut state)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        struct TempDir(PathBuf);

        impl TempDir {
            fn new(tag: &str) -> Self {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let dir = std::env::temp_dir()
                    .join(format!("kinetix-test-{tag}-{}-{nanos}", std::process::id()));
                std::fs::create_dir_all(&dir).unwrap();
                Self(dir)
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        fn sample() -> Progress {
            Progress {
                version: PROGRESS_VERSION,
                high_score: 123_456,
                unlocked: 7,
                theme: 2,
                lang_fr: true,
                sfx_on: false,
                music_on: true,
            }
        }

        fn abs(name: &str) -> PathBuf {
            std::env::temp_dir().join(name)
        }

        fn os(p: &Path) -> Option<&OsStr> {
            Some(p.as_os_str())
        }

        fn leftover_tmp(dir: &Path) -> bool {
            std::fs::read_dir(dir).unwrap().any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(PROGRESS_TMP_SUFFIX)
            })
        }

        #[test]
        fn path_windows_uses_absolute_appdata() {
            let roaming = abs("roaming");
            let p = progress_path_from(OS_WINDOWS, os(&roaming), None, None).unwrap();
            assert_eq!(p, roaming.join("Kinetix").join("progress.json"));
            assert!(progress_path_from(OS_WINDOWS, None, None, os(&roaming)).is_none());
            assert!(progress_path_from(OS_WINDOWS, Some(OsStr::new("")), None, None).is_none());
            assert!(progress_path_from(OS_WINDOWS, Some(OsStr::new("rel")), None, None).is_none());
        }

        #[test]
        fn path_macos_uses_application_support() {
            let home = abs("home-a");
            let p = progress_path_from(OS_MACOS, None, None, os(&home)).unwrap();
            assert_eq!(
                p,
                home.join("Library")
                    .join("Application Support")
                    .join("Kinetix")
                    .join("progress.json")
            );
            assert!(progress_path_from(OS_MACOS, None, None, None).is_none());
            assert!(progress_path_from(OS_MACOS, None, None, Some(OsStr::new("rel"))).is_none());
        }

        #[test]
        fn path_linux_prefers_absolute_xdg_then_home() {
            let (x, h) = (abs("xdg"), abs("home-b"));
            let xdg = progress_path_from("linux", None, os(&x), os(&h)).unwrap();
            assert_eq!(xdg, x.join("kinetix").join("progress.json"));
            let expected_home = h
                .join(".local")
                .join("share")
                .join("kinetix")
                .join("progress.json");
            let empty = progress_path_from("linux", None, Some(OsStr::new("")), os(&h)).unwrap();
            assert_eq!(empty, expected_home);
            let rel = progress_path_from("linux", None, Some(OsStr::new("rel")), os(&h)).unwrap();
            assert_eq!(rel, expected_home);
            assert!(progress_path_from("linux", None, None, None).is_none());
            assert!(progress_path_from("linux", None, None, Some(OsStr::new("rel"))).is_none());
        }

        #[test]
        fn round_trip_and_overwrite_leave_no_tmp() {
            let tmp = TempDir::new("roundtrip");
            let path = tmp.0.join("nested").join(PROGRESS_FILE_NAME);
            assert_eq!(read_progress(&path), ReadOutcome::Missing);
            let mut state = SaveState::default();
            save_with(&path, &sample(), &mut state).unwrap();
            assert_eq!(read_progress(&path), ReadOutcome::Valid(sample()));
            let updated = Progress {
                unlocked: 9,
                theme: 1,
                ..sample()
            };
            save_with(&path, &updated, &mut state).unwrap();
            assert_eq!(read_progress(&path), ReadOutcome::Valid(updated));
            assert!(!leftover_tmp(path.parent().unwrap()));
        }

        #[test]
        fn corrupt_file_is_preserved_in_bak_before_first_save() {
            let tmp = TempDir::new("corrupt");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            let bak = tmp.0.join(PROGRESS_BACKUP_FILE_NAME);
            std::fs::write(&path, "{ not json").unwrap();
            assert_eq!(read_progress(&path), ReadOutcome::Unreadable);
            let mut state = SaveState::default();
            save_with(&path, &sample(), &mut state).unwrap();
            assert_eq!(std::fs::read_to_string(&bak).unwrap(), "{ not json");
            assert_eq!(read_progress(&path), ReadOutcome::Valid(sample()));
        }

        #[test]
        fn future_version_is_preserved_in_bak_and_replaces_old_bak() {
            let tmp = TempDir::new("future");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            let bak = tmp.0.join(PROGRESS_BACKUP_FILE_NAME);
            let future = r#"{"version":99,"high_score":1,"unlocked":1,"theme":1,"lang_fr":true}"#;
            std::fs::write(&bak, "stale backup").unwrap();
            std::fs::write(&path, future).unwrap();
            let mut state = SaveState::default();
            save_with(&path, &sample(), &mut state).unwrap();
            assert_eq!(std::fs::read_to_string(&bak).unwrap(), future);
            assert_eq!(read_progress(&path), ReadOutcome::Valid(sample()));
            // Later saves of the same session must not touch the backup again.
            save_with(&path, &sample(), &mut state).unwrap();
            assert_eq!(std::fs::read_to_string(&bak).unwrap(), future);
        }

        #[test]
        fn save_is_disabled_when_backup_rename_fails() {
            let tmp = TempDir::new("nobak");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            std::fs::write(&path, "garbage").unwrap();
            // A directory named like the backup cannot be removed as a file nor replaced.
            let bak = tmp.0.join(PROGRESS_BACKUP_FILE_NAME);
            std::fs::create_dir_all(bak.join("keep")).unwrap();
            let mut state = SaveState::default();
            save_with(&path, &sample(), &mut state).unwrap();
            assert!(state.disabled);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "garbage");
        }

        #[test]
        fn save_merges_monotonic_fields_from_current_file() {
            let tmp = TempDir::new("merge");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            write_progress(&path, &sample()).unwrap();
            let stale = Progress {
                high_score: 10,
                unlocked: 1,
                theme: 5,
                lang_fr: false,
                ..sample()
            };
            let mut state = SaveState::default();
            save_with(&path, &stale, &mut state).unwrap();
            let ReadOutcome::Valid(saved) = read_progress(&path) else {
                panic!("expected a valid file");
            };
            assert_eq!(saved.high_score, 123_456);
            assert_eq!(saved.unlocked, 7);
            assert_eq!(saved.theme, 5);
            assert!(!saved.lang_fr);
        }

        #[test]
        fn data_dir_is_the_parent_of_the_progress_file() {
            let roaming = abs("roaming-dir");
            let dir = data_dir_from(OS_WINDOWS, os(&roaming), None, None).unwrap();
            assert_eq!(dir, roaming.join("Kinetix"));
            let file = progress_path_from(OS_WINDOWS, os(&roaming), None, None).unwrap();
            assert_eq!(file.parent().unwrap(), dir);
            assert!(data_dir_from(OS_WINDOWS, None, None, None).is_none());
        }

        #[test]
        fn v1_file_without_settings_loads_intact_with_audio_on() {
            let tmp = TempDir::new("v1-settings");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            std::fs::write(
                &path,
                r#"{"version":1,"high_score":4200,"unlocked":12,"theme":3,"lang_fr":true}"#,
            )
            .unwrap();
            let ReadOutcome::Valid(p) = read_progress(&path) else {
                panic!("expected a valid file");
            };
            assert_eq!(
                (p.high_score, p.unlocked, p.theme, p.lang_fr),
                (4200, 12, 3, true)
            );
            assert!(p.sfx_on && p.music_on);
            assert_eq!(p.version, PROGRESS_VERSION);
            let settings = current_settings(&path);
            assert!(settings.sfx_on && settings.music_on);
        }

        #[test]
        fn settings_round_trip_and_keep_progress_fields() {
            let tmp = TempDir::new("settings-rt");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            write_progress(&path, &sample()).unwrap();
            let ReadOutcome::Valid(mut p) = read_progress(&path) else {
                panic!("expected a valid file");
            };
            p.sfx_on = true;
            p.music_on = false;
            let mut state = SaveState::default();
            save_with(&path, &p, &mut state).unwrap();
            let settings = current_settings(&path);
            assert!(settings.sfx_on && !settings.music_on);
            let ReadOutcome::Valid(saved) = read_progress(&path) else {
                panic!("expected a valid file");
            };
            assert_eq!(saved.high_score, 123_456);
            assert_eq!(saved.unlocked, 7);
        }

        #[test]
        fn unreadable_file_yields_default_settings() {
            let tmp = TempDir::new("settings-bad");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            std::fs::write(&path, "{ not json").unwrap();
            let defaults = crate::persistence::Settings::default();
            assert_eq!(current_settings(&path), defaults);
            assert_eq!(current_settings(&tmp.0.join("absent.json")), defaults);
        }

        /// A directory where the file should be: it exists but cannot be read as a file.
        fn unreadable_dir_path(tmp: &TempDir) -> PathBuf {
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            std::fs::create_dir_all(path.join("keep")).unwrap();
            path
        }

        fn game_state() -> ProgressState {
            ProgressState {
                high_score: 10,
                unlocked: 1,
                theme: 4,
                lang_fr: true,
            }
        }

        #[test]
        fn an_io_failure_is_told_apart_from_corrupt_content() {
            let tmp = TempDir::new("io-kind");
            assert_eq!(
                read_progress(&unreadable_dir_path(&tmp)),
                ReadOutcome::IoFailed
            );
            let corrupt = tmp.0.join("corrupt.json");
            std::fs::write(&corrupt, "{ not json").unwrap();
            assert_eq!(read_progress(&corrupt), ReadOutcome::Unreadable);
        }

        #[test]
        fn a_save_never_writes_over_a_file_that_cannot_be_read() {
            let tmp = TempDir::new("io-save");
            let path = unreadable_dir_path(&tmp);
            let mut state = SaveState::default();
            assert!(save_with(&path, &sample(), &mut state).is_err());
            assert!(save_progress_with(&path, game_state(), &mut state).is_err());
            assert!(
                save_settings_with(&path, crate::persistence::Settings::default(), &mut state)
                    .is_err()
            );
            assert!(path.is_dir(), "the unreadable entry is untouched");
            assert!(!tmp.0.join(PROGRESS_BACKUP_FILE_NAME).exists());
            assert!(!state.prepared, "the next save looks at the file again");
            assert!(!state.disabled);
        }

        #[test]
        fn a_save_after_a_read_failure_retries_and_keeps_the_other_fields() {
            let tmp = TempDir::new("io-retry");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            let mut state = SaveState::default();
            // The failure happens while the file is momentarily unreadable.
            std::fs::create_dir_all(path.join("keep")).unwrap();
            assert!(save_progress_with(&path, game_state(), &mut state).is_err());
            std::fs::remove_dir_all(&path).unwrap();
            let kept = Progress {
                sfx_on: false,
                music_on: false,
                ..sample()
            };
            write_progress(&path, &kept).unwrap();

            save_progress_with(&path, game_state(), &mut state).unwrap();
            let ReadOutcome::Valid(saved) = read_progress(&path) else {
                panic!("expected a valid file");
            };
            assert!(!saved.sfx_on && !saved.music_on, "settings were not reset");
            assert_eq!((saved.theme, saved.lang_fr), (4, true));
            assert_eq!(saved.high_score, 123_456, "monotonic merge still applies");
        }

        #[test]
        fn saving_progress_keeps_the_stored_audio_settings() {
            let tmp = TempDir::new("keep-audio");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            let stored = Progress {
                sfx_on: false,
                music_on: true,
                ..sample()
            };
            write_progress(&path, &stored).unwrap();
            let mut state = SaveState::default();
            save_progress_with(&path, game_state(), &mut state).unwrap();
            let ReadOutcome::Valid(saved) = read_progress(&path) else {
                panic!("expected a valid file");
            };
            assert!(!saved.sfx_on && saved.music_on);
        }

        #[test]
        fn saving_settings_keeps_the_stored_progress() {
            let tmp = TempDir::new("keep-progress");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            write_progress(&path, &sample()).unwrap();
            let mut state = SaveState::default();
            let settings = crate::persistence::Settings {
                sfx_on: true,
                music_on: false,
            };
            save_settings_with(&path, settings, &mut state).unwrap();
            let ReadOutcome::Valid(saved) = read_progress(&path) else {
                panic!("expected a valid file");
            };
            assert!(saved.sfx_on && !saved.music_on);
            assert_eq!((saved.high_score, saved.unlocked), (123_456, 7));
            assert_eq!((saved.theme, saved.lang_fr), (2, true));
        }

        #[test]
        fn a_corrupt_file_is_still_moved_aside_before_the_first_settings_or_progress_write() {
            for settings_first in [true, false] {
                let tmp = TempDir::new("corrupt-first");
                let path = tmp.0.join(PROGRESS_FILE_NAME);
                std::fs::write(&path, "{ not json").unwrap();
                let mut state = SaveState::default();
                if settings_first {
                    let settings = crate::persistence::Settings {
                        sfx_on: false,
                        music_on: true,
                    };
                    save_settings_with(&path, settings, &mut state).unwrap();
                } else {
                    save_progress_with(&path, game_state(), &mut state).unwrap();
                }
                let bak = tmp.0.join(PROGRESS_BACKUP_FILE_NAME);
                assert_eq!(std::fs::read_to_string(&bak).unwrap(), "{ not json");
                assert!(matches!(read_progress(&path), ReadOutcome::Valid(_)));
            }
        }

        #[test]
        fn missing_fields_default_but_version_decides() {
            let tmp = TempDir::new("partial");
            let path = tmp.0.join(PROGRESS_FILE_NAME);
            std::fs::write(&path, r#"{"version":1,"unlocked":3}"#).unwrap();
            let ReadOutcome::Valid(p) = read_progress(&path) else {
                panic!("expected a valid file");
            };
            assert_eq!(
                (p.unlocked, p.high_score, p.theme, p.lang_fr),
                (3, 0, 0, false)
            );
        }
    }
}

#[cfg(test)]
mod throttle_tests {
    use super::{
        high_score_save_due, pack_web_settings, unpack_web_settings, unpack_web_state, Settings,
    };

    #[test]
    fn web_settings_word_round_trips_every_combination() {
        for sfx_on in [false, true] {
            for music_on in [false, true] {
                let settings = Settings { sfx_on, music_on };
                assert_eq!(unpack_web_settings(pack_web_settings(settings)), settings);
            }
        }
        assert_eq!(pack_web_settings(Settings::default()), 0b11);
        assert_eq!(
            unpack_web_settings(0),
            Settings {
                sfx_on: false,
                music_on: false
            }
        );
    }

    #[test]
    fn no_save_without_a_new_high_score() {
        assert!(!high_score_save_due(100, 100, 50.0, 0.0, 5.0));
        assert!(!high_score_save_due(90, 100, 50.0, 0.0, 5.0));
    }

    #[test]
    fn new_high_score_is_throttled_by_the_interval() {
        assert!(!high_score_save_due(200, 100, 12.0, 10.0, 5.0));
        assert!(high_score_save_due(200, 100, 15.0, 10.0, 5.0));
        assert!(high_score_save_due(200, 100, 99.0, 10.0, 5.0));
    }

    #[test]
    fn web_state_unpacks_16bit_unlocked_8bit_theme_1bit_lang() {
        assert_eq!(unpack_web_state(0), (0, 0, false));
        let packed = 799u32 | (5u32 << 16) | (1u32 << 24);
        assert_eq!(unpack_web_state(packed), (799, 5, true));
        let max_packed = 0xFFFFu32 | (0xFFu32 << 16) | (1u32 << 24);
        assert_eq!(unpack_web_state(max_packed), (65535, 255, true));
        let en_packed = 350u32 | (7u32 << 16);
        assert_eq!(unpack_web_state(en_packed), (350, 7, false));
    }
}
