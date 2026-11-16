//! The app window: sidebar with the five sections, settings pages, connection status.
//!
//! Every control is built from `settings.rs`, filled with the keyboard's current value, and writes
//! changes straight back over USB. The firmware stores them in flash once they settle.

use std::collections::HashMap;

use gpui::{
    div, hsla, prelude::*, px, size, App, Bounds, Context, Entity, Hsla, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
};
use gpui_component::{
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    h_flex,
    select::{Select, SelectEvent, SelectState},
    sidebar::{Sidebar, SidebarFooter, SidebarHeader, SidebarMenu, SidebarMenuItem},
    slider::{Slider, SliderEvent, SliderState},
    switch::Switch,
    v_flex, ActiveTheme as _, Icon, IndexPath, Root, Theme, ThemeMode,
};

use crate::device::Keyboard;
use crate::settings::{Kind, Section, Setting, KEYS, LIGHTING, POWER};

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
            Page::Lighting => "Changes show on the keyboard instantly and are saved once you stop adjusting.",
            Page::Keys => "How locks show and what Fn highlights. Windows/Mac is remembered per device (Fn + A / Fn + S).",
            Page::Power => "Battery saving applies on Bluetooth without the cable. Plugged in, the keyboard never sleeps.",
            Page::Firmware => "Update the keyboard's firmware or go back to the original.",
        }
    }

    /// Lucide icon file (all of Lucide is embedded via gpui-kit-assets' AllAssets).
    fn icon(self) -> Icon {
        let file = match self {
            Page::Overview => "icons/layout-dashboard.svg",
            Page::Lighting => "icons/sun.svg",
            Page::Keys => "icons/keyboard.svg",
            Page::Power => "icons/battery-full.svg",
            Page::Firmware => "icons/download.svg",
        };
        Icon::empty().path(file)
    }

    fn sections(self) -> &'static [Section] {
        match self {
            Page::Lighting => LIGHTING,
            Page::Keys => KEYS,
            Page::Power => POWER,
            _ => &[],
        }
    }
}

