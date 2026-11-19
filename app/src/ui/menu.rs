//! The menu bar (macOS), the keyboard shortcuts, and quitting and closing safely.
//!
//! Actions are handled app-wide, so they work whatever has focus, and the page actions open the
//! window again when it was closed.

use gpui::{actions, AnyWindowHandle, App, Context, Global, KeyBinding, Menu, MenuItem, PromptLevel, SystemMenuType, WeakEntity, Window};

use super::app::Configurator;
use super::appearance::Appearance;
use super::chrome;
use super::page::Page;
use crate::{tr, updater};

const REPO: &str = "https://github.com/josemanuque/keycandle";

actions!(
    keycandle,
    [
        About,
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        CloseWindow,
        Minimize,
        Zoom,
        ToggleFullScreen,
        ShowOverview,
        ShowLighting,
        ShowKeys,
        ShowMacros,
        ShowPower,
        ShowFirmware,
        ShowSettings,
        Reconnect,
        CheckForUpdates,
        AppearanceSystem,
        AppearanceLight,
        AppearanceDark,
        OpenGuide,
        OpenReleases,
        ReportIssue,
        FocusNext,
        FocusPrevious,
    ]
);

/// The open main window and its view (there's at most one).
pub struct Main {
    pub window: AnyWindowHandle,
    pub view: WeakEntity<Configurator>,
}

impl Global for Main {}

pub fn init(cx: &mut App) {
    // Cmd on macOS, Ctrl elsewhere.
    macro_rules! key {
        ($k:literal) => {
            if cfg!(target_os = "macos") { concat!("cmd-", $k) } else { concat!("ctrl-", $k) }
        };
    }
    let mut keys = vec![
        KeyBinding::new(key!("q"), Quit, None),
        KeyBinding::new(key!("w"), CloseWindow, None),
        KeyBinding::new(key!(","), ShowSettings, None),
        KeyBinding::new(key!("1"), ShowOverview, None),
        KeyBinding::new(key!("2"), ShowLighting, None),
        KeyBinding::new(key!("3"), ShowKeys, None),
        KeyBinding::new(key!("4"), ShowMacros, None),
        KeyBinding::new(key!("5"), ShowPower, None),
        KeyBinding::new(key!("6"), ShowFirmware, None),
        KeyBinding::new(key!("r"), Reconnect, None),
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrevious, None),
    ];
    if cfg!(target_os = "macos") {
        keys.extend([
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("alt-cmd-h", HideOthers, None),
            KeyBinding::new("cmd-m", Minimize, None),
            KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
        ]);
    }
    cx.bind_keys(keys);

    cx.on_action(|_: &FocusNext, cx| with_active_window(cx, |window, cx| window.focus_next(cx)));
    cx.on_action(|_: &FocusPrevious, cx| with_active_window(cx, |window, cx| window.focus_prev(cx)));
    cx.on_action(|_: &Quit, cx| quit(cx));
    cx.on_action(|_: &About, _| chrome::about());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &CloseWindow, cx| {
        with_active_window(cx, |window, cx| {
            if may_close(window, cx) {
                window.remove_window();
            }
        })
    });
    cx.on_action(|_: &Minimize, cx| with_active_window(cx, |window, _| window.minimize_window()));
    cx.on_action(|_: &Zoom, cx| with_active_window(cx, |window, _| window.zoom_window()));
    cx.on_action(|_: &ToggleFullScreen, cx| with_active_window(cx, |window, _| window.toggle_fullscreen()));
    show::<ShowOverview>(cx, Page::Overview);
    show::<ShowLighting>(cx, Page::Lighting);
    show::<ShowKeys>(cx, Page::Keys);
    show::<ShowMacros>(cx, Page::Macros);
    show::<ShowPower>(cx, Page::Power);
    show::<ShowFirmware>(cx, Page::Firmware);
    show::<ShowSettings>(cx, Page::Settings);
    cx.on_action(|_: &Reconnect, cx| with_main(cx, |this, window, cx| this.reconnect(window, cx)));
    cx.on_action(|_: &CheckForUpdates, cx| with_main(cx, |this, _, cx| this.check_for_updates(cx)));
    cx.on_action(|_: &AppearanceSystem, cx| choose(Appearance::System, cx));
    cx.on_action(|_: &AppearanceLight, cx| choose(Appearance::Light, cx));
    cx.on_action(|_: &AppearanceDark, cx| choose(Appearance::Dark, cx));
    cx.on_action(|_: &OpenGuide, cx| cx.open_url(&format!("{REPO}#readme")));
    cx.on_action(|_: &OpenReleases, cx| cx.open_url(&format!("{REPO}/releases")));
    cx.on_action(|_: &ReportIssue, cx| cx.open_url(&format!("{REPO}/issues/new")));

    set_menus(cx);
}

/// Register the action `A` as "open `page`".
fn show<A: gpui::Action + Default>(cx: &mut App, page: Page) {
    cx.on_action(move |_: &A, cx| with_main(cx, move |this, _, cx| this.open(page, cx)));
}

fn choose(choice: Appearance, cx: &mut App) {
    with_main(cx, move |_, window, cx| choice.choose(window, cx));
}

