//! Cover art rendering — Kitty graphics protocol with aggressive frame dedup.
//!
//! ## How lag is avoided
//!
//! 1. **Upload once**: PNG sent via `a=T` on first render; stored in terminal GPU mem by ID.
//! 2. **Display once per position**: `a=p` (display by ID) is sent only when the image
//!    or its screen position changes. Tracked via `RenderCache` in AppState.
//! 3. **One flush per frame**: all escape sequences are accumulated into a single `Vec<u8>`
//!    and flushed once after `terminal.draw()` completes, not per-image.
//! 4. **No PNG re-encoding**: PNG bytes and base64 are computed at fetch time, never again.

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use ratatui::{layout::Rect, style::Color, Frame};
use std::io::Write;

// ── Protocol ─────────────────────────────────────────────────────────────────

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

// ── Per-frame render cache ────────────────────────────────────────────────────

/// Tracks what was actually drawn last frame so we can skip redundant writes.
#[derive(Default)]
pub struct RenderCache {
    /// (kitty_id, x, y, w, h) → frame_number it was last drawn
    placed: std::collections::HashMap<(u32, u16, u16, u16, u16), u64>,
    /// kitty IDs already uploaded (PNG transmitted)
    pub uploaded: std::collections::HashSet<u32>,
    /// Accumulated escape bytes for this frame — flushed once at end of draw
    pub pending: Vec<u8>,
    pub frame: u64,
}

impl RenderCache {
    /// Call at the start of every `terminal.draw()` to advance the frame counter
    /// and clear stale entries from the previous frame.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.pending.clear();
        // Remove entries older than 2 frames — they're no longer on screen
        let frame = self.frame;
        self.placed.retain(|_, f| frame - *f <= 2);
    }

    /// Returns true if this image at this position was already written this frame.
    pub fn already_placed(&self, kid: u32, area: Rect) -> bool {
        let key = (kid, area.x, area.y, area.width, area.height);
        self.placed.get(&key).copied().unwrap_or(0) == self.frame
    }

    pub fn mark_placed(&mut self, kid: u32, area: Rect) {
        self.placed
            .insert((kid, area.x, area.y, area.width, area.height), self.frame);
    }

    /// Flush all pending escape sequences to stdout in one syscall.
    pub fn flush(&self) {
        if self.pending.is_empty() {
            return;
        }
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = lock.write_all(&self.pending);
        let _ = lock.flush();
    }
}

// ── CoverImage ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct CoverImage {
    /// Pre-encoded PNG base64 — computed once at fetch, never again
    pub png_b64: String,
    /// Raw bytes base64 (JPEG/PNG) — for iTerm2
    pub raw_b64: String,
    /// Decoded pixels for half-block fallback
    pub decoded: DynamicImage,
    /// Unique ID for Kitty protocol (1-based, wraps at 2^24)
    pub kitty_id: u32,
}

static KITTY_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

impl CoverImage {
    pub fn from_bytes(raw: Vec<u8>) -> Option<Self> {
        let decoded = image::load_from_memory(&raw).ok()?;
        // PNG encode once
        let mut png_buf = Vec::new();
        decoded
            .write_to(
                &mut std::io::Cursor::new(&mut png_buf),
                image::ImageFormat::Png,
            )
            .ok()?;
        let kitty_id = KITTY_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0x00FF_FFFF;
        Some(Self {
            png_b64: base64_encode(&png_buf),
            raw_b64: base64_encode(&raw),
            decoded,
            kitty_id,
        })
    }

    /// Queue a render into `cache.pending`. Nothing is written to stdout here.
    /// The caller must call `cache.flush()` once per frame after all images are queued.
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
        // Skip entirely if same image at same position was already queued this frame
        if cache.already_placed(self.kitty_id, area) {
            return;
        }
        cache.mark_placed(self.kitty_id, area);

        let cursor = format!("\x1b[{};{}H", area.y + 1, area.x + 1);

        if cache.uploaded.contains(&self.kitty_id) {
            // Already in terminal memory — display by ID only (~60 bytes)
            let seq = format!(
                "{}\x1b_Ga=p,i={},c={},r={},q=2;\x1b\\",
                cursor, self.kitty_id, area.width, area.height
            );
            cache.pending.extend_from_slice(seq.as_bytes());
        } else {
            // First time: transmit + display. Mark uploaded immediately so next
            // frame takes the fast path even before flush completes.
            cache.uploaded.insert(self.kitty_id);
            cache.pending.extend_from_slice(cursor.as_bytes());
            let chunks: Vec<&[u8]> = self.png_b64.as_bytes().chunks(4096).collect();
            for (i, chunk) in chunks.iter().enumerate() {
                let m = if i == chunks.len() - 1 { 0u8 } else { 1u8 };
                let seq = if i == 0 {
                    format!(
                        "\x1b_Ga=T,f=100,i={},c={},r={},q=2,m={};",
                        self.kitty_id, area.width, area.height, m
                    )
                } else {
                    format!("\x1b_Gm={};", m)
                };
                cache.pending.extend_from_slice(seq.as_bytes());
                cache.pending.extend_from_slice(chunk);
                cache.pending.extend_from_slice(b"\x1b\\");
            }
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
    let bytes = reqwest::get(url).await.ok()?.bytes().await.ok()?;
    CoverImage::from_bytes(bytes.to_vec())
}

// ── Base64 ────────────────────────────────────────────────────────────────────

fn base64_encode(input: &[u8]) -> String {
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
