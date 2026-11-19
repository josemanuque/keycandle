//! Firmware: what is installed, updates from GitHub (or a file), and the install progress.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, PathPromptOptions, SharedString};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    progress::Progress,
    text::TextView,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Theme,
};

use super::app::Configurator;
use super::layout::GAP;
use super::widgets::{badge, card, card_title, muted, note};
use crate::releases::{format_version, Release};
use crate::tr;
use crate::updater::{Releases, Source, Step};

/// Versions listed before "Show older versions".
const RECENT: usize = 4;

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let width = app.layout.width;
    v_flex()
        .w(px(width))
        .gap(px(GAP))
        .child(installed(app, &theme, cx))
        .when_some(job(app, &theme, cx), |el, job| el.child(job))
        .when_some(confirmation(app, &theme, cx), |el, c| el.child(c))
        .child(available(app, &theme, cx))
        .child(
            h_flex()
                .flex_wrap()
                .items_start()
                .gap(px(GAP))
                .child(other_firmware(app, &theme, width, cx))
                .child(recovery(&theme, width)),
        )
        .into_any_element()
}

/// Two cards side by side when there's room for both.
fn half(width: f32) -> f32 {
    if width >= 2.0 * 380.0 + GAP { ((width - GAP) / 2.0).floor() } else { width }
}

/// A file of your own, or back to Royal Kludge's firmware.
fn other_firmware(app: &Configurator, theme: &Theme, width: f32, cx: &mut Context<Configurator>) -> impl IntoElement {
    let busy = app.updater.step().is_busy() || app.pending_install.is_some();
    let view = cx.entity().downgrade();
    let from_file = view.clone();
    let stock = match app.updater.releases() {
        Releases::Loaded(list) => list.into_iter().find(|r| r.stock),
        _ => None,
    };
    card(theme)
        .w(px(half(width)))
        .gap_2()
        .child(card_title(tr!("firmware.other")))
        .child(muted(theme, tr!("firmware.other_detail")).text_sm())
        .child(
            h_flex()
                .pt_2()
                .gap_2()
                .flex_wrap()
                .child(
                    Button::new("from-file")
                        .outline()
                        .icon(Icon::empty().path("icons/folder-open.svg"))
                        .label(tr!("firmware.from_file"))
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            let picked = cx.prompt_for_paths(PathPromptOptions {
                                files: true,
                                directories: false,
                                multiple: false,
                                prompt: Some(tr!("firmware.install").into()),
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
                .when_some(stock, |el, release| {
                    el.child(Button::new("restore-stock").outline().label(tr!("firmware.restore_original")).disabled(busy).on_click(
                        move |_, _, cx| {
                            let source = Source::Release(release.clone());
                            let _ = view.update(cx, |this, cx| this.request_install(source, cx));
                        },
                    ))
                }),
        )
}

/// What to do when an update went wrong: the ROM bootloader can't be overwritten.
fn recovery(theme: &Theme, width: f32) -> impl IntoElement {
    card(theme)
        .w(px(half(width)))
        .gap_2()
        .child(card_title(tr!("firmware.recovery_title")))
        .child(muted(theme, tr!("firmware.recovery_detail")).text_sm())
        .child(
            h_flex()
                .id("recovery-guide")
                .pt_2()
                .gap_1()
                .items_center()
                .cursor_pointer()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.link)
                .hover(|s| s.text_color(theme.link_hover))
                .child(tr!("firmware.recovery_link"))
                .on_click(|_, _, cx| cx.open_url(RECOVERY_GUIDE)),
        )
}

/// The BOOT-pad steps.
const RECOVERY_GUIDE: &str =
    "https://github.com/josemanuque/keycandle/blob/main/stock/README.md#if-it-is-bricked-no-keys-no-lights-fnpause-does-nothing";

fn installed(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> impl IntoElement {
    let model = &app.model;
    let (title, detail): (String, String) = match (model.is_connected(), model.firmware_version(), model.in_flash_mode()) {
        (true, Some(0), _) => (tr!("firmware.older"), tr!("firmware.older_detail")),
        (true, Some(v), _) => (format_version(v), tr!("firmware.connected_detail")),
        (true, None, _) => (tr!("firmware.unknown"), tr!("firmware.connected_detail")),
        (false, _, false) if model.on_stock_firmware() => (tr!("firmware.stock"), tr!("firmware.stock_detail")),
        (false, _, true) => (tr!("firmware.flash_mode"), tr!("firmware.flash_mode_detail")),
        (false, _, false) => (tr!("firmware.not_connected"), tr!("firmware.not_connected_detail")),
    };
    let view = cx.entity().downgrade();
    let update = update_status(app, theme, cx);
    card(theme)
        .child(card_title(tr!("firmware.on_keyboard")))
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
                .children(update)
                .when(model.in_flash_mode() && !app.updater.step().is_busy(), |el| {
                    el.child(Button::new("leave-flash").outline().label(tr!("firmware.leave_flash_mode")).on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.updater.leave_bootloader();
                            cx.notify();
                        });
                    }))
                }),
        )
}

