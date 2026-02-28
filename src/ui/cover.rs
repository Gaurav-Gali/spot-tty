//! Cover art renderer using Unicode half-block technique.
//!
//! Each terminal cell = 1 col wide, 2 pixels tall via "▀":
//!   fg = top pixel,  bg = bottom pixel
//!
//! Resolution scales with cell count:
//!   - 8×4 cells  → 8×8   pixels  (sidebar rows)
//!   - 16×16 cells → 16×32 pixels  (explorer detail panel)
//!
//! Uses Lanczos3 resampling for maximum sharpness at small sizes.

use image::imageops::FilterType;
use image::GenericImageView;
use ratatui::{layout::Rect, style::Color, Frame};

#[derive(Clone, Debug)]
pub struct CoverArt {
    pub cells: Vec<Vec<(Color, Color)>>,
    pub width: u16,
    pub height: u16,
}

impl CoverArt {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let buf = frame.buffer_mut();
        for (row, row_cells) in self.cells.iter().enumerate() {
            let y = area.y + row as u16;
            if y >= area.y + area.height {
                break;
            }
            for (col, (fg, bg)) in row_cells.iter().enumerate() {
                let x = area.x + col as u16;
                if x >= area.x + area.width {
                    break;
                }
                let cell = buf.get_mut(x, y);
                cell.set_symbol("▀");
                cell.set_fg(*fg);
                cell.set_bg(*bg);
            }
        }
    }

    pub fn placeholder(width: u16, height: u16) -> Self {
        // Subtle dark grid pattern so it doesn't look like a crash
        let mut cells = Vec::with_capacity(height as usize);
        for r in 0..height {
            let mut row = Vec::with_capacity(width as usize);
            for c in 0..width {
                let checker = ((r + c) % 2 == 0);
                let fg = if checker {
                    Color::Rgb(45, 45, 55)
                } else {
                    Color::Rgb(35, 35, 45)
                };
                let bg = Color::Rgb(28, 28, 36);
                row.push((fg, bg));
            }
            cells.push(row);
        }
        Self {
            cells,
            width,
            height,
        }
    }

    /// Rasterise raw image bytes into a CoverArt of the given cell dimensions.
    pub fn from_bytes(bytes: &[u8], w: u16, h: u16) -> Option<Self> {
        let img = image::load_from_memory(bytes).ok()?;
        // Lanczos3 is the sharpest standard resampling filter
        let resized = img.resize_exact(w as u32, (h * 2) as u32, FilterType::Lanczos3);

        let mut cells = Vec::with_capacity(h as usize);
        for row in 0..h {
            let mut row_cells = Vec::with_capacity(w as usize);
            for col in 0..w {
                let top = resized.get_pixel(col as u32, (row * 2) as u32);
                let bottom = resized.get_pixel(col as u32, (row * 2 + 1) as u32);
                row_cells.push((
                    Color::Rgb(top[0], top[1], top[2]),
                    Color::Rgb(bottom[0], bottom[1], bottom[2]),
                ));
            }
            cells.push(row_cells);
        }
        Some(Self {
            cells,
            width: w,
            height: h,
        })
    }
}

/// Fetch image bytes from a URL and render at two sizes:
///   (small, large) = ((sw, sh), (lw, lh)) cells
/// Both are returned so we don't fetch twice.
pub async fn fetch_cover_dual(
    url: &str,
    small_w: u16,
    small_h: u16,
    large_w: u16,
    large_h: u16,
) -> Option<(CoverArt, CoverArt)> {
    let bytes = reqwest::get(url).await.ok()?.bytes().await.ok()?;
    let small = CoverArt::from_bytes(&bytes, small_w, small_h)?;
    let large = CoverArt::from_bytes(&bytes, large_w, large_h)?;
    Some((small, large))
}

/// Fetch at a single size (used when only one size is needed).
pub async fn fetch_cover(url: &str, w: u16, h: u16) -> Option<CoverArt> {
    let bytes = reqwest::get(url).await.ok()?.bytes().await.ok()?;
    CoverArt::from_bytes(&bytes, w, h)
}
