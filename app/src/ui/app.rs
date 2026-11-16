//! The root view: owns the keyboard model and the controls, switches pages, keeps the connection
//! and the previews alive.

use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::controls::Controls;
use super::model::Model;
use super::page::Page;
use super::widgets::muted;
use super::{lighting, overview, settings_page, sidebar};
use crate::settings::Setting;

/// How often the connection is checked.
const WATCH_EVERY: Duration = Duration::from_secs(2);
/// Preview frame time (~30 fps: smooth enough, light on the CPU).
const FRAME: Duration = Duration::from_millis(33);

pub struct Configurator {
    pub(super) page: Page,
    pub(super) model: Model,
    pub(super) controls: Controls,
    started: Instant,
}

impl Configurator {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self { page: Page::Overview, model: Model::default(), controls: Controls::default(), started: Instant::now() };
        this.connect(window, cx);
        Self::watch_connection(window, cx);
        Self::animate(window, cx);
        this
    }

    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.controls = if self.model.connect() { Controls::build(&self.model, window, cx) } else { Controls::default() };
    }

    /// Notice an unplugged keyboard, and connect again when it comes back.
    fn watch_connection(window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(WATCH_EVERY).await;
            let open = this.update_in(cx, |this, window, cx| {
                if this.model.check_lost() {
                    this.controls = Controls::default();
                    cx.notify();
                } else if !this.model.is_connected() {
                    this.connect(window, cx);
                    if this.model.is_connected() {
                        cx.notify();
                    }
                }
            });
            if open.is_err() {
                break; // window closed
            }
        })
        .detach();
    }

    /// Redraw the lighting previews while a page with one is shown.
    fn animate(window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(FRAME).await;
            let open = this.update(cx, |this, cx| {
                if this.page.is_animated() && this.model.is_connected() {
                    cx.notify();
                }
            });
            if open.is_err() {
                break;
            }
        })
        .detach();
    }

    /// Milliseconds since the app started: the previews' clock.
    pub fn clock_ms(&self) -> f32 {
        self.started.elapsed().as_secs_f32() * 1000.0
    }

    pub fn open(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        cx.notify();
    }

    /// Change a setting on the keyboard.
    pub fn write(&mut self, setting: &Setting, value: &[u8], cx: &mut Context<Self>) {
        if self.model.write(setting, value) {
            cx.notify(); // rows that depend on this value may appear or disappear
        }
    }

    /// Pick an option of a Choice setting from outside its dropdown, keeping the dropdown in step.
    pub fn choose(&mut self, setting: &Setting, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.write(setting, &[index as u8], cx);
        self.controls.show_choice(setting, index, window, cx);
    }

    fn render_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        if !self.model.is_connected() {
            return v_flex()
                .gap_3()
                .child(div().child("The keyboard isn't connected over USB."))
                .child(muted(&theme, self.model.status().to_string()))
                .into_any_element();
        }
        match self.page {
            Page::Overview => overview::render(self, cx),
            Page::Lighting => lighting::render(self, cx),
            Page::Keys | Page::Power => settings_page::render(self, self.page.sections(), cx),
            Page::Firmware => muted(&theme, "This page is coming next.").into_any_element(),
        }
    }
}

impl Render for Configurator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let page = self.render_page(cx);
        h_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(sidebar::render(self, cx))
            .child(
                v_flex()
                    .id("page")
                    .flex_1()
                    .h_full()
                    .overflow_y_scroll()
                    .p_8()
                    .gap_2()
                    .child(div().text_3xl().font_weight(FontWeight::SEMIBOLD).child(self.page.title()))
                    .child(div().pb_4().text_color(theme.muted_foreground).child(self.page.subtitle()))
                    .child(div().max_w(px(860.)).child(page)),
            )
    }
}
