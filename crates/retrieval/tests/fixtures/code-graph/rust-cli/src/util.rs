pub fn pad(s: &str, n: usize) -> String {
    format!("{s:<n$}")
}

pub fn slug(s: &str) -> String {
    s.to_lowercase().replace(' ', "-")
}

pub fn unused_helper() -> u32 {
    7
}
