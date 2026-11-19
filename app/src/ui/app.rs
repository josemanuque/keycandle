//! The root view: owns the keyboard model, the controls and the updater; switches pages; keeps
//! the connection and the previews alive.

use std::time::{Duration, Instant};

use gpui::{canvas, div, prelude::*, px, AnyElement, Context, FocusHandle, FontWeight, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::controls::Controls;
use super::layout::{Layout, MAX_CONTENT};
use super::lighting::EffectGroup;
use super::model::{Connection, Model, Poll, Values};
use super::chrome::{self, Backdrop, SidebarViews};
use super::page::Page;
use super::widgets::muted;
use super::{app_settings, firmware, lighting, menu, overview, power, settings_page, sidebar, updates};
use crate::settings::{value, Setting};
use crate::{i18n, tr};
use crate::updater::{Releases, Source, Updater};

/// How often the keyboard is checked (connection, and changes made on the keyboard itself).
const WATCH_EVERY: Duration = Duration::from_millis(500);
/// How often to look for a keyboard while none is connected.
const SEARCH_EVERY: Duration = Duration::from_secs(2);
/// Window width under which the sidebar shows only icons.
const COMPACT_BELOW: f32 = 980.0;
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
    /// Releases whose notes are open in the Firmware page.
    pub(super) open_notes: std::collections::HashSet<String>,
    /// Settings to put back once the keyboard returns from a firmware update.
    restore_after_update: Option<Values>,
    started: Instant,
    pub(super) backdrop: Backdrop,
    /// Sliders show the keyboard's numbers rather than percent (a preference).
    pub(super) raw_values: bool,
    /// The native material (and glass) behind the sidebar, on macOS.
    sidebar_views: Option<SidebarViews>,
    /// The sidebar, as a stop for Tab; the arrow keys then move through its sections.
    pub(super) sidebar_focus: FocusHandle,
    status_mark: sidebar::StatusMark,
    /// Updates: look when the app opens (a preference), the tags put off with "Later", the firmware
    /// version last seen over USB, and whether this session's notice was closed.
    pub(super) check_updates_on_start: bool,
    pub(super) dismissed_updates: Vec<String>,
    pub(super) last_firmware: Option<u32>,
    pub(super) toast_closed: bool,
}

