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

//! The player's saved level library: pure collection logic plus platform storage
//! (a `library.json` file on native builds, one browser storage key on the web).

use kinetix_core::config::MAX_LEVEL_NAME_LEN;
use serde::{Deserialize, Serialize};

/// Maximum number of levels the library keeps.
pub const MAX_LIBRARY_LEVELS: usize = 50;

/// Web storage key holding the serialized library.
#[cfg(target_arch = "wasm32")]
const LIBRARY_STORE_KEY: &str = "kinetix_library";

/// Why a level could not be added to the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryError {
    /// The library already holds `MAX_LIBRARY_LEVELS` levels (or ids are exhausted).
    Full,
}

/// Why the library could not be written to storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// No storage location exists on this platform or environment.
    Unavailable,
    /// The existing file is unreadable and could not be moved aside: it is left untouched.
    Blocked,
    /// The storage refused the write (I/O failure, quota, private mode).
    Failed,
}

/// One saved level: a stable id, a display name and its KTX1 share code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: u32,
    pub name: String,
    pub code: String,
}

/// The saved levels plus the next id to hand out (ids are never reused).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub entries: Vec<LibraryEntry>,
    pub next_id: u32,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: FIRST_LIBRARY_ID,
        }
    }
}

/// Id given to the first entry of an empty library.
const FIRST_LIBRARY_ID: u32 = 1;

impl Library {
    /// Adds a level and returns its new id; the name is stored as given.
    pub fn add(&mut self, name: String, code: String) -> Result<u32, LibraryError> {
        if self.entries.len() >= MAX_LIBRARY_LEVELS {
            return Err(LibraryError::Full);
        }
        let id = self.next_id;
        let next = id.checked_add(1).ok_or(LibraryError::Full)?;
        self.entries.push(LibraryEntry { id, name, code });
        self.next_id = next;
        Ok(id)
    }

    /// Removes the entry with this id; returns whether one was removed.
    pub fn delete(&mut self, id: u32) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != before
    }

    pub fn get(&self, id: u32) -> Option<&LibraryEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Serializes the library as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Parses a stored library. Corrupt input yields an empty library, never a panic.
    /// A parsed library is normalised: entries beyond the cap and duplicate ids are dropped
    /// and `next_id` is moved past every existing id.
    pub fn from_json(text: &str) -> Self {
        let Ok(mut parsed) = serde_json::from_str::<Library>(text) else {
            return Self::default();
        };
        let mut seen = std::collections::BTreeSet::new();
        parsed.entries.retain(|entry| seen.insert(entry.id));
        parsed.entries.truncate(MAX_LIBRARY_LEVELS);
        let floor = parsed
            .entries
            .iter()
            .map(|entry| entry.id.saturating_add(1))
            .max()
            .unwrap_or(FIRST_LIBRARY_ID);
        parsed.next_id = parsed.next_id.max(floor).max(FIRST_LIBRARY_ID);
        parsed
    }
}

/// Keeps the characters accepted by `allowed` and caps the result at `MAX_LEVEL_NAME_LEN`
/// characters.
pub fn sanitize_name(name: &str, allowed: impl Fn(char) -> bool) -> String {
    name.chars()
        .filter(|&c| allowed(c))
        .take(MAX_LEVEL_NAME_LEN)
        .collect()
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn kinetix_store_len(key_ptr: *const u8, key_len: u32) -> i32;
    fn kinetix_store_read(key_ptr: *const u8, key_len: u32, buf_ptr: *mut u8, buf_cap: u32) -> i32;
    fn kinetix_store_write(
        key_ptr: *const u8,
        key_len: u32,
        val_ptr: *const u8,
        val_len: u32,
    ) -> i32;
}

