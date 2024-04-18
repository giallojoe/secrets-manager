mod config;
mod secrets;
mod store;

use std::path::{Path, PathBuf};

pub use config::*;
use platform_dirs::AppDirs;
pub use secrets::*;
pub use store::*;

pub fn get_config_path(
    config: Option<PathBuf>,
    filename: impl AsRef<Path>,
) -> Result<PathBuf, String> {
    config.map_or_else(
        || -> Result<PathBuf, String> {
            let app_dir = AppDirs::new(Some("secrets-manager"), true)
                .ok_or_else(|| String::from("Cannot find config base path"))?;
            Ok(app_dir.config_dir.join(filename))
        },
        Ok,
    )
}

pub fn get_path(base: Option<PathBuf>, path: Option<PathBuf>) -> Result<PathBuf, std::io::Error> {
    let cwd = base.map_or_else(|| std::env::current_dir(), Ok)?;
    let mut path = path.unwrap_or_default();
    if path.is_absolute() {
        path = path.strip_prefix("/").unwrap().to_path_buf();
    }
    let cwd = PathBuf::from("/")
        .join(PathBuf::from(cwd.file_name().unwrap_or_default()))
        .join(path);
    Ok(cwd)
}
