use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, Once, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use ropey::Rope;
use serde::{Deserialize, Serialize};

use crate::config::paths::{legacy_app_state_root, user_config_root};

static ATOMIC_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PANIC_RECOVERY_SNAPSHOT: OnceLock<Mutex<Vec<RecoveryBuffer>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub(crate) struct RecoveryBuffer {
    pub path: PathBuf,
    pub text: Rope,
}

#[cfg(not(windows))]
fn replace_file_atomically(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file_atomically(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Durably replace a file without ever exposing a truncated destination.
/// The sibling temporary file keeps `rename` on the same filesystem.
pub(crate) fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
        _ => path.to_path_buf(),
    };
    let existing_metadata = fs::metadata(&target).ok();
    if existing_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.permissions().readonly())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("destination is read-only: {}", target.display()),
        ));
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("netherize");
    let sequence = ATOMIC_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let temp_path = parent.join(format!(
        ".{file_name}.netherize-tmp-{}-{sequence}-{nonce}",
        std::process::id()
    ));

    let result = (|| {
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        if let Some(metadata) = &existing_metadata {
            fs::set_permissions(&temp_path, metadata.permissions())?;
        }
        temp.write_all(contents.as_ref())?;
        temp.sync_all()?;
        drop(temp);
        replace_file_atomically(&temp_path, &target)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn recovery_file_name(index: usize, source: &Path, nonce: u128) -> String {
    use std::hash::{Hash, Hasher};

    let raw = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("untitled");
    let safe: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let mut path_hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut path_hasher);
    let path_hash = path_hasher.finish();
    format!("{nonce}-{index}-{path_hash:016x}-{safe}.recovery")
}

pub(crate) fn write_recovery_snapshot_to(
    directory: &Path,
    buffers: &[RecoveryBuffer],
) -> io::Result<Vec<PathBuf>> {
    fs::create_dir_all(directory)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut written = Vec::with_capacity(buffers.len());
    for (index, buffer) in buffers.iter().enumerate() {
        let path = directory.join(recovery_file_name(index, &buffer.path, nonce));
        atomic_write(&path, buffer.text.to_string())?;
        written.push(path);
    }
    Ok(written)
}

pub(crate) fn replace_panic_recovery_snapshot(buffers: Vec<RecoveryBuffer>) {
    let snapshot = PANIC_RECOVERY_SNAPSHOT.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut current) = snapshot.lock() {
        *current = buffers;
    }
}

pub(crate) fn install_panic_recovery_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let buffers = PANIC_RECOVERY_SNAPSHOT
                .get()
                .and_then(|snapshot| snapshot.try_lock().ok().map(|buffers| buffers.clone()))
                .unwrap_or_default();
            if !buffers.is_empty() {
                let directory = user_config_root().join("recovery");
                match write_recovery_snapshot_to(&directory, &buffers) {
                    Ok(paths) => eprintln!(
                        "[panic-recovery] saved {} dirty buffer(s) under {}",
                        paths.len(),
                        directory.display()
                    ),
                    Err(err) => eprintln!("[panic-recovery] snapshot failed: {err}"),
                }
            }
            previous(info);
        }));
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

impl WindowGeometry {
    const MIN_DIMENSION: u32 = 160;

    pub fn is_sane(self) -> bool {
        self.width >= Self::MIN_DIMENSION && self.height >= Self::MIN_DIMENSION
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppPersistentState {
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    #[serde(default)]
    pub recent_project_meta: HashMap<PathBuf, RecentProjectMeta>,
    #[serde(default)]
    pub theme_profile: Option<String>,
    /// True after the one-time first-run key-hint toast was shown.
    #[serde(default)]
    pub first_run_tour_shown: bool,
    /// Language keys for the "New LeetCode File" picker, most-recently-used
    /// first. Lets the picker surface the language you reached for last.
    #[serde(default)]
    pub recent_leetcode_languages: Vec<String>,
    /// Agent ids for the AI Chat agent picker, most-recently-used first.
    #[serde(default)]
    pub recent_ai_agents: Vec<String>,
    /// Last non-maximized window frame plus the last zoom state. Kept out of
    /// `ui.toml`: that file describes defaults, while this is per-machine state.
    #[serde(default)]
    pub window_geometry: Option<WindowGeometry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentProjectMeta {
    #[serde(default)]
    pub icon_source: Option<String>,
    #[serde(default)]
    pub last_opened_unix_secs: Option<u64>,
}

impl AppPersistentState {
    fn state_dir() -> PathBuf {
        user_config_root()
    }

    fn legacy_state_dir() -> PathBuf {
        legacy_app_state_root()
    }

    fn state_path() -> PathBuf {
        Self::state_dir().join("state.toml")
    }

    fn legacy_state_path() -> PathBuf {
        Self::legacy_state_dir().join("state.toml")
    }

    fn load_from_path(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        toml::from_str::<Self>(&text).ok()
    }

    pub fn load() -> Self {
        let path = Self::state_path();
        if let Some(state) = Self::load_from_path(&path) {
            return state;
        }

        let legacy_path = Self::legacy_state_path();
        if let Some(state) = Self::load_from_path(&legacy_path) {
            state.save();
            return state;
        }

        Self::default()
    }

    pub fn save(&self) {
        let path = Self::state_path();
        if let Some(parent) = path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            eprintln!("[AppPersistentState] create dir failed: {err}");
            return;
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(err) = atomic_write(&path, text) {
                    eprintln!("[AppPersistentState] write failed: {err}");
                }
            }
            Err(err) => {
                eprintln!("[AppPersistentState] serialize failed: {err}");
            }
        }
    }

    pub fn push_recent(&mut self, path: PathBuf) {
        self.push_recent_with_icon(path, None);
    }

    pub fn push_recent_with_icon(&mut self, path: PathBuf, icon_source: Option<String>) {
        self.recent_projects.retain(|p| p != &path);
        self.recent_projects.insert(0, path.clone());
        let keep: std::collections::HashSet<_> = self.recent_projects.iter().cloned().collect();
        self.recent_project_meta
            .retain(|path, _| keep.contains(path));
        let last_opened_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs());
        self.recent_project_meta.insert(
            path,
            RecentProjectMeta {
                icon_source,
                last_opened_unix_secs,
            },
        );
    }

