mod commands;
mod config;
mod render;
mod store;
mod util;

fn main() {
    let cfg = config::load();
    commands::run(&cfg);
}
