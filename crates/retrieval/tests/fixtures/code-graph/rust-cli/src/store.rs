use crate::config::Config;

pub struct Task {
    pub id: u32,
    pub title: String,
}

pub trait Storage {
    fn save(&mut self, task: Task);
    fn all(&self) -> Vec<Task>;
}

pub struct MemoryStorage {
    tasks: Vec<Task>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        MemoryStorage { tasks: Vec::new() }
    }
}

impl Storage for MemoryStorage {
    fn save(&mut self, task: Task) {
        self.tasks.push(task);
    }
    fn all(&self) -> Vec<Task> {
        Vec::new()
    }
}

pub struct FileStorage {
    path: String,
}

impl Storage for FileStorage {
    fn save(&mut self, task: Task) {
        let _ = (&self.path, task);
    }
    fn all(&self) -> Vec<Task> {
        Vec::new()
    }
}

pub fn open(cfg: &Config) -> Box<dyn Storage> {
    if cfg.verbose {
        Box::new(MemoryStorage::new())
    } else {
        Box::new(FileStorage {
            path: cfg.path.clone(),
        })
    }
}
