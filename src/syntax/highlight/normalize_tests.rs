use std::ops::Range;

use super::{
    engine::normalize_spans,
    HighlightCategory,
    HighlightSpan,
};

fn span(range: Range<usize>, category: HighlightCategory) -> HighlightSpan {
    HighlightSpan { range, category }
}

#[test]
fn normalize_spans_preserves_non_overlapping_spans() {
    let source = "let answer = 42;";
    let normalized = normalize_spans(
        source,
        vec![
            span(0..3, HighlightCategory::Keyword),
            span(13..15, HighlightCategory::Number),
        ],
        None,
    );

    assert_eq!(
        normalized,
        vec![
            span(0..3, HighlightCategory::Keyword),
            span(13..15, HighlightCategory::Number),
        ]
    );
}

#[test]
fn normalize_spans_prefers_higher_priority_overlap() {
    let source = "println!(\"hi\")";
    let normalized = normalize_spans(
        source,
        vec![
            span(0..14, HighlightCategory::Identifier),
            span(9..13, HighlightCategory::String),
        ],
        None,
    );

    assert_eq!(
        normalized,
        vec![
            span(0..9, HighlightCategory::Identifier),
            span(9..13, HighlightCategory::String),
            span(13..14, HighlightCategory::Identifier),
        ]
    );
}

#[test]
fn normalize_spans_merges_adjacent_same_category_segments() {
    let source = "abcdef";
    let normalized = normalize_spans(
        source,
        vec![
            span(0..2, HighlightCategory::Keyword),
            span(2..4, HighlightCategory::Keyword),
            span(4..6, HighlightCategory::Keyword),
        ],
        None,
    );

    assert_eq!(normalized, vec![span(0..6, HighlightCategory::Keyword)]);
}

#[test]
fn normalize_spans_clips_to_byte_window() {
    let source = "abcdef";
    let normalized = normalize_spans(
        source,
        vec![span(0..6, HighlightCategory::String)],
        Some(2..5),
    );

    assert_eq!(normalized, vec![span(2..5, HighlightCategory::String)]);
}

#[test]
fn normalize_spans_sanitizes_utf8_boundaries() {
    let source = "aéb";
    let normalized = normalize_spans(
        source,
        vec![span(1..2, HighlightCategory::String)],
        None,
    );

    assert_eq!(normalized, vec![span(1..3, HighlightCategory::String)]);
}
