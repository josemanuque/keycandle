//! Lighting effects simulated for the preview, as pure functions of time.
//!
//! Each key-lighting effect follows the formula of the QMK effect it stands for
//! (quantum/rgb_matrix/animations/*_anim.h), in QMK's 0..255 units: hue offsets from the key's
//! position in LED space (x 0..224, y 0..64), time scaled by the speed setting. Effects that react
//! to typing use a simulated stream of key presses so the preview is alive without typing.

use gpui::Hsla;

use super::geometry::{Key, KEYS, SIDE_LIGHTS};
use crate::color::hsv;

/// What the keyboard is set to, plus the clock.
#[derive(Clone, Copy)]
pub struct Scene {
    pub time_ms: f32,
    /// Base color and brightness, QMK units 0..255.
    pub hue: u8,
    pub sat: u8,
    pub val: u8,
    pub speed: u8,
}

pub type KeyColors = [Option<Hsla>; KEYS.len()];

/// Color in QMK units (hue wraps).
fn c(h: f32, s: f32, v: f32) -> Option<Hsla> {
    let v = v.clamp(0.0, 255.0);
    (v > 0.5).then(|| hsv(h.rem_euclid(256.0) / 256.0, (s / 255.0).clamp(0.0, 1.0), v / 255.0))
}

fn sin8(x: f32) -> f32 {
    ((x / 256.0 * std::f32::consts::TAU).sin() + 1.0) * 127.5
}

/// atan2 mapped to 0..255 like lib8tion's atan2_8.
fn atan2_8(dy: f32, dx: f32) -> f32 {
    (dy.atan2(dx) / std::f32::consts::TAU * 256.0).rem_euclid(256.0)
}

/// Small integer hash for repeatable "random" choices.
fn hash(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 12)
}

/// Simulated typing: one key press every PRESS_MS, at a pseudo-random key.
const PRESS_MS: f32 = 380.0;

/// The last `n` simulated presses: (key index, ms since that press).
fn recent_presses(time_ms: f32, n: u32) -> impl Iterator<Item = (usize, f32)> {
    let slot = (time_ms / PRESS_MS) as u32;
    (0..n.min(slot + 1)).map(move |back| {
        let s = slot - back;
        ((hash(s, 7) as usize) % KEYS.len(), time_ms - s as f32 * PRESS_MS)
    })
}

