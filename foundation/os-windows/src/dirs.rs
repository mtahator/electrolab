//! Per-user app folders (`%APPDATA%`, `%LOCALAPPDATA%`).

use std::path::PathBuf;

use ep_os::AppDirs;

/// Standard Windows folders for one app.
#[derive(Clone, Debug)]
pub struct WinDirs {
    data: PathBuf,
    config: PathBuf,
    cache: PathBuf,
}

impl WinDirs {
    /// Folders for `app` (the product name).
    ///
    /// # Errors
    /// The user's profile folders cannot be found.
    pub fn new(app: &str) -> ep_os::Result<Self> {
        let d = directories::ProjectDirs::from("", "", app)
            .ok_or(ep_os::OsError::Unsupported("no user profile folders"))?;
        Ok(Self {
            data: d.data_dir().to_path_buf(),
            config: d.config_dir().to_path_buf(),
            cache: d.cache_dir().to_path_buf(),
        })
    }
}

impl AppDirs for WinDirs {
    fn data(&self) -> PathBuf {
        self.data.clone()
    }
    fn config(&self) -> PathBuf {
        self.config.clone()
    }
    fn cache(&self) -> PathBuf {
        self.cache.clone()
    }
    fn logs(&self) -> PathBuf {
        self.cache.join("logs")
    }
}
