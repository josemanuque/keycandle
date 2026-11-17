//! Firmware: what is installed, updates from GitHub (or a file), and the install progress.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, PathPromptOptions};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    progress::Progress,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Theme,
};

use super::app::Configurator;
use super::layout::GAP;
use super::widgets::{badge, card, card_title, muted, note};
use crate::releases::{format_version, Release};
use crate::updater::{Releases, Source, Step};

/// Versions listed before "Show older versions".
const RECENT: usize = 4;

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let width = app.layout.width.min(900.0);
    v_flex()
        .w(px(width))
        .gap(px(GAP))
        .child(installed(app, &theme, cx))
        .when_some(job(app, &theme, cx), |el, job| el.child(job))
        .when_some(confirmation(app, &theme, cx), |el, c| el.child(c))
        .child(available(app, &theme, cx))
        .into_any_element()
}

fn installed(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> impl IntoElement {
    let model = &app.model;
    let (title, detail): (String, &str) = match (model.is_connected(), model.firmware_version(), model.in_flash_mode()) {
        (true, Some(0), _) => ("Older than 0.7".into(), "This version can't restart itself for updates: you'll press Fn + Pause when asked."),
        (true, Some(v), _) => (format_version(v), "Connected over USB."),
        (true, None, _) => ("Unknown".into(), "Connected over USB."),
        (false, _, false) if model.on_stock_firmware() => (
            "Original firmware".into(),
            "The Royal Kludge firmware is installed. Install a release below to switch to this project's firmware: the keyboard is put in flash mode for you.",
        ),
        (false, _, true) => ("Flash mode".into(), "The keyboard is waiting for new firmware. Install one below, or leave flash mode to use it as it is."),
        (false, _, false) => ("Not connected".into(), "Connect the keyboard with the USB cable to update it."),
    };
    let view = cx.entity().downgrade();
    card(theme)
        .child(card_title("On the keyboard"))
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .gap_4()
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child(title))
                        .child(muted(theme, detail)),
                )
                .when(model.in_flash_mode() && !app.updater.step().is_busy(), |el| {
                    el.child(Button::new("leave-flash").outline().label("Leave flash mode").on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.updater.leave_bootloader();
                            cx.notify();
                        });
                    }))
                }),
        )
}

/// The running or last install, if any.
fn job(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> Option<impl IntoElement> {
    let step = app.updater.step();
    let (title, detail, progress): (&str, String, Option<f32>) = match &step {
        Step::Idle => return None,
        Step::Downloading => ("Getting the firmware…", "Downloading and checking it.".into(), None),
        Step::WaitingForBootloader { manual: false } => ("Restarting into flash mode…", "The keyboard restarts itself.".into(), None),
        Step::WaitingForBootloader { manual: true } => (
            "Press Fn + Pause on the keyboard",
            "That puts it in flash mode; the install continues by itself. (Waiting up to 2 minutes.)".into(),
            None,
        ),
        Step::Writing(p) => ("Writing the firmware…", "Keep the cable plugged in.".into(), Some(*p)),
        Step::Starting => ("Starting the keyboard…", String::new(), None),
        Step::Done(text) => ("Done", text.clone(), None),
        Step::Failed(text) => ("The update didn't finish", format!("{text}. The keyboard can always be updated again: its flash mode is built into the chip."), None),
    };
    let view = cx.entity().downgrade();
    let icon = match &step {
        Step::Done(_) => Some("icons/circle-check.svg"),
        Step::Failed(_) => Some("icons/triangle-alert.svg"),
        _ => None,
    };
    Some(
        card(theme)
            .gap_3()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .when_some(icon, |el, icon| el.child(Icon::empty().path(icon).size_5()))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title)),
            )
            .when(!detail.is_empty(), |el| el.child(muted(theme, detail)))
            .when(step.is_busy(), |el| {
                el.child(match progress {
                    Some(p) => Progress::new("progress").value(p * 100.0),
                    None => Progress::new("progress").loading(true),
                })
            })
            .when(!step.is_busy(), |el| {
                el.child(h_flex().child(Button::new("dismiss").ghost().label("OK").on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.updater.dismiss();
                        cx.notify();
                    });
                })))
            }),
    )
}

/// "Install X?" before anything happens.
fn confirmation(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> Option<impl IntoElement> {
    let source = app.pending_install.as_ref()?;
    let stock = matches!(source, Source::Release(r) if r.stock);
    let what = if stock {
        "This puts back the original Royal Kludge firmware. This app and VIA don't work with it. To come back later, put it in flash mode with tools/stock_enter_isp.py (see the project's stock/README.md) and install a release from here.".to_string()
    } else {
        "The keyboard restarts into flash mode, the firmware is written (about 10 seconds), and it starts again. Your settings from this app are put back afterwards; a keymap changed in VIA goes back to the default.".to_string()
    };
    let view = cx.entity().downgrade();
    let cancel = view.clone();
    Some(
        card(theme)
            .gap_3()
            .border_color(theme.ring)
            .child(card_title(format!("Install {}?", source.describe())))
            .child(muted(theme, what))
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("confirm").primary().label("Install").on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| this.confirm_install(cx));
                    }))
                    .child(Button::new("cancel").ghost().label("Cancel").on_click(move |_, _, cx| {
                        let _ = cancel.update(cx, |this, cx| {
                            this.pending_install = None;
                            cx.notify();
                        });
                    })),
            ),
    )
}

