//! Overview: the keyboard as it lights up now, and a tile per main setting.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::app::Configurator;
use super::layout::{columns, GAP};
use super::lighting::live_preview;
use super::page::Page;
use super::widgets::{card, muted};
use crate::settings::value;
use crate::tr;

const MIN_TILE: f32 = 190.0;
const TILE_GAP: f32 = 16.0;

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let m = &app.model;
    let caps = if m.byte(value::CAPS_SIDE_LIGHT) != 0 { tr!("overview.caps_key_and_side") } else { tr!("overview.caps_key_only") };
    let tiles = [
        (tr!("overview.key_lighting"), m.choice_label(value::KEY_EFFECT), Page::Lighting),
        (tr!("overview.side_lights"), m.choice_label(value::SIDE_MODE), Page::Lighting),
        (tr!("overview.caps_lock"), caps, Page::Keys),
        (tr!("overview.lights_off"), m.choice_label(value::LIGHTS_OFF_AFTER), Page::Power),
    ];
    let width = app.layout.width;
    let (_, tile_width) = columns(width, MIN_TILE, TILE_GAP);
    let view = cx.entity().downgrade();
    let updates = super::updates::overview_card(app, width, cx);
    v_flex()
        .gap(px(GAP))
        .children(updates)
        .child(live_preview(app, &theme, width, false))
        .child(h_flex().w(px(width)).flex_wrap().gap(px(TILE_GAP)).children(tiles.into_iter().enumerate().map(
            |(i, (title, current, page))| {
                let view = view.clone();
                card(&theme)
                    .id(("tile", i))
                    .w(px(tile_width))
                    .p_4()
                    .cursor_pointer()
                    .hover(|s| s.border_color(theme.ring))
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| this.open(page, cx));
                    })
                    .child(muted(&theme, title.to_uppercase()))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(current))
                    .child(div().text_xs().text_color(theme.link).child(format!("{} →", page.title())))
            },
        )))
        .into_any_element()
}
