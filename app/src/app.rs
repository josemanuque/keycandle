//! The app window: sidebar with the five sections, page area, connection status.

use gpui::{div, prelude::*, px, size, App, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions};
use gpui_component::{
    h_flex,
    sidebar::{Sidebar, SidebarFooter, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex, ActiveTheme as _, IconName, Root, Theme, ThemeMode,
};

use crate::device::Keyboard;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Overview,
    Lighting,
    Keys,
    Power,
    Firmware,
}

impl Page {
    const ALL: [Page; 5] = [Page::Overview, Page::Lighting, Page::Keys, Page::Power, Page::Firmware];

    fn title(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Lighting => "Lighting",
            Page::Keys => "Keys & indicators",
            Page::Power => "Power",
            Page::Firmware => "Firmware",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Page::Overview => "Everything on the keyboard, in one place. Changes apply live and are saved on the keyboard.",
            Page::Lighting => "Key lighting and side lights.",
            Page::Keys => "How locks show, what Fn highlights, and which layout each device gets.",
            Page::Power => "Battery saving on Bluetooth without the cable.",
            Page::Firmware => "Update the keyboard's firmware or go back to the original.",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Page::Overview => IconName::LayoutDashboard,
            Page::Lighting => IconName::Sun,
            Page::Keys => IconName::CaseSensitive,
            Page::Power => IconName::BatteryFull,
            Page::Firmware => IconName::Cpu,
        }
    }
}

struct Configurator {
    page: Page,
    keyboard: Option<Keyboard>,
    status: SharedString,
}

impl Configurator {
    fn new() -> Self {
        let mut this = Self { page: Page::Overview, keyboard: None, status: SharedString::default() };
        this.connect();
        this
    }

    fn connect(&mut self) {
        match Keyboard::open() {
            Ok(kb) => {
                self.status = format!("Connected · USB · VIA protocol {}", kb.protocol_version).into();
                self.keyboard = Some(kb);
            }
            Err(e) => {
                self.status = format!("{e:#}").into();
                self.keyboard = None;
            }
        }
    }
}

impl Render for Configurator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let menu = SidebarMenu::new().children(Page::ALL.map(|page| {
            let view = view.clone();
            SidebarMenuItem::new(page.title()).icon(page.icon()).active(page == self.page).on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.page = page;
                    cx.notify();
                });
            })
        }));

        let connected = self.keyboard.is_some();
        let theme = cx.theme();
        let dot = if connected { gpui::rgb(0x4ADE80) } else { gpui::rgb(0xF87171) };

        h_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                Sidebar::new("sections")
                    .header(SidebarHeader::new().child(
                        v_flex()
                            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("RK71 Configurator"))
                            .child(div().text_xs().text_color(theme.muted_foreground).child("QMK firmware")),
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
                            .child(div().text_xs().text_color(theme.muted_foreground).child(self.status.clone())),
                    )),
            )
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .p_8()
                    .gap_2()
                    .child(div().text_3xl().font_weight(gpui::FontWeight::SEMIBOLD).child(self.page.title()))
                    .child(div().text_color(theme.muted_foreground).child(self.page.subtitle())),
            )
    }
}

pub fn run() {
    let app = gpui::Application::with_platform(gpui_platform::current_platform(false));
    app.run(|cx: &mut App| {
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
        let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() };
        cx.open_window(options, |window, cx| {
            let view = cx.new(|_| Configurator::new());
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("could not open the window");
        cx.activate(true);
    });
}
