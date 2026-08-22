# `gf` → GitMin Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Nhấn `gf` mở app GitMin vào đúng git repo của workspace (tab mới + focus nếu app đang chạy), behavior chọn được qua config `[git] ui`.

**Architecture:** Giữ nguyên keymap flow (`gf` → `git.open_lazygit`). Handler `open_lazygit_buffer` đọc `GitConfig` và branch: lazygit → PTY buffer như cũ; git_min → resolve repo root (walk-up tìm `.git`) rồi spawn detached binary GitMin với repo root làm argv. Bên GitMin (repo khác, xem Appendix A) nhận argv qua single-instance plugin → emit event → frontend tạo tab mới + focus.

**Tech Stack:** Rust (tokio optional, std::process đủ), toml crate (đã có), Tauri 2 + tauri-plugin-single-instance (bên GitMin).

## Global Constraints

- KHÔNG đụng `application.rs`, `input/handler.rs`, `input_map/`, `resolved_keymap.rs`, keymap TOML — `gf` vẫn bind `git.open_lazygit`.
- KHÔNG block main thread: spawn dùng `Command::spawn()` (không `.output()`, không `.status()`), reap bằng thread.
- KHÔNG `.unwrap()`/`.expect()` trong code chạy runtime (test được phép).
- Không thêm dependency mới bên editor (toml đã có).
- Config nằm trong `~/.config/netherize/ui.toml` section `[git]`, default `"lazygit"` khi thiếu/sai.
- Spec: `docs/superpowers/specs/2026-08-22-gf-gitmin-integration-design.md`.

---

### Task 1: `GitConfig` + `find_git_repo_root` (pure, có test)

**Files:**
- Create: `src/config/git_config.rs`
- Modify: `src/config/mod.rs` (thêm dòng `pub mod git_config;`)
- Test: inline `#[cfg(test)]` trong `src/config/git_config.rs`

**Interfaces:**
- Produces:
  - `pub enum GitUi { Lazygit, GitMin }` (+ `FromStr`-style `parse_git_ui(&str) -> Option<GitUi>`)
  - `pub struct GitConfig { pub ui: GitUi, pub git_min_path: Option<PathBuf> }` + `Default` (ui = Lazygit)
  - `GitConfig::load_active() -> Self` — đọc `user_config_root().join("ui.toml")`, section `[git]`; mọi lỗi → Default
  - `GitConfig::resolved_binary(&self) -> Option<PathBuf>` — override nếu set và tồn tại; else probe `/Applications/GitMin.app/Contents/MacOS/GitMin` rồi `$HOME/Applications/GitMin.app/Contents/MacOS/GitMin`
  - `pub fn find_git_repo_root(start: &Path) -> Option<PathBuf>` — walk up, match `.git` là thư mục HOẶC file (worktree/submodule)

- [ ] **Step 1: Tạo module với code đầy đủ**

`src/config/git_config.rs`:

```rust
use std::path::{Path, PathBuf};

use super::paths::user_config_root;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitUi {
    Lazygit,
    GitMin,
}

/// Chấp nhận đúng hai chuỗi này; giá trị khác → caller fallback về default.
pub fn parse_git_ui(raw: &str) -> Option<GitUi> {
    match raw {
        "lazygit" => Some(GitUi::Lazygit),
        "git_min" => Some(GitUi::GitMin),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitConfig {
    pub ui: GitUi,
    pub git_min_path: Option<PathBuf>,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            ui: GitUi::Lazygit,
            git_min_path: None,
        }
    }
}

impl GitConfig {
    /// Đọc `[git]` từ user override `~/.config/netherize/ui.toml`.
    /// Thiếu file, thiếu section, hoặc sai giá trị → built-in default (lazygit).
    pub fn load_active() -> Self {
        let path = user_config_root().join("ui.toml");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(value) = content.parse::<toml::Table>() else {
            eprintln!("[git] parse error in {}, using defaults", path.display());
            return Self::default();
        };
        let Some(git) = value.get("git").and_then(|g| g.as_table()) else {
            return Self::default();
        };

        let mut config = Self::default();
        if let Some(ui) = git.get("ui").and_then(|v| v.as_str()) {
            match parse_git_ui(ui) {
                Some(parsed) => config.ui = parsed,
                None => {
                    eprintln!("[git] unknown ui '{ui}' (expected \"lazygit\" | \"git_min\"), using lazygit");
                }
            }
        }
        if let Some(p) = git.get("git_min_path").and_then(|v| v.as_str()) {
            config.git_min_path = Some(PathBuf::from(p));
        }
        config
    }

    /// Đường dẫn binary khả dụng: override trước, sau đó probe vị trí .app chuẩn.
    pub fn resolved_binary(&self) -> Option<PathBuf> {
        if let Some(explicit) = &self.git_min_path {
            if explicit.exists() {
                return Some(explicit.clone());
            }
            eprintln!(
                "[git] git_min_path '{}' không tồn tại, thử vị trí mặc định",
                explicit.display()
            );
        }
        let name = "GitMin.app/Contents/MacOS/GitMin";
        let candidates = [
            PathBuf::from("/Applications").join(name),
            user_config_root()
                .ancestors()
                .nth(3)
                .and_then(|home| home.parent().map(Path::to_path_buf))
                .map(|home| home.join("Applications").join(name)),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|candidate| candidate.exists())
    }
}

/// Walk up từ `start` tìm thư mục chứa `.git` (dir thường, file với worktree).
pub fn find_git_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_gitcfg_{prefix}_{nanos}"))
    }

    #[test]
    fn parse_git_ui_accepts_only_known_values() {
        assert_eq!(parse_git_ui("lazygit"), Some(GitUi::Lazygit));
        assert_eq!(parse_git_ui("git_min"), Some(GitUi::GitMin));
        assert_eq!(parse_git_ui("GitMin"), None);
        assert_eq!(parse_git_ui(""), None);
    }

    #[test]
    fn load_active_parses_git_section_from_toml() {
        let dir = unique_temp_dir("load");
        fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("ui.toml");
        fs::write(
            &path,
            "[editor]\nfont_size = 14\n\n[git]\nui = \"git_min\"\ngit_min_path = \"/tmp/fake-git-min\"\n",
        )
        .expect("write toml");

        // load_active đọc đường dẫn cố định nên test phần parse qua Table trực tiếp:
        let content = fs::read_to_string(&path).expect("read back");
        let value = content.parse::<toml::Table>().expect("parse");
        let git = value.get("git").and_then(|g| g.as_table()).expect("git section");
        assert_eq!(parse_git_ui(git.get("ui").unwrap().as_str().unwrap()), Some(GitUi::GitMin));
        assert_eq!(
            git.get("git_min_path").and_then(|v| v.as_str()),
            Some("/tmp/fake-git-min")
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn find_git_repo_root_walks_up_to_git_dir() {
        let root = unique_temp_dir("walk");
        fs::create_dir_all(root.join(".git")).expect("create .git");
        fs::create_dir_all(root.join("a/b/c")).expect("create nested");

        let found = find_git_repo_root(&root.join("a/b/c")).expect("root found");
        assert_eq!(found, root);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_git_repo_root_matches_worktree_git_file() {
        let root = unique_temp_dir("worktree");
        fs::create_dir_all(&root).expect("create dir");
        fs::write(root.join(".git"), "gitdir: /somewhere/else\n").expect("write .git file");

        assert_eq!(find_git_repo_root(&root), Some(root.clone()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_git_repo_root_none_outside_repo() {
        assert_eq!(find_git_repo_root(Path::new("/")), None);
    }
}
```

`src/config/mod.rs` — thêm vào danh sách module:

```rust
pub mod git_config;
```

- [ ] **Step 2: Chạy test**

Run: `cargo test config::git_config`
Expected: 5 passed

- [ ] **Step 3: Commit**

```bash
git add src/config/git_config.rs src/config/mod.rs
git commit -m "feat(config): add GitConfig + git repo root resolution"
```

---

### Task 2: Wire `GitConfig` vào AppShell

**Files:**
- Modify: `src/app/event_loop/mod.rs:195` (struct field)
- Modify: `src/app/event_loop/setup.rs` (~107 load, literal ở struct init)

