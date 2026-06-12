use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColor {
    rgba_u8: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl LinearColor {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn as_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: f64::from(self.r),
            g: f64::from(self.g),
            b: f64::from(self.b),
            a: f64::from(self.a),
        }
    }
}

impl ThemeColor {
    pub const fn from_rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            rgba_u8: [r, g, b, a],
        }
    }

    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let value = hex.trim();
        let digits = value.strip_prefix('#').unwrap_or(value);

        match digits.len() {
            6 => {
                let r = parse_hex_component(&digits[0..2], value)?;
                let g = parse_hex_component(&digits[2..4], value)?;
                let b = parse_hex_component(&digits[4..6], value)?;
                Ok(Self::from_rgba_u8(r, g, b, 255))
            }
            8 => {
                let r = parse_hex_component(&digits[0..2], value)?;
                let g = parse_hex_component(&digits[2..4], value)?;
                let b = parse_hex_component(&digits[4..6], value)?;
                let a = parse_hex_component(&digits[6..8], value)?;
                Ok(Self::from_rgba_u8(r, g, b, a))
            }
            _ => Err(format!(
                "invalid hex color '{value}' (expected #RRGGBB or #RRGGBBAA)"
            )),
        }
    }

    pub const fn as_u8(self) -> [u8; 4] {
        self.rgba_u8
    }

    pub fn as_srgb_f32(self) -> [f32; 4] {
        [
            f32::from(self.rgba_u8[0]) / 255.0,
            f32::from(self.rgba_u8[1]) / 255.0,
            f32::from(self.rgba_u8[2]) / 255.0,
            f32::from(self.rgba_u8[3]) / 255.0,
        ]
    }

    pub fn as_linear(self) -> LinearColor {
        let [r, g, b, a] = self.as_srgb_f32();
        LinearColor::new(srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a)
    }

    pub fn as_f32(self) -> [f32; 4] {
        self.as_linear().as_array()
    }
}

