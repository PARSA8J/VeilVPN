//! i18n — embedded from shared/i18n.js via scripts/i18n_to_json.js. Default: English.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct I18n {
    pub default: String,
    pub rtl: Vec<String>,
    pub languages: Vec<Lang>,
    pub strings: HashMap<String, HashMap<String, String>>,
}

#[derive(Deserialize, Clone)]
pub struct Lang {
    pub code: String,
    pub name: String,
}

pub struct I18nState {
    pub data: Arc<I18n>,
    pub lang: String,
}

impl I18nState {
    pub fn new(lang: String) -> Self {
        let raw = include_str!("../assets/i18n.json");
        let data: I18n = serde_json::from_str(raw).expect("i18n.json parse");
        Self { data: Arc::new(data), lang }
    }

    pub fn is_rtl(&self) -> bool {
        self.data.rtl.iter().any(|c| *c == self.lang)
    }

    pub fn t(&self, key: &str) -> String {
        let m = self.data.strings.get(&self.lang);
        if let Some(v) = m.and_then(|m| m.get(key)) {
            return v.clone();
        }
        if let Some(v) = self.data.strings.get("en").and_then(|m| m.get(key)) {
            return v.clone();
        }
        key.to_string()
    }
}

pub const SEED: &str = include_str!("../assets/seed.txt");
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
pub const FONT_VAZIR: &[u8] = include_bytes!("../assets/fonts/Vazirmatn-Regular.ttf");
pub const FONT_CJK: &[u8] = include_bytes!("../assets/fonts/NotoSansSC.ttf");

pub fn parse_seed() -> Vec<crate::uri::Server> {
    crate::uri::parse_lines(SEED)
}