/// The interactive part of one setting's row.
enum Control {
    Toggle,
    Range(Entity<SliderState>),
    Choice(Entity<SelectState<Vec<&'static str>>>),
    Colour(Entity<ColorPickerState>),
}

type Key = (u8, u8); // (channel, value id)

struct Configurator {
    page: Page,
    keyboard: Option<Keyboard>,
    status: SharedString,
    values: HashMap<Key, Vec<u8>>,
    controls: HashMap<Key, Control>,
}

/* Colours: the keyboard stores hue + saturation (HSV, brightness separate); the picker uses HSL.
 * Full-brightness HSV(h, s, 1) is HSL(h, 1, 1 - s/2). */
fn to_hsla(v: &[u8]) -> Hsla {
    let (h, s) = (v[0] as f32 / 255.0, v[1] as f32 / 255.0);
    hsla(h, if s > 0.0 { 1.0 } else { 0.0 }, 1.0 - s / 2.0, 1.0)
}

fn from_hsla(c: Hsla) -> [u8; 2] {
    let value = c.l + c.s * c.l.min(1.0 - c.l);
    let sat = if value > 0.0 { 2.0 * (1.0 - c.l / value) } else { 0.0 };
    [(c.h * 255.0).round().clamp(0.0, 255.0) as u8, (sat * 255.0).round().clamp(0.0, 255.0) as u8]
}

impl Configurator {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            page: Page::Overview,
            keyboard: None,
            status: SharedString::default(),
            values: HashMap::new(),
            controls: HashMap::new(),
        };
        this.connect(window, cx);
        this
    }

    fn all_settings() -> impl Iterator<Item = &'static Setting> {
        [LIGHTING, KEYS, POWER].into_iter().flatten().flat_map(|s| s.settings.iter())
    }

    /// Open the keyboard, read every setting and build the controls from the values.
    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.controls.clear();
        self.values.clear();
        let kb = match Keyboard::open() {
            Ok(kb) => kb,
            Err(e) => {
                self.status = format!("{e:#}").into();
                self.keyboard = None;
                return;
            }
        };
        self.status = format!("USB · VIA protocol {}", kb.protocol_version).into();
        for setting in Self::all_settings() {
            if let Ok(v) = kb.get(setting.channel, setting.id, setting.len()) {
                self.values.insert((setting.channel, setting.id), v);
            }
        }
        self.keyboard = Some(kb);
        for setting in Self::all_settings() {
            let key = (setting.channel, setting.id);
            let Some(value) = self.values.get(&key).cloned() else { continue };
            let control = self.build_control(setting, &value, window, cx);
            self.controls.insert(key, control);
        }
    }

    fn build_control(&mut self, setting: &'static Setting, value: &[u8], window: &mut Window, cx: &mut Context<Self>) -> Control {
        match setting.kind {
            Kind::Toggle => Control::Toggle,
            Kind::Range { min, max } => {
                let state = cx.new(|_| {
                    SliderState::new().min(min as f32).max(max as f32).step(1.0).default_value(value[0] as f32)
                });
                cx.subscribe(&state, move |this, _, event: &SliderEvent, cx| {
                    let SliderEvent::Change(v) = event else { return }; // live while dragging
                    this.write(setting, &[v.start().round() as u8], cx);
                })
                .detach();
                Control::Range(state)
            }
            Kind::Choice(options) => {
                let selected = (value[0] as usize).min(options.len() - 1);
                let state = cx.new(|cx| SelectState::new(options.to_vec(), Some(IndexPath::new(selected)), window, cx));
                cx.subscribe(&state, move |this, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                    if let SelectEvent::Confirm(Some(choice)) = event {
                        if let Some(i) = options.iter().position(|o| o == choice) {
                            this.write(setting, &[i as u8], cx);
                        }
                    }
                })
                .detach();
                Control::Choice(state)
            }
            Kind::Colour => {
                let state = cx.new(|cx| ColorPickerState::new(window, cx).default_value(to_hsla(value)));
                cx.subscribe(&state, move |this, _, event: &ColorPickerEvent, cx| {
                    let ColorPickerEvent::Change(Some(colour)) = event else { return };
                    this.write(setting, &from_hsla(*colour), cx);
                })
                .detach();
                Control::Colour(state)
            }
        }
    }

    /// Send a new value to the keyboard (and ask it to save once things settle).
    fn write(&mut self, setting: &Setting, value: &[u8], cx: &mut Context<Self>) {
        let key = (setting.channel, setting.id);
        if self.values.get(&key).map(|v| v.as_slice()) == Some(value) {
            return;
        }
        if let Some(kb) = &self.keyboard {
            if let Err(e) = kb.set(setting.channel, setting.id, value).and_then(|_| kb.save(setting.channel)) {
                self.status = format!("Write failed: {e:#}").into();
            }
        }
        self.values.insert(key, value.to_vec());
        cx.notify(); // rows that depend on this value may appear or disappear
    }

    fn is_shown(&self, setting: &Setting) -> bool {
        match setting.shown_when {
            None => true,
            Some((id, allowed)) => self
                .values
                .get(&(setting.channel, id))
                .is_some_and(|v| allowed.contains(&v[0])),
        }
    }

    fn render_row(&self, setting: &'static Setting, cx: &mut Context<Self>) -> impl IntoElement {
        let key = (setting.channel, setting.id);
        let theme = cx.theme();
        let label = v_flex()
            .gap_0p5()
            .flex_1()
            .child(div().child(setting.label))
            .when(!setting.help.is_empty(), |el| el.child(div().text_xs().text_color(theme.muted_foreground).child(setting.help)));
        let control: gpui::AnyElement = match self.controls.get(&key) {
            Some(Control::Toggle) => {
                let on = self.values.get(&key).is_some_and(|v| v[0] != 0);
                let view = cx.entity().downgrade();
                Switch::new(SharedString::from(format!("toggle-{}-{}", key.0, key.1)))
                    .checked(on)
                    .on_click(move |checked, _, cx| {
                        let checked = *checked;
                        let _ = view.update(cx, |this, cx| this.write(setting, &[checked as u8], cx));
                    })
                    .into_any_element()
            }
            Some(Control::Range(state)) => {
                let shown = self.values.get(&key).map(|v| v[0]).unwrap_or(0);
                h_flex()
                    .gap_3()
                    .w(px(300.))
                    .child(div().flex_1().child(Slider::new(state)))
                    .child(div().w(px(32.)).text_right().text_color(theme.muted_foreground).child(shown.to_string()))
                    .into_any_element()
            }
            Some(Control::Choice(state)) => div().w(px(300.)).child(Select::new(state)).into_any_element(),
            Some(Control::Colour(state)) => ColorPicker::new(state).into_any_element(),
            None => div().text_color(theme.muted_foreground).child("not available").into_any_element(),
        };
        h_flex().gap_6().py_3().items_center().child(label).child(control)
    }

    fn render_page(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        if self.keyboard.is_none() {
            return v_flex()
                .gap_3()
                .child(div().child("The keyboard isn't connected over USB."))
                .child(div().text_color(theme.muted_foreground).child(self.status.clone()))
                .into_any_element();
        }
        let sections = self.page.sections();
        if sections.is_empty() {
            return div().text_color(theme.muted_foreground).child("This page is coming next.").into_any_element();
        }
        v_flex()
            .gap_5()
            .children(sections.iter().map(|section| {
                v_flex()
                    .p_5()
                    .gap_1()
                    .rounded_lg()
                    .bg(theme.secondary)
                    .border_1()
                    .border_color(theme.border)
                    .child(div().pb_2().font_weight(gpui::FontWeight::SEMIBOLD).child(section.title))
                    .children(section.settings.iter().filter(|s| self.is_shown(s)).map(|s| self.render_row(s, cx)))
            }))
            .into_any_element()
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
        let theme = cx.theme().clone();
        let dot = if connected { gpui::rgb(0x4ADE80) } else { gpui::rgb(0xF87171) };
        let page = self.render_page(cx);

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
                    .id("page")
                    .flex_1()
                    .h_full()
                    .overflow_y_scroll()
                    .p_8()
                    .gap_2()
                    .child(div().text_3xl().font_weight(gpui::FontWeight::SEMIBOLD).child(self.page.title()))
                    .child(div().pb_4().text_color(theme.muted_foreground).child(self.page.subtitle()))
                    .child(div().max_w(px(860.)).child(page)),
            )
    }
}

pub fn run() {
    let app = gpui::Application::with_platform(gpui_platform::current_platform(false)).with_assets(gpui_kit_assets::AllAssets);
    app.run(|cx: &mut App| {
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let bounds = Bounds::centered(None, size(px(1180.), px(800.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("RK71 Configurator".into()), ..Default::default() }),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| Configurator::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("could not open the window");
        cx.activate(true);
    });
}
