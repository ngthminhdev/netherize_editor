# Design: `gf` → GitMin Integration

**Date:** 2026-08-22
**Status:** Approved

## Mục tiêu

Nhấn `gf` trong netherize_editor mở app **GitMin** (git desktop client của
`~/Project/git_min`) vào đúng repo của workspace đang mở, thay vì (hoặc song song
với) lazygit. Nếu GitMin đang chạy → tạo **tab mới** cho repo đó và focus tab.
Hành vi `gf` chọn được qua config.

## Quyết định đã chốt

| Câu hỏi | Quyết định |
|---|---|
| `gf` behavior | Config chọn được: `"lazygit"` \| `"git_min"`, default `"lazygit"` |
| Sửa bên git_min? | Có — tiô thêm nhận argv + single-instance |
| Cách launch | `.app` đã build, spawn binary trực tiếp |
| Approach | **A — Spawn binary + `tauri-plugin-single-instance` forwarding** |

Bỏ deep-link (macOS hỏi quyền scheme lần đầu) và localhost IPC (quá đà).

## 1. Bên GitMin (~50 dòng)

### Rust (`src-tauri/`)

- `Cargo.toml`: thêm `tauri-plugin-single-instance`.
- `lib.rs` `run()`:
  - Register plugin **đầu tiên** (yêu cầu của plugin).
  - Callback single-instance: lấy arg cuối của argv làm repo path → emit event
    `open-repo` vào main window → return.
  - Cold start: parse `std::env::args()` trong setup, arg cuối (nếu có) là repo
    path → emit cùng event sau khi webview sẵn sàng.

### Frontend (`src/store.ts`)

- Listen event `open-repo`:
  - Path hợp lệ (là git repo) → **tạo repo tab mới** + set `activeTabId`.
    Reuse đúng logic nút Open của Repos manager — không viết mới flow mở repo.
  - Path không hợp lệ → error toast, không đổi state tabs.
- Single-instance plugin tự raise/focus cửa sổ → thỏa mãn "đang mở → tab mới +
  focus".

## 2. Bên Editor

### Config

File mới `src/config/git_config.rs` theo pattern `ui_config.rs` (serde defaults,
load từ user config TOML):

```toml
[git]
ui = "git_min"   # "lazygit" | "git_min", default "lazygit"
# git_min_path = "/Applications/GitMin.app/Contents/MacOS/git-min"  # optional override
```

### Command flow (không đụng keymap/input_map/resolved_keymap)

`gf` vẫn bind `git.open_lazygit`. Handler `open_lazygit_buffer`
(src/app/event_loop/commands_lsp.rs:307) đọc `[git] ui` và branch:

- `"lazygit"` → path cũ: PTY buffer tab như hiện tại.
- `"git_min"` → method mới `open_git_min_app(workspace_root)`:
  - Resolve **repo root**: nếu workspace root không chứa `.git`, walk up các thư
    mục cha đến khi tìm thấy; không thấy → coi như workspace chưa attach.
  - Spawn detached: `tokio::spawn` + `tokio::process::Command(binary).arg(repo_root)`
    — fire-and-forget, **không mở buffer**, không block UI thread (anti-pattern #3).
  - Binary mặc định `/Applications/GitMin.app/Contents/MacOS/git-min`
    (⚠️ cần tiô xác nhận tên thật trong `Contents/MacOS/`), override bằng
    `git_min_path`.

### Dep check

Khi `ui = "git_min"`, system-dep check kiểm tra sự tồn tại của GitMin.app thay
vì binary `lazygit`; thiếu → warning status bar kiểu dep-check hiện có.

## 3. Error handling

| Trường hợp | Hành vi |
|---|---|
| Workspace chưa attach | Skip + `eprintln!` (giữ hiện trạng) |
| Không tìm thấy GitMin.app | Warning status bar, không panic |
| Workspace không nằm trong git repo | Skip + log |
| Path sai bên GitMin | Error toast, tabs không đổi |

## 4. Testing & docs

- Editor: unit test resolve-repo-root (walk up), parse config `[git]` với
  default + override.
- GitMin: test pure function parse argv (path ở arg cuối, ignore flag khác).
- Cập nhật `DEPENDENCIES.md` (GitMin = companion app, cách cài) và `README.md`
  theo structure change rules.

## Phân công

- **Tiô**: toàn bộ section 1 (git_min).
- **Repo này**: toàn bộ section 2–4 (editor).

## Điều còn mở

- Tên binary thật trong `GitMin.app/Contents/MacOS/` — tiô xác nhận, đưa vào
  default của `git_min_path`.
