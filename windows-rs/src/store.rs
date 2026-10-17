//! App config persistence (config.json in the app data dir).

use serde::{Deserialize, Serialize};

pub fn data_dir() -> std::path::PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let p = std::path::PathBuf::from(appdata).join("Mehrveil");
            let _ = std::fs::create_dir_all(&p);
            return p;
        }
    }
    std::path::PathBuf::from("mehrveil-data")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "def_lang")]
    pub lang: String,
    #[serde(default)]
    pub auto_connect: bool,
    #[serde(default)]
    pub custom_source: String,
    #[serde(default)]
    pub selected_key: String,
}

fn def_lang() -> String {
    "en".into()
}

impl Default for Config {
    fn default() -> Self {
        Self { lang: def_lang(), auto_connect: false, custom_source: String::new(), selected_key: String::new() }
    }
}

impl Config {
    pub fn load() -> Self {
        let p = data_dir().join("config.json");
        std::fs::read_to_string(p)
            .ok()
            .and_then(|d| serde_json::from_str(&d).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        let p = data_dir().join("config.json");
        if let Ok(d) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, d);
        }
    }
}
