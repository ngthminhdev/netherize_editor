//! Install/uninstall the `netherize` shell command (VS Code's "Shell Command:
//! Install 'code' command in PATH").
//!
//! The command is a **symlink to the running executable**, never a copy — a
//! copy freezes the version at install time (the old `install.sh` bug that made
//! `netherize_editor` in PATH launch a stale build). A symlink into the app
//! bundle always launches the version the dock icon launches.
#![cfg(unix)]

use std::path::{Path, PathBuf};

pub const CLI_NAME: &str = "netherize";
/// Legacy name shipped by scripts/install.sh as a stale *copy*; re-pointed to a
/// symlink when present so old muscle memory also opens the current build.
pub const LEGACY_CLI_NAME: &str = "netherize_editor";

#[derive(Debug, PartialEq, Eq)]
pub struct InstallOutcome {
    /// Where the `netherize` symlink landed.
    pub installed_at: PathBuf,
    /// Legacy `netherize_editor` copies that were re-pointed to the live exe.
    pub repointed_legacy: Vec<PathBuf>,
    /// True when the install dir is likely missing from `$PATH`.
    pub path_hint_needed: bool,
}

/// Candidate bin dirs, best first. `/usr/local/bin` is on the default macOS
/// PATH; `~/.local/bin` needs a shell-profile line but never needs sudo.
pub fn install_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/local/bin")];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    dirs
}

fn dir_is_in_path(dir: &Path) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|entry| entry == dir))
        .unwrap_or(false)
}

/// Replace whatever sits at `dir/name` with a symlink to `exe`.
pub fn install_symlink_at(dir: &Path, name: &str, exe: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let link = dir.join(name);
    match std::fs::symlink_metadata(&link) {
        Ok(_) => std::fs::remove_file(&link)?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    std::os::unix::fs::symlink(exe, &link)?;
    Ok(link)
}

/// Escalation fallback for a root-owned `/usr/local/bin` (Apple Silicon): the
/// standard macOS GUI admin prompt via osascript, exactly what VS Code does.
fn install_with_admin_prompt(dir: &Path, exe: &Path) -> Result<PathBuf, String> {
    let link = dir.join(CLI_NAME);
    // Single-quote for the inner sh, then escape for the AppleScript string.
    let sh = format!(
        "mkdir -p '{dir}' && ln -sf '{exe}' '{link}'",
        dir = dir.display(),
        exe = exe.display(),
        link = link.display()
    );
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        sh.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let output = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|err| format!("osascript failed to start: {err}"))?;
    if output.status.success() {
        Ok(link)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Install `netherize` into PATH, pointing at `exe` (the running binary).
/// Order: /usr/local/bin without sudo → ~/.local/bin → /usr/local/bin with the
/// GUI admin prompt. Also re-points any stale legacy `netherize_editor` copy.
pub fn install_cli(exe: &Path) -> Result<InstallOutcome, String> {
    let exe = exe
        .canonicalize()
        .map_err(|err| format!("cannot resolve executable path: {err}"))?;
    let dirs = install_dirs();

    let mut installed_at = None;
    for dir in &dirs {
        if let Ok(link) = install_symlink_at(dir, CLI_NAME, &exe) {
            installed_at = Some(link);
            break;
        }
    }
    let installed_at = match installed_at {
        Some(link) => link,
        None => install_with_admin_prompt(&dirs[0], &exe)
            .map_err(|err| format!("install failed (admin prompt): {err}"))?,
    };

    // Legacy stale copies: only re-point where one already exists.
    let mut repointed_legacy = Vec::new();
    for dir in &dirs {
        let legacy = dir.join(LEGACY_CLI_NAME);
        if legacy.symlink_metadata().is_ok()
            && legacy != installed_at
            && install_symlink_at(dir, LEGACY_CLI_NAME, &exe).is_ok()
        {
            repointed_legacy.push(legacy);
        }
    }

    let installed_dir = installed_at
        .parent()
        .unwrap_or(Path::new("/"))
        .to_path_buf();
    Ok(InstallOutcome {
        path_hint_needed: !dir_is_in_path(&installed_dir),
        installed_at,
        repointed_legacy,
    })
}

/// Remove the `netherize` symlinks we own. Regular files are left alone.
pub fn uninstall_cli() -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for dir in install_dirs() {
        let link = dir.join(CLI_NAME);
        if link
            .symlink_metadata()
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false)
            && std::fs::remove_file(&link).is_ok()
        {
            removed.push(link);
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_cli_{tag}_{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn fake_exe(dir: &Path) -> PathBuf {
        let exe = dir.join("netherize_editor_bin");
        std::fs::write(&exe, b"#!/bin/sh\n").expect("write exe");
        exe
    }

    #[test]
    fn install_symlink_creates_link_pointing_at_exe() {
        let root = temp_dir("create");
        let exe = fake_exe(&root);
        let bin = root.join("bin");

        let link = install_symlink_at(&bin, CLI_NAME, &exe).expect("install");

        assert_eq!(link, bin.join(CLI_NAME));
        assert_eq!(std::fs::read_link(&link).expect("read link"), exe);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn install_symlink_replaces_stale_regular_file_copy() {
        let root = temp_dir("stale");
        let exe = fake_exe(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir");
        let stale = bin.join(CLI_NAME);
        std::fs::write(&stale, b"old frozen binary").expect("write stale copy");

        let link = install_symlink_at(&bin, CLI_NAME, &exe).expect("install over copy");

        assert!(
            link.symlink_metadata()
                .expect("meta")
                .file_type()
                .is_symlink(),
            "stale copy must become a symlink"
        );
        assert_eq!(std::fs::read_link(&link).expect("read link"), exe);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn install_symlink_overwrites_old_symlink_target() {
        let root = temp_dir("overwrite");
        let old_exe = fake_exe(&root);
        let new_exe = root.join("new_bin");
        std::fs::write(&new_exe, b"#!/bin/sh\n").expect("write new exe");
        let bin = root.join("bin");
        install_symlink_at(&bin, CLI_NAME, &old_exe).expect("first install");

        let link = install_symlink_at(&bin, CLI_NAME, &new_exe).expect("re-install");

        assert_eq!(std::fs::read_link(&link).expect("read link"), new_exe);
        let _ = std::fs::remove_dir_all(root);
    }
}
