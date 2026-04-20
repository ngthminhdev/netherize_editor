# Netherize Editor

Đây là project thử nghiệm để xây một text editor bằng Rust, hiện đang ở giai đoạn rất sớm.

## Mục tiêu hiện tại

Project đang thử ghép 3 phần chính lại với nhau:

- `ropey` để quản lý text buffer
- `winit` để tạo window và nhận input bàn phím
- `wgpu` để render lên GPU
- `cosmic-text` để shape/rasterize text

Hiện tại source đã có nền tảng cho:
- editor buffer
- renderer GPU cơ bản
- text rasterization thử nghiệm

Nhưng chưa hoàn thiện pipeline render text thật sự lên màn hình.

---

## Cấu trúc source hiện tại

### `src/main.rs`

Đây là entry point của chương trình.

Hiện file này đang chứa:

- khai báo module:
  - `renderer`
  - `text_renderer`
- struct `AppState`
- event loop theo `winit`
- logic tạo window và khởi tạo `Renderer`
- xử lý một số phím `j`, `k`, `l` để đổi màu nền
- một đoạn test tạm cho `TextRenderer` trong `main()`

### Trạng thái hiện tại của `main.rs`

File này đang hơi ở trạng thái "lai":

- có sẵn `AppState` để chạy GUI thật
- nhưng trong `main()` phần event loop đang bị comment
- thay vào đó đang test `TextRenderer::rasterize_text("Hello, World!")`

Nói ngắn gọn:
- phần GUI có khung rồi
- nhưng chương trình hiện tại đang chạy theo kiểu test text rasterization

---

### `src/editor_core.rs`

Đây là phần lõi của text editor.

File này định nghĩa struct:

- `EditorBuffer`

#### `EditorBuffer` đang quản lý gì?

- `text: Rope`
- `cursor_char_idx: usize`
- `target_col: usize`

#### Chức năng chính đã có

- tạo buffer rỗng với `new()`
- tạo buffer từ string với `from_str()`
- lấy vị trí con trỏ hiện tại:
  - `current_position()`
  - `cursor_2d_position()`
- chỉnh sửa text:
  - `insert_char(ch)`
  - `delete_backward()`
- di chuyển con trỏ:
  - `move_left()`
  - `move_right()`
  - `move_up()`
  - `move_down()`
- chuyển toàn bộ nội dung buffer thành `String` với `to_string()`

#### Ý tưởng của `target_col`

`target_col` dùng để nhớ "cột mong muốn" khi di chuyển lên/xuống.

Ví dụ:
- đang ở cột 5
- xuống một dòng ngắn hơn thì con trỏ bị snap về cột nhỏ hơn
- nhưng editor vẫn nhớ cột gốc là 5
- khi xuống dòng dài hơn nữa thì con trỏ có thể quay lại cột 5

Đây là behavior rất giống editor thật.

#### Test trong file

File này có unit test cho:
- di chuyển ngang
- di chuyển dọc với target column memory
- insert làm cập nhật `target_col`

Đây hiện là phần ổn định nhất của project.

---

### `src/renderer.rs`

Đây là renderer GPU cơ bản dùng `wgpu`.

#### Struct chính

- `Renderer<'a>`

#### Tài nguyên đang giữ

- `surface`
- `device`
- `queue`
- `config`
- `size`

#### Chức năng hiện có

- `new(window)`:
  - tạo `wgpu::Instance`
  - tạo `Surface`
  - request `Adapter`
  - request `Device` và `Queue`
  - chọn `SurfaceConfiguration`
  - configure surface
- `resize(new_size)`:
  - cập nhật width/height
  - configure lại surface
- `render(clear_color)`:
  - lấy current surface texture
  - tạo command encoder
  - mở render pass
  - clear toàn bộ màn hình bằng một màu
  - submit command buffer
  - present frame

#### `RenderError`

File này có enum `RenderError` để map các trạng thái render hiện tại:

- `Timeout`
- `Occluded`
- `Outdated`
- `Lost`
- `Validation`

Hiện renderer này **chỉ mới clear background**, chưa render text, chưa render cursor, chưa có pipeline/shader riêng.

---

### `src/text_renderer.rs`

Đây là phần thử nghiệm dùng `cosmic-text`.

#### Struct chính

- `TextRenderer`

#### Tài nguyên đang giữ

- `font_system`
- `swash_cache`

#### Chức năng hiện có

- `new()`
- `rasterize_text(text, font_size, line_height, width)`

#### `rasterize_text(...)` đang làm gì?

- tạo `Metrics`
- tạo `Buffer`
- set width cho buffer
- set text vào buffer
- shape/layout text bằng `cosmic-text`
- gọi `buffer.draw(...)`
- thu toàn bộ pixel thành:
  - `Vec<(x, y, r, g, b, a)>`

