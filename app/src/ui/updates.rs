//! New versions of the app and of the keyboard's firmware: what's newer than what's installed, and
//! how the app tells you. The release list is fetched once when the app opens (unless turned off in
//! Settings); you then get a small notice (a toast), a dot on the sidebar, and an Updates card on the
//! Overview. "Later" puts a notice off until the next version.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, Theme,
};

use super::app::Configurator;
use super::page::Page;
use super::widgets::{card, card_title, muted};
use crate::prefs::Prefs;
use crate::releases::{format_version, parse_version, update_over, Release};
use crate::tr;
use crate::updater::Releases;

/// What's newer than what's installed. Empty until the release list has arrived.
#[derive(Default)]
pub struct Available {
    pub app: Option<Release>,
    pub firmware: Option<Release>,
}

impl Available {
    pub fn any(&self) -> bool {
        self.app.is_some() || self.firmware.is_some()
    }

    /// Tags of the updates on offer, to remember a "Later".
    fn tags(&self) -> Vec<String> {
        [&self.app, &self.firmware].into_iter().flatten().map(|r| r.tag.clone()).collect()
    }
}

/// This app's version, comparable with release versions.
pub fn app_version() -> u32 {
    if super::model::preview() {
        return 0x0800;
    }
    parse_version(env!("CARGO_PKG_VERSION")).unwrap_or(0)
}

/// This app's version as shown next to an update (the one compared).
fn app_version_text() -> String {
    if super::model::preview() { format_version(app_version()) } else { env!("CARGO_PKG_VERSION").to_string() }
}

/// The firmware version on the keyboard: as connected now, or as last seen (it's on Bluetooth).
fn firmware_version(app: &Configurator) -> Option<u32> {
    app.model.firmware_version().filter(|v| *v != 0).or(app.last_firmware)
}

pub fn available(app: &Configurator) -> Available {
    let Releases::Loaded(list) = app.updater.releases() else { return Available::default() };
    let app_update = update_over(&list, Some(app_version()), true);
    let firmware = update_over(&list, firmware_version(app), false);
    Available { app: app_update, firmware }
}

/// The release list arrived, and nothing newer than what's installed is in it.
pub fn checked_up_to_date(app: &Configurator) -> bool {
    matches!(app.updater.releases(), Releases::Loaded(_)) && !available(app).any()
}

/// Updates not yet put off with "Later": what the toast announces.
fn unannounced(app: &Configurator) -> Available {
    let mut a = available(app);
    let dismissed = &app.dismissed_updates;
    a.app = a.app.filter(|r| !dismissed.contains(&r.tag));
    a.firmware = a.firmware.filter(|r| !dismissed.contains(&r.tag));
    a
}

impl Configurator {
    /// Put the current update notices off until there's a newer version.
    pub fn dismiss_updates(&mut self, cx: &mut Context<Self>) {
        let tags = available(self).tags();
        let mut prefs = Prefs::load();
        for tag in tags {
            if !prefs.dismissed_updates.contains(&tag) {
                prefs.dismissed_updates.push(tag);
            }
        }
        self.dismissed_updates = prefs.dismissed_updates.clone();
        prefs.save();
        cx.notify();
    }

    /// Whether to look for updates when the app opens (Settings).
    pub fn set_update_check(&mut self, on: bool, cx: &mut Context<Self>) {
        let mut prefs = Prefs::load();
        prefs.no_update_check = !on;
        prefs.save();
        self.check_updates_on_start = on;
        cx.notify();
    }

    /// Remember the firmware version seen over USB, for update notices while on Bluetooth.
    pub fn remember_firmware(&mut self) {
        let Some(v) = self.model.firmware_version().filter(|v| *v != 0) else { return };
        if self.last_firmware != Some(v) {
            self.last_firmware = Some(v);
            self.model.last_firmware = Some(v);
            let mut prefs = Prefs::load();
            prefs.last_firmware = Some(v);
            prefs.save();
        }
    }
}

