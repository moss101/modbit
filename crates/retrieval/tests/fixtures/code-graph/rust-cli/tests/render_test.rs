use taskcli::render::render_task;
use taskcli::store::Task;

#[test]
fn renders_a_task() {
    let t = Task { id: 1, title: "a".into() };
    assert_eq!(render_task(&t).len(), 20);
}
