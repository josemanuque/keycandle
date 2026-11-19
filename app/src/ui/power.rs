//! Power: the battery and connection as the keyboard reports them (firmware v0.8+), with the battery
//! bar switch, then the settings for running on battery.

use gpui::{div, prelude::*, px, rgb, AnyElement, Context, FontWeight, Hsla};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Theme};

use super::app::Configurator;
use super::controls;
use super::layout::GAP;
use super::model::{Battery, KeyboardState, Link};
use super::settings_page;
use super::widgets::{card, muted};
use crate::settings::{BATTERY_BAR, POWER};
use crate::tr;

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let width = app.layout.width;
    v_flex()
        .gap(px(GAP))
        .child(battery_card(app, width, cx))
        .child(settings_page::render(app, POWER, width, cx))
        .into_any_element()
}

fn battery_card(app: &Configurator, width: f32, cx: &mut Context<Configurator>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let state = app.model.state().copied();
    let (level, title) = match state {
        Some(s) => match s.battery {
            Battery::High => (3, tr!("power.battery_high")),
            Battery::Medium => (2, tr!("power.battery_medium")),
            Battery::Low => (1, tr!("power.battery_low")),
            Battery::Unknown => (0, tr!("power.battery_unknown")),
        },
        None => (0, tr!("power.battery_unknown")),
    };
    let detail = match state {
        Some(s) => format!("{} · {}", connection(&s), tr!("power.battery_source")),
        None => tr!("power.battery_needs_usb"),
    };
    let low = state.is_some_and(|s| s.battery_low);
    let switch = controls::switch(app, &BATTERY_BAR, cx);
    card(&theme).w(px(width)).child(
        h_flex()
            .flex_wrap()
            .items_center()
            .gap_6()
            .child(
                h_flex()
                    .flex_1()
                    .min_w(px(280.))
                    .gap_4()
                    .items_center()
                    .child(battery_glyph(&theme, level, low))
                    .child(
                        v_flex()
                            .gap_0p5()
                            .min_w_0()
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(title))
                            .child(muted(&theme, detail)),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        v_flex()
                            .items_end()
                            .gap_0p5()
                            .child(div().child(BATTERY_BAR.label()))
                            .when_some(BATTERY_BAR.help(), |el, help| el.child(muted(&theme, help))),
                    )
                    .child(switch),
            ),
    )
}

/// Wired, or which Bluetooth device and how its link is doing; and whether it's charging.
fn connection(state: &KeyboardState) -> String {
    if state.cable {
        return tr!("power.charging");
    }
    if state.bluetooth {
        let slot = (state.slot + 1).to_string();
        let link = match state.link {
            Link::Connected => tr!("power.link_connected"),
            Link::Connecting => tr!("power.link_connecting"),
            Link::Pairing => tr!("power.link_pairing"),
            Link::Idle => tr!("power.link_idle"),
        };
        tr!("power.mode_bluetooth", slot = slot, link = link)
    } else {
        tr!("power.mode_wired")
    }
}

/// A battery drawn as three cells, lit up to `level` (the module only reports high, medium or low).
fn battery_glyph(theme: &Theme, level: usize, low: bool) -> impl IntoElement {
    let lit: Hsla = match level {
        _ if low => rgb(0xF87171).into(),
        1 => rgb(0xFBBF24).into(),
        _ => rgb(0x4ADE80).into(),
    };
    h_flex()
        .flex_none()
        .items_center()
        .child(
            h_flex()
                .gap(px(3.))
                .p(px(4.))
                .rounded(px(7.))
                .border_2()
                .border_color(theme.muted_foreground.opacity(0.5))
                .children((0..3).map(|i| {
                    div().w(px(14.)).h(px(24.)).rounded(px(3.)).bg(if i < level { lit } else { theme.muted_foreground.opacity(0.15) })
                })),
        )
        // The terminal nub.
        .child(div().w(px(3.)).h(px(10.)).rounded_r(px(2.)).bg(theme.muted_foreground.opacity(0.5)))
}