/// Build the menu bar; again whenever the language or the appearance changes.
pub fn set_menus(cx: &mut App) {
    let appearance = Appearance::saved();
    cx.set_menus([
        Menu::new(tr!("app.name")).items([
            MenuItem::action(tr!("menu.about"), About),
            MenuItem::separator(),
            MenuItem::action(tr!("menu.settings"), ShowSettings),
            MenuItem::separator(),
            MenuItem::os_submenu(tr!("menu.services"), SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(tr!("menu.hide"), Hide),
            MenuItem::action(tr!("menu.hide_others"), HideOthers),
            MenuItem::action(tr!("menu.show_all"), ShowAll),
            MenuItem::separator(),
            MenuItem::action(tr!("menu.quit"), Quit),
        ]),
        Menu::new(tr!("menu.view")).items([
            MenuItem::action(Page::Overview.title(), ShowOverview),
            MenuItem::action(Page::Lighting.title(), ShowLighting),
            MenuItem::action(Page::Keys.title(), ShowKeys),
            MenuItem::action(Page::Macros.title(), ShowMacros),
            MenuItem::action(Page::Power.title(), ShowPower),
            MenuItem::action(Page::Firmware.title(), ShowFirmware),
            MenuItem::separator(),
            MenuItem::submenu(Menu::new(tr!("settings.appearance")).items(Appearance::ALL.map(|choice| {
                let item = match choice {
                    Appearance::System => MenuItem::action(choice.label(), AppearanceSystem),
                    Appearance::Light => MenuItem::action(choice.label(), AppearanceLight),
                    Appearance::Dark => MenuItem::action(choice.label(), AppearanceDark),
                };
                item.checked(choice == appearance)
            }))),
            MenuItem::separator(),
            MenuItem::action(tr!("menu.full_screen"), ToggleFullScreen),
        ]),
        Menu::new(tr!("menu.keyboard")).items([
            MenuItem::action(tr!("menu.reconnect"), Reconnect),
            MenuItem::separator(),
            MenuItem::action(tr!("menu.check_updates"), CheckForUpdates),
        ]),
        Menu::new(tr!("menu.window")).items([
            MenuItem::action(tr!("menu.minimize"), Minimize),
            MenuItem::action(tr!("menu.zoom"), Zoom),
            MenuItem::separator(),
            MenuItem::action(tr!("menu.close"), CloseWindow),
        ]),
        Menu::new(tr!("menu.help")).items([
            MenuItem::action(tr!("menu.guide"), OpenGuide),
            MenuItem::action(tr!("menu.releases"), OpenReleases),
            MenuItem::separator(),
            MenuItem::action(tr!("menu.report_issue"), ReportIssue),
        ]),
    ]);
}

/// Quit, unless firmware is being written: then say why not.
fn quit(cx: &mut App) {
    if !updater::flashing() {
        cx.quit();
        return;
    }
    with_active_window(cx, |window, cx| {
        window.activate_window();
        explain_busy(window, cx);
    });
}

/// Whether the window may close now (the close button, Cmd+W). Not while firmware is being
/// written: the install runs in this window and would be lost.
pub fn may_close(window: &mut Window, cx: &mut App) -> bool {
    if updater::flashing() {
        explain_busy(window, cx);
        return false;
    }
    true
}

fn explain_busy(window: &mut Window, cx: &mut App) {
    let ok = tr!("busy.ok");
    let _ = window.prompt(PromptLevel::Warning, &tr!("busy.title"), Some(&tr!("busy.detail")), &[ok.as_str()], cx);
}

// Menu actions arrive while GPUI is updating the active window, which can't be updated again from
// inside (the call fails silently). And while a menu action is handled, anything that spins the
// run loop (hidapi does, looking for the keyboard) lets the app's other tasks run and borrow the
// app a second time, which panics. So the work runs as a task of its own, after the menu is done.

fn later(cx: &mut App, f: impl FnOnce(&mut App) + 'static) {
    cx.spawn(async move |cx| {
        let _ = cx.update(f);
    })
    .detach();
}

fn with_active_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    later(cx, move |cx| {
        let window = cx.active_window().or_else(|| cx.try_global::<Main>().map(|m| m.window));
        if let Some(window) = window {
            let _ = window.update(cx, |_, window, cx| f(window, cx));
        }
    });
}

/// Run `f` on the main view, opening the window first if it was closed.
fn with_main(cx: &mut App, f: impl FnOnce(&mut Configurator, &mut Window, &mut Context<Configurator>) + 'static) {
    later(cx, move |cx| {
        let open = cx.try_global::<Main>().filter(|m| m.view.upgrade().is_some()).map(|m| (m.window, m.view.clone()));
        let (window, view) = match open {
            Some(main) => main,
            None => {
                super::open_window(Page::Overview, cx);
                let Some(main) = cx.try_global::<Main>() else { return };
                (main.window, main.view.clone())
            }
        };
        let _ = window.update(cx, |_, window, cx| {
            window.activate_window();
            let _ = view.update(cx, |this, cx| f(this, window, cx));
        });
    });
}