fn available(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> impl IntoElement {
    let busy = app.updater.step().is_busy() || app.pending_install.is_some();
    let installed = app.model.firmware_version();
    let view = cx.entity().downgrade();
    let refresh = view.clone();
    let from_file = view.clone();

    let header = h_flex()
        .justify_between()
        .items_center()
        .child(card_title("Available"))
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("from-file")
                        .ghost()
                        .icon(Icon::empty().path("icons/folder-open.svg"))
                        .label("From a file…")
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            let picked = cx.prompt_for_paths(PathPromptOptions {
                                files: true,
                                directories: false,
                                multiple: false,
                                prompt: Some("Install".into()),
                            });
                            let view = from_file.clone();
                            cx.spawn(async move |cx| {
                                if let Ok(Ok(Some(paths))) = picked.await {
                                    if let Some(path) = paths.into_iter().next() {
                                        let _ = view.update(cx, |this, cx| this.request_install(Source::File(path), cx));
                                    }
                                }
                            })
                            .detach();
                        }),
                )
                .child(
                    Button::new("refresh")
                        .ghost()
                        .icon(Icon::empty().path("icons/refresh-cw.svg"))
                        .label("Check again")
                        .disabled(matches!(app.updater.releases(), Releases::Checking))
                        .on_click(move |_, _, cx| {
                            let _ = refresh.update(cx, |this, cx| {
                                this.updater.check();
                                cx.notify();
                            });
                        }),
                ),
        );

    let body: AnyElement = match app.updater.releases() {
        Releases::NotChecked | Releases::Checking => {
            v_flex().gap_2().child(muted(theme, "Asking GitHub…")).child(Progress::new("checking").loading(true)).into_any_element()
        }
        Releases::Failed(e) => note(theme, format!("Couldn't get the list from GitHub: {e}")).into_any_element(),
        Releases::Loaded(list) => {
            let newest = list.iter().find(|r| !r.stock).map(|r| r.tag.clone());
            let total = list.len();
            let shown: Vec<Release> = if app.show_all_releases {
                list
            } else {
                // the newest few, plus the original firmware
                list.into_iter().enumerate().filter(|(i, r)| *i < RECENT || r.stock).map(|(_, r)| r).collect()
            };
            let hidden = total - shown.len();
            let toggle = view.clone();
            v_flex()
                .children(shown.into_iter().enumerate().map(|(i, release)| {
                    release_row(theme, i, release, newest.as_deref(), installed, busy, view.clone())
                }))
                .when(hidden > 0, |el| {
                    el.child(h_flex().pt_2().border_t_1().border_color(theme.border).child(
                        Button::new("show-all").ghost().label(format!("Show {hidden} older versions")).on_click(move |_, _, cx| {
                            let _ = toggle.update(cx, |this, cx| {
                                this.show_all_releases = true;
                                cx.notify();
                            });
                        }),
                    ))
                })
                .into_any_element()
        }
    };

    card(theme).gap_2().child(header).child(body)
}

fn release_row(
    theme: &Theme,
    i: usize,
    release: Release,
    newest: Option<&str>,
    installed: Option<u32>,
    busy: bool,
    view: gpui::WeakEntity<Configurator>,
) -> impl IntoElement {
    let is_installed = installed.is_some_and(|v| v != 0 && release.version() == Some(v));
    let is_newest = newest == Some(release.tag.as_str());
    let title = if release.stock { "Original firmware (Royal Kludge V1.12)".to_string() } else { release.tag.clone() };
    let action = if release.stock {
        "Go back"
    } else if is_installed {
        "Reinstall"
    } else {
        "Install"
    };
    let button = Button::new(("install", i)).label(action).disabled(busy);
    let button = if is_newest && !is_installed { button.primary() } else { button.outline() };
    h_flex()
        .py_3()
        .gap_4()
        .items_center()
        .border_t_1()
        .border_color(theme.border)
        .child(
            v_flex()
                .flex_1()
                .gap_0p5()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                        .when(is_newest, |el| el.child(badge(theme, "Newest")))
                        .when(is_installed, |el| el.child(badge(theme, "Installed")))
                        .when(release.prerelease, |el| el.child(badge(theme, "Test build"))),
                )
                .child(muted(theme, format!("{} · {}", release.date, release.file_name()))),
        )
        .child(button.on_click(move |_, _, cx| {
            let source = Source::Release(release.clone());
            let _ = view.update(cx, |this, cx| this.request_install(source, cx));
        }))
}
