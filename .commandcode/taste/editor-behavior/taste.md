# editor-behavior
- Giữ dirty buffer khi switch tab, chỉ discard khi user save hoặc đóng app. Undo history tồn tại đến khi tắt app. Confidence: 0.80
- Khi focus lại app và file đang dirty bị thay đổi từ bên ngoài, hiển thị popup hỏi overwrite (y/n). Confidence: 0.70
- Khi xoá file từ file tree, tự động close buffer tab đang mở file đó. Confidence: 0.70
- ESC trong insert mode: vừa tắt completion popup, vừa cancel pending LSP request, vừa về normal mode. Confidence: 0.70
- Clear semantic highlights khi thực hiện insert/edit/delete để tránh highlight persistence sau khi text đã thay đổi. Confidence: 0.70
- Clear selection highlight khi switch qua tab khác, không để highlight tồn tại ở vị trí cũ. Confidence: 0.70
- Zenmode (space zm) phải tự động tắt/hide right dock. Confidence: 0.70
- Format code (space fm) phải undoable bằng u (undo). Confidence: 0.70
- Git decoration phải hiển thị ngay khi mở file (từ file picker hoặc file tree), không đợi switch tab mới hiển thị. Confidence: 0.70
