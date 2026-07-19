//! Spatial focus navigation: given block centers, pick the nearest block in a
//! requested direction within a 90° cone. Pure and unit-tested.

use super::model::BlockId;

/// Cardinal navigation direction (`hjkl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// From `from` (the focused block's center), return the id of the nearest other
/// block lying in `dir` within a 90° cone. `None` when no block lies that way.
///
/// The cone test keeps focus moves intuitive: pressing `l` only jumps to blocks
/// that are genuinely to the right (dominant horizontal displacement), not ones
/// that are mostly above/below.
pub fn nearest_in_direction(
    centers: &[(BlockId, (f32, f32))],
    from: (f32, f32),
    from_id: BlockId,
    dir: Dir,
) -> Option<BlockId> {
    let mut best: Option<(BlockId, f32)> = None;
    for &(id, (cx, cy)) in centers {
        if id == from_id {
            continue;
        }
        let dx = cx - from.0;
        let dy = cy - from.1;
        let in_cone = match dir {
            Dir::Right => dx > 0.0 && dx >= dy.abs(),
            Dir::Left => dx < 0.0 && (-dx) >= dy.abs(),
            Dir::Down => dy > 0.0 && dy >= dx.abs(),
            Dir::Up => dy < 0.0 && (-dy) >= dx.abs(),
        };
        if !in_cone {
            continue;
        }
        let dist = dx * dx + dy * dy;
        if best.map(|(_, b)| dist < b).unwrap_or(true) {
            best = Some((id, dist));
        }
    }
    best.map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_nearest_in_cone() {
        let centers = [
            (0, (0.0, 0.0)),
            (1, (100.0, 0.0)),  // right, near
            (2, (300.0, 0.0)),  // right, far
            (3, (0.0, 200.0)),  // down
            (4, (-100.0, 0.0)), // left
        ];
        assert_eq!(
            nearest_in_direction(&centers, (0.0, 0.0), 0, Dir::Right),
            Some(1)
        );
        assert_eq!(
            nearest_in_direction(&centers, (0.0, 0.0), 0, Dir::Down),
            Some(3)
        );
        assert_eq!(
            nearest_in_direction(&centers, (0.0, 0.0), 0, Dir::Left),
            Some(4)
        );
        assert_eq!(nearest_in_direction(&centers, (0.0, 0.0), 0, Dir::Up), None);
    }

    #[test]
    fn excludes_self_and_off_cone() {
        // A block up-and-slightly-right should NOT be reachable via Right
        // (vertical displacement dominates).
        let centers = [(0, (0.0, 0.0)), (1, (10.0, 200.0))];
        assert_eq!(
            nearest_in_direction(&centers, (0.0, 0.0), 0, Dir::Right),
            None
        );
        assert_eq!(
            nearest_in_direction(&centers, (0.0, 0.0), 0, Dir::Down),
            Some(1)
        );
    }
}
