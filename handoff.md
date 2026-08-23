# HANDOFF — Benchmark Release & Large-file Fixes

> Phiên làm việc: 2026-08-23 · **Chưa commit gì** — chạy `git status` trước khi làm tiếp.
> Mục đích: bàn giao ngữ cảnh đầy đủ để phiên sau tiếp tục work hiệu năng/large-file mà không phải khám phá lại từ đầu.

## 1. Bối cảnh & kết quả

Mục tiêu: benchmark toàn bộ app trước release + xử lý các phát hiện.

- **Report chính:** `benchmarks/report_release_bench.html` (mở bằng browser)
- **Dữ liệu thô:** `benchmarks/baselines/bench_latest.json` (CPU-side) · `app_probe.json` (probe GUI thật) · criterion ở `target/criterion/`
- Kết luận: blocker #1 (UI freeze) ĐÃ FIX. Còn 2 blocker dính nhau: **#2 RAM ~3.4GB khi mở file ~10MB** và **#3 quyết định nâng cap 10 MiB** (nâng cap mà chưa fix #2 thì RAM nổ theo tỉ lệ thuận).

## 2. Hạ tầng benchmark đã xây

| Thành phần | File | Chạy bằng |
|---|---|---|
| Criterion micro-bench (edit-loop rust/js/ts/go, incremental parse, open file lớn) | `benches/editor_bench.rs` | `cargo bench --bench editor_bench` |
| E2E CPU runner — 12 scenario: open/jump/edit+scroll/layout/undo/search/save/full-parse/workspace-scan/fuzzy/grep/**open 100MB user file**/RSS-CPU | `benches/e2e_perf_runner.rs` | `cargo bench --bench e2e_perf_runner` |
| **Probe GUI thật** — self-drive qua dispatch → layout → wgpu submit → present; đo startup, keystroke latency p50/p95/p99, scroll FPS, autocomplete, GPU pass, RSS checkpoint | `src/app/event_loop/perf_probe.rs` | `NETH_PERF_PROBE=1 ./target/release/netherize_editor` (~45s, tự exit, tự ghi `benchmarks/baselines/app_probe.json`) |
| GPU pass timing qua wgpu timestamp queries (opt-in) | `src/render/gpu_timing.rs` | Tự bật khi probe chạy |

Vận hành probe — những điều đã mất công học:

- Build **--release** bắt buộc. Không chạy song song instance khác (đã từng 2 instance kẹt nhau, beachball).
- macOS throttle redraw cửa sổ nền → probe gọi `window.focus_window()` lúc Warmup VÀ `about_to_wait` đặt `set_control_flow(WaitUntil(+8ms))` (`application.rs`) để tự đánh thức loop. Bỏ một trong hai là probe treo.
- Nếu render trả Err (Occluded/Timeout) probe vẫn tiến triển phase + có stall-guard 5s phát báo cáo phần (đừng revert về "chỉ push sample khi Ok" như bản đầu).
- Probe tự sinh scratch `/tmp/netherize_probe_10mb.log` (10 MiB − 64KB, vừa dưới cap).
- Phase flow: Warmup(0.7s, focus window) → OpenLarge(đo open_ms) → Typing×150 → Scrolling×120 → OpenRust(`src/main.rs`) → Completion(chờ ≤30s, poll `app_state.completion()`) → Done(ghi JSON + `exit_requested=true`). Heartbeat stderr `[perf_probe] …`.
- Debug RSS curve đang bật trong phase Typing (in mỗi 25 inserts) — giữ lại, hữu ích.
- Data quirks đã biết: `cpu_typing_percent` đọc 0.0 (cửa sấy sampling quá ngắn); scroll `interval_p95_ms` nhiễu instrumentation — tin vào `fps_avg` (tính theo time-span).

## 3. Fix đã land trong phiên này (verify: build sạch, 1256/1256 tests pass)

