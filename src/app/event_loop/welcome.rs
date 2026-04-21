use super::*;

pub(super) fn welcome_screen_content(theme: &ThemeConfig) -> (String, Vec<StyledTextSpan>) {
    let lines = [
        "N E T H E R I Z E",
        "Zero-latency keyboard-first editor",
        "Author: ngthminhdev",
        "",
        "╭────────────────── Start ───────────────────╮",
        "│  [ Cmd + O ]      Open Project / File      │",
        "│  [ Space p j ]    Recent Projects          │",
        "│  [ Space f f ]    Find File                │",
        "╰────────────────────────────────────────────╯",
    ];
    let text = lines.join("\n");
    let mut styled = Vec::new();

    let cyan = theme.ui.cyan.as_u8();
    let accent = theme.ui.accent.as_u8();
    let fg_ghost = theme.ui.fg_ghost.as_u8();
    let fg_dim = theme.ui.fg_dim.as_u8();
    let magenta = theme.ui.magenta.as_u8();

    push_span(&mut styled, &text, "N E T H E R I Z E", cyan);
    push_span(
        &mut styled,
        &text,
        "Zero-latency keyboard-first editor",
        fg_ghost,
    );
    push_span(&mut styled, &text, "Author: ngthminhdev", magenta);

    push_span(
        &mut styled,
        &text,
        "╭────────────────── Start ───────────────────╮",
        fg_ghost,
    );
    push_span(
        &mut styled,
        &text,
        "╰────────────────────────────────────────────╯",
        fg_ghost,
    );

    push_span(&mut styled, &text, "[ Cmd + O ]", accent);
    push_span(&mut styled, &text, "Open Project / File", fg_dim);

    push_span(&mut styled, &text, "[ Space p j ]", accent);
    push_span(&mut styled, &text, "Recent Projects", fg_dim);

    push_span(&mut styled, &text, "[ Space f f ]", accent);
    push_span(&mut styled, &text, "Find File", fg_dim);

    // push_span(&mut styled, &text, "[ Space b ]", accent);
    // push_span(&mut styled, &text, "New Buffer", fg_dim);

    (text, styled)
}

fn push_span(spans: &mut Vec<StyledTextSpan>, text: &str, needle: &str, color: [u8; 4]) {
    if let Some(start) = text.find(needle) {
        spans.push(StyledTextSpan::new(start, start + needle.len(), color));
    }
}
