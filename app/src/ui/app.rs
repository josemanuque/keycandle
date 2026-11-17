//! The root view: owns the keyboard model, the controls and the updater; switches pages; keeps
//! the connection and the previews alive.

use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::controls::Controls;
use super::layout::Layout;
use super::lighting::EffectGroup;
use super::model::{Model, Values};
use super::page::Page;
use super::widgets::muted;
use super::{firmware, lighting, overview, settings_page, sidebar};
use crate::settings::{value, Setting};
use crate::updater::{Releases, Source, Updater};

/// How often the connection is checked.
const WATCH_EVERY: Duration = Duration::from_secs(2);
/// Preview frame time (~30 fps: smooth enough, light on the CPU).
const FRAME: Duration = Duration::from_millis(33);

pub struct Configurator {
    pub(super) page: Page,
    pub(super) model: Model,
    pub(super) controls: Controls,
    pub(super) layout: Layout,
    pub(super) effect_group: EffectGroup,
    pub(super) updater: Updater,
    /// Chosen in the Firmware page, waiting for "Install".
    pub(super) pending_install: Option<Source>,
    pub(super) show_all_releases: bool,
    /// Settings to put back once the keyboard returns from a firmware update.
    restore_after_update: Option<Values>,
    started: Instant,
}

impl Configurator {
    pub fn new(page: Page, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            page,
            model: Model::default(),
            controls: Controls::default(),
            layout: Layout::default(),
            effect_group: EffectGroup::default(),
            updater: Updater::default(),
            pending_install: None,
            show_all_releases: false,
            restore_after_update: None,
            started: Instant::now(),
        };
        this.connect(window, cx);
        this.open(page, cx);
        Self::watch_connection(window, cx);
        Self::animate(window, cx);
        this
    }

    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.model.connect() {
            self.controls = Controls::default();
            return;
        }
        if let Some(saved) = self.restore_after_update.take() {
            self.model.restore(&saved);
        }
        self.effect_group = EffectGroup::of(self.model.byte(value::KEY_EFFECT));
        self.controls = Controls::build(&self.model, window, cx);
    }

    /// Notice an unplugged keyboard (or flash mode), and connect again when it comes back.
    fn watch_connection(window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(WATCH_EVERY).await;
            let open = this.update_in(cx, |this, window, cx| {
                if this.updater.step().is_busy() {
                    return; // the updater owns the USB device until it is done
                }
                if this.model.check_lost() {
                    this.controls = Controls::default();
                } else if !this.model.is_connected() {
                    this.connect(window, cx);
                }
                cx.notify();
            });
            if open.is_err() {
                break; // window closed
            }
        })
        .detach();
    }

    /// Redraw while something moves: a lighting preview, or an update in progress.
    fn animate(window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(FRAME).await;
            let open = this.update(cx, |this, cx| {
                let preview = this.page.is_animated() && this.model.is_connected();
                let working = this.updater.step().is_busy() || matches!(this.updater.releases(), Releases::Checking);
                if preview || (this.page == Page::Firmware && working) {
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
        if page == Page::Firmware && matches!(self.updater.releases(), Releases::NotChecked) {
            self.updater.check();
        }
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

    /// Ask before installing firmware.
    pub fn request_install(&mut self, source: Source, cx: &mut Context<Self>) {
        self.pending_install = Some(source);
        cx.notify();
    }

    /// Install the firmware chosen in `request_install`.
    pub fn confirm_install(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.pending_install.take() else { return };
        let to_stock = matches!(&source, Source::Release(r) if r.stock);
        if self.model.is_connected() && !to_stock {
            self.restore_after_update = Some(self.model.snapshot());
        }
        let keyboard = self.model.take_keyboard();
        self.controls = Controls::default();
        self.updater.install(source, keyboard);
        cx.notify();
    }

    fn render_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        if self.page == Page::Firmware {
            return firmware::render(self, cx); // works without a connection too
        }
        if !self.model.is_connected() {
            let text = if self.model.in_flash_mode() {
                "The keyboard is in flash mode. Open Firmware to install an update or leave flash mode."
            } else if self.model.on_stock_firmware() {
                "The keyboard has its original Royal Kludge firmware, which this app can't configure. Open Firmware to install this project's firmware."
            } else {
                "The keyboard isn't connected over USB."
            };
            return v_flex()
                .gap_3()
                .child(div().child(text))
                .child(muted(&theme, self.model.status().to_string()))
                .into_any_element();
        }
        match self.page {
            Page::Overview => overview::render(self, cx),
            Page::Lighting => lighting::render(self, cx),
            Page::Keys | Page::Power => settings_page::render(self, self.page.sections(), self.layout.width, cx),
            Page::Firmware => unreachable!("handled above"),
        }
    }
}

impl Render for Configurator {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.layout = Layout::for_window(f32::from(window.viewport_size().width));
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
                    .child(div().pb_4().max_w(px(self.layout.width)).text_color(theme.muted_foreground).child(self.page.subtitle()))
                    .child(page),
            )
    }
}
