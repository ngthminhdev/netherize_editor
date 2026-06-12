//! Procedural cell shapes — vẽ Block Elements (U+2580–U+259F) và Box Drawing
//! (U+2500–U+257F) bằng hình chữ nhật phủ kín cell thay vì glyph font.
//!
//! Lý do: glyph font chỉ phủ line-box tự nhiên của font (~font_size × 1.2),
//! trong khi cell terminal dùng `panel_line_height` (vd 22px cho font 14px).
//! Kết quả là border `┃` và logo khối `▀▄█` bị hở khe giữa các hàng/cột
//! ("đứt nét"). Terminal emulator thật (kitty, alacritty) đều tự vẽ các ký tự
//! này; ta làm tương tự bằng quad solid qua text pipeline.

/// Một hình chữ nhật trong tọa độ cell-relative (pixel), kèm hệ số alpha
/// (dùng cho shade ░▒▓).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub alpha: f32,
}

impl CellRect {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            alpha: 1.0,
        }
    }

    fn shaded(x: f32, y: f32, w: f32, h: f32, alpha: f32) -> Self {
        Self { x, y, w, h, alpha }
    }
}

/// Bốn arm của box-drawing char, mỗi arm có weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    None,
    Light,
    Heavy,
}

/// Trả danh sách rect phủ cell cho ký tự khối/viền, `None` nếu ký tự không
/// thuộc nhóm này (caller fallback về glyph font, vd dashed `┄`, diagonal `╱`).
pub fn solid_cell_rects(ch: char, cell_w: f32, cell_h: f32) -> Option<Vec<CellRect>> {
    if let Some(rects) = block_element_rects(ch, cell_w, cell_h) {
        return Some(rects);
    }
    box_drawing_rects(ch, cell_w, cell_h)
}

fn block_element_rects(ch: char, w: f32, h: f32) -> Option<Vec<CellRect>> {
    let r = |x: f32, y: f32, rw: f32, rh: f32| CellRect::new(x, y, rw, rh);
    let rects = match ch {
        '\u{2580}' => vec![r(0.0, 0.0, w, h / 2.0)], // ▀ upper half
        // ▁▂▃▄▅▆▇█ — lower k/8
        '\u{2581}'..='\u{2588}' => {
            let k = (ch as u32 - 0x2580) as f32;
            let rh = h * k / 8.0;
            vec![r(0.0, h - rh, w, rh)]
        }
        // ▉▊▋▌▍▎▏ — left (8-k)/8 với k = offset từ 0x2588
        '\u{2589}'..='\u{258F}' => {
            let k = (ch as u32 - 0x2588) as f32;
            let rw = w * (8.0 - k) / 8.0;
            vec![r(0.0, 0.0, rw, h)]
        }
        '\u{2590}' => vec![r(w / 2.0, 0.0, w / 2.0, h)], // ▐ right half
        '\u{2591}' => vec![CellRect::shaded(0.0, 0.0, w, h, 0.25)], // ░
        '\u{2592}' => vec![CellRect::shaded(0.0, 0.0, w, h, 0.5)], // ▒
        '\u{2593}' => vec![CellRect::shaded(0.0, 0.0, w, h, 0.75)], // ▓
        '\u{2594}' => vec![r(0.0, 0.0, w, h / 8.0)],     // ▔ upper eighth
        '\u{2595}' => vec![r(w * 7.0 / 8.0, 0.0, w / 8.0, h)], // ▕ right eighth
        // Quadrants ▖▗▘▙▚▛▜▝▞▟ — encode (UL, UR, LL, LR)
        '\u{2596}'..='\u{259F}' => {
            let quads: (bool, bool, bool, bool) = match ch {
                '\u{2596}' => (false, false, true, false),
                '\u{2597}' => (false, false, false, true),
                '\u{2598}' => (true, false, false, false),
                '\u{2599}' => (true, false, true, true),
                '\u{259A}' => (true, false, false, true),
                '\u{259B}' => (true, true, true, false),
                '\u{259C}' => (true, true, false, true),
                '\u{259D}' => (false, true, false, false),
                '\u{259E}' => (false, true, true, false),
                '\u{259F}' => (false, true, true, true),
                _ => return None,
            };
            quadrant_rects(quads, w, h)
        }
        _ => return None,
    };
    Some(rects)
}

