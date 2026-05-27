# ui
- Luôn render sẵn UI placeholder (không có text) để tránh layout bị giật khi element xuất hiện/biến mất. Confidence: 0.70
- Breadcrumb bar luôn được render (kể cả khi trống) để tránh layout bị giật khi ra/vào scope có breadcrumb. Confidence: 0.80
- Welcome page chỉ tắt khi mở file tree, terminal, hoặc command palette — không tắt khi ấn phím khác. Confidence: 0.70
- Recent projects trong welcome page giới hạn 5 item. Confidence: 0.70
- Tắt right dock (space aa) phải auto focus về main editor. Confidence: 0.70
- Disable smooth scroll trên toàn bộ app. Confidence: 0.75
- Khi switch tab, xoá selection highlight của tab cũ — highlight phải per-buffer, không tồn tại khi tab không active. Confidence: 0.70
