use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::{env, path::Path};

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// The `ninja` command to execute in `run` mode.
    pub ninja: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            ninja: "ninja".to_string(),
        }
    }
}

pub struct Config {
    pub global: GlobalConfig,
    pub data: Figment,
}

impl Config {
    fn figment() -> Figment {
        // The configuration is usually at `~/.config/fake.toml`.
        let config_base = env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
            let home = env::var("HOME").expect("$HOME not set");
            home + "/.config"
        });
        let config_path = Path::new(&config_base).join("fake.toml");

        // Use our defaults, overridden by the TOML config file.
        Figment::from(Serialized::defaults(GlobalConfig::default())).merge(Toml::file(config_path))
    }

    pub fn new() -> Result<Self, figment::Error> {
        let fig = Self::figment();
        let cfg: GlobalConfig = fig.extract()?;
        Ok(Self {
            data: fig,
            global: cfg,
        })
    }
}