/// Next to the installed version: "Up to date", or the newer release with its Install button.
fn update_status(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> Option<AnyElement> {
    let installed = app.model.firmware_version().filter(|v| *v != 0 && app.model.is_connected())?;
    let Releases::Loaded(list) = app.updater.releases() else { return None };
    let newest = list.into_iter().filter(|r| !r.stock).max_by_key(|r| r.version().unwrap_or(0))?;
    let newest_version = newest.version()?;
    if newest_version <= installed {
        return Some(
            h_flex()
                .gap_2()
                .items_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::CircleCheck).size_4().text_color(theme.success))
                .child(tr!("firmware.up_to_date"))
                .into_any_element(),
        );
    }
    let busy = app.updater.step().is_busy() || app.pending_install.is_some();
    let view = cx.entity().downgrade();
    let tag = newest.tag.clone();
    Some(
        h_flex()
            .gap_3()
            .items_center()
            .child(div().text_sm().child(tr!("firmware.update_available", version = tag)))
            .child(Button::new("install-newest").primary().label(tr!("firmware.install")).disabled(busy).on_click(move |_, _, cx| {
                let source = Source::Release(newest.clone());
                let _ = view.update(cx, |this, cx| this.request_install(source, cx));
            }))
            .into_any_element(),
    )
}

/// The install's stages, as the design shows them: done, now, or still to come.
fn stages(theme: &Theme, step: &Step) -> impl IntoElement {
    let current = match step {
        Step::Downloading => 0,
        Step::WaitingForBootloader { .. } => 1,
        Step::Writing(_) => 2,
        Step::Starting => 3,
        _ => 4, // finished
    };
    let failed = matches!(step, Step::Failed(_));
    let names = [
        (tr!("firmware.stage.get"), tr!("firmware.stage.get_detail")),
        (tr!("firmware.stage.flash_mode"), tr!("firmware.stage.flash_mode_detail")),
        (tr!("firmware.stage.write"), tr!("firmware.stage.write_detail")),
        (tr!("firmware.stage.start"), tr!("firmware.stage.start_detail")),
    ];
    h_flex().flex_wrap().gap_3().children(names.into_iter().enumerate().map(|(i, (title, text))| {
        let done = i < current && !failed;
        let now = i == current;
        let (mark_bg, mark_fg) = if done {
            (theme.success.opacity(0.18), theme.success)
        } else if now {
            (theme.primary, theme.primary_foreground)
        } else {
            (theme.muted, theme.muted_foreground)
        };
        v_flex()
            .flex_1()
            .min_w(px(170.))
            .gap_1p5()
            .p_3p5()
            .rounded_lg()
            .bg(theme.background)
            .border_1()
            .border_color(if now { theme.primary } else { theme.border })
            .child(
                div()
                    .size_6()
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .bg(mark_bg)
                    .text_color(mark_fg)
                    .child(if done {
                        Icon::new(IconName::Check).size_3p5().into_any_element()
                    } else {
                        (i + 1).to_string().into_any_element()
                    }),
            )
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(muted(theme, text))
    }))
}

