//! The sidebar: app name, the sections, and the connection state.

use gpui::{div, prelude::*, rgb, Context, FontWeight};
use gpui_component::{
    h_flex,
    sidebar::{Sidebar, SidebarFooter, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex, ActiveTheme as _,
};

use super::app::Configurator;
use super::page::Page;
use super::widgets::muted;

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let view = cx.entity().downgrade();
    let menu = SidebarMenu::new().children(Page::ALL.map(|page| {
        let view = view.clone();
        SidebarMenuItem::new(page.title()).icon(page.icon()).active(page == app.page).on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| this.open(page, cx));
        })
    }));

    let connected = app.model.is_connected();
    let dot = if connected { rgb(0x4ADE80) } else { rgb(0xF87171) };
    Sidebar::new("sections")
        .header(SidebarHeader::new().child(
            v_flex()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("RK71 Configurator"))
                .child(muted(&theme, "QMK firmware")),
        ))
        .child(menu)
        .footer(SidebarFooter::new().child(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().size_2().rounded_full().bg(dot))
                        .child(if connected { "Connected" } else { "Not connected" }),
                )
                .child(muted(&theme, app.model.status().to_string())),
        ))
}
