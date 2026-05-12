use super::categories::HighlightCategory;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub category: HighlightCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightEdit {
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

impl HighlightEdit {
    pub fn insert(start: usize, inserted_len: usize) -> Self {
        Self {
            start,
            old_end: start,
            new_end: start.saturating_add(inserted_len),
        }
    }

    pub fn delete(start: usize, old_end: usize) -> Self {
        Self {
            start,
            old_end,
            new_end: start,
        }
    }
}

pub fn apply_highlight_edits(spans: &mut Vec<HighlightSpan>, edits: &[HighlightEdit]) {
    if spans.is_empty() || edits.is_empty() {
        return;
    }

    let mut adjusted = std::mem::take(spans);
    for edit in edits {
        adjusted = adjusted
            .into_iter()
            .filter_map(|span| transform_span_by_edit(span, *edit))
            .collect();
    }

    *spans = coalesce_spans(adjusted);
}

pub fn merge_highlight_spans(
    spans: &mut Vec<HighlightSpan>,
    replacement: Vec<HighlightSpan>,
    covered_byte_range: Option<Range<usize>>,
) {
    let Some(window) = covered_byte_range else {
        *spans = coalesce_spans(replacement);
        return;
    };

    let mut merged = Vec::with_capacity(spans.len() + replacement.len() + 2);
    for span in std::mem::take(spans) {
        if span.range.end <= window.start || span.range.start >= window.end {
            merged.push(span);
            continue;
        }

        if span.range.start < window.start {
            merged.push(HighlightSpan {
                range: span.range.start..window.start,
                category: span.category,
            });
        }
        if span.range.end > window.end {
            merged.push(HighlightSpan {
                range: window.end..span.range.end,
                category: span.category,
            });
        }
    }

    merged.extend(replacement);
    *spans = coalesce_spans(merged);
}

pub fn overlay_highlight_layers(
    base: &[HighlightSpan],
    overrides: &[HighlightSpan],
) -> Vec<HighlightSpan> {
    if overrides.is_empty() {
        return base.to_vec();
    }

    let mut merged = coalesce_spans(base.to_vec());
    for span in overrides.iter().cloned() {
        let window = span.range.clone();
        merge_highlight_spans(&mut merged, vec![span], Some(window));
    }
    merged
}

pub fn expand_merge_window(
    existing: &[HighlightSpan],
    replacement: &[HighlightSpan],
    mut window: Range<usize>,
) -> Range<usize> {
    for span in existing {
        if span.range.end > window.start && span.range.start < window.end {
            window.start = window.start.min(span.range.start);
            window.end = window.end.max(span.range.end);
        }
    }

    for span in replacement {
        if span.range.end > window.start && span.range.start < window.end {
            window.start = window.start.min(span.range.start);
            window.end = window.end.max(span.range.end);
        }
    }

    window
}

pub(crate) fn transform_span_by_edit(
    span: HighlightSpan,
    edit: HighlightEdit,
) -> Option<HighlightSpan> {
    let start = span.range.start;
    let end = span.range.end;
    if start >= end {
        return None;
    }

    if edit.old_end == edit.start {
        let inserted_len = edit.new_end.saturating_sub(edit.start);
        if inserted_len == 0 {
            return Some(span);
        }

        if end <= edit.start {
            return Some(span);
        }

        if start > edit.start {
            return Some(HighlightSpan {
                range: (start + inserted_len)..(end + inserted_len),
                category: span.category,
            });
        }

        return Some(HighlightSpan {
            range: start..(end + inserted_len),
            category: span.category,
        });
    }

    if end <= edit.start {
        return Some(span);
    }

    let removed_len = edit.old_end.saturating_sub(edit.start);
    if removed_len == 0 {
        return Some(span);
    }

    if start >= edit.old_end {
        return Some(HighlightSpan {
            range: (start - removed_len)..(end - removed_len),
            category: span.category,
        });
    }

    let new_start = if start >= edit.start {
        edit.start
    } else {
        start
    };
    let new_end = if end <= edit.old_end {
        edit.start
    } else {
        end - removed_len
    };

    (new_start < new_end).then_some(HighlightSpan {
        range: new_start..new_end,
        category: span.category,
    })
}

pub(crate) fn coalesce_spans(mut spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    spans.retain(|span| span.range.start < span.range.end);
    spans.sort_by_key(|span| (span.range.start, span.range.end));

    let mut merged: Vec<HighlightSpan> = Vec::with_capacity(spans.len());
    for mut span in spans {
        if let Some(last) = merged.last_mut() {
            if span.range.start < last.range.end {
                if span.category == last.category {
                    last.range.end = last.range.end.max(span.range.end);
                    continue;
                }

                span.range.start = last.range.end;
                if span.range.start >= span.range.end {
                    continue;
                }
            }

            if last.category == span.category && last.range.end == span.range.start {
                last.range.end = span.range.end;
                continue;
            }
        }

        merged.push(span);
    }

    merged
}
