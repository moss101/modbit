use crate::config::Config;
use crate::render;
use crate::store::{self, Storage, Task};
use crate::util::slug;

pub fn run(cfg: &Config) {
    let mut storage = store::open(cfg);
    add(storage.as_mut(), "write docs");
    list(storage.as_ref());
}

fn add(storage: &mut dyn Storage, title: &str) {
    storage.save(Task {
        id: 1,
        title: slug(title),
    });
}

fn list(storage: &dyn Storage) {
    println!("{}", render::render_all(&storage.all()));
}
