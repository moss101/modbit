use taskcli::store::{MemoryStorage, Storage, Task};

#[test]
fn saves_a_task() {
    let mut s = MemoryStorage::new();
    s.save(Task { id: 1, title: "a".into() });
    assert!(s.all().is_empty());
}
