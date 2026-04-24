# Netherize Editor — Module 12 Handoff (Compact)

## 1) Bối cảnh nhanh
- Dự án đang ở **Module 12 (Phase 2 + Phase 3)**.
- Mục tiêu chính của chuỗi task vừa rồi:
  - Đánh thức UI thật (Explorer/Terminal/Center) thay cho placeholder cũ.
  - Mở rộng keyboard coverage theo focus context.
  - Nạp theme từ `.toml` + fallback an toàn.
  - Sửa UX lag/focus Explorer <-> Editor.
  - Làm giao diện dễ nhìn hơn, gần style VSCode/Zed hơn.

## 2) Vấn đề chính đã gặp
- Vẫn còn quad placeholder trắng/xám cũ trong render pass.
- Text editor từng bị lệch tọa độ (không bám đúng Center bounds).
- Explorer có lúc highlight 1 folder nhưng Enter lại mở node khác.
- Focus qua lại Explorer/Editor cảm giác lag.
- Dock phải có hiện tượng bị "khuyết/sliver".
- Status bar thấp/nhỏ khó nhìn.
- Cursor dạng block vuông xấu (không giống phong cách Zed/Nvim mong muốn).
- UI kích thước/màu có chỗ hardcode nên khó tune.

## 3) Những gì đã implement

### 3.1 Rendering / Layout
- Render theo `RegionModel` và token theme cho từng vùng:
  - Left/Right sidebar: `theme.ui.sidebar_bg`
  - Bottom panel: `theme.ui.panel_bg`
  - Center: `theme.editor.bg`
  - Status bar: `theme.ui.status_bar_bg`
- Editor render đã đi theo **Center bounds offset** + **scissor theo Center**.
- Explorer và Terminal render đúng panel bounds tương ứng.
- Bỏ phụ thuộc vào placeholder pass cũ (không còn render khối trắng mock giữa màn hình).

### 3.2 Explorer UX + key routing
- Explorer tree render đầy đủ icon:
  - Folder đóng: `▶`
  - Folder mở: `▼`
- Có highlight row đang active bằng `theme.ui.selection_bg`.
- Context keymap cho Explorer:
  - `j/k`, `ArrowDown/ArrowUp`: move
  - `h/ArrowLeft`: collapse hoặc parent
  - `l/ArrowRight`: expand hoặc child
  - `Enter`: toggle folder hoặc open file
- Khi open file từ Explorer:
  - gọi luồng `OpenFile`
  - parse/syntax refresh
  - gửi `didOpen` cho LSP
  - tự trả focus về Editor.

### 3.3 Focus/lag & terminal coexistence
- Cải thiện redraw/layout path:
  - phân tách **editor full layout** vs **caret-only update** khi chỉ di chuyển con trỏ.
- Cache snapshot Explorer + cache selection quads để tránh rebuild dư.
- Terminal input đi qua command router theo focus context (không đi luồng tách rời).
- Có phím thoát focus terminal về editor (`Esc` + mapping liên quan).

### 3.4 Theme & config runtime
- Theme từ file TOML đã nạp runtime + fallback an toàn khi parse lỗi.
- Bổ sung cấu hình UI độc lập mới:
  - `config/ui/default.toml`
  - parser/runtime state ở `src/config/ui_config.rs`
- App startup nạp `UiConfig` và apply vào:
  - window title/size
  - layout dimensions
  - dock visibility/size
  - spacing/padding
  - cursor style
  - status bar typography

### 3.5 Cursor style theo kiểu Nvim/Zed
- Hỗ trợ shape: `beam | block | underline` (alias `zed`, `nvim`).
- Cấu hình hiện tại đặt `shape = "nvim"` trong `config/ui/default.toml`:
  - Normal/Visual dùng block
  - Insert fallback beam (mảnh hơn) qua mode logic.

### 3.6 Fix dock phải bị khuyết
- Trong layout compute, RightSidebar được canh **flush mép phải viewport** để tránh seam/sliver do float rounding.
- Có thêm test regression cho hành vi này.

## 4) File quan trọng đã chạm
- UI config mới:
  - `config/ui/default.toml`
  - `src/config/ui_config.rs`
  - `src/config/mod.rs`
- Startup wiring / render orchestration:
  - `src/app/event_loop.rs`
- GPU/text rendering:
  - `src/render/renderer.rs`
- Layout engine:
  - `src/workbench/layout_engine.rs`
- Theme:
  - `config/themes/default-dark.toml`
  - `src/config/theme_config.rs`
- Panel defaults/state:
  - `src/workbench/panel_state.rs`

## 5) Cấu hình nên chỉnh khi muốn tune nhanh

### 5.1 Kích thước dock/status/layout
- File: `config/ui/default.toml`
- Nhóm cần chỉnh:
  - `[layout]`: `top_bar_height`, `status_bar_height`, `region_gap`
  - `[docks]`: `left_size_px`, `right_size_px`, `bottom_size_px`, `*_visible`
  - `[status_bar]`: `font_size`, `line_height`, `padding_x`
  - `[spacing]`: `editor_padding`, `panel_padding`, `explorer_padding`

### 5.2 Cursor
- File: `config/ui/default.toml`
- Nhóm `[cursor]`:
  - `shape = "zed"` (beam) hoặc `"nvim"` (block)
  - `beam_width`, `block_width`, `underline_height`

### 5.3 Màu sắc
- File: `config/themes/default-dark.toml`
- Chỉnh trong `[editor]`, `[ui]`, `[syntax]`.

## 6) Kiểm thử
- Đã chạy: `cargo test -q`
- Kết quả: **pass toàn bộ (128 tests)**.

## 7) Lưu ý handoff
- Repo đang có nhiều thay đổi cùng lúc (bao gồm vài file unrelated/deleted test sample trong working tree).
- Nếu bàn giao cho người khác làm tiếp, nên tách commit theo nhóm:
  1. `ui_config + startup wiring`
  2. `renderer/layout fixes`
  3. `theme/default tuning`
  4. `explorer/focus improvements`
- Sau mỗi lần đổi `.toml`, nên restart app để chắc chắn nạp config/theme mới từ startup.
