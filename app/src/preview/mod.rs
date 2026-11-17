//! The drawn RK71 used by the Overview and Lighting pages, at any size.

pub mod effects;
pub mod geometry;

use gpui::{div, prelude::*, px, rgb, Hsla, IntoElement, Pixels};

use crate::color::as_light;
use effects::{KeyColors, SideColors};
use geometry::{KEYS, ROWS, SIDE_LIGHTS, WIDTH_UNITS};

/// Full-size key unit in px; `scale` multiplies everything.
const UNIT: f32 = 40.0;
/// How much of the key's light shows through the keycap's top face.
const FACE_TINT: f32 = 0.35;

/// Draw the keyboard with these key and side-light colors (`None` = unlit).
pub fn keyboard(scale: f32, keys: &KeyColors, side: &SideColors) -> impl IntoElement {
    let u = UNIT * scale;
    let at = |v: f32| -> Pixels { px(v * scale) };
    let unlit: Hsla = rgb(0xC9CDD4).into();
    let face: Hsla = rgb(0xF4F5F7).into();
    let legend: Hsla = rgb(0x9AA0AA).into();
    let legends = scale >= 0.6;
    let width = WIDTH_UNITS * u;
    let height = ROWS as f32 * u;

    let caps = KEYS.iter().zip(keys.iter()).map(move |(key, color)| {
        let light = color.map(as_light);
        div()
            .absolute()
            .left(px(key.x * u))
            .top(px(key.row as f32 * u))
            .w(px(key.w * u - 4.0 * scale))
            .h(px(u - 4.0 * scale))
            .rounded(at(7.0))
            .bg(unlit)
            .when_some(light, |el, light| el.child(div().absolute().inset_0().rounded(at(7.0)).bg(light)))
            // Full size: the keycap's top face, with the light showing round its edge and through
            // the legend. Small: just the light.
            .when(legends, |el| {
                el.child(
                    div()
                        .absolute()
                        .top(at(2.0))
                        .left(at(2.0))
                        .right(at(2.0))
                        .bottom(at(5.0))
                        .rounded(at(5.0))
                        .bg(light.map_or(face, |l| face.blend(l.opacity(FACE_TINT))))
                        .px(at(4.0))
                        .py(at(3.0))
                        .text_size(at(if key.label.chars().count() > 3 { 8.0 } else { 11.0 }))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(light.map_or(legend, |l| legend.blend(l)))
                        .child(key.label),
                )
            })
    });

    let side_light = move |left: f32, i: usize, color: Option<Hsla>| {
        div()
            .absolute()
            .left(at(left))
            .top(at(66.0 + i as f32 * 30.0))
            .w(at(5.0))
            .h(at(18.0))
            .rounded(at(3.0))
            .bg(rgb(0x3A3F48))
            .when_some(color.map(as_light), |el, light| el.child(div().size_full().rounded(at(3.0)).bg(light)))
    };
    let lights = (0..SIDE_LIGHTS).flat_map(move |i| [side_light(11.0, i, side[i]), side_light(744.0, i, side[i])]);

    div()
        .relative()
        .w(at(760.0))
        .h(at(300.0))
        .child(
            div()
                .absolute()
                .top(at(20.0))
                .left(at(20.0))
                .right(at(20.0))
                .bottom(at(20.0))
                .rounded(at(18.0))
                .bg(rgb(0xE4E6EA))
                .child(div().absolute().left(at(20.0)).top(at(30.0)).w(px(width)).h(px(height)).children(caps)),
        )
        .children(lights)
}