/// One update: what it is, what you have, and its action.
fn update_row(theme: &Theme, icon: &'static str, title: String, detail: String, action: AnyElement) -> impl IntoElement {
    h_flex()
        .flex_wrap()
        .items_center()
        .gap_4()
        .py_3()
        .border_t_1()
        .border_color(theme.border)
        .child(
            div()
                .size_9()
                .flex_none()
                .rounded_lg()
                .bg(theme.muted)
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.primary)
                .child(Icon::empty().path(icon).size_4()),
        )
        .child(v_flex().flex_1().min_w(px(220.)).gap_0p5().child(div().font_weight(FontWeight::SEMIBOLD).child(title)).child(muted(theme, detail)))
        .child(action)
}

/// "Download" for the app (its release page) and "Install" for the firmware (the Firmware page).
fn app_action(release: &Release, id: &'static str) -> AnyElement {
    let page = release.page.clone();
    Button::new(id).primary().label(tr!("updates.download")).on_click(move |_, _, cx| cx.open_url(&page)).into_any_element()
}

fn firmware_action(id: &'static str, cx: &mut Context<Configurator>) -> AnyElement {
    let view = cx.entity().downgrade();
    Button::new(id)
        .primary()
        .label(tr!("updates.install"))
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| this.open(Page::Firmware, cx));
        })
        .into_any_element()
}

/// The Overview's Updates card, or a quiet "up to date" line.
pub fn overview_card(app: &Configurator, width: f32, cx: &mut Context<Configurator>) -> Option<AnyElement> {
    let theme = cx.theme().clone();
    let a = available(app);
    if !a.any() {
        return checked_up_to_date(app).then(|| {
            h_flex()
                .gap_2()
                .items_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::CircleCheck).size_4().text_color(theme.success))
                .child(tr!("updates.all_up_to_date"))
                .into_any_element()
        });
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    if let Some(r) = &a.app {
        let detail = tr!("updates.app_detail", installed = app_version_text());
        rows.push(update_row(&theme, "icons/app-window.svg", tr!("updates.app_title", version = r.tag.clone()), detail, app_action(r, "overview-app")).into_any_element());
    }
    if let Some(r) = &a.firmware {
        let installed = firmware_version(app).map(format_version).unwrap_or_default();
        let detail = tr!("updates.firmware_detail", installed = installed);
        rows.push(update_row(&theme, "icons/cpu.svg", tr!("updates.firmware_title", version = r.tag.clone()), detail, firmware_action("overview-fw", cx)).into_any_element());
    }
    Some(
        card(&theme)
            .w(px(width))
            .border_color(theme.ring.opacity(0.5))
            .child(card_title(tr!("updates.title")))
            .children(rows)
            .into_any_element(),
    )
}

/// The small notice in the corner when updates arrive. "Later" puts it off until the next version.
pub fn toast(app: &Configurator, cx: &mut Context<Configurator>) -> Option<AnyElement> {
    if app.toast_closed {
        return None;
    }
    let a = unannounced(app);
    let (title, detail) = match (&a.app, &a.firmware) {
        (Some(r), Some(f)) => (tr!("updates.toast_two"), tr!("updates.toast_two_detail", app = r.tag.clone(), firmware = f.tag.clone())),
        (Some(r), None) => (tr!("updates.app_title", version = r.tag.clone()), tr!("updates.toast_app_detail")),
        (None, Some(f)) => (tr!("updates.firmware_title", version = f.tag.clone()), tr!("updates.toast_firmware_detail")),
        (None, None) => return None,
    };
    let theme = cx.theme().clone();
    let view = cx.entity().downgrade();
    let (later, close, show) = (view.clone(), view.clone(), view);
    Some(
        v_flex()
            .absolute()
            .right_6()
            .bottom_6()
            .w(px(360.))
            .gap_2p5()
            .p_4()
            .rounded_xl()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
            .child(
                h_flex()
                    .items_start()
                    .gap_2p5()
                    .child(div().mt(px(6.)).size_2().flex_none().rounded_full().bg(theme.primary))
                    .child(v_flex().flex_1().gap_0p5().child(div().font_weight(FontWeight::SEMIBOLD).child(title)).child(muted(&theme, detail)))
                    .child(Button::new("toast-close").ghost().xsmall().icon(IconName::Close).on_click(move |_, _, cx| {
                        let _ = close.update(cx, |this, cx| {
                            this.toast_closed = true;
                            cx.notify();
                        });
                    })),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(Button::new("toast-later").ghost().small().label(tr!("updates.later")).on_click(move |_, _, cx| {
                        let _ = later.update(cx, |this, cx| this.dismiss_updates(cx));
                    }))
                    .child(Button::new("toast-show").primary().small().label(tr!("updates.show")).on_click(move |_, _, cx| {
                        let _ = show.update(cx, |this, cx| {
                            this.toast_closed = true;
                            this.open(Page::Overview, cx);
                        });
                    })),
            )
            .into_any_element(),
    )
}