| Fix | File |
|---|---|
| **[Blocker #1] UI freeze gõ file lớn**: cap inline plaintext regex highlight 1 MiB (`INLINE_PLAINTEXT_BYTE_THRESHOLD`) — quá ngưỡng thì clear spans, KHÔNG regex toàn file trên main thread | `src/syntax/highlight/mod.rs` · nhánh Plaintext trong `submit_parse_for_active_buffer` (`src/app/event_loop/setup.rs` ~1090) |
| Throttle panic-recovery snapshot 1500ms (trước clone toàn buffer MỖI tick trong `about_to_wait`) | `application.rs` (const `RECOVERY_SNAPSHOT_INTERVAL`) + field `last_recovery_snapshot_at` (`mod.rs`, `setup.rs`) |
| Surface lỗi refuse mở file bằng toast đỏ (trước giờ im lặng ở explorer/generic path) | `commands_explorer.rs` arm `ExplorerToggleOrOpen|ExplorerOpenFile` (~780) · `commands_editor.rs` (`surface_failure` gồm `Command::OpenFile`) |
| Message refuse thân thiện kèm MB | `overlays.rs::ensure_interactive_text_file_size` |
| VN→EN audit (toàn src chỉ ra đúng 3 chỗ): popup Terminal Edit, hint LeetCode Expected, log git_min_path | `commands_terminal.rs` ~575 · `async_results/leetcode_fetch.rs` ~110 · `config/git_config.rs` ~75 |
| Breadcrumb branch tái dùng cache text (bớt clone 10MB/frame) | `application.rs` ~3256 |
| GPU timing opt-in qua `Features::TIMESTAMP_QUERY` chỉ khi probe; thường giữ `Features::empty()` | `render/renderer/lifecycle.rs` · `lifecycle/frame.rs` · `render/mod.rs` |
| README layout + lessons mới | `README.md` · `docs/project-knowledge/lessons.md` (mục 2026-08-23) |

## 4. Work mở — thứ tự ưu tiên

### 4.1 Virtualized shaping (blocker #2 — RAM 3.4GB) — VIỆC LỚN NHẤT

- **Nguyên nhân gốc (chứng minh bằng stack sample + RSS curve, đừng nghi ngờ lại):** `Renderer::update_editor_content` tại `src/render/renderer/editor/viewport.rs` (~502–540) gọi `text_system.set_size(Some(width), None)` (chiều cao vô hạn) rồi `set_text_with_spans` toàn bộ text → cosmic-text shape MỌI dòng (~150k dòng/10MB) ở frame đầu, metadata giữ vĩnh viễn để tính cuộn/jump khi word-wrap. Comment gốc ngay đó: *"Allow cosmic-text to shape full height; scissor clips the visible region"*.
- RSS curve thực nghiệm (đừng đoán lại): mở file → 268→3629MB trong ~2s rồi plateau; gõ 150 phím phẳng tuyệt đối.
- **Hướng sửa:** shape theo cửa sổ dòng hiển thị ± overscan; vùng ngoài dùng chiều cao dòng giả định/uniform hoặc index gia tăng.
- **Rủi ro — các hàm phụ thuộc tổng chiều cao đã shape (phá hỏng = không được ship):**
  - `visual_y_for_logical_scroll_with_folds` (+ folded ranges)
  - `rebuild_layout_projection`
  - `Renderer::hit_test_editor_char_index` (mouse click → byte)
  - jump-to-line / Ctrl+O jump / smooth-scroll tween (`advance_scroll_anim`)
- **Checklist test thủ công sau khi sửa:** cuộn mượt tới đáy file lớn; jump-to-line giữa file; click/drag chọn text; fold/unfold; soft-wrap on/off; minimap nếu bật.

### 4.2 Quyết định cap 10 MiB (blocker #3)

- Hằng số `INTERACTIVE_TEXT_FILE_LIMIT_BYTES` tại `src/app/app_state/overlays.rs:3`; test `app_state/tests.rs:8` (dùng 10MiB+1 expect refuse).
- Nếu nâng (VD 256MB): PHẢI xong 4.1 trước. Cân nhắc chế độ view-only cho file siêu lớn (không undo history/shaping).
- File test: `large.log` (100MB) ở repo root — bench scenario `open_user_100mb_large_log` đã có trong e2e runner.

### 4.3 Cold-edit spike (~1.2s đúng 1 lần)

- Repro qua probe: `typing.max_ms` ≈ 1.2s vs p99 16ms. Nghi vấn: edit đầu tiên trigger git-diff baseline init + external-watch setup + syntax submit đầu trên buffer lớn.
- Hướng: pre-warm nền ngay sau open, hoặc chia nhỏ công việc. Chưa investigate sâu.

### 4.4 Startup number sạch

- Đang 2.3s/5.1s — nhiễm session restore + PTY spawn + lúc nào window được focus. PROCESS_START đánh dấu ở dòng đầu `main()` (`perf_probe::mark_process_start()`).
- Muốn cold-start chuẩn: thêm env riêng (VD `NETH_STARTUP_ONLY=1`) tắt session restore/PTY khi đo.

### 4.5 Autocomplete/LSP latency

- Probe mở `src/main.rs`, dispatch `AppendAfterCursor` + `TriggerCompletion`, poll `app_state.completion()` — nhưng rust-analyzer KHÔNG attach (không log spawn). Điểm nghi: `desired_lsp_server_for_active_file` / `queue_lsp_server_start` (`setup.rs` ~1727–1900) cần điều kiện chưa thỏa trong ngữ cảnh probe, hoặc server ready >30s.
- rust-analyzer 1.92 có sẵn tại `~/.cargo/bin/rust-analyzer`. Cần instrument log tại queue_lsp_server_start để thấy vì sao không spawn, hoặc đo thủ công.

## 5. Lệnh kiểm tra nhanh sau khi sửa code

```bash
cargo build --release && cargo test --release          # 1256 tests phải xanh
cargo bench --bench e2e_perf_runner                     # CPU-side, ~1 phút
NETH_PERF_PROBE=1 ./target/release/netherize_editor     # probe GUI ~45s
# Kiểm tra RSS balloon còn không: grep rss_after benchmarks/baselines/app_probe.json
#   rss_after_typing_mb nên ~300–400MB (KHÔNG phải 3xxx)
```

Lưu ý quy trình project (AGENTS.md): sửa cấu trúc file → cập nhật README layout + chạy `npx gitnexus analyze`; học được fact mới → append vào `docs/project-knowledge/lessons.md`.

