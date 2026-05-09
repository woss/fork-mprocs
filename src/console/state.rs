use crate::kernel::kernel_message::SharedVt;
use crate::kernel::task::{TaskId, TaskStatus};

pub struct ConsoleState {
  pub tasks: Vec<ConsoleTaskEntry>,
  pub selected: usize,

  pub quit_modal: bool,
}

pub struct ConsoleTaskEntry {
  pub id: TaskId,
  pub path: String,
  pub status: TaskStatus,
  pub vt: Option<SharedVt>,
}
