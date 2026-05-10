use crate::{
  console::state::ConsoleState,
  term::{Grid, grid::Rect},
};

pub trait Pane: Send {
  fn render(
    &mut self,
    grid: &mut Grid,
    area: Rect,
    state: &mut ConsoleState,
    focused: bool,
  );
}