    pub fn remove_recent(&mut self, path: &Path) -> bool {
        let before_len = self.recent_projects.len();
        self.recent_projects.retain(|recent| recent != path);
        self.recent_project_meta.remove(path);
        before_len != self.recent_projects.len()
    }

    /// Record `language_key` as the most-recently-used LeetCode language,
    /// moving it to the front of the MRU list (deduped, capped).
    pub fn push_recent_leetcode_language(&mut self, language_key: &str) {
        const MAX_RECENT_LANGUAGES: usize = 12;
        self.recent_leetcode_languages
            .retain(|key| key != language_key);
        self.recent_leetcode_languages
            .insert(0, language_key.to_string());
        self.recent_leetcode_languages
            .truncate(MAX_RECENT_LANGUAGES);
    }

    /// Record `agent_id` as the most-recently-used AI agent (front, dedup, cap).
    pub fn push_recent_ai_agent(&mut self, agent_id: &str) {
        const MAX_RECENT_AGENTS: usize = 12;
        self.recent_ai_agents.retain(|id| id != agent_id);
        self.recent_ai_agents.insert(0, agent_id.to_string());
        self.recent_ai_agents.truncate(MAX_RECENT_AGENTS);
    }

    pub fn infer_project_icon_source(path: &Path) -> String {
        const MARKERS: &[&str] = &[
            "Cargo.toml",
            "package.json",
            "go.mod",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "gradlew",
            "flake.nix",
            "default.nix",
            "deno.json",
            "tsconfig.json",
            "pyproject.toml",
            "requirements.txt",
            "build.zig",
            "CMakeLists.txt",
            "Makefile",
            "README.md",
        ];
        for marker in MARKERS {
            let candidate = path.join(marker);
            if candidate.exists() {
                return candidate.display().to_string();
            }
        }
        path.display().to_string()
    }

    pub fn most_recent_existing(&self) -> Option<PathBuf> {
        self.recent_projects
            .iter()
            .find(|p| p.exists() && p.is_dir())
            .cloned()
    }

    pub fn configured_theme_profile(&self) -> Option<&str> {
        self.theme_profile
            .as_deref()
            .map(str::trim)
            .filter(|profile| !profile.is_empty())
    }

