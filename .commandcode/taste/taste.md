# Taste (Continuously Learned by [CommandCode][cmd])

[cmd]: https://commandcode.ai/

# communication
- Giao tiếp bằng tiếng Việt trong dự án Netherize Editor. Confidence: 0.90

# cursor
- Tắt blink cursor, giữ cursor tĩnh (không nhấp nháy). Confidence: 0.70

# code-style
- Khi loại bỏ functionality, xóa luôn dead code (unused fields, constants) và comment không cần thiết. Confidence: 0.70

# cli
- Luôn prefix shell commands với `rtk` để tiết kiệm token consumption. Confidence: 0.85

# syntax
- Với file .txt, dùng terminal regex highlighting (string, number, keyword, plaintext pattern). Confidence: 0.85
- Language không có syntax engine match thì fallback về plaintext. Confidence: 0.85

# architecture
- Khi feature không hoạt động ổn định, tách riêng ra branch khác thay vì giữ code broken trong codebase chính. Confidence: 0.65
- Lazy-load LSP và language tools: chỉ prompt cài đặt khi user mở file của ngôn ngữ đó, không bắt cài tất cả khi start app. Confidence: 0.70

# indexing
- Index toàn bộ source và package khi mở project, ưu tiên đúng và nhanh, RAM không phải là vấn đề. Confidence: 0.85

# editor-behavior
See [editor-behavior/taste.md](editor-behavior/taste.md)

# completion
- Khi accept completion cho function: nếu có params thì chèn () và đặt cursor vào giữa; nếu không có params thì chèn () nhưng không move cursor; nếu đã có sẵn () trước khi accept thì không chèn thêm. Confidence: 0.75
- Sau khi accept completion, giữ nguyên viewport, không nhảy hay căn giữa line. Confidence: 0.70
- Completion item khi accept phải replace text đang gõ, không được append thêm. Confidence: 0.70
- Sort completion items theo mức độ liên quan đến keyword đang gõ, ưu tiên kết quả khớp nhất lên đầu. Confidence: 0.70

# git
- Commit với message rõ ràng, tập trung vào từng code change cụ thể để dễ debug sau này. Confidence: 0.70

# zenmode
- Movement keys (gg, G, etc.) phải hoạt động trong zenmode, không bị block. Confidence: 0.70
- Zenmode + terminal: không block input khi không ở T-COPY mode, vẫn cho phép gõ bình thường. Confidence: 0.70

# file-tree
- File bị ẩn (hidden files) phải dim toàn bộ line để phân biệt rõ với file hiển thị bình thường. Confidence: 0.70

# tab
- MD preview tab phải đóng được bằng q hoặc space x giống như search/grep/references tab. Confidence: 0.70

# ui
See [ui/taste.md](ui/taste.md)