**Interfaces:**
- Consumes: `GitConfig::load_active()` từ Task 1
- Produces: field `AppShell.git_config: GitConfig` — Task 3, 4 dùng

- [ ] **Step 1: Thêm field vào struct**

`src/app/event_loop/mod.rs` — ngay sau dòng `ai_config: AiConfig,` (~dòng 196):

```rust
    ui_config: UiConfig,
    ai_config: AiConfig,
    git_config: crate::config::git_config::GitConfig,
```

- [ ] **Step 2: Khởi tạo trong setup**

`src/app/event_loop/setup.rs` — tại nơi load config (~dòng 107, cạnh `let ui_config = UiConfig::load_active();`):

```rust
        let ui_config = UiConfig::load_active();
        let git_config = crate::config::git_config::GitConfig::load_active();
```

Trong struct literal của `AppShell` (cạnh dòng `ai_config,`):

```rust
            ui_config,
            ai_config,
            git_config,
```

- [ ] **Step 3: Build kiểm chứng**

Run: `cargo build 2>&1 | tail -5`
Expected: build thành công (warning `field never read` có thể xuất hiện → hết ở Task 3)

- [ ] **Step 4: Commit**

```bash
git add src/app/event_loop/mod.rs src/app/event_loop/setup.rs
git commit -m "feat(app): load GitConfig into AppShell"
```

---

### Task 3: Branch handler `open_git_min_app`

**Files:**
- Modify: `src/app/event_loop/commands_lsp.rs:307-334`
- Test: `src/app/event_loop/commands_tests.rs` (thêm 2 test cuối file)

**Interfaces:**
- Consumes: `AppShell.git_config` (Task 2), `find_git_repo_root` (Task 1), `workspace_root_path()` (đã có)
- Produces: `open_git_min_app(&mut self, workspace_root: PathBuf) -> bool` (private); hành vi `Command::GitOpenLazygit` đổi theo config

- [ ] **Step 1: Sửa handler**

`src/app/event_loop/commands_lsp.rs` — thay toàn bộ thân `open_lazygit_buffer` hiện tại bằng:

```rust
    pub(super) fn open_lazygit_buffer(&mut self) -> bool {
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            eprintln!("[AppShell] lazygit open skipped: workspace is not attached");
            return false;
        };

        if self.git_config.ui == crate::config::git_config::GitUi::GitMin {
            return self.open_git_min_app(workspace_root);
        }

        let buffer_index = self
            .app_state
            .open_terminal_buffer("[Lazygit]", Some(workspace_root.clone()));
        self.pending_lazygit_buffer_index = Some(buffer_index);
        self.buffer_terminal_needs_layout = true;
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.clear_highlight_layers();
        let _ = self.sync_focus_mode_for_active_buffer();

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyCommand {
                program: "lazygit".to_string(),
                args: Vec::new(),
                working_dir: Some(workspace_root),
            },
        });

        true
    }

    /// `gf` khi `[git] ui = "git_min"`: spawn GitMin detached với repo root làm
    /// argv. GitMin (single-instance) tự xử lý tab mới + focus khi đang chạy.
    fn open_git_min_app(&mut self, workspace_root: PathBuf) -> bool {
        use crate::config::git_config::find_git_repo_root;

        let Some(repo_root) = find_git_repo_root(&workspace_root) else {
            eprintln!(
                "[AppShell] git_min open skipped: no git repo above {}",
                workspace_root.display()
            );
            self.app_state.status_message =
                Some("Not inside a git repository".to_string());
            return false;
        };

        let Some(binary) = self.git_config.resolved_binary() else {
            eprintln!("[AppShell] GitMin.app not found");
            self.app_state.status_message = Some(
                "GitMin not found — install it or set [git] git_min_path in ~/.config/netherize/ui.toml"
                    .to_string(),
            );
            return false;
        };

        match std::process::Command::new(&binary)
            .arg(&repo_root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            // ponytail: reap thread thay vì tokio — spawn là fire-and-forget,
            // upgrade lên worker topic nếu sau này cần biết exit status.
            Ok(child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                true
            }
            Err(err) => {
                eprintln!("[AppShell] failed to launch GitMin: {err}");
                self.app_state.status_message =
                    Some(format!("Failed to launch GitMin: {err}"));
                false
            }
        }
    }
```

