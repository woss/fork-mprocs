use std::fmt::Debug;

use crate::term::{key::Key, mouse::MouseEvent};

#[derive(Debug)]
pub enum ProcMsg {
  SendKey(Key),
  SendMouse(MouseEvent),

  ScrollUp,
  ScrollDown,
  ScrollUpLines { n: usize },
  ScrollDownLines { n: usize },

  Resize { w: u16, h: u16 },
}

#[derive(Debug)]
pub enum ProcEvent {
  Exited(u32),
  Started,
}
