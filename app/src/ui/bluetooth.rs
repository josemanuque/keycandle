//! The banner over the settings pages while the keyboard is only on Bluetooth: what that means, and
//! whether the changes reached it.

use gpui::{div, prelude::*, rgb, AnyElement, Context, FontWeight};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::app::Configurator;
use super::widgets::muted;
use crate::btlink::Status;
use crate::tr;

pub fn banner(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let (dot, status) = match app.model.bluetooth_status() {
        Some(Status::Sending(n)) => (rgb(0x60A5FA), tr!("bluetooth.sending", count = n)),
        Some(Status::Failed(error)) => (rgb(0xF87171), tr!("bluetooth.failed", error = error)),
        _ => (rgb(0x4ADE80), tr!("bluetooth.saved")),
    };
    v_flex()
        .gap_1()
        .px_4()
        .py_3()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .child(
            h_flex()
                .gap_3()
                .flex_wrap()
                .items_center()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(tr!("bluetooth.title")))
                .child(h_flex().gap_2().items_center().child(div().size_2().rounded_full().bg(dot)).child(div().text_sm().child(status))),
        )
        .child(muted(&theme, tr!("bluetooth.detail")))
        .into_any_element()
}