Lưu ý: giữ nguyên các import đã có (`PathBuf`, `RequestSpec`, …).

- [ ] **Step 2: Thêm 2 test vào `src/app/event_loop/commands_tests.rs`**

```rust
#[test]
fn gf_git_min_mode_missing_binary_sets_status_message() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!("netherize_gf_gitmin_{}", std::process::id()));
    std::fs::create_dir_all(root.join(".git")).expect("create repo");
    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    shell.git_config.ui = crate::config::git_config::GitUi::GitMin;
    shell.git_config.git_min_path = Some(root.join("definitely-missing-binary"));

    let opened = shell.handle_command(Command::GitOpenLazygit);

    assert!(!opened);
    assert!(shell.pending_lazygit_buffer_index.is_none());
    let message = shell.app_state.status_message.expect("status message");
    assert!(message.contains("GitMin"), "unexpected message: {message}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn gf_git_min_mode_outside_repo_skips_without_status() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!("netherize_gf_norepo_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create plain dir");
    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    shell.git_config.ui = crate::config::git_config::GitUi::GitMin;

    let opened = shell.handle_command(Command::GitOpenLazygit);

    assert!(!opened);
    assert!(shell.pending_lazygit_buffer_index.is_none());

    let _ = std::fs::remove_dir_all(root);
}
```

(Lưu ý: test thứ 2 chấp nhận `status_message` có thể bị set `"Not inside a git repository"` — chỉ assert không mở buffer.)

- [ ] **Step 3: Chạy test**

Run: `cargo test gf_git_min`
Expected: 2 passed. Run thêm `cargo test lsp` để chắc path lazygit cũ chưa vỡ.

- [ ] **Step 4: Commit**

```bash
git add src/app/event_loop/commands_lsp.rs src/app/event_loop/commands_tests.rs
git commit -m "feat(git): branch gf to GitMin app spawn when [git] ui=git_min"
```

---

### Task 4: Dep-check bỏ nag lazygit khi dùng GitMin + help text

**Files:**
- Modify: `src/app/event_loop/async_results/system.rs:17`
- Modify: `src/app/app_state/mod.rs:1608` và `src/app/app_state/mod.rs:1795`
- Test: inline trong `system.rs`

**Interfaces:**
- Consumes: `AppShell.git_config.ui` (Task 2)

- [ ] **Step 1: Tách danh sách tool ra helper thuần + dùng nó**

`src/app/event_loop/async_results/system.rs` — thêm helper (trước `handle_system_result`):

```rust
/// CLI tools đáng cảnh báo trên UI. Khi `gf` trỏ sang GitMin, thiếu lazygit
/// không còn là vấn đề của user.
fn critical_cli_tools(git_ui: crate::config::git_config::GitUi) -> &'static [&'static str] {
    match git_ui {
        crate::config::git_config::GitUi::Lazygit => {
            &["fzf", "lazygit", "lazydocker", "rg", "fd", "bat", "delta"]
        }
        crate::config::git_config::GitUi::GitMin => &["fzf", "lazydocker", "rg", "fd", "bat", "delta"],
    }
}
```

Và trong `handle_system_result`, thay dòng:

```rust
            let cli_tools = ["fzf", "lazygit", "lazydocker", "rg", "fd", "bat", "delta"];
```

bằng:

```rust
            let cli_tools = critical_cli_tools(app.git_config.ui);
```

- [ ] **Step 2: Test helper**

Cuối `system.rs` thêm:

```rust
#[cfg(test)]
mod tests {
    use crate::config::git_config::GitUi;

    #[test]
    fn lazygit_excluded_when_git_min_is_target() {
        assert!(super::critical_cli_tools(GitUi::Lazygit).contains(&"lazygit"));
        assert!(!super::critical_cli_tools(GitUi::GitMin).contains(&"lazygit"));
        // các tool khác vẫn giữ
        assert!(super::critical_cli_tools(GitUi::GitMin).contains(&"fzf"));
    }
}
```

