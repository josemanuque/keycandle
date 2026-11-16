//! Overview: the keyboard as it lights up now, and a tile per main setting.

use gpui::{div, prelude::*, AnyElement, Context, FontWeight};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::app::Configurator;
use super::lighting::live_preview;
use super::page::Page;
use super::widgets::{card, muted};
use crate::settings::value;

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let m = &app.model;
    let caps = if m.byte(value::CAPS_SIDE_LIGHT) != 0 { "Key + side light" } else { "On the key" };
    let tiles = [
        ("Key lighting", m.choice_label(value::KEY_EFFECT), Page::Lighting),
        ("Side lights", m.choice_label(value::SIDE_MODE), Page::Lighting),
        ("Caps Lock indicator", caps, Page::Keys),
        ("Lighting off on battery", m.choice_label(value::LIGHTS_OFF_AFTER), Page::Power),
    ];
    let view = cx.entity().downgrade();
    v_flex()
        .gap_5()
        .child(live_preview(app, &theme))
        .child(h_flex().gap_4().children(tiles.into_iter().enumerate().map(|(i, (title, current, page))| {
            let view = view.clone();
            card(&theme)
                .id(("tile", i))
                .flex_1()
                .p_4()
                .cursor_pointer()
                .hover(|s| s.border_color(theme.ring))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.open(page, cx));
                })
                .child(muted(&theme, title.to_uppercase()))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(current))
                .child(div().text_xs().text_color(theme.link).child(format!("{} →", page.title())))
        })))
        .into_any_element()
}
