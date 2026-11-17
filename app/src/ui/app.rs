//! The root view: owns the keyboard model, the controls and the updater; switches pages; keeps
//! the connection and the previews alive.

use std::time::{Duration, Instant};

use gpui::{canvas, div, prelude::*, px, AnyElement, Context, FontWeight, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::controls::Controls;
use super::layout::{Layout, MAX_CONTENT};
use super::lighting::EffectGroup;
use super::model::{Connection, Model, Poll, Values};
use super::page::Page;
use super::widgets::muted;
use super::{app_settings, firmware, lighting, overview, power, settings_page, sidebar};
use crate::settings::{value, Setting};
use crate::{i18n, tr};
use crate::updater::{Releases, Source, Updater};

/// How often the keyboard is checked (connection, and changes made on the keyboard itself).
const WATCH_EVERY: Duration = Duration::from_millis(500);
/// How often to look for a keyboard while none is connected.
const SEARCH_EVERY: Duration = Duration::from_secs(2);
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
        cx.spawn_in(window, async move |this, cx| {
            let mut since_search = Duration::ZERO;
            loop {
                cx.background_executor().timer(WATCH_EVERY).await;
                since_search += WATCH_EVERY;
                let open = this.update_in(cx, |this, window, cx| {
                    if this.updater.step().is_busy() {
                        return; // the updater owns the USB device until it is done
                    }
                    if this.model.is_connected() {
                        match this.model.poll() {
                            Poll::Same => return,
                            Poll::Lost => this.controls = Controls::default(),
                            Poll::Changed(ids) => {
                                this.controls.sync(&ids, &this.model, window, cx);
                                this.effect_group = EffectGroup::of(this.model.byte(value::KEY_EFFECT));
                            }
                            Poll::StateChanged => {}
                        }
                    } else if since_search >= SEARCH_EVERY {
                        since_search = Duration::ZERO;
                        this.connect(window, cx);
                    }
                    cx.notify();
                });
                if open.is_err() {
                    break; // window closed
                }
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

    /// Switch the app's language; controls are rebuilt so their option lists follow.
    pub fn set_language(&mut self, code: &str, window: &mut Window, cx: &mut Context<Self>) {
        i18n::set(code);
        if self.model.is_connected() {
            self.controls = Controls::build(&self.model, window, cx);
        }
        cx.notify();
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
        // These work without a keyboard too.
        match self.page {
            Page::Firmware => return firmware::render(self, cx),
            Page::Settings => return app_settings::render(self, cx),
            _ => {}
        }
        if !self.model.is_connected() {
            let (text, detail) = match self.model.connection() {
                Connection::FlashMode => (tr!("connection.page_flash_mode"), None),
                Connection::Stock => (tr!("connection.page_stock"), None),
                Connection::Missing(reason) => (tr!("connection.page_not_connected"), Some(reason.clone())),
                _ => (tr!("connection.page_not_connected"), None),
            };
            return v_flex()
                .gap_3()
                .child(div().child(text))
                .when_some(detail, |el, detail| el.child(muted(&theme, detail)))
                .into_any_element();
        }
        match self.page {
            Page::Overview => overview::render(self, cx),
            Page::Lighting => lighting::render(self, cx),
            Page::Keys => settings_page::render(self, self.page.sections(), self.layout.width, cx),
            Page::Power => power::render(self, cx),
            Page::Firmware | Page::Settings => unreachable!("handled above"),
        }
    }
}

impl Render for Configurator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let page = self.render_page(cx);
        // Measure the room the page really gets; pages size their columns and previews from it.
        let view = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, _, cx| {
                let width = f32::from(bounds.size.width);
                cx.defer(move |cx| {
                    let _ = view.update(cx, |this, cx| {
                        if this.layout.set_width(width) {
                            cx.notify();
                        }
                    });
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
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
                    .child(div().relative().w_full().max_w(px(MAX_CONTENT)).child(measure).child(page)),
            )
    }
}
