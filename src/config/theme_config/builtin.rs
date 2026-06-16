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
            default_file_icon: "built_in:file".to_string(),
            default_folder_icon: "built_in:folder".to_string(),
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
        yank_flash: None,
        bracket_ripple: None,
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
        folder_closed: icon("built_in:folder", ui.amber),
        folder_open: icon("built_in:folder_open", ui.amber),
        default_file: icon("built_in:file", ui.fg_ghost),
        rust: icon("built_in:rust", ui.warning),
        javascript: icon("built_in:node", ui.amber),
        typescript: icon("built_in:typescript", ui.info),
        tsx: icon("built_in:typescript", ui.info),
        jsx: icon("built_in:node", ui.amber),
        java: icon("built_in:java", ui.error),
        kotlin: icon("built_in:kotlin", ui.magenta),
        c: icon("built_in:c", ui.info),
        cpp: icon("built_in:cpp", ui.info),
        csharp: icon("built_in:csharp", ui.success),
        dart: icon("built_in:dart", ui.cyan),
        swift: icon("built_in:swift", ui.warning),
        php: icon("built_in:php", ui.magenta),
        ruby: icon("built_in:ruby", ui.error),
        lua: icon("built_in:lua", ui.info),
        zig: icon("built_in:zig", ui.warning),
        scala: icon("built_in:scala", ui.error),
        docker: icon("built_in:docker", ui.info),
        sql: icon("built_in:sql", ui.amber),
        xml: icon("built_in:xml", ui.amber),
        gradle: icon("built_in:gradle", ui.success),
        vue: icon("built_in:vue", ui.success),
        svelte: icon("built_in:svelte", ui.error),
        astro: icon("built_in:astro", ui.warning),
        elm: icon("built_in:elm", ui.info),
        haskell: icon("built_in:haskell", ui.magenta),
        ocaml: icon("built_in:ocaml", ui.amber),
        r: icon("built_in:r", ui.info),
        perl: icon("built_in:perl", ui.info),
        clojure: icon("built_in:clojure", ui.success),
        fsharp: icon("built_in:fsharp", ui.info),
        nim: icon("built_in:nim", ui.amber),
        solidity: icon("built_in:sol", ui.fg_dim),
        graphql: icon("built_in:graphql", ui.magenta),
        toml: icon("built_in:toml", ui.fg_ghost),
        yaml: icon("built_in:yaml", ui.amber),
        makefile: icon("built_in:makefile", ui.warning),
        cmake: icon("built_in:cmake", ui.info),
        nginx: icon("built_in:nginx", ui.success),
        terraform: icon("built_in:terraform", ui.magenta),
        ansible: icon("built_in:ansible", ui.error),
        python: icon("built_in:python", ui.info),
        go: icon("built_in:go", ui.cyan),
        config: icon("built_in:conf", ui.fg_ghost),
        json: icon("built_in:json", ui.amber),
        markdown: icon("built_in:markdown", ui.info),
        html: icon("built_in:html", ui.error),
        css: icon("built_in:css", ui.info),
        sass: icon("built_in:sass", ui.magenta),
        shell: icon("built_in:shell", ui.success),
        git: icon("built_in:git", ui.warning),
        lock: icon("built_in:lock", ui.fg_ghost),
        image: icon("built_in:image", ui.success),
        proto: icon("built_in:proto", ui.info),
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
