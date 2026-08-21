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
