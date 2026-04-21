use super::*;

pub(super) fn welcome_screen_content(theme: &ThemeConfig) -> (String, Vec<StyledTextSpan>) {
    let lines = [
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "                          Netherize Editor",
        "",
        "                    Author: ngthminhdev",
        "",
        "                 Press Mod+P to open the file picker",
        "                   Press i to enter INSERT mode",
        "                Press Esc to return to NORMAL mode",
    ];
    let text = lines.join("\n");
    let mut styled = Vec::new();

    push_highlight_once(
        &mut styled,
        &text,
        "Netherize Editor",
        theme.ui.accent.as_u8(),
    );
    push_highlight_once(&mut styled, &text, "Author:", theme.ui.amber.as_u8());
    push_highlight_once(&mut styled, &text, "ngthminhdev", theme.ui.magenta.as_u8());
    push_highlight_once(&mut styled, &text, "Mod+O", theme.ui.cyan.as_u8());
    push_highlight_once(&mut styled, &text, "Mod+P", theme.ui.cyan.as_u8());
    push_highlight_once(&mut styled, &text, "INSERT", theme.ui.success.as_u8());
    push_highlight_once(&mut styled, &text, "NORMAL", theme.ui.accent.as_u8());

    (text, styled)
}

fn push_highlight_once(spans: &mut Vec<StyledTextSpan>, text: &str, needle: &str, color: [u8; 4]) {
    if let Some(start) = text.find(needle) {
        spans.push(StyledTextSpan::new(start, start + needle.len(), color));
    }
}