pub fn srgb_to_linear(channel: f32) -> f32 {
    let value = channel.clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(channel: f32) -> f32 {
    let value = channel.clamp(0.0, 1.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

pub fn srgb_rgba_to_linear_f32(rgba: [u8; 4]) -> [f32; 4] {
    [
        srgb_to_linear(f32::from(rgba[0]) / 255.0),
        srgb_to_linear(f32::from(rgba[1]) / 255.0),
        srgb_to_linear(f32::from(rgba[2]) / 255.0),
        f32::from(rgba[3]) / 255.0,
    ]
}

pub fn linear_rgba_to_srgb_u8(rgba: [f32; 4]) -> [u8; 4] {
    [
        f32_channel_to_u8(linear_to_srgb(rgba[0])),
        f32_channel_to_u8(linear_to_srgb(rgba[1])),
        f32_channel_to_u8(linear_to_srgb(rgba[2])),
        f32_channel_to_u8(rgba[3]),
    ]
}

fn parse_hex_component(component: &str, original: &str) -> Result<u8, String> {
    u8::from_str_radix(component, 16)
        .map_err(|_| format!("invalid hex color '{original}' (bad component '{component}')"))
}

fn f32_channel_to_u8(channel: f32) -> u8 {
    let clamped = channel.clamp(0.0, 1.0);
    (clamped * 255.0).round() as u8
}

#[derive(Debug, Clone)]
pub struct ThemeConfig {
    pub name: String,
    pub description: Option<String>,
    pub editor: EditorThemeTokens,
    pub ui: UiThemeTokens,
    pub git: GitThemeTokens,
    pub syntax: SyntaxThemeTokens,
    pub icons: IconThemeTokens,
    pub exact_icons: HashMap<String, String>,
    pub extension_icons: HashMap<String, String>,
    pub default_file_icon: String,
    pub default_folder_icon: String,
}

#[derive(Debug, Clone)]
pub struct EditorThemeTokens {
    pub bg: ThemeColor,
    pub fg: ThemeColor,
    pub cursor: ThemeColor,
    pub selection: ThemeColor,
    pub gutter: ThemeColor,
    pub gutter_active: ThemeColor,
    pub indent_guide: ThemeColor,
    pub rainbow_brackets: Vec<ThemeColor>,
    pub font_size: f32,
    pub line_height: f32,
    pub font_family: Option<String>,
    pub nerd_font_family: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UiThemeTokens {
    pub bg: ThemeColor,
    pub sidebar_bg: ThemeColor,
    pub panel_bg: ThemeColor,
    pub terminal_bg: ThemeColor,
    pub overlay_bg: ThemeColor,
    pub status_bar_bg: ThemeColor,
    pub border_color: ThemeColor,
    pub selection_bg: ThemeColor,
    pub dirty_indicator: ThemeColor,
    pub fg: ThemeColor,
    pub fg_dim: ThemeColor,
    pub fg_ghost: ThemeColor,
    pub accent: ThemeColor,
    pub cyan: ThemeColor,
    pub magenta: ThemeColor,
    pub amber: ThemeColor,
    pub success: ThemeColor,
    pub warning: ThemeColor,
    pub info: ThemeColor,
    pub error: ThemeColor,
    pub mode_normal: ThemeColor,
    pub mode_insert: ThemeColor,
    pub mode_visual: ThemeColor,
    pub sidebar_font_size: f32,
    pub sidebar_line_height: f32,
    pub panel_font_size: f32,
    pub panel_line_height: f32,
    /// Default width of the left explorer sidebar, px.
    pub sidebar_width: f32,
    /// Default width of the right inspector sidebar, px.
    pub right_sidebar_width: f32,
    /// Default height of the bottom terminal panel, px.
    pub bottom_panel_height: f32,
    /// Height of the top tab/title bar, px.
    pub top_bar_height: f32,
    /// Height of the status bar, px.
    pub status_bar_height: f32,
}

#[derive(Debug, Clone)]
pub struct GitThemeTokens {
    pub modified_sidebar: ThemeColor,
    pub added_sidebar: ThemeColor,
    pub modified_gutter: ThemeColor,
    pub added_gutter: ThemeColor,
    pub deleted_gutter: ThemeColor,
}

#[derive(Debug, Clone)]
pub struct SyntaxThemeTokens {
    pub keyword: ThemeColor,
    pub keyword_control: ThemeColor,
    pub keyword_storage: ThemeColor,
    pub string: ThemeColor,
    pub string_escape: ThemeColor,
    pub function: ThemeColor,
    pub function_builtin: ThemeColor,
    pub comment: ThemeColor,
    pub comment_doc: ThemeColor,
    pub r#type: ThemeColor,
    pub type_builtin: ThemeColor,
    pub number: ThemeColor,
    pub boolean: ThemeColor,
    pub identifier: ThemeColor,
    pub variable: ThemeColor,
    pub variable_builtin: ThemeColor,
    pub parameter: ThemeColor,
    pub field: ThemeColor,
    pub property: ThemeColor,
    pub constant: ThemeColor,
    pub operator: ThemeColor,
    pub punctuation: ThemeColor,
    pub escape: ThemeColor,
    pub r#macro: ThemeColor,
    pub lifetime: ThemeColor,
    pub constructor: ThemeColor,
    pub attribute: ThemeColor,
    pub namespace: ThemeColor,
    pub tag: ThemeColor,
    pub markup_strong: ThemeColor,
    pub markup_italic: ThemeColor,
    pub markup_inline_code: ThemeColor,
    pub markup_link: ThemeColor,
}

#[derive(Debug, Clone)]
pub struct FileIconThemeTokens {
    pub glyph: String,
    pub color: ThemeColor,
}

#[derive(Debug, Clone)]
pub struct IconThemeTokens {
    pub explorer_file_marker: String,
    pub explorer_folder_collapsed_marker: String,
    pub explorer_folder_expanded_marker: String,
    pub file_picker_dot: String,
    pub folder_closed: FileIconThemeTokens,
    pub folder_open: FileIconThemeTokens,
    pub default_file: FileIconThemeTokens,
    pub rust: FileIconThemeTokens,
    pub javascript: FileIconThemeTokens,
    pub typescript: FileIconThemeTokens,
    pub tsx: FileIconThemeTokens,
    pub jsx: FileIconThemeTokens,
    pub java: FileIconThemeTokens,
    pub kotlin: FileIconThemeTokens,
    pub c: FileIconThemeTokens,
    pub cpp: FileIconThemeTokens,
    pub csharp: FileIconThemeTokens,
    pub dart: FileIconThemeTokens,
    pub swift: FileIconThemeTokens,
    pub php: FileIconThemeTokens,
    pub ruby: FileIconThemeTokens,
    pub lua: FileIconThemeTokens,
    pub zig: FileIconThemeTokens,
    pub scala: FileIconThemeTokens,
    pub docker: FileIconThemeTokens,
    pub sql: FileIconThemeTokens,
    pub xml: FileIconThemeTokens,
    pub gradle: FileIconThemeTokens,
    pub vue: FileIconThemeTokens,
    pub svelte: FileIconThemeTokens,
    pub astro: FileIconThemeTokens,
    pub elm: FileIconThemeTokens,
    pub haskell: FileIconThemeTokens,
    pub ocaml: FileIconThemeTokens,
    pub r: FileIconThemeTokens,
    pub perl: FileIconThemeTokens,
    pub clojure: FileIconThemeTokens,
    pub fsharp: FileIconThemeTokens,
    pub nim: FileIconThemeTokens,
    pub solidity: FileIconThemeTokens,
    pub graphql: FileIconThemeTokens,
    pub toml: FileIconThemeTokens,
    pub yaml: FileIconThemeTokens,
    pub makefile: FileIconThemeTokens,
    pub cmake: FileIconThemeTokens,
    pub nginx: FileIconThemeTokens,
    pub terraform: FileIconThemeTokens,
    pub ansible: FileIconThemeTokens,
    pub python: FileIconThemeTokens,
    pub go: FileIconThemeTokens,
    pub config: FileIconThemeTokens,
    pub json: FileIconThemeTokens,
    pub markdown: FileIconThemeTokens,
    pub html: FileIconThemeTokens,
    pub css: FileIconThemeTokens,
    pub sass: FileIconThemeTokens,
    pub shell: FileIconThemeTokens,
    pub git: FileIconThemeTokens,
    pub lock: FileIconThemeTokens,
    pub image: FileIconThemeTokens,
    pub proto: FileIconThemeTokens,
}

fn normalize_icon_filename(filename: &str) -> String {
    let file_name = Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(filename);
    file_name.trim().to_ascii_lowercase()
}

fn special_icon_for_filename(filename: &str) -> Option<&'static str> {
    if let Some(icon) = nestjs_role_icon_for_filename(filename) {
        return Some(icon);
    }

    if filename == "dockerfile"
        || filename.starts_with("dockerfile.")
        || filename.ends_with(".dockerfile")
    {
        return Some("built_in:docker");
    }
    if filename == ".dockerignore" {
        return Some("built_in:dockerignore");
    }
    if matches!(
        filename,
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml"
    ) {
        return Some("built_in:docker");
    }

    if filename == "cargo.lock" {
        return Some("built_in:cargolock");
    }
    if filename == "package.json" {
        return Some("built_in:node");
    }
    if matches!(filename, "package-lock.json" | "npm-shrinkwrap.json") {
        return Some("built_in:npmlock");
    }
    if filename == "pnpm-lock.yaml" {
        return Some("built_in:pnpmlock");
    }
    if filename == "yarn.lock" {
        return Some("built_in:yarnlock");
    }
    if matches!(filename, "bun.lock" | "bun.lockb") {
        return Some("built_in:bunlock");
    }
    if filename == "composer.lock" {
        return Some("built_in:composerlock");
    }
    if filename == "flake.lock" {
        return Some("built_in:flakelock");
    }
    if filename == "pubspec.lock" {
        return Some("built_in:flutterlock");
    }
    if filename == "mix.lock" {
        return Some("built_in:mixlock");
    }
    if filename == "poetry.lock" {
        return Some("built_in:poetrylock");
    }

    if filename.starts_with("tsconfig") && filename.ends_with(".json") {
        return Some("built_in:tsconfig");
    }
    if filename.starts_with("next.config.") {
        return Some("built_in:nextconfig");
    }
    if filename.starts_with("svelte.config.") {
        return Some("built_in:svelteconfig");
    }
    if filename.starts_with("vue.config.") {
        return Some("built_in:vueconfig");
    }
    if filename.starts_with("astro.config.") {
        return Some("built_in:astroconfig");
    }
    if filename.starts_with("postcss.config.") {
        return Some("built_in:postcssconfig");
    }
    if filename == ".editorconfig" {
        return Some("built_in:editorconfig");
    }

    if filename.starts_with("vitest.config.") {
        return Some("built_in:vitest");
    }
    if filename.starts_with("jest.config.")
        || filename == ".jestrc"
        || filename.starts_with(".jestrc.")
    {
        return Some("built_in:jest");
    }
    if filename.starts_with("cypress.config.ts") {
        return Some("built_in:cypressts");
    }
    if filename.starts_with("cypress.config.js") {
        return Some("built_in:cypressjs");
    }
    if filename.starts_with("playwright.config.") {
        return Some("built_in:playright");
    }
    if filename.contains(".stories.") || filename.contains(".story.") {
        return Some("built_in:storybook");
    }
    if filename.contains(".test.ts") || filename.contains(".spec.ts") {
        return Some("built_in:testts");
    }
    if filename.contains(".test.js") || filename.contains(".spec.js") {
        return Some("built_in:testjs");
    }

    if filename == ".eslintignore" {
        return Some("built_in:eslintignore");
    }
    if filename.starts_with("eslint.config.")
        || filename == ".eslintrc"
        || filename.starts_with(".eslintrc.")
    {
        return Some("built_in:eslint");
    }
    if filename == ".prettierignore" {
        return Some("built_in:prettierignore");
    }
    if filename.starts_with("prettier.config.")
        || filename == ".prettierrc"
        || filename.starts_with(".prettierrc.")
    {
        return Some("built_in:prettier");
    }
    if filename.starts_with("babel.config.")
        || filename == ".babelrc"
        || filename.starts_with(".babelrc.")
    {
        return Some("built_in:babel");
    }
    if filename.starts_with("webpack.config.") {
        return Some("built_in:webpack");
    }
    if filename.starts_with("rollup.config.") {
        return Some("built_in:rollup");
    }
    if filename.starts_with("vite.config.") {
        return Some("built_in:vite");
    }
    if filename.starts_with("tailwind.config.") {
        return Some("built_in:tailwind");
    }
    if filename.starts_with("uno.config.") || filename.starts_with("unocss.config.") {
        return Some("built_in:unocss");
    }
    if filename.starts_with("windi.config.") {
        return Some("built_in:windi");
    }
    if matches!(filename, "biome.json" | "biome.jsonc") {
        return Some("built_in:biome");
    }
    if filename == "oxlint.json" {
        return Some("built_in:oxlint");
    }
    if filename.starts_with("commitlint.config.") {
        return Some("built_in:commitlint");
    }
    if matches!(filename, ".browserslistrc" | "browserslist") {
        return Some("built_in:browserslist");
    }
    if filename == "nodemon.json" {
        return Some("built_in:nodemon");
    }
    if filename == ".nvmrc" {
        return Some("built_in:nvm");
    }
    if filename == "nx.json" {
        return Some("built_in:nx");
    }
    if filename == "turbo.json" {
        return Some("built_in:turbo");
    }

    if filename.starts_with("nuxt.config.") {
        return Some("built_in:nuxt");
    }
    if filename.starts_with("remix.config.") {
        return Some("built_in:remix");
    }
    if filename == "tauri.conf.json" {
        return Some("built_in:tauri");
    }
    if filename == "vercel.json" {
        return Some("built_in:vercel");
    }
    if filename == "netlify.toml" {
        return Some("built_in:netlify");
    }

    if filename == "schema.prisma" {
        return Some("built_in:prisma");
    }
    if filename.starts_with("drizzle.config.") {
        return Some("built_in:drizzle");
    }
    if filename == ".sequelizerc" || filename.starts_with("sequelize.config.") {
        return Some("built_in:sequelize");
    }
    if filename.starts_with("knexfile.") {
        return Some("built_in:knex");
    }

    if matches!(filename, ".gitignore" | ".gitattributes" | ".gitmodules") {
        return Some("built_in:git");
    }
    if filename == ".gitlab-ci.yml" || filename == ".gitlab-ci.yaml" {
        return Some("built_in:gitlab");
    }
    if filename == ".pre-commit-config.yaml" || filename == ".pre-commit-config.yml" {
        return Some("built_in:precommit");
    }
    if filename == ".gitkeep" {
        return Some("built_in:keep");
    }

    if filename.starts_with("readme") {
        return Some("built_in:readme");
    }
    if filename.starts_with("license") || filename.starts_with("copying") {
        return Some("built_in:license");
    }
    if filename.ends_with(".code-workspace") {
        return Some("built_in:codeworkspace");
    }
    if filename == "manifest.json" || filename == "manifest.webmanifest" {
        return Some("built_in:manifest");
    }

    None
}

fn built_in_icon_id_for_extension(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "rs" => "built_in:rust",
        "js" | "mjs" | "cjs" => "built_in:javascript",
        "ts" | "mts" | "cts" => "built_in:typescript",
        "tsx" => "built_in:tsx",
        "jsx" => "built_in:reactjs",
        "java" => "built_in:java",
        "kt" | "kts" => "built_in:kotlin",
        "c" | "h" => "built_in:c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => "built_in:cpp",
        "cs" => "built_in:csharp",
        "dart" => "built_in:dart",
        "swift" => "built_in:swift",
        "php" => "built_in:php",
        "rb" => "built_in:ruby",
        "lua" => "built_in:lua",
        "zig" => "built_in:zig",
        "scala" | "sc" => "built_in:scala",
        "py" | "pyw" => "built_in:python",
        "go" => "built_in:go",
        "ini" | "env" => "built_in:conf",
        "json" | "jsonc" => "built_in:json",
        "md" | "mdx" | "markdown" => "built_in:markdown",
        "html" | "htm" => "built_in:html",
        "css" => "built_in:css",
        "scss" | "sass" => "built_in:sass",
        "sh" | "bash" | "zsh" | "fish" => "built_in:shell",
        "gitignore" | "gitmodules" | "gitattributes" => "built_in:git",
        "lock" => "built_in:lock",
        "proto" | "protobuf" => "built_in:proto",
        "dockerfile" | "containerfile" => "built_in:docker",
        "sql" => "built_in:sql",
        "xml" | "xsd" | "xsl" | "xslt" | "plist" => "built_in:xml",
        "gradle" => "built_in:gradle",
        "vue" => "built_in:vue",
        "svelte" => "built_in:svelte",
        "astro" => "built_in:astro",
        "elm" => "built_in:elm",
        "hs" => "built_in:haskell",
        "ml" | "mli" => "built_in:ocaml",
        "r" => "built_in:r",
        "pl" | "pm" => "built_in:perl",
        "clj" | "cljs" | "cljc" | "edn" => "built_in:clojure",
        "fs" | "fsi" | "fsx" => "built_in:fsharp",
        "nim" => "built_in:nim",
        "sol" => "built_in:sol",
        "graphql" | "gql" => "built_in:graphql",
        "toml" => "built_in:toml",
        "yaml" | "yml" => "built_in:yaml",
        "mk" | "mak" => "built_in:makefile",
        "cmake" => "built_in:cmake",
        "conf" | "nginxconf" => "built_in:nginx",
        "tf" | "tfvars" | "hcl" => "built_in:terraform",
        "ansible" => "built_in:ansible",
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" => "built_in:image",
        _ => return None,
    })
}

fn nestjs_role_icon_for_filename(filename: &str) -> Option<&'static str> {
    const ROLES: &[(&str, &str)] = &[
        ("service", "built_in:nestjsservice"),
        ("controller", "built_in:nestjscontroller"),
        ("decorator", "built_in:nestjsdecorator"),
        ("dto", "built_in:nestjsdto"),
        ("entity", "built_in:nestjsentity"),
        ("filter", "built_in:nestjsfilter"),
        ("guard", "built_in:nestjsguard"),
        ("interceptor", "built_in:nestjsinterceptor"),
        ("module", "built_in:nestjsmodule"),
        ("repository", "built_in:nestjsrepository"),
        ("resolver", "built_in:nestjsresolver"),
        ("scheduler", "built_in:nestscheduler"),
    ];

    for (role, icon) in ROLES {
        let dot_role = format!(".{role}");
        let dash_role = format!("-{role}");
        if filename.ends_with(&dot_role)
            || filename.contains(&format!("{dot_role}."))
            || filename.ends_with(&dash_role)
            || filename.contains(&format!("{dash_role}."))
        {
            return Some(icon);
        }
    }

    None
}