fn distance(a: &Key, b: &Key) -> f32 {
    let ((ax, ay), (bx, by)) = (a.led_point(), b.led_point());
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

/// Key colors for one effect (index into settings::KEY_EFFECTS; 0 = off).
pub fn key_colors(effect: usize, s: &Scene) -> KeyColors {
    let (h, sat, v) = (s.hue as f32, s.sat as f32, s.val as f32);
    // QMK: time = scale16by8(timer, speed/4 + 1), used modulo 256
    let time = s.time_ms * (s.speed as f32 / 4.0 + 1.0) / 256.0;
    let scale = 64.0 * s.speed as f32 / 255.0;
    let mut out: KeyColors = [None; KEYS.len()];

    for (i, key) in KEYS.iter().enumerate() {
        let (x, y) = key.led_point();
        let (dx, dy) = (x - 112.0, y - 32.0);
        out[i] = match effect {
            0 => None,
            1 => c(h, sat, v),
            2 => c(h + scale * (y / 16.0).floor(), sat, v),         // gradient up/down
            3 => c(h + scale * x / 32.0, sat, v),                   // gradient left/right
            4 => c(h, sat, v * ((sin8(time / 2.0) - 127.5).abs() * 2.0 / 255.0)), // breathing
            5 => c(h, sat, v - (x * 228.0 / 255.0 + 28.0 - time.rem_euclid(256.0)).abs() * 8.0), // band
            6 => c(time, sat, v),                                   // cycle all
            7 => c(x - time, sat, v),                               // cycle left/right
            8 => c(y - time, sat, v),                               // cycle up/down
            9 => c(dy.abs() + x - time, sat, v),                    // rainbow chevron
            10 => c(atan2_8(dy, dx) + time, sat, v),                // cycle pinwheel
            11 => c((dx * dx + dy * dy).sqrt() - time - atan2_8(dy, dx), sat, v), // cycle spiral
            12 => {
                // raindrops: keys take a nearby hue at random moments
                let bucket = ((s.time_ms + hash(i as u32, 1) as f32 % 2000.0) / 1400.0) as u32;
                let off = (hash(i as u32, bucket) % 64) as f32 - 32.0;
                c(h + off, sat, v)
            }
            13 => {
                // jellybean raindrops: random hue and saturation per key
                let bucket = ((s.time_ms + hash(i as u32, 2) as f32 % 2000.0) / 1200.0) as u32;
                let r = hash(i as u32, bucket);
                c((r % 256) as f32, 128.0 + ((r >> 8) % 128) as f32, v)
            }
            14 => {
                // pixel rain: a few random keys lit in random colors
                let bucket = ((s.time_ms + hash(i as u32, 3) as f32 % 3000.0) / 900.0) as u32;
                let r = hash(i as u32, bucket);
                (r % 5 == 0).then(|| c((r >> 8) as f32 % 256.0, 255.0, v)).flatten()
            }
            15 => {
                // typing heatmap: heat around recent presses, blue (cold) to red (hot)
                let heat: f32 = recent_presses(s.time_ms, 12)
                    .map(|(k, age)| {
                        let near = (1.0 - distance(key, &KEYS[k]) / 40.0).max(0.0);
                        near * (1.0 - age / (PRESS_MS * 12.0)).max(0.0)
                    })
                    .sum::<f32>()
                    .min(1.0);
                (heat > 0.02).then(|| c(170.0 * (1.0 - heat), 255.0, v)).flatten()
            }
            16 => {
                // digital rain: green drops falling down each column
                let column = (x / 13.0) as u32;
                let fall = (s.time_ms / 90.0 + (hash(column, 4) % 40) as f32) % 12.0;
                let gap = (fall - key.row as f32).abs();
                (gap < 1.5).then(|| c(85.0, 255.0, v * (1.0 - gap / 1.5))).flatten()
            }
            17 => {
                // reactive: a pressed key lights up and fades
                let glow = recent_presses(s.time_ms, 3)
                    .filter(|&(k, _)| k == i)
                    .map(|(_, age)| (1.0 - age / 900.0).max(0.0))
                    .fold(0.0, f32::max);
                c(h, sat, v * glow)
            }
            18 | 19 | 20 => {
                // wide reactive / splash / multi splash: rings spreading from presses
                let presses = if effect == 19 { 1 } else { 4 };
                let mut glow = 0.0f32;
                let mut hue_shift = 0.0f32;
                for (k, age) in recent_presses(s.time_ms, presses) {
                    let ring = age * 0.12; // LED units per ms
                    let off = (distance(key, &KEYS[k]) - ring).abs();
                    let g = (1.0 - off / 18.0).max(0.0) * (1.0 - age / 1500.0).max(0.0);
                    if g > glow {
                        glow = g;
                        hue_shift = ring;
                    }
                }
                if effect == 18 {
                    c(h, sat, v * glow)
                } else {
                    c(h + hue_shift * glow, sat, v * (0.25 + 0.75 * glow))
                }
            }
            _ => c(h, sat, v),
        };
    }
    out
}

/// Side-light setup (see the firmware's side_update / side_effect).
#[derive(Clone, Copy)]
pub struct SideScene {
    pub time_ms: f32,
    /// 0 follow keyboard, 1 fixed color, 2 off
    pub mode: u8,
    /// Color shown before the effect (keyboard color or fixed), QMK units.
    pub hue: u8,
    pub sat: u8,
    /// 0 static, 1 breathing, 2 color cycle, 3 type pulse, 4 wave
    pub effect: u8,
    /// 0..31
    pub speed: u8,
}

pub type SideColors = [Option<Hsla>; SIDE_LIGHTS];

pub fn side_colors(s: &SideScene) -> SideColors {
    if s.mode == 2 {
        return [None; SIDE_LIGHTS];
    }
    let period = 4000.0 - s.speed as f32 * 100.0;
    let angle = s.time_ms.rem_euclid(period) / period * 256.0;
    let (mut h, sat, mut v) = (s.hue as f32, s.sat as f32, 255.0);
    let mut lit = [true; SIDE_LIGHTS];
    match s.effect {
        1 => v *= (32.0 + sin8(angle) * 223.0 / 255.0) / 255.0,
        2 => h = angle,
        3 => {
            let since = s.time_ms.rem_euclid(PRESS_MS);
            let flare = (1.0 - since / (period / 4.0)).max(0.0);
            v *= (48.0 + flare * 207.0) / 255.0;
        }
        4 => {
            let step = (s.time_ms.rem_euclid(period) / period * 8.0) as usize; // 0..7
            let at = if step < 5 { step } else { 8 - step };
            for (p, on) in lit.iter_mut().enumerate() {
                *on = p == at || p == at + 1;
            }
        }
        _ => {}
    }
    std::array::from_fn(|p| if lit[p] { c(h, sat, v) } else { None })
}