/// Gộp các quadrant liền kề theo hàng để tránh seam hairline giữa quad.
fn quadrant_rects((ul, ur, ll, lr): (bool, bool, bool, bool), w: f32, h: f32) -> Vec<CellRect> {
    let hw = w / 2.0;
    let hh = h / 2.0;
    let mut rects = Vec::with_capacity(2);
    // Hàng trên
    match (ul, ur) {
        (true, true) => rects.push(CellRect::new(0.0, 0.0, w, hh)),
        (true, false) => rects.push(CellRect::new(0.0, 0.0, hw, hh)),
        (false, true) => rects.push(CellRect::new(hw, 0.0, hw, hh)),
        (false, false) => {}
    }
    // Hàng dưới
    match (ll, lr) {
        (true, true) => rects.push(CellRect::new(0.0, hh, w, hh)),
        (true, false) => rects.push(CellRect::new(0.0, hh, hw, hh)),
        (false, true) => rects.push(CellRect::new(hw, hh, hw, hh)),
        (false, false) => {}
    }
    rects
}

fn box_drawing_rects(ch: char, w: f32, h: f32) -> Option<Vec<CellRect>> {
    use Arm::{Heavy, Light, None as N};
    // (up, down, left, right)
    let arms: (Arm, Arm, Arm, Arm) = match ch {
        '─' => (N, N, Light, Light),
        '━' => (N, N, Heavy, Heavy),
        '│' => (Light, Light, N, N),
        '┃' => (Heavy, Heavy, N, N),
        '┌' | '╭' => (N, Light, N, Light),
        '┏' => (N, Heavy, N, Heavy),
        '┐' | '╮' => (N, Light, Light, N),
        '┓' => (N, Heavy, Heavy, N),
        '└' | '╰' => (Light, N, N, Light),
        '┗' => (Heavy, N, N, Heavy),
        '┘' | '╯' => (Light, N, Light, N),
        '┛' => (Heavy, N, Heavy, N),
        '├' => (Light, Light, N, Light),
        '┣' => (Heavy, Heavy, N, Heavy),
        '┤' => (Light, Light, Light, N),
        '┫' => (Heavy, Heavy, Heavy, N),
        '┬' => (N, Light, Light, Light),
        '┳' => (N, Heavy, Heavy, Heavy),
        '┴' => (Light, N, Light, Light),
        '┻' => (Heavy, N, Heavy, Heavy),
        '┼' => (Light, Light, Light, Light),
        '╋' => (Heavy, Heavy, Heavy, Heavy),
        '╴' => (N, N, Light, N),
        '╵' => (Light, N, N, N),
        '╶' => (N, N, N, Light),
        '╷' => (N, Light, N, N),
        '╸' => (N, N, Heavy, N),
        '╹' => (Heavy, N, N, N),
        '╺' => (N, N, N, Heavy),
        '╻' => (N, Heavy, N, N),
        // Double lines — xấp xỉ bằng heavy single (đủ tốt cho TUI).
        '═' => (N, N, Heavy, Heavy),
        '║' => (Heavy, Heavy, N, N),
        '╔' => (N, Heavy, N, Heavy),
        '╗' => (N, Heavy, Heavy, N),
        '╚' => (Heavy, N, N, Heavy),
        '╝' => (Heavy, N, Heavy, N),
        '╠' => (Heavy, Heavy, N, Heavy),
        '╣' => (Heavy, Heavy, Heavy, N),
        '╦' => (N, Heavy, Heavy, Heavy),
        '╩' => (Heavy, N, Heavy, Heavy),
        '╬' => (Heavy, Heavy, Heavy, Heavy),
        _ => return None,
    };
    Some(arm_rects(arms, w, h))
}

fn arm_thickness(arm: Arm, base: f32) -> f32 {
    match arm {
        Arm::None => 0.0,
        Arm::Light => base,
        Arm::Heavy => base * 2.0,
    }
}

