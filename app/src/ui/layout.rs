//! Responsive sizing: how much room the page has, and how to split it.
//!
//! GPUI lays out with flexbox but has no media queries, so pages ask this for their widths. The
//! width is measured from the page's content area after each layout (see app.rs), so it is exact
//! whatever the window, sidebar or scrollbar do; it is capped so lines stay readable on very wide
//! screens.

/// Widest the content gets.
pub const MAX_CONTENT: f32 = 1480.0;
const MIN_CONTENT: f32 = 280.0;
/// From this content width on, pages use two columns.
const WIDE: f32 = 980.0;
pub const GAP: f32 = 20.0;

#[derive(Clone, Copy)]
pub struct Layout {
    /// Width available to the page's content, in px.
    pub width: f32,
    /// The window's height, in px (previews keep to a share of it).
    pub window_height: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self { width: 860.0, window_height: 800.0 }
    }
}

impl Layout {
    /// Take a measured content width. Returns whether it changed (then the page lays out again).
    pub fn set_width(&mut self, measured: f32) -> bool {
        let width = measured.clamp(MIN_CONTENT, MAX_CONTENT).floor();
        let changed = (width - self.width).abs() >= 1.0;
        self.width = width;
        changed
    }

    pub fn is_wide(&self) -> bool {
        self.width >= WIDE
    }

    /// Two columns: `ratio` of the width for the first, the rest (minus the gap) for the second.
    pub fn split(&self, ratio: f32) -> (f32, f32) {
        let first = ((self.width - GAP) * ratio).floor();
        (first, self.width - GAP - first)
    }
}

/// How many columns of at least `min` px fit in `width` with `gap` between them, and how wide
/// each one is.
pub fn columns(width: f32, min: f32, gap: f32) -> (usize, f32) {
    let n = (((width + gap) / (min + gap)).floor() as usize).max(1);
    (n, ((width - gap * (n - 1) as f32) / n as f32).floor())
}

/// Scale for the drawn keyboard (760 × 300 px at 1.0) to fill `width`, never larger than `max`.
pub fn keyboard_scale(width: f32, max: f32) -> f32 {
    (width / 760.0).min(max)
}

impl Layout {
    /// Scale for a page's main keyboard preview: as wide as `width`, but at most ~40 % of the window's
    /// height, so short windows still show what's below it.
    pub fn preview_scale(&self, width: f32) -> f32 {
        keyboard_scale(width, 1.4).min(self.window_height * 0.4 / 300.0)
    }
}
