//! Power: the battery and connection as the keyboard reports them (firmware v0.8+), then the power
//! settings.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Theme};

use super::app::Configurator;
use super::layout::GAP;
use super::model::{Battery, KeyboardState, Link};
use super::settings_page;
use super::widgets::{card, muted};
use crate::settings::POWER;
use crate::tr;

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let width = app.layout.width;
    v_flex()
        .gap(px(GAP))
        .when_some(app.model.state().copied(), |el, state| el.child(status_card(&theme, &state, width)))
        .child(settings_page::render(app, POWER, width, cx))
        .into_any_element()
}

fn status_card(theme: &Theme, state: &KeyboardState, width: f32) -> impl IntoElement {
    let (icon, battery) = if state.cable {
        ("icons/battery-charging.svg", tr!("power.charging"))
    } else {
        match state.battery {
            Battery::High => ("icons/battery-full.svg", tr!("power.battery_high")),
            Battery::Medium => ("icons/battery-medium.svg", tr!("power.battery_medium")),
            Battery::Low => ("icons/battery-low.svg", tr!("power.battery_low")),
            Battery::Unknown => ("icons/battery.svg", tr!("power.battery_unknown")),
        }
    };
    let connection = if state.bluetooth {
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
    };
    card(theme).w(px(width.min(900.0))).p_5().child(
        h_flex()
            .gap_4()
            .items_center()
            .child(Icon::empty().path(icon).size_8().text_color(if state.battery_low { theme.danger } else { theme.foreground }))
            .child(
                v_flex()
                    .gap_0p5()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(battery))
                    .child(muted(theme, connection)),
            ),
    )
}
