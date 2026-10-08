use crate::store::Task;
use crate::util::pad;

pub fn render_task(task: &Task) -> String {
    pad(&task.title, 20)
}

pub fn render_all(tasks: &[Task]) -> String {
    tasks.iter().map(render_task).collect()
}
