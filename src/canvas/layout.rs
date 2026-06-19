//! Deterministic placement of spawned relation blocks around a focal block.
//! Callers stack in a column to the **left**; definitions then callees stack in
//! a column to the **right**. No physics, no overlap. Pure and unit-tested.

use super::model::{BlockRelation, WorldRect};

/// A relation block's computed world rectangle, tagged with how it relates to
/// the focal block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedRelation {
    pub relation: BlockRelation,
    pub world: WorldRect,
}

/// Compute placements for the relations spawned from `focal`. Each relation
/// block uses `block_w`×`block_h`; columns are offset by `gap_x` horizontally
/// and entries within a column by `gap_y` vertically, vertically centred on the
/// focal block.
///
/// Left column: callers (top→bottom). Right column: definitions then callees
/// (top→bottom). Returns rects guaranteed not to overlap the focal block or
/// each other.
pub fn place_relations(
    focal: WorldRect,
    block_w: f32,
    block_h: f32,
    n_def: usize,
    n_callers: usize,
    n_callees: usize,
    gap_x: f32,
    gap_y: f32,
) -> Vec<PlacedRelation> {
    let (fcx, fcy) = focal.center();
    let step = block_h + gap_y;

    // Column origin Y so that `count` stacked blocks are vertically centred on
    // the focal center.
    let col_top = |count: usize| -> f32 {
        if count == 0 {
            fcy
        } else {
            let total = count as f32 * block_h + (count.saturating_sub(1)) as f32 * gap_y;
            fcy - total * 0.5
        }
    };

    let mut out = Vec::with_capacity(n_def + n_callers + n_callees);

    // Left column — callers.
    let left_x = focal.x - gap_x - block_w;
    let left_top = col_top(n_callers);
    for i in 0..n_callers {
        out.push(PlacedRelation {
            relation: BlockRelation::Caller,
            world: WorldRect::new(left_x, left_top + i as f32 * step, block_w, block_h),
        });
    }

    // Right column — definitions, then callees, stacked together.
    let right_x = focal.x + focal.w + gap_x;
    let right_count = n_def + n_callees;
    let right_top = col_top(right_count);
    let mut row = 0usize;
    for _ in 0..n_def {
        out.push(PlacedRelation {
            relation: BlockRelation::Definition,
            world: WorldRect::new(right_x, right_top + row as f32 * step, block_w, block_h),
        });
        row += 1;
    }
    for _ in 0..n_callees {
        out.push(PlacedRelation {
            relation: BlockRelation::Callee,
            world: WorldRect::new(right_x, right_top + row as f32 * step, block_w, block_h),
        });
        row += 1;
    }

    let _ = fcx; // center x retained for symmetry/readability
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focal() -> WorldRect {
        WorldRect::new(0.0, 0.0, 200.0, 120.0)
    }

    #[test]
    fn callers_left_definitions_and_callees_right() {
        let placed = place_relations(focal(), 180.0, 100.0, 1, 2, 2, 60.0, 20.0);
        assert_eq!(placed.len(), 5);
        let f = focal();
        for p in &placed {
            match p.relation {
                BlockRelation::Caller => assert!(p.world.x + p.world.w <= f.x),
                BlockRelation::Definition | BlockRelation::Callee => {
                    assert!(p.world.x >= f.x + f.w)
                }
                BlockRelation::Focal => unreachable!(),
            }
        }
    }

    #[test]
    fn no_overlap_among_placed_or_with_focal() {
        let placed = place_relations(focal(), 180.0, 100.0, 1, 3, 3, 60.0, 20.0);
        let f = focal();
        for (i, a) in placed.iter().enumerate() {
            assert!(!a.world.intersects(&f), "block {i} overlaps focal");
            for (j, b) in placed.iter().enumerate() {
                if i != j {
                    assert!(!a.world.intersects(&b.world), "blocks {i} & {j} overlap");
                }
            }
        }
    }

    #[test]
    fn empty_relations_produce_nothing() {
        assert!(place_relations(focal(), 180.0, 100.0, 0, 0, 0, 60.0, 20.0).is_empty());
    }
}
