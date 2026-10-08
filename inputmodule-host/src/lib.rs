pub mod font;
pub mod protocol;

pub const WIDTH: usize = 9;
pub const HEIGHT: usize = 34;
pub const BW_BYTES: usize = (WIDTH * HEIGHT).div_ceil(8);
pub type Grid = [[bool; HEIGHT]; WIDTH];

pub fn empty_grid() -> Grid {
    [[false; HEIGHT]; WIDTH]
}

pub fn encode_bw_grid(grid: &Grid) -> [u8; BW_BYTES] {
    let mut bytes = [0; BW_BYTES];
    for (x, col) in grid.iter().enumerate() {
        for (y, &pixel) in col.iter().enumerate() {
            if pixel {
                let bit = x + WIDTH * y;
                bytes[bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    bytes
}

pub fn render_string(s: &str) -> Grid {
    let glyphs: Vec<_> = s.chars().take(5).map(font::convert_font).collect();
    render_font(&glyphs)
}

pub fn render_font(glyphs: &[Vec<u8>]) -> Grid {
    let mut grid = empty_grid();
    for (index, glyph) in glyphs.iter().take(5).enumerate() {
        for y in 0..6 {
            for (x, col) in grid.iter_mut().enumerate().skip(2).take(5) {
                col[y + index * 7] = glyph.get(x - 2 + y * 5) == Some(&1);
            }
        }
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrips_every_pixel_and_zeroes_padding() {
        for x in 0..WIDTH {
            for y in 0..HEIGHT {
                let mut grid = empty_grid();
                grid[x][y] = true;
                let bytes = encode_bw_grid(&grid);
                let bit = x + WIDTH * y;
                assert_eq!(bytes[bit / 8], 1 << (bit % 8));
                assert_eq!(bytes.iter().map(|b| b.count_ones()).sum::<u32>(), 1);
                assert_eq!(bytes[BW_BYTES - 1] & 0xfc, 0);
            }
        }
    }

    #[test]
    fn text_is_bounded_to_five_glyphs() {
        assert_eq!(render_string("12:34ignored"), render_string("12:34"));
        assert!(render_string("12:34").iter().flatten().any(|&p| p));
        assert_eq!(render_font(&[vec![]]), empty_grid());
    }
}
