#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LowercaseByteMap {
    lower_start: usize,
    lower_end: usize,
    original_start: usize,
    original_end: usize,
}

pub fn compute_label_match_ranges(label: &str, query: &str) -> Vec<(usize, usize)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let (label_lower, byte_map) = build_lowercase_byte_map(label);
    let query_lower = trimmed.to_lowercase();

    if let Some(start) = label_lower.find(&query_lower) {
        let end = start + query_lower.len();
        return map_lower_range_to_original(&byte_map, start, end)
            .into_iter()
            .collect();
    }

    let mut ranges = Vec::new();
    let mut search_start = 0usize;
    for q_char in query_lower.chars() {
        let Some((lower_start, lower_end)) = label_lower[search_start..]
            .char_indices()
            .find(|(_, c)| *c == q_char)
            .map(|(offset, c)| {
                let start = search_start + offset;
                (start, start + c.len_utf8())
            })
        else {
            return Vec::new();
        };

        let Some(range) = map_lower_range_to_original(&byte_map, lower_start, lower_end) else {
            return Vec::new();
        };
        push_match_range(&mut ranges, range);
        search_start = lower_end;
    }

    ranges
}

pub fn score_label_match(label: &str, query: &str) -> Option<(i64, Vec<(usize, usize)>)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Some((0, Vec::new()));
    }

    let (label_lower, byte_map) = build_lowercase_byte_map(label);
    let query_lower = trimmed.to_lowercase();

    if let Some(start) = label_lower.find(&query_lower) {
        let end = start + query_lower.len();
        let prefix_bonus = (label_lower.len().saturating_sub(start) as i64).min(64);
        let range = map_lower_range_to_original(&byte_map, start, end)?;
        return Some((10_000 + prefix_bonus, vec![range]));
    }

    let mut ranges = Vec::new();
    let mut score = 0_i64;
    let mut search_start = 0usize;
    let mut previous_end = None;

    for q_char in query_lower.chars() {
        let found = label_lower[search_start..]
            .char_indices()
            .find(|(_, c)| *c == q_char)
            .map(|(offset, c)| {
                let start = search_start + offset;
                (start, start + c.len_utf8())
            });

        let (lower_start, lower_end) = found?;
        let range = map_lower_range_to_original(&byte_map, lower_start, lower_end)?;
        score += 100;
        if lower_start == 0 {
            score += 25;
        }
        if let Some(prev_end) = previous_end {
            if lower_start == prev_end {
                score += 40;
            } else {
                score -= (lower_start.saturating_sub(prev_end) as i64).min(24);
            }
        } else {
            score += (32_i64 - lower_start as i64).max(0);
        }

        push_match_range(&mut ranges, range);
        previous_end = Some(lower_end);
        search_start = lower_end;
    }

    Some((score, ranges))
}

fn build_lowercase_byte_map(label: &str) -> (String, Vec<LowercaseByteMap>) {
    let mut lowered = String::new();
    let mut byte_map = Vec::new();
    let mut chars = label.char_indices().peekable();

    while let Some((original_start, ch)) = chars.next() {
        let original_end = chars.peek().map(|(idx, _)| *idx).unwrap_or(label.len());
        for lowered_ch in ch.to_lowercase() {
            let lower_start = lowered.len();
            lowered.push(lowered_ch);
            let lower_end = lowered.len();
            byte_map.push(LowercaseByteMap {
                lower_start,
                lower_end,
                original_start,
                original_end,
            });
        }
    }

    (lowered, byte_map)
}

fn map_lower_range_to_original(
    byte_map: &[LowercaseByteMap],
    lower_start: usize,
    lower_end: usize,
) -> Option<(usize, usize)> {
    if lower_start >= lower_end {
        return None;
    }

    let start = byte_map
        .iter()
        .find(|entry| lower_start >= entry.lower_start && lower_start < entry.lower_end)
        .map(|entry| entry.original_start)?;
    let end = byte_map
        .iter()
        .find(|entry| lower_end > entry.lower_start && lower_end <= entry.lower_end)
        .map(|entry| entry.original_end)?;

    (start < end).then_some((start, end))
}

fn push_match_range(ranges: &mut Vec<(usize, usize)>, range: (usize, usize)) {
    if let Some(last) = ranges.last_mut()
        && range.0 <= last.1
    {
        last.1 = last.1.max(range.1);
        return;
    }
    ranges.push(range);
}

#[cfg(test)]
mod tests {
    use super::{compute_label_match_ranges, score_label_match};

    #[test]
    fn match_ranges_map_expanding_lowercase_back_to_original_bytes() {
        let label = "İİabc";
        let ranges = compute_label_match_ranges(label, "c");
        assert_eq!(ranges, vec![(6, 7)]);
    }

    #[test]
    fn scored_match_uses_original_byte_ranges_for_unicode_labels() {
        let label = "İİabc";
        let (_, ranges) = score_label_match(label, "abc").expect("score");
        assert_eq!(ranges, vec![(4, 7)]);
    }
}
