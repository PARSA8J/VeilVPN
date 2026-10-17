//! App config persistence (config.json in data dir).

use serde::{Deserialize, Serialize};

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
        let p = crate::license::data_dir().join("config.json");
        std::fs::read_to_string(p)
            .ok()
            .and_then(|d| serde_json::from_str(&d).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        let p = crate::license::data_dir().join("config.json");
        if let Ok(d) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, d);
        }
    }
}
