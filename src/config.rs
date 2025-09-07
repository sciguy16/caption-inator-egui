use color_eyre::{eyre::eyre, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub region: Option<String>,
    pub key: Option<String>,
    pub wordlist_dir: Option<PathBuf>,
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let path = if let Some(path) = path {
            path
        } else {
            // if path not set then try the current dir and the exe dir
            let cwd_version = std::env::current_dir()?.join("config.toml");
            let exe_dir_version = std::env::current_exe()?
                .parent()
                .unwrap()
                .join("config.toml");
            if cwd_version.exists() {
                cwd_version
            } else if exe_dir_version.exists() {
                exe_dir_version
            } else {
                return Err(eyre!("Unable to find config file"));
            }
        };
        let content = std::fs::read_to_string(path)?;
        toml::de::from_str(&content).map_err(Into::into)
    }
}
