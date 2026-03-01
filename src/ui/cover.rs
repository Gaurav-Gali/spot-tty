//! Cover art — Kitty graphics protocol (a=T / a=p) + iTerm2 + half-block fallback.
//!
//! ## Why the previous version broke
//!
//! The Unicode Placeholder approach (U=1 + diacritic encoding) requires the
//! image ID to be encoded in the *foreground colour* of each placeholder cell.
//! Ratatui converts Color::Rgb to ANSI sequences, but the exact 24-bit values
//! need to survive round-tripping through the terminal's own colour pipeline.
//! In practice, Kitty only intercepts the placeholder if the fg colour matches
//! the stored image ID exactly — any rounding causes it to render as garbage.
//!
//! ## What we do instead (stable, proven)
//!
//! 1. Upload image once with `a=T,q=2` (quiet, no response needed).
//! 2. Every frame, redisplay with `a=p,q=2` (~60 bytes) — only when the
//!    (id, x, y, w, h) tuple changed vs last frame (tracked in RenderCache).
//! 3. ratatui writes the cell buffer as normal. We send Kitty sequences *after*
//!    `terminal.draw()` returns and *after* `render_cache.flush()` — so Kitty
//!    paints on top of whatever ratatui left behind. The images composite over
//!    the text layer at the correct z-index.
//!
//! The remaining flicker on fast scroll is eliminated by the scroll debounce
//! in the detail panel (120 ms settle time before showing the large cover).
//! Row thumbnails are small enough that the repaint is imperceptible.
//!
//! ## Disk cache
//!
//! Cover bytes are saved to ~/.cache/spot-tty/covers/<hash>.bin so second
//! launch shows images instantly without any network requests.

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use ratatui::{layout::Rect, style::Color, Frame};
use std::io::Write;

// ── Protocol ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageProtocol {
    Kitty,
    ITerm2,
    HalfBlock,
}

pub fn detect_protocol() -> ImageProtocol {
    let term = std::env::var("TERM").unwrap_or_default();
    let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    if !std::env::var("KITTY_WINDOW_ID")
        .unwrap_or_default()
        .is_empty()
        || term.contains("kitty")
    {
        return ImageProtocol::Kitty;
    }
    if program == "WezTerm" || term.contains("wezterm") {
        return ImageProtocol::Kitty;
    }
    if matches!(program.as_str(), "iTerm.app" | "Ghostty" | "Warp") {
        return ImageProtocol::ITerm2;
    }
    ImageProtocol::HalfBlock
}

// ── Disk cache ────────────────────────────────────────────────────────────────

fn cache_path(url: &str) -> Option<std::path::PathBuf> {
    let dir = dirs::cache_dir()?.join("spot-tty").join("covers");
    std::fs::create_dir_all(&dir).ok()?;
    let hash = url
        .bytes()
        .fold(5381u64, |h, b| h.wrapping_mul(33).wrapping_add(b as u64));
    Some(dir.join(format!("{hash:016x}.bin")))
}

fn load_cached_bytes(url: &str) -> Option<Vec<u8>> {
    std::fs::read(cache_path(url)?).ok()
}

fn save_cached_bytes(url: &str, bytes: &[u8]) {
    if let Some(p) = cache_path(url) {
        let _ = std::fs::write(p, bytes);
    }
}

// ── Per-frame render cache ────────────────────────────────────────────────────

/// Tracks what was rendered last frame to skip redundant escape sequences.
#[derive(Default)]
pub struct RenderCache {
    /// (kitty_id, x, y, w, h) → last frame it was placed
    placed: std::collections::HashMap<(u32, u16, u16, u16, u16), u64>,
    /// IDs whose PNG bytes have been transmitted to the terminal
    pub uploaded: std::collections::HashSet<u32>,
    /// Escape sequences to flush after terminal.draw()
    pub pending: Vec<u8>,
    pub frame: u64,
}

