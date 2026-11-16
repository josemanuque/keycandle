//! Colour conversions between the keyboard and the screen.
//!
//! The keyboard stores colours as HSV hue + saturation (0..255 each; brightness is a separate
//! setting). GPUI draws HSL. Kept in one place so every view converts the same way.

use gpui::{hsla, Hsla};

/// HSV (each 0..1) to a GPUI colour.
pub fn hsv(h: f32, s: f32, v: f32) -> Hsla {
    let l = v * (1.0 - s / 2.0);
    let sl = if l <= 0.0 || l >= 1.0 { 0.0 } else { (v - l) / l.min(1.0 - l) };
    hsla(h.rem_euclid(1.0), sl, l, 1.0)
}

/// An LED colour as light on a surface: full-brightness hue with the LED's brightness as opacity,
/// so a dim LED fades toward the unlit key instead of turning black.
pub fn as_light(c: Hsla) -> Hsla {
    let v = c.l + c.s * c.l.min(1.0 - c.l);
    let s = if v > 0.0 { 2.0 * (1.0 - c.l / v) } else { 0.0 };
    Hsla { a: v.clamp(0.0, 1.0), ..hsv(c.h, s, 1.0) }
}

/// A stored keyboard colour ([hue, saturation]) at full brightness.
pub fn from_stored(hs: &[u8]) -> Hsla {
    hsv(hs[0] as f32 / 255.0, hs[1] as f32 / 255.0, 1.0)
}

/// A picked colour back to the keyboard's [hue, saturation] (its brightness is dropped).
pub fn to_stored(c: Hsla) -> [u8; 2] {
    let v = c.l + c.s * c.l.min(1.0 - c.l);
    let s = if v > 0.0 { 2.0 * (1.0 - c.l / v) } else { 0.0 };
    [to_byte(c.h), to_byte(s)]
}

fn to_byte(x: f32) -> u8 {
    (x * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_colours_round_trip() {
        for hs in [[0u8, 255u8], [43, 255], [170, 128], [85, 0]] {
            let back = to_stored(from_stored(&hs));
            assert!((back[0] as i16 - hs[0] as i16).abs() <= 1 || hs[1] == 0, "{hs:?} -> {back:?}");
            assert!((back[1] as i16 - hs[1] as i16).abs() <= 1, "{hs:?} -> {back:?}");
        }
    }
}
