use std::path::{Path, PathBuf};

use super::paths::user_config_root;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitUi {
    Lazygit,
    GitMin,
}

/// Chấp nhận đúng hai chuỗi này; giá trị khác → caller fallback về default.
pub fn parse_git_ui(raw: &str) -> Option<GitUi> {
    match raw {
        "lazygit" => Some(GitUi::Lazygit),
        "git_min" => Some(GitUi::GitMin),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitConfig {
    pub ui: GitUi,
    pub git_min_path: Option<PathBuf>,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            ui: GitUi::Lazygit,
            git_min_path: None,
        }
    }
}

impl GitConfig {
    /// Đọc `[git]` từ user override `~/.config/netherize/ui.toml`.
    /// Thiếu file, thiếu section, hoặc sai giá trị → built-in default (lazygit).
    pub fn load_active() -> Self {
        let path = user_config_root().join("ui.toml");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(value) = content.parse::<toml::Table>() else {
            eprintln!("[git] parse error in {}, using defaults", path.display());
            return Self::default();
        };
        let Some(git) = value.get("git").and_then(|g| g.as_table()) else {
            return Self::default();
        };

        let mut config = Self::default();
        if let Some(ui) = git.get("ui").and_then(|v| v.as_str()) {
            match parse_git_ui(ui) {
                Some(parsed) => config.ui = parsed,
                None => {
                    eprintln!(
                        "[git] unknown ui '{ui}' (expected \"lazygit\" | \"git_min\"), using lazygit"
                    );
                }
            }
        }
        if let Some(p) = git.get("git_min_path").and_then(|v| v.as_str()) {
            config.git_min_path = Some(PathBuf::from(p));
        }
        config
    }

    /// Đường dẫn binary khả dụng: override trước, sau đó probe vị trí .app chuẩn.
    pub fn resolved_binary(&self) -> Option<PathBuf> {
        if let Some(explicit) = &self.git_min_path {
            if explicit.exists() {
                return Some(explicit.clone());
            }
            eprintln!(
                "[git] git_min_path '{}' không tồn tại, thử vị trí mặc định",
                explicit.display()
            );
        }
        let name = "GitMin.app/Contents/MacOS/GitMin";
        let candidates = [
            Some(PathBuf::from("/Applications").join(name)),
            user_config_root()
                .ancestors()
                .nth(3)
                .map(Path::to_path_buf)
                .map(|home| home.join("Applications").join(name)),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|candidate| candidate.exists())
    }

    /// Ghi `[git] ui` vào user override `ui.toml`, giữ nguyên mọi section khác.
    pub fn save_ui_target(&self) -> Result<(), String> {
        Self::write_ui_target_to(&user_config_root().join("ui.toml"), self.ui)
    }

    /// Bản testable: ghi target vào file TOML bất kỳ mà không đụng section khác.
    pub fn write_ui_target_to(path: &Path, ui: GitUi) -> Result<(), String> {
        let mut root = match std::fs::read_to_string(path) {
            Ok(content) => content
                .parse::<toml::Table>()
                .map_err(|err| format!("parse error in {}: {err}", path.display()))?,
            Err(_) => toml::Table::new(),
        };
        let git = root
            .entry("git")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or_else(|| format!("[git] section in {} is not a table", path.display()))?;
        let ui_str = match ui {
            GitUi::Lazygit => "lazygit",
            GitUi::GitMin => "git_min",
        };
        git.insert("ui".to_string(), toml::Value::String(ui_str.to_string()));
        let text = toml::to_string_pretty(&root)
            .map_err(|err| format!("serialize ui config failed: {err}"))?;
        crate::app::persistence::atomic_write(path, text)
            .map_err(|err| format!("write ui config failed: {err}"))
    }
}

