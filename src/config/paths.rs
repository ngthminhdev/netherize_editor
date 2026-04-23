use std::path::PathBuf;

fn home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

pub fn user_config_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        home_dir().join(".netherize_editor")
    }

    #[cfg(not(target_os = "windows"))]
    {
        home_dir().join(".config").join("netherize")
    }
}

pub fn legacy_app_state_root() -> PathBuf {
    home_dir().join(".netherize_editor")
}
