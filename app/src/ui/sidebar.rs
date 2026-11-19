//! The sidebar: app name, the sections, and the connection state.

use gpui::{div, prelude::*, rgb, Context, FontWeight, Rgba};
use gpui_component::{
    h_flex,
    sidebar::{Sidebar, SidebarFooter, SidebarMenu, SidebarMenuItem},
    v_flex, ActiveTheme as _,
};

use super::app::Configurator;
use super::model::Connection;
use super::page::Page;
use super::widgets::muted;
use crate::tr;

/// The sidebar's width, and with only icons (gpui-component's defaults).
pub const WIDTH: f32 = 255.0;
pub const COMPACT_WIDTH: f32 = 48.0;

/// `compact`: icons only (narrow windows).
pub fn render(app: &Configurator, compact: bool, cx: &mut Context<Configurator>) -> impl IntoElement {
    let theme = cx.theme().clone();
    // Room for the window buttons, which sit in the sidebar when there's no title bar.
    let title_bar = app.backdrop.title_bar_height();
    let view = cx.entity().downgrade();
    // Rows close together, like a native source list.
    let menu = SidebarMenu::new().gap_0p5().children(Page::ALL.map(|page| {
        let view = view.clone();
        SidebarMenuItem::new(page.title()).icon(page.icon()).active(page == app.page).on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| this.open(page, cx));
        })
    }));

    let (dot, state, detail) = connection_text(app.model.connection(), app.model.write_error());
    let status_dot = div().size_2().flex_none().rounded_full().bg(dot);
    Sidebar::new("sections")
        .collapsible(true)
        .collapsed(compact)
        // A plain header: gpui-component's SidebarHeader is a hoverable button, which lit up the
        // title bar strip behind the window buttons.
        .header(if compact {
            div().h(title_bar)
        } else {
            v_flex()
                .px_2()
                .pb_2()
                .pt(title_bar)
                .child(div().font_weight(FontWeight::SEMIBOLD).child(tr!("app.name")))
                .child(muted(&theme, tr!("app.subtitle")))
        })
        .child(menu)
        .footer(SidebarFooter::new().child(if compact {
            h_flex().w_full().justify_center().child(status_dot).into_any_element()
        } else {
            v_flex()
                .gap_1()
                .child(h_flex().gap_2().items_center().child(status_dot).child(state))
                .when_some(detail, |el, detail| el.child(muted(&theme, detail)))
                .into_any_element()
        }))
}

/// Status dot, headline and detail for the connection.
fn connection_text(connection: &Connection, write_error: Option<&str>) -> (Rgba, String, Option<String>) {
    let (green, amber, red) = (rgb(0x4ADE80), rgb(0xFBBF24), rgb(0xF87171));
    match connection {
        Connection::Usb { protocol } => {
            let detail = match write_error {
                Some(error) => tr!("connection.write_failed", error = error),
                None => tr!("connection.usb", version = protocol),
            };
            (green, tr!("connection.connected"), Some(detail))
        }
        Connection::FlashMode => (amber, tr!("connection.flash_mode"), Some(tr!("connection.flash_mode_detail"))),
        Connection::Stock => (amber, tr!("connection.stock"), Some(tr!("connection.stock_detail"))),
        Connection::Unplugged => (red, tr!("connection.not_connected"), Some(tr!("connection.unplugged"))),
        Connection::Missing(reason) => (red, tr!("connection.not_connected"), Some(reason.clone())),
        Connection::NotFound => (red, tr!("connection.not_connected"), None),
        Connection::Bluetooth => (rgb(0x60A5FA), tr!("connection.bluetooth"), Some(tr!("connection.bluetooth_detail"))),
    }
}