Nói đơn giản:
- file này đang rasterize text bằng CPU
- trả về danh sách pixel trắng của chữ
- nhưng chưa đẩy dữ liệu đó sang GPU để hiển thị thật lên window

#### Hạn chế hiện tại

`TextRenderer` hiện chưa:
- giữ `Buffer` lâu dài
- đồng bộ trực tiếp với `EditorBuffer`
- trả về layout cursor
- tạo atlas texture
- render bằng `wgpu`

---

## Luồng hoạt động hiện tại

Nếu nhìn theo kiến trúc tổng quát, project đang có các mảnh như sau:

### 1. Editor text state
`editor_core.rs`

- lưu text bằng `Rope`
- quản lý con trỏ
- xử lý insert/delete/move

### 2. Window + GPU
`renderer.rs`

- mở surface GPU
- clear background

### 3. Text shaping/rasterization
`text_renderer.rs`

- biến string thành pixels bằng `cosmic-text`

### 4. App shell
`main.rs`

- tạo window
- xử lý input
- gọi renderer

---

## Phần nào đã có, phần nào chưa có

### Đã có
- text buffer bằng `ropey`
- cursor movement logic
- unit tests cho editor buffer
- window lifecycle với `winit`
- GPU init với `wgpu`
- background clear pass
- text rasterization thử nghiệm bằng `cosmic-text`

### Chưa có
- đồng bộ chính thức giữa `EditorBuffer` và `TextRenderer`
- text buffer persistent trong `TextRenderer`
- render text thật sự lên GPU
- texture atlas cho glyph
- render cursor block kiểu Vim
- mode system hoàn chỉnh (`Normal`, `Insert`, `Command`) đang dùng thật
- keyboard editing flow hoàn chỉnh
- syntax highlighting
- scrolling
- file open/save

---

## Kiến trúc mong muốn tiếp theo

Hướng phát triển hợp lý tiếp theo là:

### Bước 1: giữ `TextBuffer` sống lâu
Thay vì mỗi lần rasterize lại tạo `Buffer` mới, nên cho `TextRenderer` giữ:

- `FontSystem`
- `SwashCache`
- `Buffer`

và cập nhật nội dung bằng `set_text(...)`.

### Bước 2: nối `EditorBuffer` với `TextRenderer`
Luồng dữ liệu mong muốn:

- user gõ phím
- `EditorBuffer` cập nhật text
- `EditorBuffer.to_string()`
- đổ string đó sang `TextRenderer`
- `TextRenderer` shape lại layout

### Bước 3: render text bằng GPU
Hiện tại text chỉ ra `Vec<pixel>`.
Cần một pipeline để:
- đưa glyph/atlas lên GPU
- render texture đó lên screen

### Bước 4: render cursor
Lấy tọa độ glyph tại vị trí cursor rồi vẽ một block/quad đè lên.

---

## Dependency chính

Từ `Cargo.toml`, các thư viện quan trọng hiện đang dùng là:

- `ropey`
- `winit`
- `wgpu`
- `cosmic-text`
- `pollster`
- `tokio`
- `font-kit`
- `swash`

### Vai trò ngắn gọn

- `ropey`: text buffer hiệu quả cho editor
- `winit`: window + keyboard events
- `wgpu`: giao tiếp GPU
- `cosmic-text`: layout và rasterize text
- `pollster`: block async khi khởi tạo GPU
- `font-kit`, `swash`: font/glyph rasterization support

---

## Tình trạng hiện tại của project

Project đang ở mức:

**prototype / playground**

Nó chưa phải editor hoàn chỉnh, nhưng đã có các "viên gạch" quan trọng:

- core text buffer
- basic renderer
- text rasterizer
- app skeleton

Đây là nền tảng tốt để đi tiếp sang:
- text rendering thật
- cursor rendering
- input editing loop hoàn chỉnh

---

## Gợi ý thứ tự đọc code

Nếu bạn mới vào project, nên đọc theo thứ tự này:

1. `src/editor_core.rs`
   - dễ hiểu nhất
   - thấy rõ logic editor buffer

2. `src/renderer.rs`
   - hiểu phần khởi tạo GPU và clear screen

3. `src/text_renderer.rs`
   - hiểu cách cosmic-text shape và rasterize chữ

4. `src/main.rs`
   - xem cách các phần được nối lại với nhau

---

## Tóm tắt 1 câu

Project này hiện là một bản prototype của text editor viết bằng Rust, đã có:
- editor buffer bằng `ropey`
- renderer nền bằng `wgpu`
- text rasterization bằng `cosmic-text`

nhưng chưa ghép xong thành một pipeline render text editor hoàn chỉnh.