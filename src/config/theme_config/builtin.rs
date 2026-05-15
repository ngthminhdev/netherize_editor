use std::collections::HashMap;

use super::model::{
    EditorThemeTokens, FileIconThemeTokens, GitThemeTokens, IconThemeTokens, SyntaxThemeTokens,
    ThemeColor, ThemeConfig, UiThemeTokens,
};

impl ThemeConfig {
    pub fn builtin_dark() -> Self {
        let ui = builtin_ui_tokens();

        Self {
            name: "builtin-dark".to_string(),
            description: Some("Vibrant dark theme optimized for 0-latency wgpu rendering".into()),
            editor: builtin_editor_tokens(),
            git: builtin_git_tokens(),
            syntax: builtin_syntax_tokens(),
            icons: builtin_icon_tokens(&ui),
            exact_icons: HashMap::new(),
            extension_icons: HashMap::new(),
            default_file_icon: "󰈔".to_string(),
            default_folder_icon: "󰉋".to_string(),
            ui,
        }
    }
}

fn builtin_editor_tokens() -> EditorThemeTokens {
    EditorThemeTokens {
        bg: rgb(13, 16, 23),
        fg: rgb(242, 244, 248),
        cursor: rgb(155, 229, 100),
        selection: rgb(30, 37, 53),
        gutter: rgb(13, 16, 23),
        gutter_active: rgb(216, 222, 234),
        indent_guide: rgba(143, 152, 170, 56),
        rainbow_brackets: vec![
            rgb(47, 211, 246),
            rgb(231, 122, 233),
            rgb(245, 182, 58),
            rgb(155, 229, 100),
            rgb(255, 123, 114),
            rgb(183, 138, 255),
        ],
        font_size: 15.0,
        line_height: 26.0,
        font_family: None,
        nerd_font_family: None,
    }
}

fn builtin_ui_tokens() -> UiThemeTokens {
    UiThemeTokens {
        bg: rgb(7, 8, 13),
        sidebar_bg: rgb(15, 19, 32),
        panel_bg: rgb(18, 22, 34),
        terminal_bg: rgb(11, 15, 24),
        overlay_bg: rgba(5, 7, 12, 217),
        status_bar_bg: rgb(11, 14, 22),
        border_color: rgb(43, 48, 64),
        selection_bg: rgb(34, 39, 54),
        dirty_indicator: rgb(245, 182, 58),
        fg: rgb(242, 244, 248),
        fg_dim: rgb(183, 191, 204),
        fg_ghost: rgb(109, 116, 131),
        accent: rgb(155, 229, 100),
        cyan: rgb(47, 211, 246),
        magenta: rgb(231, 122, 233),
        amber: rgb(245, 182, 58),
        success: rgb(103, 214, 124),
        warning: rgb(242, 184, 75),
        info: rgb(73, 198, 248),
        error: rgb(255, 123, 114),
        mode_normal: rgb(47, 211, 246),
        mode_insert: rgb(155, 229, 100),
        mode_visual: rgb(231, 122, 233),
        sidebar_font_size: 11.0,
        sidebar_line_height: 14.0,
        panel_font_size: 14.0,
        panel_line_height: 22.0,
        sidebar_width: 280.0,
        right_sidebar_width: 500.0,
        bottom_panel_height: 220.0,
        top_bar_height: 34.0,
        status_bar_height: 22.0,
    }
}

fn builtin_syntax_tokens() -> SyntaxThemeTokens {
    SyntaxThemeTokens {
        keyword: rgb(231, 122, 233),
        keyword_control: rgb(231, 122, 233),
        keyword_storage: rgb(255, 149, 92),
        string: rgb(103, 214, 124),
        string_escape: rgb(255, 123, 114),
        function: rgb(47, 211, 246),
        function_builtin: rgb(105, 195, 255),
        comment: rgb(109, 116, 131),
        comment_doc: rgb(143, 152, 170),
        r#type: rgb(245, 182, 58),
        type_builtin: rgb(234, 205, 97),
        number: rgb(255, 123, 114),
        boolean: rgb(245, 182, 58),
        identifier: rgb(242, 244, 248),
        variable: rgb(242, 244, 248),
        variable_builtin: rgb(255, 149, 92),
        parameter: rgb(155, 229, 100),
        field: rgb(73, 198, 248),
        property: rgb(183, 191, 204),
        constant: rgb(245, 182, 58),
        operator: rgb(183, 191, 204),
        punctuation: rgb(143, 152, 170),
        escape: rgb(255, 123, 114),
        r#macro: rgb(231, 122, 233),
        lifetime: rgb(255, 123, 114),
        constructor: rgb(245, 182, 58),
        attribute: rgb(231, 122, 233),
        namespace: rgb(245, 182, 58),
        tag: rgb(245, 182, 58),
        markup_strong: rgb(231, 122, 233),
        markup_italic: rgb(143, 152, 170),
        markup_inline_code: rgb(103, 214, 124),
        markup_link: rgb(47, 211, 246),
    }
}

