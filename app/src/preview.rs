//! Drawn RK71: the real key geometry (from tools/gen_rk71.py) with the current key and side-light
//! colours. `None` = unlit.

use gpui::{div, prelude::*, px, rgb, Hsla, IntoElement};

const U: f32 = 40.0; // one key unit, px
const GAP: f32 = 4.0;

/// (legend, x, width) per row, in key units.
const ROWS: [&[(&str, f32, f32)]; 5] = [
    &[("Esc", 0.0, 1.0), ("1", 1.0, 1.0), ("2", 2.0, 1.0), ("3", 3.0, 1.0), ("4", 4.0, 1.0), ("5", 5.0, 1.0), ("6", 6.0, 1.0), ("7", 7.0, 1.0), ("8", 8.0, 1.0), ("9", 9.0, 1.0), ("0", 10.0, 1.0), ("-", 11.0, 1.0), ("=", 12.0, 1.0), ("Back", 13.0, 2.0), ("Ins", 15.0, 1.0), ("Home", 16.0, 1.0)],
    &[("Tab", 0.0, 1.5), ("Q", 1.5, 1.0), ("W", 2.5, 1.0), ("E", 3.5, 1.0), ("R", 4.5, 1.0), ("T", 5.5, 1.0), ("Y", 6.5, 1.0), ("U", 7.5, 1.0), ("I", 8.5, 1.0), ("O", 9.5, 1.0), ("P", 10.5, 1.0), ("[", 11.5, 1.0), ("]", 12.5, 1.0), ("\\", 13.5, 1.5), ("Del", 15.0, 1.0), ("End", 16.0, 1.0)],
    &[("Caps", 0.0, 1.75), ("A", 1.75, 1.0), ("S", 2.75, 1.0), ("D", 3.75, 1.0), ("F", 4.75, 1.0), ("G", 5.75, 1.0), ("H", 6.75, 1.0), ("J", 7.75, 1.0), ("K", 8.75, 1.0), ("L", 9.75, 1.0), (";", 10.75, 1.0), ("'", 11.75, 1.0), ("Enter", 12.75, 2.25), ("Pause", 15.0, 1.0), ("PgUp", 16.0, 1.0)],
    &[("Shift", 0.0, 2.25), ("Z", 2.25, 1.0), ("X", 3.25, 1.0), ("C", 4.25, 1.0), ("V", 5.25, 1.0), ("B", 6.25, 1.0), ("N", 7.25, 1.0), ("M", 8.25, 1.0), (",", 9.25, 1.0), (".", 10.25, 1.0), ("/", 11.25, 1.0), ("Shift", 12.25, 2.75), ("↑", 15.0, 1.0), ("PgDn", 16.0, 1.0)],
    &[("Ctrl", 0.0, 1.25), ("Win", 1.25, 1.25), ("Alt", 2.5, 1.25), ("", 3.75, 6.25), ("Alt", 10.0, 1.25), ("Fn", 11.25, 1.25), ("Ctrl", 12.5, 1.25), ("←", 14.0, 1.0), ("↓", 15.0, 1.0), ("→", 16.0, 1.0)],
];

pub fn keyboard(key: Option<Hsla>, side: Option<Hsla>) -> impl IntoElement {
    let unlit: Hsla = rgb(0xC9CDD4).into();
    let glow = key.unwrap_or(unlit);
    let legend = key.unwrap_or(rgb(0x9AA0AA).into());

    let keys = ROWS.iter().enumerate().flat_map(move |(r, row)| {
        row.iter().map(move |&(label, x, w)| {
            div()
                .absolute()
                .left(px(x * U))
                .top(px(r as f32 * U))
                .w(px(w * U - GAP))
                .h(px(U - GAP))
                .rounded(px(7.))
                .bg(glow)
                .child(
                    div()
                        .absolute()
                        .top(px(2.))
                        .left(px(2.))
                        .right(px(2.))
                        .bottom(px(5.))
                        .rounded(px(5.))
                        .bg(rgb(0xF4F5F7))
                        .px(px(6.))
                        .py(px(3.))
                        .text_size(px(if label.len() > 3 { 9. } else { 11. }))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(legend)
                        .child(label),
                )
        })
    });

    let side_colour = side.unwrap_or(rgb(0x3A3F48).into());
    let lights = |left: f32| {
        (0..6).map(move |i| {
            div().absolute().left(px(left)).top(px(66. + i as f32 * 30.)).w(px(5.)).h(px(18.)).rounded(px(3.)).bg(side_colour)
        })
    };

    div()
        .relative()
        .w(px(760.))
        .h(px(300.))
        .child(
            div()
                .absolute()
                .top(px(20.))
                .left(px(20.))
                .right(px(20.))
                .bottom(px(20.))
                .rounded(px(18.))
                .bg(rgb(0xE4E6EA))
                .child(div().absolute().left(px(20.)).top(px(30.)).w(px(680.)).h(px(200.)).children(keys)),
        )
        .children(lights(11.))
        .children(lights(744.))
}