/// The running or last install, if any.
fn job(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> Option<impl IntoElement> {
    let step = app.updater.step();
    let (title, detail, progress): (String, String, Option<f32>) = match &step {
        Step::Idle => return None,
        Step::Downloading => (tr!("firmware.step.downloading"), tr!("firmware.step.downloading_detail"), None),
        Step::WaitingForBootloader { manual: false } => (tr!("firmware.step.restarting"), tr!("firmware.step.restarting_detail"), None),
        Step::WaitingForBootloader { manual: true } => (tr!("firmware.step.press_keys"), tr!("firmware.step.press_keys_detail"), None),
        Step::Writing(p) => (tr!("firmware.step.writing"), tr!("firmware.step.writing_detail"), Some(*p)),
        Step::Starting => (tr!("firmware.step.starting"), String::new(), None),
        Step::Done(name) => (tr!("firmware.step.done"), tr!("firmware.step.installed", name = name), None),
        Step::LeftFlashMode => (tr!("firmware.step.done"), tr!("firmware.step.left_flash_mode"), None),
        Step::Failed(error) => (tr!("firmware.step.failed"), tr!("firmware.step.failed_detail", error = error), None),
    };
    let view = cx.entity().downgrade();
    let icon = match &step {
        Step::Done(_) | Step::LeftFlashMode => Some("icons/circle-check.svg"),
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
            .when(step.is_busy(), |el| el.child(stages(theme, &step)))
            .when(!detail.is_empty(), |el| el.child(muted(theme, detail)))
            .when(step.is_busy(), |el| {
                el.child(match progress {
                    Some(p) => Progress::new("progress").value(p * 100.0),
                    None => Progress::new("progress").loading(true),
                })
            })
            .when(!step.is_busy(), |el| {
                el.child(h_flex().child(Button::new("dismiss").ghost().label(tr!("firmware.ok")).on_click(move |_, _, cx| {
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
    let what = if stock { tr!("firmware.confirm_stock") } else { tr!("firmware.confirm_update") };
    let view = cx.entity().downgrade();
    let cancel = view.clone();
    Some(
        card(theme)
            .gap_3()
            .border_color(theme.ring)
            .child(card_title(tr!("firmware.confirm_title", name = source.describe())))
            .child(muted(theme, what))
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("confirm").primary().label(tr!("firmware.install")).on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| this.confirm_install(cx));
                    }))
                    .child(Button::new("cancel").ghost().label(tr!("firmware.cancel")).on_click(move |_, _, cx| {
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

    let header = h_flex()
        .justify_between()
        .items_center()
        .child(card_title(tr!("firmware.available")))
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("refresh")
                        .ghost()
                        .icon(Icon::empty().path("icons/refresh-cw.svg"))
                        .label(tr!("firmware.check_again"))
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
            v_flex().gap_2().child(muted(theme, tr!("firmware.asking_github"))).child(Progress::new("checking").loading(true)).into_any_element()
        }
        Releases::Failed(e) => note(theme, tr!("firmware.github_failed", error = e)).into_any_element(),
        Releases::Loaded(list) => {
            let list: Vec<Release> = list.into_iter().filter(|r| !r.stock).collect();
            let newest = list.iter().find(|r| !r.stock).map(|r| r.tag.clone());
            let total = list.len();
            let shown: Vec<Release> = if app.show_all_releases {
                list
            } else {
                list.into_iter().take(RECENT).collect()
            };
            let hidden = total - shown.len();
            let older = total.saturating_sub(RECENT);
            let toggle = view.clone();
            let label = if app.show_all_releases { tr!("firmware.show_fewer") } else { tr!("firmware.show_older", count = hidden) };
            v_flex()
                .children(shown.into_iter().enumerate().map(|(i, release)| {
                    let open = app.open_notes.contains(&release.tag);
                    release_row(theme, i, release, newest.as_deref(), installed, busy, open, view.clone())
                }))
                .when(older > 0, |el| {
                    el.child(h_flex().pt_2().border_t_1().border_color(theme.border).child(
                        Button::new("show-all").ghost().label(label).on_click(move |_, _, cx| {
                            let _ = toggle.update(cx, |this, cx| {
                                this.show_all_releases = !this.show_all_releases;
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
    notes_open: bool,
    view: gpui::WeakEntity<Configurator>,
) -> impl IntoElement {
    let is_installed = installed.is_some_and(|v| v != 0 && release.version() == Some(v));
    let is_newest = newest == Some(release.tag.as_str());
    let title = if release.stock { tr!("firmware.stock_release") } else { release.tag.clone() };
    let action = if release.stock {
        tr!("firmware.go_back")
    } else if is_installed {
        tr!("firmware.reinstall")
    } else {
        tr!("firmware.install")
    };
    let button = Button::new(("install", i)).label(action).disabled(busy);
    // Install is the main action (blue); reinstalling or going back to stock are secondary.
    let button = if is_installed || release.stock { button.outline() } else { button.primary() };
    let tag = release.tag.clone();
    let notes = release.notes.clone();
    let notes_view = view.clone();
    // A link under the version it describes: blue and bold so it reads as something to click.
    let notes_link = h_flex()
        .id(("notes", i))
        .gap_1()
        .items_center()
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.link)
        .hover(|s| s.text_color(theme.link_hover))
        .child(if notes_open { tr!("firmware.hide_notes") } else { tr!("firmware.whats_new") })
        .child(Icon::empty().path(if notes_open { "icons/chevron-up.svg" } else { "icons/chevron-down.svg" }).size_3())
        .on_click(move |_, _, cx| {
            let tag = tag.clone();
            let _ = notes_view.update(cx, |this, cx| {
                if !this.open_notes.remove(&tag) {
                    this.open_notes.insert(tag);
                }
                cx.notify();
            });
        });
    let row = h_flex()
        .py_3()
        .gap_4()
        .items_center()
        .child(
            v_flex()
                .flex_1()
                .gap_0p5()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                        .when(is_newest, |el| el.child(badge(theme, tr!("firmware.newest"))))
                        .when(is_installed, |el| el.child(badge(theme, tr!("firmware.installed")))),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(muted(theme, format!("{} · {}", release.date, release.file_name())))
                        .child(notes_link),
                ),
        )
        .child(button.on_click(move |_, _, cx| {
            let source = Source::Release(release.clone());
            let _ = view.update(cx, |this, cx| this.request_install(source, cx));
        }));
    v_flex().border_t_1().border_color(theme.border).child(row).when(notes_open, |el| {
        el.child(div().pb_4().child(if notes.is_empty() {
            muted(theme, tr!("firmware.no_notes")).into_any_element()
        } else {
            TextView::markdown(SharedString::from(format!("notes-{i}")), notes).into_any_element()
        }))
    })
}
