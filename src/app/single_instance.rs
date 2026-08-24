//! Single-instance routing: a second `netherize_editor` launch forwards its
//! open request (dirs/files) to the already-running instance over a unix
//! socket and exits, so the running window switches workspace / opens the
//! files instead of a duplicate process appearing in the dock.
//!
//! Protocol: one JSON line (`Vec<PathBuf>`), then a single `b'1'` ack byte.
//! `--new-instance` (or a dead/stale socket) bypasses forwarding.
// ponytail: std UnixStream + serde_json line protocol; upgrade to a real IPC
// crate only if multi-window messaging ever needs more than "open these paths".

#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

const IO_TIMEOUT: Duration = Duration::from_secs(1);

/// Env var set on the detached child so it does not re-exec itself again
/// (which would loop forever).
const DETACHED_CHILD_ENV: &str = "NETHERIZE_DETACHED_CHILD";

/// Pure decision helper — kept separate so tests cover every branch without
/// spawning processes.
fn should_reexec_detached(already_detached: bool, launched_from_terminal: bool) -> bool {
    !already_detached && launched_from_terminal
}

fn launched_from_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Terminal-launch convenience: when started from an interactive shell,
/// re-exec ourselves as a detached child (own process group, stdio
/// disconnected) and report success so the caller can exit immediately and
/// hand the terminal back — VS Code-style CLI behavior. The detached child
/// runs the normal startup path (single-instance forwarding included), so a
/// second `netherize <dir>` still routes into the live instance.
///
/// Returns false when already detached (env guard), not launched from a
/// terminal (dock/Finder launch — nothing to free), or spawn failed; the
/// caller then continues in the foreground as before.
pub fn reexec_detached_from_terminal() -> bool {
    let already_detached = std::env::var_os(DETACHED_CHILD_ENV).is_some();
    if !should_reexec_detached(already_detached, launched_from_terminal()) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut command = std::process::Command::new(exe);
    command.args(std::env::args_os().skip(1));
    command.env(DETACHED_CHILD_ENV, "1");
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        // Own process group: closing the terminal window sends SIGHUP to the
        // shell's foreground group; the editor must not be in it.
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    match command.spawn() {
        Ok(child) => {
            println!("[netherize] launched detached (pid {})", child.id());
            true
        }
        Err(err) => {
            eprintln!("[netherize] detach failed ({err}); continuing in foreground");
            false
        }
    }
}

/// Socket lives next to state.toml so it is per-user and predictable.
pub fn default_socket_path() -> PathBuf {
    crate::config::paths::user_config_root().join("instance.sock")
}

/// Canonicalized CLI paths this launch wants opened (dirs and files).
pub fn cli_open_paths() -> Vec<PathBuf> {
    std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter_map(|p| p.canonicalize().ok())
        .filter(|p| p.is_dir() || p.is_file())
        .collect()
}

/// Try to hand `paths` to a live instance listening on `sock`.
/// Returns true only when the running instance acknowledged the request.
pub fn try_forward_at(sock: &Path, paths: &[PathBuf]) -> bool {
    let Ok(stream) = UnixStream::connect(sock) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let Ok(line) = serde_json::to_string(paths) else {
        return false;
    };
    let mut stream = stream;
    if stream.write_all(line.as_bytes()).is_err() || stream.write_all(b"\n").is_err() {
        return false;
    }
    let mut ack = [0u8; 1];
    stream.read_exact(&mut ack).is_ok() && ack[0] == b'1'
}

/// Bind the instance socket. A connect-probe distinguishes "another live
/// instance owns this" (None) from a stale file left by a crash (reclaimed).
pub fn bind_at(sock: &Path) -> Option<UnixListener> {
    if sock.exists() {
        if UnixStream::connect(sock).is_ok() {
            return None; // live sibling instance
        }
        let _ = std::fs::remove_file(sock);
    }
    if let Some(parent) = sock.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    UnixListener::bind(sock).ok()
}

/// Accept loop on a background thread; each valid message invokes `on_open`.
pub fn spawn_listener(listener: UnixListener, on_open: impl Fn(Vec<PathBuf>) + Send + 'static) {
    std::thread::Builder::new()
        .name("netherize-single-instance".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let Ok(paths) = serde_json::from_str::<Vec<PathBuf>>(line.trim()) else {
                    continue;
                };
                let mut stream = reader.into_inner();
                let _ = stream.write_all(b"1");
                on_open(paths);
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn scratch_sock(tag: &str) -> PathBuf {
        // Unix socket paths are capped (~104 bytes on macOS) — keep them short.
        std::env::temp_dir().join(format!("nz-{}-{}.sock", tag, std::process::id()))
    }

    #[test]
    fn reexec_decision_covers_all_branches() {
        // Fresh launch from an interactive terminal → detach.
        assert!(should_reexec_detached(false, true));
        // Dock/Finder launch (no controlling tty) → stay foreground.
        assert!(!should_reexec_detached(false, false));
        // Detached child re-launching itself → never re-exec again.
        assert!(!should_reexec_detached(true, true));
        assert!(!should_reexec_detached(true, false));
    }

    #[test]
    fn forward_delivers_paths_to_listener_callback() {
        let sock = scratch_sock("fwd");
        let _ = std::fs::remove_file(&sock);
        let listener = bind_at(&sock).expect("bind fresh socket");
        let (tx, rx) = mpsc::channel();
        spawn_listener(listener, move |paths| {
            let _ = tx.send(paths);
        });

        let sent = vec![PathBuf::from("/tmp/netherize-remote-open")];
        assert!(try_forward_at(&sock, &sent), "forward should be acked");
        let got = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("listener callback fired");
        assert_eq!(got, sent);
        let _ = std::fs::remove_file(&sock);
    }

    #[test]
    fn stale_socket_file_is_reclaimed_by_bind() {
        let sock = scratch_sock("stale");
        let _ = std::fs::remove_file(&sock);
        drop(bind_at(&sock).expect("first bind")); // listener dropped → stale file remains
        assert!(sock.exists(), "stale socket file left behind");

        assert!(
            !try_forward_at(&sock, &[]),
            "no listener → forward must fail"
        );
        assert!(
            bind_at(&sock).is_some(),
            "stale file must be reclaimed and re-bound"
        );
        let _ = std::fs::remove_file(&sock);
    }

    #[test]
    fn bind_refuses_when_live_instance_owns_socket() {
        let sock = scratch_sock("live");
        let _ = std::fs::remove_file(&sock);
        let _keep_alive = bind_at(&sock).expect("first bind");
        assert!(
            bind_at(&sock).is_none(),
            "second bind must yield to the live instance"
        );
        let _ = std::fs::remove_file(&sock);
    }
}
