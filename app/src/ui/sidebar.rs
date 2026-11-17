//! The sidebar: app name, the sections, the connection state and the language.

use gpui::{div, prelude::*, px, rgb, Context, FontWeight, Rgba};
use gpui_component::{
    h_flex,
    sidebar::{Sidebar, SidebarFooter, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex, ActiveTheme as _, Theme,
};

use super::app::Configurator;
use super::model::Connection;
use super::page::Page;
use super::widgets::{muted, segment, segmented};
use crate::{i18n, tr};

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let view = cx.entity().downgrade();
    let menu = SidebarMenu::new().children(Page::ALL.map(|page| {
        let view = view.clone();
        SidebarMenuItem::new(page.title()).icon(page.icon()).active(page == app.page).on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| this.open(page, cx));
        })
    }));

    let (dot, state, detail) = connection_text(app.model.connection(), app.model.write_error());
    Sidebar::new("sections")
        .header(SidebarHeader::new().child(
            v_flex()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(tr!("app.name")))
                .child(muted(&theme, tr!("app.subtitle"))),
        ))
        .child(menu)
        .footer(SidebarFooter::new().child(
            v_flex()
                .w_full()
                .gap_3()
                .child(
                    v_flex()
                        .gap_1()
                        .child(h_flex().gap_2().items_center().child(div().size_2().rounded_full().bg(dot)).child(state))
                        .when_some(detail, |el, detail| el.child(muted(&theme, detail))),
                )
                .child(language_picker(&theme, cx)),
        ))
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
    }
}

/// EN / ES / PT: switches every text at once and is remembered.
fn language_picker(theme: &Theme, cx: &mut Context<Configurator>) -> impl IntoElement {
    let current = i18n::current();
    let view = cx.entity().downgrade();
    segmented(theme).children(i18n::LANGUAGES.iter().enumerate().map(|(i, &(code, _name))| {
        let view = view.clone();
        let short = code[..2].to_uppercase();
        segment(theme, ("lang", i), code == current)
            .min_w(px(0.))
            .px_1()
            .py_1()
            .text_xs()
            .on_click(move |_, window, cx| {
                let _ = view.update(cx, |this, cx| this.set_language(code, window, cx));
            })
            .child(short)
    }))
}
