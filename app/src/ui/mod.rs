//! The app window.
//!
//! - `app`: root view (owns the model and controls; connection and animation loops)
//! - `model`: the keyboard and its values, no UI
//! - `page`: the sections in the sidebar
//! - `sidebar`, `overview`, `lighting`, `settings_page`, `firmware`: what each part draws
//! - `layout`: responsive widths (columns, preview scale)
//! - `controls`: the control and row for each setting
//! - `widgets`: shared building blocks

mod app;
mod app_settings;
mod appearance;
mod chrome;
mod controls;
mod firmware;
mod layout;
mod lighting;
mod menu;
pub mod model;
mod overview;
mod page;
mod power;
mod settings_page;
mod sidebar;
mod widgets;

use gpui::{prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use gpui_component::{Root, Theme, ThemeRegistry};

use page::Page;

/// Open the window, on `page` (or the Overview).
pub fn run(page: Option<&str>) {
    let page = page.and_then(Page::from_name).unwrap_or(Page::Overview);
    let app = gpui::Application::with_platform(gpui_platform::current_platform(false)).with_assets(gpui_kit_assets::AllAssets);
    // macOS: closing the window keeps the app in the Dock; clicking it there opens the window again.
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            open_window(Page::Overview, cx);
        }
    });
    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        install_theme(cx);
        menu::init(cx);
        // Elsewhere, closing the only window quits.
        #[cfg(not(target_os = "macos"))]
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        open_window(page, cx);
    });
}

/// Open the main window on `page` and bring the app to the front.
fn open_window(page: Page, cx: &mut App) {
    let backdrop = chrome::Backdrop::detect();
    let bounds = Bounds::centered(None, size(px(1180.), px(800.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(640.), px(480.))),
        titlebar: Some(TitlebarOptions {
            title: Some(crate::tr!("app.name").into()),
            appears_transparent: backdrop.is_translucent(),
            traffic_light_position: backdrop.traffic_lights(),
        }),
        window_background: backdrop.window_background(),
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        window.on_window_should_close(cx, menu::may_close);
        // Light or dark: the user's choice, or like the system (and follow it when it changes).
        appearance::apply(window, cx);
        window.observe_window_appearance(|window, cx| appearance::apply(window, cx)).detach();
        let view = cx.new(|cx| app::Configurator::new(page, backdrop, window, cx));
        cx.set_global(menu::Main { window: window.window_handle(), view: view.downgrade() });
        cx.new(|cx| Root::new(view, window, cx))
    })
    .expect("could not open the window");
    cx.activate(true);
}

/// The design's colors (assets/theme.json), as the light and dark theme; the system's appearance
/// picks between them.
fn install_theme(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    registry.load_themes_from_str(include_str!("../../assets/theme.json")).expect("assets/theme.json is valid");
    let light = registry.themes().get("RK71 Light").cloned();
    let dark = registry.themes().get("RK71 Dark").cloned();
    Theme::update(cx, |theme| {
        if let Some(light) = light {
            theme.light_theme = light;
        }
        if let Some(dark) = dark {
            theme.dark_theme = dark;
        }
    });
}