/// Walk up từ `start` tìm thư mục chứa `.git` (dir thường, file với worktree).
pub fn find_git_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_gitcfg_{prefix}_{nanos}"))
    }

    #[test]
    fn parse_git_ui_accepts_only_known_values() {
        assert_eq!(parse_git_ui("lazygit"), Some(GitUi::Lazygit));
        assert_eq!(parse_git_ui("git_min"), Some(GitUi::GitMin));
        assert_eq!(parse_git_ui("GitMin"), None);
        assert_eq!(parse_git_ui(""), None);
    }

    #[test]
    fn load_active_parses_git_section_from_toml() {
        let dir = unique_temp_dir("load");
        fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("ui.toml");
        fs::write(
            &path,
            "[editor]\nfont_size = 14\n\n[git]\nui = \"git_min\"\ngit_min_path = \"/tmp/fake-git-min\"\n",
        )
        .expect("write toml");

        // load_active đọc đường dẫn cố định nên test phần parse qua Table trực tiếp:
        let content = fs::read_to_string(&path).expect("read back");
        let value = content.parse::<toml::Table>().expect("parse");
        let git = value
            .get("git")
            .and_then(|g| g.as_table())
            .expect("git section");
        assert_eq!(
            parse_git_ui(git.get("ui").unwrap().as_str().unwrap()),
            Some(GitUi::GitMin)
        );
        assert_eq!(
            git.get("git_min_path").and_then(|v| v.as_str()),
            Some("/tmp/fake-git-min")
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn find_git_repo_root_walks_up_to_git_dir() {
        let root = unique_temp_dir("walk");
        fs::create_dir_all(root.join(".git")).expect("create .git");
        fs::create_dir_all(root.join("a/b/c")).expect("create nested");

        let found = find_git_repo_root(&root.join("a/b/c")).expect("root found");
        assert_eq!(found, root);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_git_repo_root_matches_worktree_git_file() {
        let root = unique_temp_dir("worktree");
        fs::create_dir_all(&root).expect("create dir");
        fs::write(root.join(".git"), "gitdir: /somewhere/else\n").expect("write .git file");

        assert_eq!(find_git_repo_root(&root), Some(root.clone()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_git_repo_root_none_outside_repo() {
        assert_eq!(find_git_repo_root(Path::new("/")), None);
    }

    #[test]
    fn write_ui_target_preserves_other_sections() {
        let dir = unique_temp_dir("save");
        let path = dir.join("ui.toml");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(
            &path,
            "[editor]\nfont_size = 14\n\n[git]\nui = \"lazygit\"\ngit_min_path = \"/tmp/x\"\n",
        )
        .expect("seed toml");

        GitConfig::write_ui_target_to(&path, GitUi::GitMin).expect("save");

        let value = fs::read_to_string(&path)
            .expect("read back")
            .parse::<toml::Table>()
            .expect("parse back");
        assert_eq!(
            value
                .get("git")
                .and_then(|g| g.get("ui"))
                .and_then(|v| v.as_str()),
            Some("git_min")
        );
        // Section khác và key khác trong [git] phải còn nguyên.
        assert_eq!(
            value
                .get("editor")
                .and_then(|e| e.get("font_size"))
                .and_then(|v| v.as_integer()),
            Some(14)
        );
        assert_eq!(
            value
                .get("git")
                .and_then(|g| g.get("git_min_path"))
                .and_then(|v| v.as_str()),
            Some("/tmp/x")
        );

        // File chưa tồn tại → tạo mới chỉ với [git].
        let fresh = dir.join("fresh.toml");
        GitConfig::write_ui_target_to(&fresh, GitUi::Lazygit).expect("create");
        let value = fs::read_to_string(&fresh)
            .expect("read fresh")
            .parse::<toml::Table>()
            .expect("parse fresh");
        assert_eq!(
            value
                .get("git")
                .and_then(|g| g.get("ui"))
                .and_then(|v| v.as_str()),
            Some("lazygit")
        );

        let _ = fs::remove_dir_all(dir);
    }
}
