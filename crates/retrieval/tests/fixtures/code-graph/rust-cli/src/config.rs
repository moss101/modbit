pub struct Config {
    pub path: String,
    pub verbose: bool,
}

pub fn load() -> Config {
    Config {
        path: default_path(),
        verbose: false,
    }
}

pub fn default_path() -> String {
    "tasks.json".to_string()
}
