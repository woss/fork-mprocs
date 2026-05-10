use crate::color;
use crate::term::Grid;
use crate::term::attrs::Attrs;
use crate::term::grid::{BorderType, Rect};

pub fn draw_frame(grid: &mut Grid, area: Rect, title: &str, focused: bool) {
  if area.width < 2 || area.height < 2 {
    return;
  }
  let (border, fg) = if focused {
    (BorderType::Thick.chars(), color!("#bee6f4"))
  } else {
    (BorderType::Plain.chars(), color!("#666666"))
  };
  let attrs = Attrs::default().fg(fg).bg(color!("#111111"));
  grid.draw_block(area, &border, attrs);
  grid.draw_text(area.move_left(1).move_right(-2), title, attrs);
}
