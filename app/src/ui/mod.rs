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
mod controls;
mod firmware;
mod layout;
mod lighting;
mod model;
mod overview;
mod page;
mod settings_page;
mod sidebar;
mod widgets;

use gpui::{prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use gpui_component::{Root, Theme};

/// Open the window, on `page` (or the Overview).
pub fn run(page: Option<&str>) {
    let page = page.and_then(page::Page::from_name).unwrap_or(page::Page::Overview);
    let app = gpui::Application::with_platform(gpui_platform::current_platform(false)).with_assets(gpui_kit_assets::AllAssets);
    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        // 1180 × 800, or 90 % of a smaller screen.
        let screen = cx.primary_display().map(|d| d.bounds().size);
        let fit = |want: f32, have: Option<f32>| px(have.map_or(want, |h| want.min(h * 0.9)));
        let window_size = size(
            fit(1180., screen.map(|s| f32::from(s.width))),
            fit(800., screen.map(|s| f32::from(s.height))),
        );
        let bounds = Bounds::centered(None, window_size, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("RK71 Configurator".into()), ..Default::default() }),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            // Light or dark like the system, and follow it when it changes.
            Theme::sync_system_appearance(Some(window), cx);
            window.observe_window_appearance(|window, cx| Theme::sync_system_appearance(Some(window), cx)).detach();
            let view = cx.new(|cx| app::Configurator::new(page, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("could not open the window");
        cx.activate(true);
    });
}
