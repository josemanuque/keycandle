//! The app window.
//!
//! - `app`: root view (owns the model and controls; connection and animation loops)
//! - `model`: the keyboard and its values, no UI
//! - `page`: the sections in the sidebar
//! - `sidebar`, `overview`, `lighting`, `settings_page`: what each part draws
//! - `controls`: the control and row for each setting
//! - `widgets`: shared building blocks

mod app;
mod controls;
mod lighting;
mod model;
mod overview;
mod page;
mod settings_page;
mod sidebar;
mod widgets;

use gpui::{prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use gpui_component::{Root, Theme};

pub fn run() {
    let app = gpui::Application::with_platform(gpui_platform::current_platform(false)).with_assets(gpui_kit_assets::AllAssets);
    app.run(|cx: &mut App| {
        gpui_component::init(cx);
        let bounds = Bounds::centered(None, size(px(1180.), px(800.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("RK71 Configurator".into()), ..Default::default() }),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            // Light or dark like the system, and follow it when it changes.
            Theme::sync_system_appearance(Some(window), cx);
            window.observe_window_appearance(|window, cx| Theme::sync_system_appearance(Some(window), cx)).detach();
            let view = cx.new(|cx| app::Configurator::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("could not open the window");
        cx.activate(true);
    });
}
