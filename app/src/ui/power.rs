//! Power: the battery and connection as the keyboard reports them (firmware v0.8+), with the battery
//! bar switch, then the settings for running on battery.

use gpui::{div, prelude::*, px, rgb, AnyElement, Context, FontWeight, Hsla};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _, Theme};

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
    // Over USB the keyboard says everything; over Bluetooth (firmware v0.9+) its battery and device.
    let state = app.model.state().copied().or(app.model.bluetooth_state());
    let checking = app.model.battery_checking();
    let battery = state.map_or(Battery::Unknown, |s| s.battery);
    let cable = state.is_some_and(|s| s.cable);
    let level = match battery {
        Battery::High => 3,
        Battery::Medium => 2,
        Battery::Low => 1,
        Battery::Unknown => 0,
    };
    let level_word = match battery {
        Battery::High => tr!("power.level_high"),
        Battery::Medium => tr!("power.level_medium"),
        Battery::Low => tr!("power.level_low"),
        Battery::Unknown => tr!("power.level_unknown"),
    };
    let (title, detail) = match state {
        _ if checking => (tr!("power.checking"), tr!("power.checking_detail")),
        // On the cable: charging, or done (the module's "high" is as full as it says).
        Some(_) if cable && level == 3 => (tr!("power.charged"), tr!("power.on_cable", level = level_word)),
        Some(_) if cable => (tr!("power.charging"), tr!("power.on_cable", level = level_word)),
        Some(s) => (
            match battery {
                Battery::High => tr!("power.battery_high"),
                Battery::Medium => tr!("power.battery_medium"),
                Battery::Low => tr!("power.battery_low"),
                Battery::Unknown => tr!("power.battery_unknown"),
            },
            format!("{} · {}", connection(&s), tr!("power.battery_source")),
        ),
        None => (tr!("power.battery_unknown"), tr!("power.battery_needs_usb")),
    };
    let look = if checking {
        Look::Checking(app.clock_ms())
    } else {
        Look::Level { level, low: state.is_some_and(|s| s.battery_low), cable }
    };
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
                    .child(battery_glyph(&theme, look))
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

/// Which Bluetooth device and how its link is doing, or wired.
fn connection(state: &KeyboardState) -> String {
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

/// What the battery glyph shows.
enum Look {
    /// Asking the keyboard: the cells fill one by one, over and over (the clock in ms).
    Checking(f32),
    /// Lit up to `level` of 3 (the module only reports high, medium or low); on the cable, with the
    /// USB-C plug and a charging (or charged) badge.
    Level { level: usize, low: bool, cable: bool },
}

const GREEN: u32 = 0x4ADE80;
const AMBER: u32 = 0xFBBF24;
const RED: u32 = 0xF87171;

/// A battery drawn as three cells.
fn battery_glyph(theme: &Theme, look: Look) -> impl IntoElement {
    let dim = theme.muted_foreground.opacity(0.15);
    let outline = theme.muted_foreground.opacity(0.5);
    let (lit, colour, cable, badge): (usize, Hsla, bool, Option<(&str, u32)>) = match look {
        Look::Checking(ms) => ((ms / 320.0) as usize % 4, theme.muted_foreground.opacity(0.55), false, None),
        Look::Level { level, low, cable } => {
            let colour = match level {
                _ if low => RED,
                1 => AMBER,
                _ => GREEN,
            };
            let badge = cable.then(|| if level == 3 { ("icons/check.svg", GREEN) } else { ("icons/zap.svg", AMBER) });
            (level, rgb(colour).into(), cable, badge)
        }
    };
    let body = div()
        .relative()
        .child(
            h_flex()
                .gap(px(3.))
                .p(px(4.))
                .rounded(px(7.))
                .border_2()
                .border_color(outline)
                .children((0..3).map(|i| div().w(px(14.)).h(px(24.)).rounded(px(3.)).bg(if i < lit { colour } else { dim }))),
        )
        // Charging (a bolt) or charged (a tick), on the battery's corner.
        .when_some(badge, |el, (icon, tint)| {
            el.child(
                div()
                    .absolute()
                    .right(px(-9.))
                    .bottom(px(-9.))
                    .size(px(22.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.secondary) // the card's own colour: a ring cut out of the battery
                    .child(
                        div()
                            .size(px(18.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(tint))
                            .child(Icon::empty().path(icon).with_size(px(12.)).text_color(rgb(0x111111))),
                    ),
            )
        });
    h_flex()
        .flex_none()
        .items_center()
        .when(cable, |el| el.child(usb_c_plug(theme)))
        .child(body)
        // The terminal nub.
        .child(div().w(px(3.)).h(px(10.)).rounded_r(px(2.)).bg(outline))
}

/// A USB-C cable plugged into the battery's left side: the cable, the plug's grip, its slim tip.
fn usb_c_plug(theme: &Theme) -> impl IntoElement {
    let colour = theme.muted_foreground.opacity(0.7);
    h_flex()
        .flex_none()
        .items_center()
        .mr(px(3.))
        .child(div().w(px(16.)).h(px(3.)).rounded_l(px(2.)).bg(colour))
        .child(div().w(px(13.)).h(px(16.)).rounded(px(4.)).bg(colour))
        .child(div().w(px(6.)).h(px(9.)).rounded_r(px(3.)).bg(theme.muted_foreground.opacity(0.4)))
}
