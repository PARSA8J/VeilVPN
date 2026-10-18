//! Seed server list (real, always-available fallback) + app icon.

pub const SEED: &str = include_str!("../assets/seed.txt");
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

pub fn parse_seed() -> Vec<crate::uri::Server> {
    crate::uri::parse_lines(SEED)
}