impl Configurator {
    pub fn new(page: Page, backdrop: Backdrop, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sidebar_views = SidebarViews::install(backdrop, window, sidebar::WIDTH);
        let prefs = crate::prefs::Prefs::load();
        let mut this = Self {
            page,
            model: Model::default(),
            controls: Controls::default(),
            layout: Layout::default(),
            effect_group: EffectGroup::default(),
            updater: Updater::default(),
            pending_install: None,
            show_all_releases: false,
            open_notes: Default::default(),
            restore_after_update: None,
            started: Instant::now(),
            backdrop,
            raw_values: prefs.raw_values,
            check_updates_on_start: !prefs.no_update_check,
            dismissed_updates: prefs.dismissed_updates,
            last_firmware: prefs.last_firmware,
            toast_closed: false,
            sidebar_views,
            sidebar_focus: cx.focus_handle(),
            status_mark: Default::default(),
        };
        this.model.last_firmware = this.last_firmware;
        this.connect(window, cx);
        if this.check_updates_on_start {
            this.updater.check(); // in the background; the notice shows when the answer is in
        }
        this.open(page, cx);
        Self::watch_connection(window, cx);
        Self::animate(window, cx);
        // The sidebar's selection follows whether the window is the active one.
        cx.observe_window_activation(window, |_, _, cx| cx.notify()).detach();
        this
    }

    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.model.connect() {
            self.controls = Controls::default();
            if self.model.has_settings() {
                // Only on Bluetooth: the remembered values, sent one way.
                self.effect_group = EffectGroup::of(self.model.byte(value::KEY_EFFECT));
                self.controls = Controls::build(&self.model, window, cx);
            }
            return;
        }
        if let Some(saved) = self.restore_after_update.take() {
            let _ = self.model.restore(&saved);
        }
        self.remember_firmware();
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
                    if this.model.on_bluetooth() {
                        let changed = this.model.bluetooth_updates();
                        if !changed.is_empty() {
                            this.controls.sync(&changed, &this.model, window, cx);
                            this.effect_group = EffectGroup::of(this.model.byte(value::KEY_EFFECT));
                        }
                        if since_search >= SEARCH_EVERY {
                            since_search = Duration::ZERO;
                            if this.model.recheck_bluetooth() {
                                this.connect(window, cx); // the cable came back, or it's gone
                            }
                        }
                    } else if this.model.is_connected() {
                        match this.model.poll() {
                            Poll::Same => {
                                if !this.model.activity_fading() {
                                    return;
                                }
                            }
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
                let preview = this.page.is_animated() && this.model.has_settings();
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

    /// Look for the keyboard again now (Keyboard ▸ Reconnect).
    pub fn reconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.updater.step().is_busy() {
            self.connect(window, cx);
            cx.notify();
        }
    }

    /// Fetch the release list again and show it.
    pub fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.updater.releases(), Releases::Checking) {
            self.updater.check();
        }
        self.open(Page::Firmware, cx);
    }

    /// Switch the app's language; controls and menus are rebuilt so their labels follow.
    pub fn set_language(&mut self, code: &str, window: &mut Window, cx: &mut Context<Self>) {
        i18n::set(code);
        if self.model.has_settings() {
            self.controls = Controls::build(&self.model, window, cx);
        }
        menu::set_menus(cx);
        cx.notify();
    }

    /// Percent or the keyboard's own numbers on the sliders; remembered.
    pub fn set_raw_values(&mut self, raw: bool, cx: &mut Context<Self>) {
        self.raw_values = raw;
        let mut prefs = crate::prefs::Prefs::load();
        prefs.raw_values = raw;
        prefs.save();
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
        if !self.model.can_install() {
            cx.notify(); // the cable was pulled while asking: nothing starts
            return;
        }
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
        if !self.model.has_settings() {
            let (text, detail) = match self.model.connection() {
                Connection::FlashMode => (tr!("connection.page_flash_mode"), None),
                Connection::Stock => (tr!("connection.page_stock"), None),
                Connection::Bluetooth => (tr!("connection.page_bluetooth_no_values"), None),
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        // Narrow windows: the sidebar shrinks to its icons so the page keeps its room.
        let viewport = window.viewport_size();
        let compact = f32::from(viewport.width) < COMPACT_BELOW;
        self.layout.window_height = f32::from(viewport.height);
        let page = self.render_page(cx);
        let mark = self.status_mark.update(sidebar::outcome(&self.model), Instant::now());
        if mark.is_some_and(|mark| mark.moving) {
            window.request_animation_frame();
        }
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
        if let Some(views) = &mut self.sidebar_views {
            views.set_width(if compact { self.backdrop.rail_width() } else { sidebar::WIDTH });
        }
        // Translucent windows (macOS) have no title bar: the page keeps clear of its strip, which
        // still moves the window. Only the page is opaque; the backdrop shows under the sidebar.
        let title_bar = self.backdrop.title_bar_height();
        let scroll = v_flex()
            .id("page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_8()
            .pb_8()
            .when(title_bar == px(0.), |el| el.pt_8())
            .gap_2()
            .child(div().text_3xl().font_weight(FontWeight::SEMIBOLD).child(self.page.title()))
            .child(div().pb_4().text_color(theme.muted_foreground).child(self.page.subtitle()))
            .child(div().relative().w_full().min_w_0().max_w(px(MAX_CONTENT)).child(measure).child(page));
        h_flex()
            .relative()
            .size_full()
            .text_color(theme.foreground)
            .child(sidebar::render(self, compact, mark, window, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0() // shrink to the window, not grow with the content (the width is measured below)
                    .h_full()
                    .bg(theme.background)
                    .child(chrome::title_strip(title_bar))
                    .child(scroll),
            )
            .when(compact, |el| el.children(sidebar::activity_popup(self, cx)))
            .children(updates::toast(self, cx))
    }
}
