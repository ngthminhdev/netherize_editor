use std::path::{Path, PathBuf};

use crate::syntax::syntax_engine::LanguageId;

const RUST_ROOT_MARKERS: &[&str] = &["Cargo.toml", ".git"];
const JS_TS_ROOT_MARKERS: &[&str] = &["package.json", "tsconfig.json", ".git"];
const GO_ROOT_MARKERS: &[&str] = &["go.mod", ".git"];
const SQL_ROOT_MARKERS: &[&str] = &[".sqls.json", "docker-compose.yml", ".git"];
const NO_ROOT_MARKERS: &[&str] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageProfile {
    pub key: &'static str,
    pub language_label: &'static str,
    pub language_id: &'static str,
    pub syntax_language_id: Option<LanguageId>,
    pub lsp_binary: &'static str,
    pub launch_args: &'static [&'static str],
    pub install_command: &'static str,
    pub root_markers: &'static [&'static str],
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
}

static LANGUAGE_REGISTRY: &[LanguageProfile] = &[
    LanguageProfile {
        key: "rust",
        language_label: "Rust",
        language_id: "rust",
        syntax_language_id: Some(LanguageId::Rust),
        lsp_binary: "rust-analyzer",
        launch_args: &[],
        install_command: "rustup component add rust-analyzer",
        root_markers: RUST_ROOT_MARKERS,
        extensions: &["rs"],
        filenames: &[],
    },
    LanguageProfile {
        key: "javascript",
        language_label: "JavaScript",
        language_id: "javascript",
        syntax_language_id: Some(LanguageId::JavaScript),
        lsp_binary: "typescript-language-server",
        launch_args: &["--stdio"],
        install_command: "npm install -g typescript typescript-language-server",
        root_markers: JS_TS_ROOT_MARKERS,
        extensions: &["js", "mjs", "cjs"],
        filenames: &[],
    },
    LanguageProfile {
        key: "jsx",
        language_label: "JavaScript (JSX)",
        language_id: "javascriptreact",
        syntax_language_id: Some(LanguageId::Jsx),
        lsp_binary: "typescript-language-server",
        launch_args: &["--stdio"],
        install_command: "npm install -g typescript typescript-language-server",
        root_markers: JS_TS_ROOT_MARKERS,
        extensions: &["jsx"],
        filenames: &[],
    },
    LanguageProfile {
        key: "typescript",
        language_label: "TypeScript",
        language_id: "typescript",
        syntax_language_id: Some(LanguageId::TypeScript),
        lsp_binary: "typescript-language-server",
        launch_args: &["--stdio"],
        install_command: "npm install -g typescript typescript-language-server",
        root_markers: JS_TS_ROOT_MARKERS,
        extensions: &["ts"],
        filenames: &[],
    },
    LanguageProfile {
        key: "tsx",
        language_label: "TypeScript (TSX)",
        language_id: "typescriptreact",
        syntax_language_id: Some(LanguageId::Tsx),
        lsp_binary: "typescript-language-server",
        launch_args: &["--stdio"],
        install_command: "npm install -g typescript typescript-language-server",
        root_markers: JS_TS_ROOT_MARKERS,
        extensions: &["tsx"],
        filenames: &[],
    },
    LanguageProfile {
        key: "go",
        language_label: "Go",
        language_id: "go",
        syntax_language_id: Some(LanguageId::Go),
        lsp_binary: "gopls",
        launch_args: &[],
        install_command: "go install golang.org/x/tools/gopls@latest",
        root_markers: GO_ROOT_MARKERS,
        extensions: &["go"],
        filenames: &[],
    },
    LanguageProfile {
        key: "sql",
        language_label: "SQL",
        language_id: "sql",
        syntax_language_id: Some(LanguageId::Sql),
        lsp_binary: "sqls",
        launch_args: &[],
        install_command: "go install github.com/sqls-server/sqls@latest",
        root_markers: SQL_ROOT_MARKERS,
        extensions: &["sql"],
        filenames: &[],
    },
    LanguageProfile {
        key: "yaml",
        language_label: "YAML",
        language_id: "yaml",
        syntax_language_id: Some(LanguageId::Yaml),
        lsp_binary: "yaml-language-server",
        launch_args: &["--stdio"],
        install_command: "npm install -g yaml-language-server",
        root_markers: NO_ROOT_MARKERS,
        extensions: &["yaml", "yml"],
        filenames: &[],
    },
    LanguageProfile {
        key: "dockerfile",
        language_label: "Dockerfile",
        language_id: "dockerfile",
        syntax_language_id: Some(LanguageId::Dockerfile),
        lsp_binary: "docker-langserver",
        launch_args: &["--stdio"],
        install_command: "npm install -g dockerfile-language-server-nodejs",
        root_markers: NO_ROOT_MARKERS,
        extensions: &[],
        filenames: &["Dockerfile", "Dockerfile*"],
    },
    LanguageProfile {
        key: "json",
        language_label: "JSON",
        language_id: "json",
        syntax_language_id: Some(LanguageId::Json),
        lsp_binary: "vscode-json-language-server",
        launch_args: &["--stdio"],
        install_command: "npm install -g vscode-langservers-extracted",
        root_markers: NO_ROOT_MARKERS,
        extensions: &["json"],
        filenames: &[],
    },
    LanguageProfile {
        key: "bash",
        language_label: "Bash",
        language_id: "shellscript",
        syntax_language_id: Some(LanguageId::Bash),
        lsp_binary: "bash-language-server",
        launch_args: &["start"],
        install_command: "npm install -g bash-language-server",
        root_markers: NO_ROOT_MARKERS,
        extensions: &["sh"],
        filenames: &[],
    },
    LanguageProfile {
        key: "markdown",
        language_label: "Markdown",
        language_id: "markdown",
        syntax_language_id: Some(LanguageId::Markdown),
        lsp_binary: "",
        launch_args: &[],
        install_command: "",
        root_markers: NO_ROOT_MARKERS,
        extensions: &["md", "markdown", "mdx"],
        filenames: &[],
    },
];