/// Loads the library from storage; any failure yields an empty library.
#[cfg(target_arch = "wasm32")]
pub fn load_library() -> Library {
    let key = LIBRARY_STORE_KEY.as_bytes();
    let len = unsafe { kinetix_store_len(key.as_ptr(), key.len() as u32) };
    if len <= 0 {
        return Library::default();
    }
    let mut buf = vec![0u8; len as usize];
    let read = unsafe {
        kinetix_store_read(
            key.as_ptr(),
            key.len() as u32,
            buf.as_mut_ptr(),
            buf.len() as u32,
        )
    };
    if read != len {
        return Library::default();
    }
    String::from_utf8(buf)
        .map(|text| Library::from_json(&text))
        .unwrap_or_default()
}

/// Writes the library to storage.
#[cfg(target_arch = "wasm32")]
pub fn save_library(library: &Library) -> Result<(), SaveError> {
    let key = LIBRARY_STORE_KEY.as_bytes();
    let json = library.to_json();
    let ok = unsafe {
        kinetix_store_write(
            key.as_ptr(),
            key.len() as u32,
            json.as_ptr(),
            json.len() as u32,
        )
    };
    if ok == 1 {
        Ok(())
    } else {
        Err(SaveError::Failed)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Library, SaveError};
    use crate::persistence::native::{data_dir, rename_with_retry, write_text_atomic};
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    const LIBRARY_FILE_NAME: &str = "library.json";
    const LIBRARY_BACKUP_FILE_NAME: &str = "library.json.bak";
    const READ_ATTEMPTS: usize = 2;

    /// Result of reading the library file.
    #[derive(Debug, PartialEq, Eq)]
    pub enum ReadOutcome {
        Missing,
        Valid(Library),
        /// I/O failure or corrupt JSON: the file must not be overwritten before a backup.
        Unreadable,
    }

    /// Per-session write state of the library file, independent of the progress file's.
    #[derive(Debug, Default)]
    pub struct SaveState {
        pub prepared: bool,
        pub disabled: bool,
    }

    static SESSION: Mutex<SaveState> = Mutex::new(SaveState {
        prepared: false,
        disabled: false,
    });

    pub fn library_path() -> Option<PathBuf> {
        data_dir().map(|dir| dir.join(LIBRARY_FILE_NAME))
    }

    pub fn read_library(path: &Path) -> ReadOutcome {
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
        match serde_json::from_str::<Library>(&text) {
            Ok(_) => ReadOutcome::Valid(Library::from_json(&text)),
            Err(_) => ReadOutcome::Unreadable,
        }
    }

    /// Before the first write of a session, moves an unreadable file aside to `.bak`.
    fn prepare_first_write(path: &Path, state: &mut SaveState) {
        if state.prepared {
            return;
        }
        state.prepared = true;
        if read_library(path) != ReadOutcome::Unreadable {
            return;
        }
        let backup = path.with_file_name(LIBRARY_BACKUP_FILE_NAME);
        let _ = std::fs::remove_file(&backup);
        if rename_with_retry(path, &backup).is_err() {
            state.disabled = true;
        }
    }

    pub fn save_with(
        path: &Path,
        library: &Library,
        state: &mut SaveState,
    ) -> Result<(), SaveError> {
        prepare_first_write(path, state);
        if state.disabled {
            return Err(SaveError::Blocked);
        }
        write_text_atomic(path, &library.to_json()).map_err(|_| SaveError::Failed)
    }

    pub fn load_from(path: &Path) -> Library {
        match read_library(path) {
            ReadOutcome::Valid(library) => library,
            _ => Library::default(),
        }
    }

    pub fn save_session(path: &Path, library: &Library) -> Result<(), SaveError> {
        let mut state = SESSION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        save_with(path, library, &mut state)
    }

    #[cfg(test)]
    pub const BACKUP_NAME: &str = LIBRARY_BACKUP_FILE_NAME;
    #[cfg(test)]
    pub const FILE_NAME: &str = LIBRARY_FILE_NAME;
}

/// Loads the library from `library.json`; a missing or unreadable file yields an empty one.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_library() -> Library {
    native::library_path()
        .map(|path| native::load_from(&path))
        .unwrap_or_default()
}