impl ThemeConfig {
    pub fn sidebar_arrow(&self, is_dir: bool, is_expanded: bool) -> &str {
        if is_dir {
            if is_expanded {
                self.icons.explorer_folder_expanded_marker.as_str()
            } else {
                self.icons.explorer_folder_collapsed_marker.as_str()
            }
        } else {
            self.icons.explorer_file_marker.as_str()
        }
    }

    pub fn file_icon_for_path(
        &self,
        path: &Path,
        is_dir: bool,
        is_expanded: bool,
    ) -> &FileIconThemeTokens {
        if is_dir {
            if is_expanded {
                &self.icons.folder_open
            } else {
                &self.icons.folder_closed
            }
        } else {
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            self.file_icon_for_extension(extension)
        }
    }

    pub fn file_icon_for_extension(&self, ext: &str) -> &FileIconThemeTokens {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => &self.icons.rust,
            "js" | "mjs" | "cjs" => &self.icons.javascript,
            "ts" => &self.icons.typescript,
            "tsx" => &self.icons.tsx,
            "jsx" => &self.icons.jsx,
            "java" => &self.icons.java,
            "kt" | "kts" => &self.icons.kotlin,
            "c" | "h" => &self.icons.c,
            "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => &self.icons.cpp,
            "cs" => &self.icons.csharp,
            "dart" => &self.icons.dart,
            "swift" => &self.icons.swift,
            "php" => &self.icons.php,
            "rb" => &self.icons.ruby,
            "lua" => &self.icons.lua,
            "zig" => &self.icons.zig,
            "scala" | "sc" => &self.icons.scala,
            "py" | "pyw" => &self.icons.python,
            "go" => &self.icons.go,
            "ini" | "env" => &self.icons.config,
            "json" | "jsonc" => &self.icons.json,
            "md" | "mdx" | "markdown" => &self.icons.markdown,
            "html" | "htm" => &self.icons.html,
            "css" => &self.icons.css,
            "scss" | "sass" => &self.icons.sass,
            "sh" | "bash" | "zsh" | "fish" => &self.icons.shell,
            "gitignore" | "gitmodules" | "gitattributes" => &self.icons.git,
            "lock" => &self.icons.lock,
            "proto" | "protobuf" => &self.icons.proto,
            "dockerfile" | "containerfile" => &self.icons.docker,
            "sql" => &self.icons.sql,
            "xml" | "xsd" | "xsl" | "xslt" | "plist" => &self.icons.xml,
            "gradle" => &self.icons.gradle,
            "vue" => &self.icons.vue,
            "svelte" => &self.icons.svelte,
            "astro" => &self.icons.astro,
            "elm" => &self.icons.elm,
            "hs" => &self.icons.haskell,
            "ml" | "mli" => &self.icons.ocaml,
            "r" => &self.icons.r,
            "pl" | "pm" => &self.icons.perl,
            "clj" | "cljs" | "cljc" | "edn" => &self.icons.clojure,
            "fs" | "fsi" | "fsx" => &self.icons.fsharp,
            "nim" => &self.icons.nim,
            "sol" => &self.icons.solidity,
            "graphql" | "gql" => &self.icons.graphql,
            "toml" => &self.icons.toml,
            "yaml" | "yml" => &self.icons.yaml,
            "mk" | "mak" => &self.icons.makefile,
            "cmake" => &self.icons.cmake,
            "conf" | "nginxconf" => &self.icons.nginx,
            "tf" | "tfvars" | "hcl" => &self.icons.terraform,
            "ansible" => &self.icons.ansible,
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" => &self.icons.image,
            _ => &self.icons.default_file,
        }
    }

    pub fn icon_theme_for_filename(&self, filename: &str, is_dir: bool) -> &FileIconThemeTokens {
        if is_dir {
            return &self.icons.folder_closed;
        }

        if filename.eq_ignore_ascii_case("cargo.toml") {
            return &self.icons.rust;
        }
        if filename.eq_ignore_ascii_case("package.json")
            || filename.eq_ignore_ascii_case("package-lock.json")
        {
            return &self.icons.javascript;
        }
        if filename.eq_ignore_ascii_case("tsconfig.json") {
            return &self.icons.typescript;
        }
        if filename.eq_ignore_ascii_case("go.mod") || filename.eq_ignore_ascii_case("go.sum") {
            return &self.icons.go;
        }
        if filename.eq_ignore_ascii_case("pom.xml") {
            return &self.icons.java;
        }
        if filename.eq_ignore_ascii_case("flake.nix")
            || filename.eq_ignore_ascii_case("default.nix")
        {
            return &self.icons.config;
        }
        if filename.eq_ignore_ascii_case("pyproject.toml")
            || filename.eq_ignore_ascii_case("requirements.txt")
        {
            return &self.icons.python;
        }
        if filename.eq_ignore_ascii_case("build.zig") {
            return &self.icons.zig;
        }
        if filename.eq_ignore_ascii_case("readme.md") {
            return &self.icons.markdown;
        }
        let filename_lower = filename.to_ascii_lowercase();
        if filename_lower.contains("dockerfile") || filename_lower.contains("containerfile") {
            return &self.icons.docker;
        }
        if filename.eq_ignore_ascii_case("build.gradle")
            || filename.eq_ignore_ascii_case("build.gradle.kts")
            || filename.eq_ignore_ascii_case("settings.gradle")
            || filename.eq_ignore_ascii_case("settings.gradle.kts")
            || filename.eq_ignore_ascii_case("gradle.properties")
            || filename.eq_ignore_ascii_case("gradlew")
        {
            return &self.icons.java;
        }
        if filename.eq_ignore_ascii_case("makefile")
            || filename.eq_ignore_ascii_case("gnumakefile")
            || filename.eq_ignore_ascii_case("justfile")
        {
            return &self.icons.makefile;
        }
        if filename.eq_ignore_ascii_case("cmakelists.txt") {
            return &self.icons.cmake;
        }
        if filename.eq_ignore_ascii_case("nginx.conf") {
            return &self.icons.nginx;
        }
        if filename.eq_ignore_ascii_case("ansible.cfg")
            || filename.eq_ignore_ascii_case("playbook.yml")
            || filename.eq_ignore_ascii_case("playbook.yaml")
        {
            return &self.icons.ansible;
        }

        if let Some(exact) = filename.strip_prefix('.') {
            return self.file_icon_for_extension(exact);
        }

        let extension = Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        self.file_icon_for_extension(extension)
    }

    pub fn icon_theme_for_path(
        &self,
        path: &Path,
        is_dir: bool,
        is_expanded: bool,
    ) -> &FileIconThemeTokens {
        if is_dir {
            if is_expanded {
                &self.icons.folder_open
            } else {
                &self.icons.folder_closed
            }
        } else {
            let filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            self.icon_theme_for_filename(filename, false)
        }
    }

    pub fn get_icon_for_file(&self, filename: &str, is_dir: bool) -> &str {
        if is_dir {
            return self.default_folder_icon.as_str();
        }

        let normalized_filename = normalize_icon_filename(filename);

        if let Some(icon) = self.exact_icons.get(filename) {
            return icon.as_str();
        }
        if let Some(icon) = self.exact_icons.get(normalized_filename.as_str()) {
            return icon.as_str();
        }

        if let Some(icon) = special_icon_for_filename(&normalized_filename) {
            return icon;
        }

        let extension = Path::new(normalized_filename.as_str())
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_string);

        if let Some(ext) = extension.as_deref() {
            if let Some(icon) = self.extension_icons.get(ext) {
                return icon.as_str();
            }
            if let Some(icon) = built_in_icon_id_for_extension(ext) {
                return icon;
            }
        }

        self.default_file_icon.as_str()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{ThemeColor, ThemeConfig, linear_rgba_to_srgb_u8, srgb_to_linear};

    #[test]
    fn parse_hex_color_supports_rgb_and_rgba() {
        let rgb = ThemeColor::from_hex("#AABBCC").expect("rgb hex should parse");
        assert_eq!(rgb.as_u8(), [0xAA, 0xBB, 0xCC, 0xFF]);

        let rgba = ThemeColor::from_hex("A1B2C3DD").expect("rgba hex should parse");
        assert_eq!(rgba.as_u8(), [0xA1, 0xB2, 0xC3, 0xDD]);
    }

    #[test]
    fn srgb_mid_gray_decodes_to_expected_linear_value() {
        let decoded = srgb_to_linear(0.5);
        assert!((decoded - 0.214_041_14).abs() < 1e-6, "{decoded}");
    }

    #[test]
    fn linear_round_trip_preserves_srgb_bytes() {
        let color = ThemeColor::from_hex("#8080C0CC").expect("theme color should parse");
        assert_eq!(linear_rgba_to_srgb_u8(color.as_f32()), color.as_u8());
    }

    #[test]
    fn file_icon_lookup_prefers_dir_then_exact_then_extension_then_default() {
        let mut theme = ThemeConfig::builtin_dark();
        theme.default_file_icon = "built_in:file".to_string();
        theme.default_folder_icon = "built_in:folder".to_string();
        theme.exact_icons = HashMap::from([
            ("Dockerfile".to_string(), "🐳".to_string()),
            ("README.md".to_string(), "📘".to_string()),
        ]);
        theme.extension_icons = HashMap::from([
            ("md".to_string(), "📝".to_string()),
            ("sql".to_string(), "🗄️".to_string()),
        ]);

        assert_eq!(theme.get_icon_for_file("src", true), "built_in:folder");
        assert_eq!(theme.get_icon_for_file("Dockerfile", false), "🐳");
        assert_eq!(theme.get_icon_for_file("README.md", false), "📘");
        assert_eq!(theme.get_icon_for_file("schema.SQL", false), "🗄️");
        assert_eq!(
            theme.get_icon_for_file("user.service.ts", false),
            "built_in:nestjsservice"
        );
        assert_eq!(
            theme.get_icon_for_file("user-service.ts", false),
            "built_in:nestjsservice"
        );
        assert_eq!(
            theme.get_icon_for_file("auth.service", false),
            "built_in:nestjsservice"
        );
        assert_eq!(
            theme.get_icon_for_file("auth-service", false),
            "built_in:nestjsservice"
        );
        assert_eq!(
            theme.get_icon_for_file("user.controller.ts", false),
            "built_in:nestjscontroller"
        );
        assert_eq!(
            theme.get_icon_for_file("auth-module.ts", false),
            "built_in:nestjsmodule"
        );
        assert_eq!(
            theme.get_icon_for_file("create-user.dto.ts", false),
            "built_in:nestjsdto"
        );
        assert_eq!(
            theme.get_icon_for_file("jwt.guard.ts", false),
            "built_in:nestjsguard"
        );
        assert_eq!(
            theme.get_icon_for_file("user.test.ts", false),
            "built_in:testts"
        );
        assert_eq!(
            theme.get_icon_for_file("button.stories.tsx", false),
            "built_in:storybook"
        );
        assert_eq!(theme.get_icon_for_file("Dockerfile", false), "🐳");
        assert_eq!(
            theme.get_icon_for_file("Dockerfile.dev", false),
            "built_in:docker"
        );
        assert_eq!(
            theme.get_icon_for_file("src/Dockerfile", false),
            "built_in:docker"
        );
        assert_eq!(
            theme.get_icon_for_file("pnpm-lock.yaml", false),
            "built_in:pnpmlock"
        );
        assert_eq!(
            theme.get_icon_for_file("vite.config.ts", false),
            "built_in:vite"
        );
        assert_eq!(
            theme.get_icon_for_file(".eslintrc.json", false),
            "built_in:eslint"
        );
        assert_eq!(
            theme.get_icon_for_file("schema.prisma", false),
            "built_in:prisma"
        );
        assert_eq!(theme.get_icon_for_file("README.md", false), "📘");
        assert_eq!(
            theme.get_icon_for_file("LICENSE", false),
            "built_in:license"
        );
        assert_eq!(
            theme.get_icon_for_file("index.ts", false),
            "built_in:typescript"
        );
        assert_eq!(
            theme.get_icon_for_file("slot-random-service.js", false),
            "built_in:nestjsservice"
        );
        assert_eq!(theme.get_icon_for_file("notes.md", false), "📝");
        assert_eq!(
            theme.get_icon_for_file("unknown.bin", false),
            "built_in:file"
        );
    }
}
