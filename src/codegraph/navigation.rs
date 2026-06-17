//! `hjkl` navigation state machine for the Code Graph HUD.
//!
//! Layout is three columns — Callers (left) · Center · Callees (right):
//! - `h` (Left):  Callee→Center; Center→first Caller (if any); Caller→stays.
//! - `l` (Right): Caller→Center; Center→first Callee (if any); Callee→stays.
//! - `j`/`k` (Down/Up): move within the focused column, clamped.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Center,
    Caller(usize),
    Callee(usize),
}

#[derive(Debug, Clone, Copy)]
pub enum NavKey {
    Left,
    Right,
    Up,
    Down,
}

/// Pure transition. `n_callers`/`n_callees` are the visible counts.
pub fn navigate(focus: Focus, key: NavKey, n_callers: usize, n_callees: usize) -> Focus {
    match (focus, key) {
        (Focus::Callee(_), NavKey::Left) => Focus::Center,
        (Focus::Center, NavKey::Left) if n_callers > 0 => Focus::Caller(0),
        (Focus::Caller(_), NavKey::Right) => Focus::Center,
        (Focus::Center, NavKey::Right) if n_callees > 0 => Focus::Callee(0),

        (Focus::Caller(i), NavKey::Down) => {
            Focus::Caller((i + 1).min(n_callers.saturating_sub(1)))
        }
        (Focus::Caller(i), NavKey::Up) => Focus::Caller(i.saturating_sub(1)),
        (Focus::Callee(i), NavKey::Down) => {
            Focus::Callee((i + 1).min(n_callees.saturating_sub(1)))
        }
        (Focus::Callee(i), NavKey::Up) => Focus::Callee(i.saturating_sub(1)),

        (other, _) => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_moves_into_columns() {
        assert_eq!(navigate(Focus::Center, NavKey::Left, 3, 3), Focus::Caller(0));
        assert_eq!(navigate(Focus::Center, NavKey::Right, 3, 3), Focus::Callee(0));
    }

    #[test]
    fn columns_return_to_center() {
        assert_eq!(navigate(Focus::Caller(2), NavKey::Right, 3, 3), Focus::Center);
        assert_eq!(navigate(Focus::Callee(1), NavKey::Left, 3, 3), Focus::Center);
    }

    #[test]
    fn vertical_clamps_within_column() {
        assert_eq!(navigate(Focus::Caller(0), NavKey::Up, 3, 3), Focus::Caller(0));
        assert_eq!(navigate(Focus::Caller(2), NavKey::Down, 3, 3), Focus::Caller(2));
        assert_eq!(navigate(Focus::Caller(0), NavKey::Down, 3, 3), Focus::Caller(1));
    }

    #[test]
    fn empty_column_blocks_entry() {
        assert_eq!(navigate(Focus::Center, NavKey::Left, 0, 3), Focus::Center);
    }
}