/// Writes the library to `library.json`, preserving an unreadable file as `.bak` first.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_library(library: &Library) -> Result<(), SaveError> {
    let path = native::library_path().ok_or(SaveError::Unavailable)?;
    native::save_session(&path, library)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(n: usize) -> String {
        format!("KTX1-{n}")
    }

    #[test]
    fn add_assigns_increasing_ids_and_get_finds_them() {
        let mut lib = Library::default();
        let a = lib.add("Alpha".into(), code(1)).unwrap();
        let b = lib.add("Beta".into(), code(2)).unwrap();
        assert!(b > a);
        assert_eq!(lib.get(a).unwrap().name, "Alpha");
        assert_eq!(lib.get(b).unwrap().code, code(2));
        assert!(lib.get(b + 100).is_none());
    }

    #[test]
    fn add_refuses_beyond_the_cap_and_keeps_state() {
        let mut lib = Library::default();
        for n in 0..MAX_LIBRARY_LEVELS {
            lib.add(format!("L{n}"), code(n)).unwrap();
        }
        let next = lib.next_id;
        assert_eq!(lib.add("extra".into(), code(99)), Err(LibraryError::Full));
        assert_eq!(lib.entries.len(), MAX_LIBRARY_LEVELS);
        assert_eq!(lib.next_id, next);
    }

    #[test]
    fn delete_removes_only_the_matching_id_and_ids_are_not_reused() {
        let mut lib = Library::default();
        let a = lib.add("A".into(), code(1)).unwrap();
        let b = lib.add("B".into(), code(2)).unwrap();
        assert!(lib.delete(a));
        assert!(!lib.delete(a));
        assert!(lib.get(a).is_none() && lib.get(b).is_some());
        let c = lib.add("C".into(), code(3)).unwrap();
        assert!(c > b);
    }

    #[test]
    fn full_library_accepts_again_after_a_delete() {
        let mut lib = Library::default();
        let first = lib.add("first".into(), code(0)).unwrap();
        for n in 1..MAX_LIBRARY_LEVELS {
            lib.add(format!("L{n}"), code(n)).unwrap();
        }
        assert!(lib.add("x".into(), code(1)).is_err());
        assert!(lib.delete(first));
        assert!(lib.add("x".into(), code(1)).is_ok());
    }

    #[test]
    fn exhausted_ids_report_full() {
        let mut lib = Library {
            entries: Vec::new(),
            next_id: u32::MAX,
        };
        assert_eq!(lib.add("n".into(), code(1)), Err(LibraryError::Full));
        assert!(lib.entries.is_empty());
    }

    #[test]
    fn sanitize_keeps_allowed_chars_and_caps_the_length() {
        let allowed = |c: char| c.is_ascii_alphanumeric() || c == ' ';
        assert_eq!(sanitize_name("Ab 12!?#", allowed), "Ab 12");
        assert_eq!(sanitize_name("", allowed), "");
        assert_eq!(sanitize_name("!!!", allowed), "");
        let long = "a".repeat(MAX_LEVEL_NAME_LEN + 10);
        assert_eq!(
            sanitize_name(&long, allowed).chars().count(),
            MAX_LEVEL_NAME_LEN
        );
        let accents = "e\u{301}e".repeat(MAX_LEVEL_NAME_LEN);
        assert!(sanitize_name(&accents, |_| true).chars().count() <= MAX_LEVEL_NAME_LEN);
    }

    #[test]
    fn json_round_trip_preserves_entries_and_next_id() {
        let mut lib = Library::default();
        lib.add("One".into(), code(1)).unwrap();
        let b = lib.add("Two".into(), code(2)).unwrap();
        lib.delete(b);
        let back = Library::from_json(&lib.to_json());
        assert_eq!(back, lib);
    }

    #[test]
    fn corrupt_or_foreign_json_yields_an_empty_library() {
        for text in ["", "{ not json", "[]", "null", "42", r#"{"entries":"x"}"#] {
            assert_eq!(Library::from_json(text), Library::default(), "{text}");
        }
    }

    #[test]
    fn from_json_repairs_next_id_duplicates_and_oversize() {
        let low_next = r#"{"entries":[{"id":9,"name":"n","code":"c"}],"next_id":1}"#;
        let lib = Library::from_json(low_next);
        assert_eq!(lib.next_id, 10);
        let dup = r#"{"entries":[{"id":3,"name":"a","code":"c"},{"id":3,"name":"b","code":"c"}],"next_id":4}"#;
        let lib = Library::from_json(dup);
        assert_eq!(lib.entries.len(), 1);
        assert_eq!(lib.entries[0].name, "a");
        let many: Vec<String> = (1..=MAX_LIBRARY_LEVELS + 5)
            .map(|i| format!(r#"{{"id":{i},"name":"n","code":"c"}}"#))
            .collect();
        let text = format!(r#"{{"entries":[{}],"next_id":1}}"#, many.join(","));
        let lib = Library::from_json(&text);
        assert_eq!(lib.entries.len(), MAX_LIBRARY_LEVELS);
        assert!(lib.next_id > MAX_LIBRARY_LEVELS as u32);
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod storage {
        use super::*;
        use crate::library::native::{
            load_from, read_library, save_with, ReadOutcome, SaveState, BACKUP_NAME, FILE_NAME,
        };
        use std::path::PathBuf;

        struct TempDir(PathBuf);

        impl TempDir {
            fn new(tag: &str) -> Self {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let dir = std::env::temp_dir().join(format!(
                    "kinetix-lib-test-{tag}-{}-{nanos}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                Self(dir)
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn file_round_trip_and_missing_file() {
            let tmp = TempDir::new("rt");
            let path = tmp.0.join("nested").join(FILE_NAME);
            assert_eq!(read_library(&path), ReadOutcome::Missing);
            assert_eq!(load_from(&path), Library::default());
            let mut lib = Library::default();
            lib.add("Saved".into(), code(7)).unwrap();
            let mut state = SaveState::default();
            save_with(&path, &lib, &mut state).unwrap();
            assert_eq!(load_from(&path), lib);
            assert!(!tmp.0.join("nested").join(BACKUP_NAME).exists());
        }

        #[test]
        fn corrupt_file_loads_empty_and_is_backed_up_before_first_write() {
            let tmp = TempDir::new("corrupt");
            let path = tmp.0.join(FILE_NAME);
            let bak = tmp.0.join(BACKUP_NAME);
            std::fs::write(&path, "{ not json").unwrap();
            assert_eq!(load_from(&path), Library::default());
            let mut lib = Library::default();
            lib.add("Fresh".into(), code(1)).unwrap();
            let mut state = SaveState::default();
            save_with(&path, &lib, &mut state).unwrap();
            assert_eq!(std::fs::read_to_string(&bak).unwrap(), "{ not json");
            assert_eq!(load_from(&path), lib);
        }

        #[test]
        fn save_is_blocked_when_the_backup_cannot_be_made() {
            let tmp = TempDir::new("blocked");
            let path = tmp.0.join(FILE_NAME);
            std::fs::write(&path, "garbage").unwrap();
            std::fs::create_dir_all(tmp.0.join(BACKUP_NAME).join("keep")).unwrap();
            let mut state = SaveState::default();
            let result = save_with(&path, &Library::default(), &mut state);
            assert_eq!(result, Err(SaveError::Blocked));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "garbage");
        }

        #[test]
        fn library_state_is_independent_of_progress_state() {
            let tmp = TempDir::new("indep");
            let path = tmp.0.join(FILE_NAME);
            let mut a = SaveState::default();
            let b = SaveState::default();
            save_with(&path, &Library::default(), &mut a).unwrap();
            assert!(a.prepared && !b.prepared);
        }
    }
}
