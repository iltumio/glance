use crate::games::{Grid, HEIGHT, WIDTH};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

pub struct MatrixWidget<'a> {
    grid: &'a Grid,
    highlight: Option<crate::widgets::Bounds>,
}
impl<'a> MatrixWidget<'a> {
    pub fn new(grid: &'a Grid) -> Self {
        Self {
            grid,
            highlight: None,
        }
    }
    pub fn highlight(mut self, bounds: Option<crate::widgets::Bounds>) -> Self {
        self.highlight = bounds;
        self
    }
}

pub fn geometry(area: Rect) -> Option<(Rect, bool)> {
    let compact = area.height < HEIGHT as u16;
    let (width, height) = if compact {
        (WIDTH as u16, HEIGHT.div_ceil(2) as u16)
    } else {
        ((WIDTH * 2) as u16, HEIGHT as u16)
    };
    if area.width < width || area.height < height {
        return None;
    }
    Some((
        Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        ),
        compact,
    ))
}
impl Widget for MatrixWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some((matrix, compact)) = geometry(area) else {
            return;
        };
        let sx = matrix.x;
        let sy = matrix.y;
        for (x, col) in self.grid.iter().enumerate() {
            if compact {
                for row in 0..HEIGHT.div_ceil(2) {
                    let top = col[row * 2];
                    let bottom = col.get(row * 2 + 1).copied().unwrap_or(false);
                    // Braille keeps both LED rows as separate dots in one cell.
                    let symbol = match (top, bottom) {
                        (true, true) => "⠅",
                        (true, false) => "⠁",
                        (false, true) => "⠄",
                        _ => " ",
                    };
                    buf.set_string(
                        sx + x as u16,
                        sy + row as u16,
                        symbol,
                        Style::default().fg(Color::White).bg(
                            if self.highlight.is_some_and(|bounds| {
                                bounds.contains(x, row * 2) || bounds.contains(x, row * 2 + 1)
                            }) {
                                Color::DarkGray
                            } else {
                                Color::Black
                            },
                        ),
                    );
                }
            } else {
                for (y, &pixel) in col.iter().enumerate() {
                    let selected = self.highlight.is_some_and(|bounds| bounds.contains(x, y));
                    buf.set_string(
                        sx + x as u16 * 2,
                        sy + y as u16,
                        if pixel { "● " } else { "· " },
                        Style::default()
                            .fg(if pixel {
                                Color::White
                            } else if selected {
                                Color::Black
                            } else {
                                Color::DarkGray
                            })
                            .bg(if selected {
                                Color::DarkGray
                            } else {
                                Color::Black
                            }),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_grid_contains_both_top_and_bottom_pixels() {
        let area = Rect::new(0, 0, 9, 17);
        let mut grid = [[false; HEIGHT]; WIDTH];
        grid[0][0] = true;
        grid[1][1] = true;
        grid[8][33] = true;
        let mut buf = Buffer::empty(area);
        MatrixWidget::new(&grid).render(area, &mut buf);
        assert_eq!(buf[(0, 0)].symbol(), "⠁");
        assert_eq!(buf[(1, 0)].symbol(), "⠄");
        assert_eq!(buf[(8, 16)].symbol(), "⠄");
    }
}