/// Settings: what's installed and what's new, and whether to look when the app opens.
pub fn settings_card(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> impl IntoElement {
    let a = available(app);
    let checking = matches!(app.updater.releases(), Releases::Checking);
    let failed = matches!(app.updater.releases(), Releases::Failed(_));
    let status = if checking {
        tr!("updates.checking")
    } else if failed {
        tr!("updates.check_failed")
    } else if matches!(app.updater.releases(), Releases::NotChecked) {
        tr!("updates.not_checked")
    } else {
        String::new()
    };
    let app_line = match &a.app {
        Some(r) => tr!("updates.newer_available", version = r.tag.clone()),
        None => tr!("updates.up_to_date"),
    };
    let fw_installed = firmware_version(app).map(format_version);
    let fw_line = match (&a.firmware, &fw_installed) {
        (Some(r), _) => tr!("updates.newer_available", version = r.tag.clone()),
        (None, Some(_)) => tr!("updates.up_to_date"),
        (None, None) => tr!("updates.firmware_unknown"),
    };
    let view = cx.entity().downgrade();
    let (toggle, check) = (view.clone(), view);
    let app_action = a.app.as_ref().map(|r| app_action(r, "settings-app"));
    let fw_action = a.firmware.as_ref().map(|_| firmware_action("settings-fw", cx));
    let row = |title: String, line: String, action: Option<AnyElement>| {
        h_flex()
            .flex_wrap()
            .items_center()
            .gap_4()
            .py_3()
            .border_t_1()
            .border_color(theme.border)
            .child(v_flex().flex_1().min_w(px(200.)).gap_0p5().child(div().child(title)).child(muted(theme, line)))
            .children(action)
    };
    card(theme)
        .child(
            h_flex()
                .justify_between()
                .items_baseline()
                .gap_4()
                .child(card_title(tr!("updates.title")))
                .child(
                    h_flex().gap_3().items_center().when(!status.is_empty(), |el| el.child(muted(theme, status))).child(
                        Button::new("check-now").ghost().small().icon(Icon::empty().path("icons/refresh-cw.svg")).label(tr!("updates.check_now")).disabled(checking).on_click(
                            move |_, _, cx| {
                                let _ = check.update(cx, |this, cx| {
                                    this.toast_closed = false;
                                    this.updater.check();
                                    cx.notify();
                                });
                            },
                        ),
                    ),
                ),
        )
        .child(row(tr!("updates.app_installed", version = app_version_text()), app_line, app_action))
        .child(row(
            fw_installed.map_or_else(|| tr!("updates.firmware"), |v| tr!("updates.firmware_installed", version = v)),
            fw_line,
            fw_action,
        ))
        .child(
            h_flex()
                .items_center()
                .gap_6()
                .pt_3()
                .border_t_1()
                .border_color(theme.border)
                .child(
                    v_flex()
                        .flex_1()
                        .gap_0p5()
                        .child(div().child(tr!("updates.check_on_start")))
                        .child(muted(theme, tr!("updates.check_on_start_help"))),
                )
                .child(Switch::new("check-on-start").checked(app.check_updates_on_start).on_click(move |&on, _, cx| {
                    let _ = toggle.update(cx, |this, cx| this.set_update_check(on, cx));
                })),
        )
}