    pub fn set_theme_profile(&mut self, theme_profile: Option<String>) {
        self.theme_profile = theme_profile
            .map(|profile| profile.trim().to_string())
            .filter(|profile| !profile.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_existing_file_without_leaving_temp_files() {
        let dir = std::env::temp_dir().join(format!(
            "netherize-atomic-write-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("create atomic write test dir");
        let path = dir.join("state.toml");
        std::fs::write(&path, b"old").expect("seed destination");

        atomic_write(&path, b"new").expect("atomic replace");

        assert_eq!(std::fs::read(&path).expect("read destination"), b"new");
        assert_eq!(
            std::fs::read_dir(&dir)
                .expect("read test dir")
                .filter_map(Result::ok)
                .count(),
            1,
            "the temporary sibling must be removed after rename"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_atomic_replace_keeps_destination_and_cleans_temporary_sibling() {
        let dir = std::env::temp_dir().join(format!(
            "netherize-atomic-failure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        let destination = dir.join("existing-directory");
        std::fs::create_dir_all(&destination).expect("create destination directory");

        assert!(atomic_write(&destination, b"must fail").is_err());

        assert!(destination.is_dir());
        assert_eq!(
            std::fs::read_dir(&dir)
                .expect("read test dir")
                .filter_map(Result::ok)
                .count(),
            1,
            "failed replacement must clean its temporary sibling"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn atomic_write_refuses_to_replace_an_explicitly_read_only_file() {
        let dir = std::env::temp_dir().join(format!(
            "netherize-atomic-readonly-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("create readonly test dir");
        let path = dir.join("readonly.txt");
        std::fs::write(&path, "old").expect("seed readonly file");
        let original_permissions = std::fs::metadata(&path)
            .expect("inspect original readonly file")
            .permissions();
        let mut permissions = std::fs::metadata(&path)
            .expect("inspect readonly file")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).expect("make file readonly");

        assert!(atomic_write(&path, b"new").is_err());
        assert_eq!(
            std::fs::read_to_string(&path).expect("read original"),
            "old"
        );

        let _ = std::fs::set_permissions(&path, original_permissions);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_updates_a_symlink_target_without_replacing_the_link() {
        let dir = std::env::temp_dir().join(format!(
            "netherize-atomic-symlink-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("create symlink test dir");
        let target = dir.join("target.txt");
        let link = dir.join("link.txt");
        std::fs::write(&target, "old").expect("seed symlink target");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");

        atomic_write(&link, b"new").expect("write through symlink");

        assert!(
            std::fs::symlink_metadata(&link)
                .expect("inspect link")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).expect("read target"),
            "new"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn panic_recovery_writes_each_dirty_buffer_to_a_durable_sibling() {
        let dir = std::env::temp_dir().join(format!(
            "netherize-recovery-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        let buffers = vec![
            RecoveryBuffer {
                path: PathBuf::from("/project/src/main.rs"),
                text: Rope::from_str("fn main() {}\n"),
            },
            RecoveryBuffer {
                path: PathBuf::from("/project/notes todo.md"),
                text: Rope::from_str("unsaved\n"),
            },
        ];

        let written = write_recovery_snapshot_to(&dir, &buffers).expect("write recovery files");

        assert_eq!(written.len(), 2);
        assert_eq!(
            std::fs::read_to_string(&written[0]).expect("read first"),
            buffers[0].text.to_string()
        );
        assert_eq!(
            std::fs::read_to_string(&written[1]).expect("read second"),
            buffers[1].text.to_string()
        );
        assert!(written.iter().all(|path| path.starts_with(&dir)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn window_geometry_rejects_degenerate_sizes() {
        assert!(
            !WindowGeometry {
                x: 10,
                y: 20,
                width: 0,
                height: 800,
                maximized: false,
            }
            .is_sane()
        );
        assert!(
            WindowGeometry {
                x: 10,
                y: 20,
                width: 1280,
                height: 800,
                maximized: true,
            }
            .is_sane()
        );
    }

    #[test]
    fn legacy_persistent_state_without_window_geometry_still_deserializes() {
        let state: AppPersistentState =
            toml::from_str("recent_projects = []\nfirst_run_tour_shown = true\n")
                .expect("deserialize legacy state");

        assert!(state.window_geometry.is_none());
        assert!(state.first_run_tour_shown);
    }

    #[test]
    fn leetcode_language_mru_moves_to_front_and_dedups() {
        let mut state = AppPersistentState::default();
        state.push_recent_leetcode_language("python");
        state.push_recent_leetcode_language("go");
        state.push_recent_leetcode_language("python"); // re-use floats to front
        assert_eq!(state.recent_leetcode_languages, vec!["python", "go"]);
    }

    #[test]
    fn leetcode_language_mru_is_capped() {
        let mut state = AppPersistentState::default();
        for i in 0..20 {
            state.push_recent_leetcode_language(&format!("lang{i}"));
        }
        assert_eq!(state.recent_leetcode_languages.len(), 12);
        // Most recent stays at the front.
        assert_eq!(state.recent_leetcode_languages[0], "lang19");
    }

    #[test]
    fn remove_recent_drops_path_and_metadata() {
        let project_a = PathBuf::from("/tmp/netherize-project-a");
        let project_b = PathBuf::from("/tmp/netherize-project-b");
        let mut state = AppPersistentState::default();

        state.push_recent_with_icon(project_a.clone(), Some("a-icon".to_string()));
        state.push_recent_with_icon(project_b.clone(), Some("b-icon".to_string()));

        assert!(state.remove_recent(&project_a));
        assert_eq!(state.recent_projects, vec![project_b.clone()]);
        assert!(!state.recent_project_meta.contains_key(&project_a));
        assert!(state.recent_project_meta.contains_key(&project_b));
        assert!(!state.remove_recent(&project_a));
    }
}
