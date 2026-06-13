use std::path::{Path, PathBuf};

use crate::syntax::syntax_engine::LanguageId;

const RUST_ROOT_MARKERS: &[&str] = &["Cargo.toml", ".git"];
const JS_TS_ROOT_MARKERS: &[&str] = &["package.json", "tsconfig.json", ".git"];
// `go.work` first so multi-module workspaces root gopls at the workspace level
// (enabling cross-module references) instead of at an inner module's go.mod.
const GO_ROOT_MARKERS: &[&str] = &["go.work", "go.mod", ".git"];
const JAVA_ROOT_MARKERS: &[&str] = &["pom.xml", "build.gradle", "build.gradle.kts", ".git"];
const PYTHON_ROOT_MARKERS: &[&str] = &["pyproject.toml", "setup.py", "setup.cfg", ".git"];
const SQL_ROOT_MARKERS: &[&str] = &[".sqls.json", "docker-compose.yml", ".git"];
const DART_ROOT_MARKERS: &[&str] = &["pubspec.yaml", "analysis_options.yaml", ".git"];
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

/// A secondary language server that runs *alongside* a language's primary
/// server (e.g. ruff for linting + formatting next to pyright for Python).
/// Companions receive the same document sync but only serve the capabilities
/// they advertise; the primary keeps serving completion/hover/definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompanionServer {
    pub binary: &'static str,
    pub launch_args: &'static [&'static str],
    pub install_command: &'static str,
}

static PYTHON_COMPANIONS: &[CompanionServer] = &[CompanionServer {
    // `ruff server` speaks LSP: fast lint diagnostics, formatting and quick-fix
    // code actions. It is meant to run together with pyright.
    binary: "ruff",
    launch_args: &["server"],
    install_command: "brew install ruff  (or: pip install ruff)",
}];

/// Companion servers for a language profile `key`, if any.
pub fn companion_servers_for_key(key: &str) -> &'static [CompanionServer] {
    match key {
        "python" => PYTHON_COMPANIONS,
        _ => &[],
    }
}

/// Companion servers to launch for the file at `path`, if any.
pub fn companion_servers_for_path(path: &Path) -> &'static [CompanionServer] {
    language_profile_for_path(path)
        .map(|profile| companion_servers_for_key(profile.key))
        .unwrap_or(&[])
}

/// All server binaries (primary + companions) expected to serve `language_id`.
/// Used to fan document sync out to exactly the servers running for a file's
/// language, without leaking it to unrelated servers sharing the same root.
pub fn expected_server_binaries(language_id: &str) -> Vec<&'static str> {
    let mut binaries = Vec::new();
    if let Some(profile) = language_profile_for_language_id(language_id) {
        if !profile.lsp_binary.is_empty() {
            binaries.push(profile.lsp_binary);
        }
        for companion in companion_servers_for_key(profile.key) {
            binaries.push(companion.binary);
        }
    }
    binaries
}