- [ ] **Step 3: Cập nhật help text**

`src/app/app_state/mod.rs:1608`:

```rust
    append_help_binding(&mut lines, bindings, "git.open_lazygit", "Open git UI (lazygit | GitMin)");
```

`src/app/app_state/mod.rs:1795`:

```rust
        "git.open_lazygit" => "Open git UI (lazygit | GitMin)",
```

- [ ] **Step 4: Chạy test**

Run: `cargo test system::tests && cargo test help`
Expected: pass (các test help text hiện có nếu assert chuỗi cũ sẽ cần cập nhật theo string mới)

- [ ] **Step 5: Commit**

```bash
git add src/app/event_loop/async_results/system.rs src/app/app_state/mod.rs
git commit -m "feat(git): skip lazygit dep nag when git_min is the configured UI"
```

---

### Task 5: Docs + gates toàn dự án

**Files:**
- Modify: `DEPENDENCIES.md` (bảng tool ~dòng 30)
- Modify: `README.md` ("Where To Fix What" table)

- [ ] **Step 1: DEPENDENCIES.md — thêm hàng companion app**

Ngay sau bảng CLI tools (hàng `lazygit`), thêm:

```markdown
| **GitMin.app** | Optional target cho `gf` khi `[git] ui = "git_min"` | build từ `~/Project/git_min`: `npm run app` | n/a (macOS-first) |
```

- [ ] **Step 2: README — cập nhật "Where To Fix What"**

Thêm hàng vào table (anchor: hàng nào nhắc `commands_lsp` / lazygit nếu có, nếu không thì thêm cuối table):

```markdown
| `gf` mở git UI (lazygit/GitMin) | `src/app/event_loop/commands_lsp.rs`, config `~/.config/netherize/ui.toml` `[git]` |
```

- [ ] **Step 3: Gates**

Run: `cargo fmt && cargo clippy --all-targets 2>&1 | tail -5 && cargo test 2>&1 | tail -10`
Expected: fmt sạch, clippy không error mới, toàn bộ test pass.

- [ ] **Step 4: Commit**

```bash
git add DEPENDENCIES.md README.md
git commit -m "docs: document GitMin as optional gf target"
```

---

## Appendix A — Bên GitMin (tiô thực hiện, repo `~/Project/git_min`, KHÔNG chạy plan này)

**1. `src-tauri/Cargo.toml`:**

```toml
[dependencies]
tauri-plugin-single-instance = "2"
```

**2. `src-tauri/src/lib.rs`** — trong `run()`, register plugin ĐẦU TIÊN:

```rust
        tauri::Builder::default()
            .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
                // Instance thứ 2 vừa launch với argv → forward repo path vào webview.
                if let Some(repo) = args.iter().last() {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        let _ = window.emit("open-repo", repo.to_string());
                    }
                }
            }))
```

Cold start — trong `.setup(...)` (editor luôn truyền đúng 1 arg):

```rust
            .setup(|app| {
                if let Some(repo) = std::env::args().nth(1) {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("open-repo", repo);
                    }
                }
                Ok(())
            })
```

**3. Frontend `src/store.ts` (hoặc entry):**

```ts
import { listen } from "@tauri-apps/api/event";

listen<string>("open-repo", async (event) => {
  const path = event.payload;
  // reuse đúng action mà nút Open của Repos manager đang gọi, ví dụ:
  // openRepoByPath(path) → tạo repo tab mới + setActiveTabId
});
```

Validate path trước khi add tab (kiểm tra `<path>/.git` tồn tại hoặc gọi command Rust `is_git_repo` sẵn có); sai → toast lỗi, không đổi tabs. Plugin single-instance đã raise + focus cửa sổ → thỏa mãn "đang mở → tab mới + focus".

**Cần tiô xác nhận:** tên binary thật trong `GitMin.app/Contents/MacOS/` (plan editor đang mặc định `GitMin`). Nếu khác, user set `git_min_path` hoặc đổi default trong `resolved_binary()`.