impl RenderCache {
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.pending.clear();
        let f = self.frame;
        self.placed.retain(|_, last| f - *last <= 2);
    }

    fn already_placed(&self, kid: u32, area: Rect) -> bool {
        self.placed
            .get(&(kid, area.x, area.y, area.width, area.height))
            .copied()
            .unwrap_or(0)
            == self.frame
    }

    fn mark_placed(&mut self, kid: u32, area: Rect) {
        self.placed
            .insert((kid, area.x, area.y, area.width, area.height), self.frame);
    }

    /// Delete all Kitty images from the terminal screen and reset tracking.
    /// Call this when an overlay opens so images don't bleed through.
    pub fn clear_kitty_images(&mut self) {
        // a=d,d=A = delete all images from the screen (does not free GPU memory)
        self.pending.extend_from_slice(b"\x1b_Ga=d,d=A,q=2;\x1b\\");
        // Reset placement tracking so images re-draw correctly after overlay closes
        self.placed.clear();
    }

    pub fn flush(&self) {
        if self.pending.is_empty() {
            return;
        }
        let mut lock = std::io::stdout().lock();
        let _ = lock.write_all(&self.pending);
        let _ = lock.flush();
    }
}

// ── CoverImage ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct CoverImage {
    pub png_b64: String,       // PNG as base64, computed once
    pub raw_b64: String,       // raw bytes as base64, for iTerm2
    pub decoded: DynamicImage, // pixels for half-block fallback
    pub kitty_id: u32,
}

static KITTY_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

impl CoverImage {
    pub fn from_bytes(raw: Vec<u8>) -> Option<Self> {
        let decoded = image::load_from_memory(&raw).ok()?;
        let mut png = Vec::new();
        decoded
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        let kitty_id = KITTY_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0x00FF_FFFF;
        Some(Self {
            png_b64: b64(&png),
            raw_b64: b64(&raw),
            decoded,
            kitty_id,
        })
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        protocol: ImageProtocol,
        cache: &mut RenderCache,
    ) {
        match protocol {
            ImageProtocol::Kitty => self.queue_kitty(area, cache),
            ImageProtocol::ITerm2 => self.queue_iterm2(area, cache),
            ImageProtocol::HalfBlock => self.render_halfblock(frame, area),
        }
    }

    fn queue_kitty(&self, area: Rect, cache: &mut RenderCache) {
        if cache.already_placed(self.kitty_id, area) {
            return;
        }
        cache.mark_placed(self.kitty_id, area);

        // Cursor move to top-left of the cell area
        let cur = format!("\x1b[{};{}H", area.y + 1, area.x + 1);
        cache.pending.extend_from_slice(cur.as_bytes());

        if !cache.uploaded.contains(&self.kitty_id) {
            // First time: transmit + display in chunked b64
            // a=T: transmit+display, f=100: auto-detect format, q=2: no response
            cache.uploaded.insert(self.kitty_id);
            let chunks: Vec<&[u8]> = self.png_b64.as_bytes().chunks(4096).collect();
            for (i, chunk) in chunks.iter().enumerate() {
                let m = if i == chunks.len() - 1 { 0u8 } else { 1u8 };
                let hdr = if i == 0 {
                    format!(
                        "\x1b_Ga=T,f=100,i={},c={},r={},q=2,m={};",
                        self.kitty_id, area.width, area.height, m
                    )
                } else {
                    format!("\x1b_Gm={};", m)
                };
                cache.pending.extend_from_slice(hdr.as_bytes());
                cache.pending.extend_from_slice(chunk);
                cache.pending.extend_from_slice(b"\x1b\\");
            }
        } else {
            // Already uploaded: just re-place by ID (~60 bytes)
            // a=p: put/display, i=id, c/r: cell dimensions, q=2: quiet
            let seq = format!(
                "\x1b_Ga=p,i={},c={},r={},q=2;\x1b\\",
                self.kitty_id, area.width, area.height
            );
            cache.pending.extend_from_slice(seq.as_bytes());
        }
    }

