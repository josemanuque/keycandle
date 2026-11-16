//! Responsive sizing: how much room the page has, and how to split it.
//!
//! GPUI lays out with flexbox but has no media queries, so pages ask this for their widths: the
//! window's width minus the sidebar, capped so lines stay readable on very wide screens.

/// gpui-component's sidebar width.
const SIDEBAR: f32 = 255.0;
/// Page padding (both sides) and room for the scrollbar.
const CHROME: f32 = 2.0 * 32.0 + 12.0;
const MAX_CONTENT: f32 = 1480.0;
const MIN_CONTENT: f32 = 320.0;
/// From this content width on, pages use two columns.
const WIDE: f32 = 980.0;
pub const GAP: f32 = 20.0;

#[derive(Clone, Copy)]
pub struct Layout {
    /// Width available to the page's content, in px.
    pub width: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self { width: 860.0 }
    }
}

impl Layout {
    pub fn for_window(window_width: f32) -> Self {
        Self { width: (window_width - SIDEBAR - CHROME).clamp(MIN_CONTENT, MAX_CONTENT) }
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
    (n, (width - gap * (n - 1) as f32) / n as f32)
}

/// Scale for the drawn keyboard (760 px at 1.0) to fill `width`, never larger than `max`.
pub fn keyboard_scale(width: f32, max: f32) -> f32 {
    (width / 760.0).min(max)
}
