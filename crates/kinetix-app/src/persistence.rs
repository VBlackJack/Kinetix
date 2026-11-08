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

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn kinetix_save_state(high_score: u32, unlocked: u32, theme: u32, lang: u32);
    fn kinetix_load_packed() -> u32;
    fn kinetix_load_highscore() -> u32;
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
pub fn save_progress(high_score: u64, unlocked: usize, theme: usize, lang_fr: bool) {
    let Some(path) = native::progress_path() else {
        return;
    };
    let progress = native::Progress {
        version: native::PROGRESS_VERSION,
        high_score,
        unlocked,
        theme,
        lang_fr,
    };
    // Persistence must never crash the game; failures are reported in debug builds only.
    if let Err(_err) = native::save_session(&path, &progress) {
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
mod native {
    use serde::{Deserialize, Serialize};
    use std::ffi::OsStr;
    use std::io::{ErrorKind, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::Duration;

    pub const PROGRESS_VERSION: u32 = 1;
    const PROGRESS_FILE_NAME: &str = "progress.json";
    const PROGRESS_BACKUP_FILE_NAME: &str = "progress.json.bak";
    const PROGRESS_TMP_PREFIX: &str = "progress.json.";
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
    }

    impl Default for Progress {
        fn default() -> Self {
            Self {
                version: PROGRESS_VERSION,
                high_score: 0,
                unlocked: 0,
                theme: 0,
                lang_fr: false,
            }
        }
    }

    /// Result of reading the progress file.
    #[derive(Debug, PartialEq, Eq)]
    pub enum ReadOutcome {
        Missing,
        Valid(Progress),
        /// I/O failure, corrupt JSON or incompatible version: the file must not be overwritten.
        Unreadable,
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

    /// Resolves the progress file path from injected values (pure, testable).
    /// Base directories must be absolute; a relative `XDG_DATA_HOME` falls back to `HOME`.
    pub fn progress_path_from(
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
        Some(base.join(app_dir).join(PROGRESS_FILE_NAME))
    }

    /// Resolves the progress file path for the running platform; `None` disables persistence.
    pub fn progress_path() -> Option<PathBuf> {
        let appdata = std::env::var_os(ENV_APPDATA);
        let xdg = std::env::var_os(ENV_XDG_DATA_HOME);
        let home = std::env::var_os(ENV_HOME);
        progress_path_from(
            std::env::consts::OS,
            appdata.as_deref(),
            xdg.as_deref(),
            home.as_deref(),
        )
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
            return ReadOutcome::Unreadable;
        };
        match serde_json::from_str::<Progress>(&text) {
            Ok(progress) if progress.version == PROGRESS_VERSION => ReadOutcome::Valid(progress),
            _ => ReadOutcome::Unreadable,
        }
    }

    fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
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

    /// Writes to a unique temporary sibling, syncs it, then renames over the target.
    pub fn write_progress(path: &Path, progress: &Progress) -> std::io::Result<()> {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(
            "{PROGRESS_TMP_PREFIX}{}{PROGRESS_TMP_SUFFIX}",
            std::process::id()
        ));
        let result = (|| {
            let json = serde_json::to_string(progress).map_err(std::io::Error::other)?;
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
            drop(file);
            rename_with_retry(&tmp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    /// Before the first write of a session, moves an unreadable file aside to `.bak`.
    fn prepare_first_write(path: &Path, state: &mut SaveState) {
        if state.prepared {
            return;
        }
        state.prepared = true;
        if read_progress(path) != ReadOutcome::Unreadable {
            return;
        }
        let backup = path.with_file_name(PROGRESS_BACKUP_FILE_NAME);
        let _ = std::fs::remove_file(&backup);
        if rename_with_retry(path, &backup).is_err() {
            state.disabled = true;
        }
    }

    /// Saves with session safeguards: backup of an unreadable file, then monotonic merge.
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
        if let ReadOutcome::Valid(current) = read_progress(path) {
            merged.high_score = merged.high_score.max(current.high_score);
            merged.unlocked = merged.unlocked.max(current.unlocked);
        }
        write_progress(path, &merged)
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