/// Whether `server_name` (a binary name or full path) is a known companion
/// linter/formatter rather than a primary language server. Used to keep
/// intelligence requests (hover/completion/definition) routed to the primary.
pub fn binary_is_companion(server_name: &str) -> bool {
    let stem = Path::new(server_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(server_name);
    PYTHON_COMPANIONS
        .iter()
        .any(|companion| companion.binary == stem)
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
        key: "python",
        language_label: "Python",
        language_id: "python",
        syntax_language_id: Some(LanguageId::Python),
        lsp_binary: "pyright-langserver",
        launch_args: &["--stdio"],
        install_command: "npm install -g pyright",
        root_markers: PYTHON_ROOT_MARKERS,
        extensions: &["py"],
        filenames: &[],
    },
    LanguageProfile {
        key: "java",
        language_label: "Java",
        language_id: "java",
        syntax_language_id: Some(LanguageId::Java),
        lsp_binary: "jdtls",
        launch_args: &["-data", ".jdtls_data"],
        install_command: "brew install jdtls",
        root_markers: JAVA_ROOT_MARKERS,
        extensions: &["java"],
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
    LanguageProfile {
        key: "protobuf",
        language_label: "Protobuf",
        language_id: "protobuf",
        syntax_language_id: Some(LanguageId::Protobuf),
        lsp_binary: "",
        launch_args: &[],
        install_command: "",
        root_markers: NO_ROOT_MARKERS,
        extensions: &["proto"],
        filenames: &[],
    },
    LanguageProfile {
        key: "dotenv",
        language_label: "Dotenv",
        language_id: "dotenv",
        syntax_language_id: Some(LanguageId::Dotenv),
        lsp_binary: "",
        launch_args: &[],
        install_command: "",
        root_markers: NO_ROOT_MARKERS,
        extensions: &["env"],
        filenames: &[".env", ".env*", "env.*"],
    },
    LanguageProfile {
        key: "xml",
        language_label: "XML",
        language_id: "xml",
        syntax_language_id: Some(LanguageId::Xml),
        lsp_binary: "",
        launch_args: &[],
        install_command: "",
        root_markers: NO_ROOT_MARKERS,
        extensions: &["xml"],
        filenames: &[],
    },
    LanguageProfile {
        key: "dart",
        language_label: "Dart",
        language_id: "dart",
        syntax_language_id: Some(LanguageId::Dart),
        lsp_binary: "dart",
        launch_args: &["language-server"],
        install_command: "brew tap dart-lang/dart && brew install dart",
        root_markers: DART_ROOT_MARKERS,
        extensions: &["dart"],
        filenames: &[],
    },
    LanguageProfile {
        key: "plaintext",
        language_label: "Plain Text",
        language_id: "plaintext",
        syntax_language_id: Some(LanguageId::Plaintext),
        lsp_binary: "",
        launch_args: &[],
        install_command: "",
        root_markers: NO_ROOT_MARKERS,
        extensions: &["txt"],
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
            profile.filenames.iter().any(|candidate| {
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
    let path = Path::new(binary);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(binary);
    let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or(binary);
    LANGUAGE_REGISTRY.iter().find(|profile| {
        profile.lsp_binary == binary
            || profile.lsp_binary == stem
            || profile.lsp_binary == file_name
    })
}

/// Resolve the gopls root for `file_path`. A `go.work` anywhere up the tree
/// defines a multi-module workspace and takes priority, so gopls roots at the
/// workspace (enabling cross-module references) instead of an inner module's
/// `go.mod`. Falls back to the nearest `go.mod` / `.git`.
pub fn find_go_module_root(file_path: &Path) -> PathBuf {
    let workspace = find_project_root(file_path, &["go.work"]);
    if workspace.join("go.work").exists() {
        return workspace;
    }
    find_project_root(file_path, &["go.mod", ".git"])
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

    use crate::syntax::syntax_engine::LanguageId;

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
        for name in [
            "Dockerfile-base",
            "Dockerfile.base",
            "Dockerfile-all",
            "Dockerfile.dev",
        ] {
            let path = PathBuf::from(format!("/tmp/{}", name));
            let profile =
                language_profile_for_path(&path).expect(&format!("{} should match", name));
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
    fn find_go_module_root_prefers_go_work_over_nested_go_mod() {
        let root = unique_temp_dir("go_work");
        let workspace = root.join("ws");
        let module = workspace.join("svc");
        let nested = module.join("internal");
        fs::create_dir_all(&nested).expect("create dirs");
        fs::write(workspace.join("go.work"), "go 1.22\n").expect("write go.work");
        fs::write(module.join("go.mod"), "module svc\n").expect("write go.mod");

        let detected = super::find_go_module_root(&nested.join("main.go"));
        assert_eq!(detected, workspace);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_go_module_root_falls_back_to_nearest_go_mod() {
        let root = unique_temp_dir("go_mod_only");
        let module = root.join("svc");
        let nested = module.join("internal");
        fs::create_dir_all(&nested).expect("create dirs");
        fs::write(module.join("go.mod"), "module svc\n").expect("write go.mod");

        let detected = super::find_go_module_root(&nested.join("main.go"));
        assert_eq!(detected, module);
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

    #[test]
    fn workspace_server_detection_supports_java_maven_marker() {
        let root = unique_temp_dir("java_maven");
        let project = root.join("workspace");
        let nested = project.join("src/main/java/com/example");
        fs::create_dir_all(&nested).expect("create nested dirs");
        fs::write(project.join("pom.xml"), "<project/>").expect("write pom.xml");

        let file_path = nested.join("Main.java");
        let profile = language_profile_for_path(&file_path).expect("java profile");
        assert_eq!(profile.key, "java");
        assert_eq!(profile.lsp_binary, "jdtls");
        let detected = find_project_root(&file_path, profile.root_markers);
        assert_eq!(detected, project);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_server_detection_supports_java_gradle_marker() {
        let root = unique_temp_dir("java_gradle");
        let project = root.join("workspace");
        let nested = project.join("src/main/java/com/example");
        fs::create_dir_all(&nested).expect("create nested dirs");
        fs::write(project.join("build.gradle"), "").expect("write build.gradle");

        let file_path = nested.join("Main.java");
        let profile = language_profile_for_path(&file_path).expect("java profile");
        assert_eq!(profile.key, "java");
        let detected = find_project_root(&file_path, profile.root_markers);
        assert_eq!(detected, project);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_server_detection_matches_java_extension() {
        let profile = language_profile_for_path(PathBuf::from("/tmp/MyClass.java").as_path())
            .expect("java profile");
        assert_eq!(profile.key, "java");
        assert_eq!(profile.language_id, "java");
        assert_eq!(profile.lsp_binary, "jdtls");
        assert_eq!(profile.syntax_language_id, Some(LanguageId::Java));
    }
}