/// Dựng các rect từ arm spec. Tâm cell là điểm giao; mỗi arm kéo từ tâm ra
/// mép cell. Phần giao được phủ bởi rect dọc/ngang chồng lên nhau (cùng màu
/// nên không thấy seam).
fn arm_rects((up, down, left, right): (Arm, Arm, Arm, Arm), w: f32, h: f32) -> Vec<CellRect> {
    // Bề dày cơ sở: tỉ lệ theo cell, tối thiểu 1px (light ~1/10 cell width).
    let base = (w / 8.0).max(1.0);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let mut rects = Vec::with_capacity(2);

    // Trục dọc (up/down). Bề dày lấy max của hai arm để khớp tại tâm.
    let v_thick = arm_thickness(up, base).max(arm_thickness(down, base));
    if v_thick > 0.0 {
        let x = cx - v_thick / 2.0;
        let top = if up != Arm::None { 0.0 } else { cy - v_thick / 2.0 };
        let bottom = if down != Arm::None {
            h
        } else {
            cy + v_thick / 2.0
        };
        rects.push(CellRect::new(x, top, v_thick, bottom - top));
    }

    // Trục ngang (left/right).
    let h_thick = arm_thickness(left, base).max(arm_thickness(right, base));
    if h_thick > 0.0 {
        let y = cy - h_thick / 2.0;
        let lhs = if left != Arm::None {
            0.0
        } else {
            cx - h_thick / 2.0
        };
        let rhs = if right != Arm::None {
            w
        } else {
            cx + h_thick / 2.0
        };
        rects.push(CellRect::new(lhs, y, rhs - lhs, h_thick));
    }

    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_block_fills_entire_cell() {
        let rects = solid_cell_rects('█', 8.4, 22.0).expect("block");
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 8.4, 22.0));
    }

    #[test]
    fn upper_half_block_covers_top_half() {
        let rects = solid_cell_rects('▀', 8.0, 20.0).expect("upper half");
        assert_eq!(rects, vec![CellRect::new(0.0, 0.0, 8.0, 10.0)]);
    }

    #[test]
    fn lower_half_block_covers_bottom_half() {
        let rects = solid_cell_rects('▄', 8.0, 20.0).expect("lower half");
        assert_eq!(rects, vec![CellRect::new(0.0, 10.0, 8.0, 10.0)]);
    }

    #[test]
    fn shades_carry_alpha() {
        let rects = solid_cell_rects('▒', 8.0, 20.0).expect("shade");
        assert_eq!(rects[0].alpha, 0.5);
        assert_eq!((rects[0].w, rects[0].h), (8.0, 20.0));
    }

    #[test]
    fn vertical_line_spans_full_cell_height() {
        // Đây chính là fix "đứt nét": │ và ┃ phải phủ kín chiều cao cell.
        for ch in ['│', '┃', '║'] {
            let rects = solid_cell_rects(ch, 8.4, 22.0).expect("vertical");
            let v = rects
                .iter()
                .find(|r| r.h == 22.0 && r.y == 0.0)
                .unwrap_or_else(|| panic!("'{ch}' must span full cell height"));
            assert!(v.w >= 1.0);
        }
    }

    #[test]
    fn horizontal_line_spans_full_cell_width() {
        for ch in ['─', '━', '═'] {
            let rects = solid_cell_rects(ch, 8.4, 22.0).expect("horizontal");
            assert!(
                rects.iter().any(|r| r.w == 8.4 && r.x == 0.0),
                "'{ch}' must span full cell width"
            );
        }
    }

    #[test]
    fn corner_arms_reach_cell_edges() {
        // ┌ = down + right: trục dọc chạm đáy, trục ngang chạm mép phải.
        let rects = solid_cell_rects('┌', 8.0, 20.0).expect("corner");
        assert!(rects.iter().any(|r| (r.y + r.h - 20.0).abs() < 0.01));
        assert!(rects.iter().any(|r| (r.x + r.w - 8.0).abs() < 0.01));
    }

    #[test]
    fn half_up_line_stops_at_center() {
        // ╹ (heavy up): từ mép trên xuống tâm.
        let rects = solid_cell_rects('╹', 8.0, 20.0).expect("half up");
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert_eq!(r.y, 0.0);
        assert!((r.y + r.h - (10.0 + r.w / 2.0)).abs() <= r.w);
    }

    #[test]
    fn quadrants_merge_rows() {
        // ▙ = UL + LL + LR → hàng trên 1 rect trái, hàng dưới 1 rect full.
        let rects = solid_cell_rects('▙', 8.0, 20.0).expect("quadrant");
        assert_eq!(rects.len(), 2);
        assert!(rects.contains(&CellRect::new(0.0, 0.0, 4.0, 10.0)));
        assert!(rects.contains(&CellRect::new(0.0, 10.0, 8.0, 10.0)));
    }

    #[test]
    fn non_block_chars_fall_through_to_font() {
        for ch in ['a', 'ạ', '┄', '╱', '◆', ' '] {
            assert!(
                solid_cell_rects(ch, 8.0, 20.0).is_none(),
                "'{ch}' should render via font glyph"
            );
        }
    }
}
