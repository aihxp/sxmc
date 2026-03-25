use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = env_path("SXMC_CONFIG_HOME") {
        return dir;
    }
    if let Some(dir) = env_path("XDG_CONFIG_HOME") {
        return dir.join("sxmc");
    }
    #[cfg(windows)]
    {
        if let Some(dir) = env_path("APPDATA") {
            return dir.join("sxmc");
        }
        if let Some(dir) = env_path("USERPROFILE") {
            return dir.join(".config").join("sxmc");
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = env_path("HOME") {
            return dir.join(".config").join("sxmc");
        }
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("sxmc")
}

pub fn cache_dir() -> PathBuf {
    if let Some(dir) = env_path("SXMC_CACHE_HOME") {
        return dir;
    }
    if let Some(dir) = env_path("XDG_CACHE_HOME") {
        return dir.join("sxmc");
    }
    #[cfg(windows)]
    {
        if let Some(dir) = env_path("LOCALAPPDATA") {
            return dir.join("sxmc");
        }
        if let Some(dir) = env_path("USERPROFILE") {
            return dir.join(".cache").join("sxmc");
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = env_path("HOME") {
            return dir.join(".cache").join("sxmc");
        }
    }
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("sxmc")
}