pub fn all_language_profiles() -> &'static [LanguageProfile] {
    LANGUAGE_REGISTRY
}

pub fn language_profile_for_extension(ext: &str) -> Option<&'static LanguageProfile> {
    let needle = ext.trim_start_matches('.').to_ascii_lowercase();
    LANGUAGE_REGISTRY.iter().find(|profile| {
        profile
            .extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&needle))
    })
}

pub fn language_profile_for_path(path: &Path) -> Option<&'static LanguageProfile> {
    if let Some(file_name) = path.file_name().and_then(|value| value.to_str())
        && let Some(profile) = LANGUAGE_REGISTRY.iter().find(|profile| {
            profile
                .filenames
                .iter()
                .any(|candidate| {
                    if let Some(prefix) = candidate.strip_suffix('*') {
                        file_name.starts_with(prefix)
                    } else {
                        candidate.eq_ignore_ascii_case(file_name)
                    }
                })
        })
    {
        return Some(profile);
    }

    path.extension()
        .and_then(|value| value.to_str())
        .and_then(language_profile_for_extension)
}

pub fn language_profile_for_language_id(language_id: &str) -> Option<&'static LanguageProfile> {
    LANGUAGE_REGISTRY
        .iter()
        .find(|profile| profile.language_id.eq_ignore_ascii_case(language_id))
}

pub fn language_profile_for_binary(binary: &str) -> Option<&'static LanguageProfile> {
    LANGUAGE_REGISTRY
        .iter()
        .find(|profile| profile.lsp_binary == binary)
}

pub fn find_project_root(file_path: &Path, markers: &[&str]) -> PathBuf {
    let anchor_dir = if file_path.is_dir() {
        file_path.to_path_buf()
    } else {
        file_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    if markers.is_empty() {
        return anchor_dir;
    }

    for dir in anchor_dir.ancestors() {
        if markers.iter().any(|marker| dir.join(marker).exists()) {
            return dir.to_path_buf();
        }
    }

    anchor_dir
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{SQL_ROOT_MARKERS, find_project_root, language_profile_for_path};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_language_registry_{label}_{stamp}"))
    }

    #[test]
    fn language_profile_detects_dockerfile_by_filename() {
        let profile = language_profile_for_path(PathBuf::from("/tmp/Dockerfile").as_path())
            .expect("dockerfile profile");
        assert_eq!(profile.key, "dockerfile");
        assert_eq!(profile.language_id, "dockerfile");
    }

    #[test]
    fn language_profile_detects_dockerfile_variants() {
        for name in ["Dockerfile-base", "Dockerfile.base", "Dockerfile-all", "Dockerfile.dev"] {
            let path = PathBuf::from(format!("/tmp/{}", name));
            let profile = language_profile_for_path(&path).expect(&format!("{} should match", name));
            assert_eq!(profile.key, "dockerfile");
        }
    }

    #[test]
    fn language_profile_detects_sql_by_extension() {
        let profile = language_profile_for_path(PathBuf::from("/tmp/schema.sql").as_path())
            .expect("sql profile");
        assert_eq!(profile.key, "sql");
        assert_eq!(profile.language_id, "sql");
        assert_eq!(profile.lsp_binary, "sqls");
    }

    #[test]
    fn find_project_root_prefers_nearest_marker_ancestor() {
        let root = unique_temp_dir("nearest_root");
        let project = root.join("workspace");
        let nested = project.join("apps/web/src");
        fs::create_dir_all(&nested).expect("create nested dirs");
        fs::write(project.join("package.json"), "{}").expect("write package.json");
        fs::create_dir_all(project.join(".git")).expect("create .git dir");

        let file_path = nested.join("main.tsx");
        let detected = find_project_root(&file_path, &["package.json", ".git"]);

        assert_eq!(detected, project);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_project_root_supports_sql_workspace_markers() {
        let root = unique_temp_dir("sql_root");
        let project = root.join("workspace");
        let nested = project.join("db/migrations");
        fs::create_dir_all(&nested).expect("create nested dirs");
        fs::write(project.join(".sqls.json"), "{}").expect("write .sqls.json");

        let file_path = nested.join("001_init.sql");
        let detected = find_project_root(&file_path, SQL_ROOT_MARKERS);

        assert_eq!(detected, project);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_project_root_falls_back_to_file_parent_when_marker_missing() {
        let root = unique_temp_dir("fallback_parent");
        let nested = root.join("docs/specs");
        fs::create_dir_all(&nested).expect("create nested dirs");

        let file_path = nested.join("openapi.yaml");
        let detected = find_project_root(&file_path, &["package.json", "go.mod"]);

        assert_eq!(detected, nested);
        let _ = fs::remove_dir_all(root);
    }
}
