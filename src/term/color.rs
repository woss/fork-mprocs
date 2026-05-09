#[macro_export]
macro_rules! color {
  ($hex:literal) => {{
    const COLOR: $crate::term::Color = {
      let bytes = $hex.as_bytes();
      assert!(bytes.len() == 7, "expected #RRGGBB format");
      assert!(bytes[0] == b'#', "expected #RRGGBB format");

      const fn hex_digit(b: u8) -> u8 {
        match b {
          b'0'..=b'9' => b - b'0',
          b'a'..=b'f' => b - b'a' + 10,
          b'A'..=b'F' => b - b'A' + 10,
          _ => panic!("invalid hex digit"),
        }
      }

      let r = hex_digit(bytes[1]) * 16 + hex_digit(bytes[2]);
      let g = hex_digit(bytes[3]) * 16 + hex_digit(bytes[4]);
      let b = hex_digit(bytes[5]) * 16 + hex_digit(bytes[6]);
      $crate::term::Color::Rgb(r, g, b)
    };
    COLOR
  }};
}

/// Represents a foreground or background color for cells.
#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum Color {
  /// The default terminal color.
  Default,

  /// An indexed terminal color.
  Idx(u8),

  /// An RGB terminal color. The parameters are (red, green, blue).
  Rgb(u8, u8, u8),
}

#[allow(dead_code)]
impl Color {
  pub const BLACK: Self = Color::Idx(0);
  pub const RED: Self = Color::Idx(1);
  pub const GREEN: Self = Color::Idx(2);
  pub const YELLOW: Self = Color::Idx(3);
  pub const BLUE: Self = Color::Idx(4);
  pub const MAGENTA: Self = Color::Idx(5);
  pub const CYAN: Self = Color::Idx(6);
  pub const WHITE: Self = Color::Idx(7);

  pub const BRIGHT_BLACK: Self = Color::Idx(8);
  pub const BRIGHT_RED: Self = Color::Idx(9);
  pub const BRIGHT_GREEN: Self = Color::Idx(10);
  pub const BRIGHT_YELLOW: Self = Color::Idx(11);
  pub const BRIGHT_BLUE: Self = Color::Idx(12);
  pub const BRIGHT_MAGENTA: Self = Color::Idx(13);
  pub const BRIGHT_CYAN: Self = Color::Idx(14);
  pub const BRIGHT_WHITE: Self = Color::Idx(15);
}

impl Color {
  pub fn dim(self, factor: u8) -> Self {
    let (r, g, b) = match self {
      Color::Default => (200, 200, 200),
      Color::Rgb(r, g, b) => (r, g, b),
      Color::Idx(i) => idx_to_rgb(i),
    };
    let f = factor as u16;
    Color::Rgb(
      (r as u16 * f / 255) as u8,
      (g as u16 * f / 255) as u8,
      (b as u16 * f / 255) as u8,
    )
  }
}

fn idx_to_rgb(idx: u8) -> (u8, u8, u8) {
  match idx {
    0 => (0, 0, 0),
    1 => (170, 0, 0),
    2 => (0, 170, 0),
    3 => (170, 170, 0),
    4 => (0, 0, 170),
    5 => (170, 0, 170),
    6 => (0, 170, 170),
    7 => (170, 170, 170),
    8 => (85, 85, 85),
    9 => (255, 85, 85),
    10 => (85, 255, 85),
    11 => (255, 255, 85),
    12 => (85, 85, 255),
    13 => (255, 85, 255),
    14 => (85, 255, 255),
    15 => (255, 255, 255),
    16..=231 => {
      let i = idx - 16;
      let b_idx = i % 6;
      let g_idx = (i / 6) % 6;
      let r_idx = i / 36;
      let component = |c: u8| if c == 0 { 0 } else { 55 + 40 * c };
      (component(r_idx), component(g_idx), component(b_idx))
    }
    232..=255 => {
      let v = 8 + 10 * (idx - 232);
      (v, v, v)
    }
  }
}

impl Default for Color {
  fn default() -> Self {
    Self::Default
  }
}