fn builtin_git_tokens() -> GitThemeTokens {
    GitThemeTokens {
        modified_sidebar: rgb(226, 192, 141),
        added_sidebar: rgb(127, 214, 140),
        modified_gutter: rgb(86, 156, 214),
        added_gutter: rgb(80, 216, 144),
        deleted_gutter: rgb(241, 76, 76),
    }
}

fn builtin_icon_tokens(ui: &UiThemeTokens) -> IconThemeTokens {
    IconThemeTokens {
        explorer_file_marker: "·".to_string(),
        explorer_folder_collapsed_marker: "▶".to_string(),
        explorer_folder_expanded_marker: "▼".to_string(),
        file_picker_dot: "●".to_string(),
        folder_closed: icon("\u{F07B}", ui.amber),
        folder_open: icon("\u{F07C}", ui.amber),
        default_file: icon("\u{F15B}", ui.fg_ghost),
        rust: icon("\u{E7A8}", ui.warning),
        javascript: icon("\u{E781}", ui.amber),
        typescript: icon("\u{E628}", ui.info),
        tsx: icon("\u{E7BA}", ui.info),
        jsx: icon("\u{E7BA}", ui.amber),
        java: icon("\u{E738}", ui.error),
        kotlin: icon("\u{E634}", ui.magenta),
        c: icon("\u{E61E}", ui.info),
        cpp: icon("\u{E61D}", ui.info),
        csharp: icon("\u{F81A}", ui.success),
        dart: icon("\u{E798}", ui.cyan),
        swift: icon("\u{E755}", ui.warning),
        php: icon("\u{E73D}", ui.magenta),
        ruby: icon("\u{E739}", ui.error),
        lua: icon("\u{E620}", ui.info),
        zig: icon("\u{E6A9}", ui.warning),
        scala: icon("\u{E737}", ui.error),
        docker: icon("\u{F308}", ui.info),
        sql: icon("\u{E706}", ui.amber),
        xml: icon("\u{E619}", ui.amber),
        gradle: icon("\u{E70E}", ui.success),
        vue: icon("\u{FD42}", ui.success),
        svelte: icon("\u{E697}", ui.error),
        astro: icon("\u{E6B3}", ui.warning),
        elm: icon("\u{E62C}", ui.info),
        haskell: icon("\u{E61F}", ui.magenta),
        ocaml: icon("\u{E67A}", ui.amber),
        r: icon("\u{F25D}", ui.info),
        perl: icon("\u{E769}", ui.info),
        clojure: icon("\u{E768}", ui.success),
        fsharp: icon("\u{E7A7}", ui.info),
        nim: icon("\u{E677}", ui.amber),
        solidity: icon("\u{E6A8}", ui.fg_dim),
        graphql: icon("\u{E662}", ui.magenta),
        toml: icon("\u{E615}", ui.fg_ghost),
        yaml: icon("\u{E60B}", ui.amber),
        makefile: icon("\u{E779}", ui.warning),
        cmake: icon("\u{E794}", ui.info),
        nginx: icon("\u{F146B}", ui.success),
        terraform: icon("\u{E69B}", ui.magenta),
        ansible: icon("\u{E615}", ui.error),
        python: icon("\u{E73C}", ui.info),
        go: icon("\u{E724}", ui.cyan),
        config: icon("\u{E615}", ui.fg_ghost),
        json: icon("\u{E60B}", ui.amber),
        markdown: icon("\u{F48A}", ui.info),
        html: icon("\u{E736}", ui.error),
        css: icon("\u{E749}", ui.info),
        sass: icon("\u{E603}", ui.magenta),
        shell: icon("\u{F489}", ui.success),
        git: icon("\u{F1D3}", ui.warning),
        lock: icon("\u{F13E}", ui.fg_ghost),
        image: icon("\u{F1C5}", ui.success),
        proto: icon("\u{F471}", ui.info),
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> ThemeColor {
    ThemeColor::from_rgba_u8(r, g, b, 255)
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> ThemeColor {
    ThemeColor::from_rgba_u8(r, g, b, a)
}

fn icon(glyph: &str, color: ThemeColor) -> FileIconThemeTokens {
    FileIconThemeTokens {
        glyph: glyph.to_string(),
        color,
    }
}
