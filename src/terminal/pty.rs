use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// Wrapper cấp project quanh crate `portable-pty`.
/// Mục tiêu: không để app layer phụ thuộc trực tiếp vào API PTY bên thứ 3.
pub struct PtyProvider;

impl PtyProvider {
    pub fn new() -> Self {
        Self
    }

    pub fn spawn_shell(
        &self,
        requested_shell: Option<&str>,
        working_dir: Option<&Path>,
    ) -> Result<SpawnedShell, String> {
        let shell_program = resolve_shell_program(requested_shell);
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 30,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| format!("open pty failed: {err}"))?;

        let mut command = CommandBuilder::new(&shell_program);
        if let Some(cwd) = working_dir {
            command.cwd(cwd);
        }
        command.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| format!("spawn shell {:?} failed: {err}", shell_program))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| format!("clone pty reader failed: {err}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|err| format!("take pty writer failed: {err}"))?;

        let working_dir = if let Some(cwd) = working_dir {
            cwd.to_path_buf()
        } else {
            std::env::current_dir().map_err(|err| format!("resolve current dir failed: {err}"))?
        };

        Ok(SpawnedShell {
            shell_program,
            working_dir,
            reader,
            process: Arc::new(PtyProcess::new(pair.master, child, writer)),
        })
    }
}

pub struct SpawnedShell {
    pub shell_program: String,
    pub working_dir: PathBuf,
    pub reader: Box<dyn Read + Send>,
    pub process: Arc<PtyProcess>,
}

/// Runtime handle của một PTY session đang sống.
pub struct PtyProcess {
    _master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
}

impl PtyProcess {
    fn new(
        master: Box<dyn MasterPty + Send>,
        child: Box<dyn Child + Send>,
        writer: Box<dyn Write + Send>,
    ) -> Self {
        Self {
            _master: Mutex::new(master),
            child: Mutex::new(child),
            writer: Mutex::new(writer),
        }
    }

    pub fn write_input(&self, input: &str) -> Result<usize, String> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| "pty writer lock poisoned".to_string())?;
        writer
            .write_all(input.as_bytes())
            .map_err(|err| format!("pty write failed: {err}"))?;
        writer
            .flush()
            .map_err(|err| format!("pty flush failed: {err}"))?;
        Ok(input.len())
    }

    pub fn try_wait_status(&self) -> Result<Option<i32>, String> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| "pty child lock poisoned".to_string())?;
        let status = child
            .try_wait()
            .map_err(|err| format!("pty try_wait failed: {err}"))?;
        Ok(status.map(|exit| exit.exit_code() as i32))
    }

    pub fn close(&self) -> Result<(), String> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| "pty child lock poisoned".to_string())?;
        child
            .kill()
            .map_err(|err| format!("kill pty child failed: {err}"))?;
        Ok(())
    }
}

pub fn resolve_shell_program(requested_shell: Option<&str>) -> String {
    if let Some(shell) = requested_shell {
        let shell = shell.trim();
        if !shell.is_empty() {
            return shell.to_string();
        }
    }

    if let Ok(shell) = std::env::var("SHELL") {
        let shell = shell.trim();
        if !shell.is_empty() {
            return shell.to_string();
        }
    }

    #[cfg(target_os = "macos")]
    {
        return "/bin/zsh".to_string();
    }

    #[cfg(target_os = "windows")]
    {
        return "cmd.exe".to_string();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "/bin/bash".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_shell_program;

    #[test]
    fn resolve_shell_prefers_explicit_value() {
        assert_eq!(
            resolve_shell_program(Some("/usr/local/bin/fish")),
            "/usr/local/bin/fish".to_string()
        );
    }

    #[test]
    fn resolve_shell_never_returns_empty() {
        let resolved = resolve_shell_program(None);
        assert!(!resolved.trim().is_empty());
    }
}