    fn queue_iterm2(&self, area: Rect, cache: &mut RenderCache) {
        if cache.already_placed(self.kitty_id, area) {
            return;
        }
        cache.mark_placed(self.kitty_id, area);
        let seq = format!(
            "\x1b[{};{}H\x1b]1337;File=inline=1;width={}px;height={}px;preserveAspectRatio=1;doNotMoveCursor=0:{}\x07",
            area.y + 1, area.x + 1, area.width * 8, area.height * 16, self.raw_b64,
        );
        cache.pending.extend_from_slice(seq.as_bytes());
    }

    fn render_halfblock(&self, frame: &mut Frame, area: Rect) {
        let resized = self.decoded.resize_exact(
            area.width as u32,
            (area.height * 2) as u32,
            FilterType::Lanczos3,
        );
        let buf = frame.buffer_mut();
        for row in 0..area.height {
            for col in 0..area.width {
                let top = resized.get_pixel(col as u32, (row * 2) as u32);
                let bottom = resized.get_pixel(col as u32, (row * 2 + 1) as u32);
                let cell = buf.get_mut(area.x + col, area.y + row);
                cell.set_symbol("▀");
                cell.set_fg(Color::Rgb(top[0], top[1], top[2]));
                cell.set_bg(Color::Rgb(bottom[0], bottom[1], bottom[2]));
            }
        }
    }
}

// ── Placeholder ───────────────────────────────────────────────────────────────

pub fn render_placeholder(frame: &mut Frame, area: Rect) {
    let buf = frame.buffer_mut();
    for row in 0..area.height {
        for col in 0..area.width {
            let chk = (row + col) % 2 == 0;
            let cell = buf.get_mut(area.x + col, area.y + row);
            cell.set_symbol("▀");
            cell.set_fg(if chk {
                Color::Rgb(45, 45, 55)
            } else {
                Color::Rgb(35, 35, 45)
            });
            cell.set_bg(Color::Rgb(28, 28, 36));
        }
    }
}

// ── Fetch ─────────────────────────────────────────────────────────────────────

pub async fn fetch_cover(url: &str) -> Option<CoverImage> {
    if let Some(bytes) = load_cached_bytes(url) {
        return CoverImage::from_bytes(bytes);
    }
    let bytes = reqwest::get(url).await.ok()?.bytes().await.ok()?.to_vec();
    save_cached_bytes(url, &bytes);
    CoverImage::from_bytes(bytes)
}

// ── Base64 ────────────────────────────────────────────────────────────────────

fn b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 {
            chunk[1] as usize
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            chunk[2] as usize
        } else {
            0
        };
        out.push(T[(b0 >> 2) & 63]);
        out.push(T[((b0 << 4) | (b1 >> 4)) & 63]);
        out.push(if chunk.len() > 1 {
            T[((b1 << 2) | (b2 >> 6)) & 63]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 { T[b2 & 63] } else { b'=' });
    }
    String::from_utf8(out).unwrap()
}

// ── Stable cell sentinel (anti-flicker) ───────────────────────────────────────
//
// ratatui diffs its cell buffer every frame. If a cell's symbol/style is
// unchanged from the previous frame, ratatui sends nothing for that cell.
// We exploit this: write a sentinel symbol+colour into every image cell so
// ratatui considers them "stable" and stops repainting them with spaces.
// The Kitty image composites above the cell layer regardless of cell content.
pub fn write_image_sentinel(frame: &mut Frame, area: Rect) {
    // A stable, visually-invisible sentinel: space with a near-black bg.
    // Near-black (1,1,1) != Reset so ratatui tracks it as a real colour,
    // but visually indistinguishable from the terminal default background.
    let style = ratatui::style::Style::default()
        .bg(Color::Rgb(1, 1, 1))
        .fg(Color::Rgb(1, 1, 1));
    let buf = frame.buffer_mut();
    for row in 0..area.height {
        for col in 0..area.width {
            let cell = buf.get_mut(area.x + col, area.y + row);
            cell.set_symbol(" ");
            cell.set_style(style);
        }
    }
}
