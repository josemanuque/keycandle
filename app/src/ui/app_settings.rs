//! Settings: the app's own preferences (language, appearance). The keyboard's settings are on the
//! other pages.

use gpui::{div, prelude::*, px, AnyElement, Context};
use gpui_component::{h_flex, ActiveTheme as _, Theme};

use super::app::Configurator;
use super::appearance::Appearance;
use super::layout::{columns, GAP};

/// Narrowest a card gets before the two stack.
const MIN_CARD: f32 = 440.0;
use super::widgets::{card, card_title, muted, segment, segmented};
use crate::{i18n, tr};

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    // Side by side when there's room, like the settings pages.
    let (n, _) = columns(app.layout.width, MIN_CARD, GAP);
    let n = n.min(2);
    let card_width = ((app.layout.width - GAP * (n - 1) as f32) / n as f32).floor();
    h_flex()
        .flex_wrap()
        .items_start()
        .gap(px(GAP))
        .child(div().w(px(app.layout.width)).child(super::updates::settings_card(app, &theme, cx)))
        .child(div().w(px(card_width)).child(language(&theme, cx)))
        .child(div().w(px(card_width)).child(appearance(&theme, cx)))
        .child(div().w(px(card_width)).child(slider_values(app, &theme, cx)))
        .into_any_element()
}

fn slider_values(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> impl IntoElement {
    let view = cx.entity().downgrade();
    card(theme)
        .gap_3()
        .child(card_title(tr!("settings.slider_values")))
        .child(segmented(theme).children([(false, tr!("settings.slider_percent")), (true, tr!("settings.slider_raw"))].into_iter().map(
            |(raw, label)| {
                let view = view.clone();
                segment(theme, ("slider-values", raw as usize), app.raw_values == raw)
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| this.set_raw_values(raw, cx));
                    })
                    .child(label)
            },
        )))
        .child(muted(theme, tr!("settings.slider_values_help")))
}

fn language(theme: &Theme, cx: &mut Context<Configurator>) -> impl IntoElement {
    let current = i18n::current();
    let view = cx.entity().downgrade();
    card(theme)
        .gap_3()
        .child(card_title(tr!("settings.language")))
        .child(segmented(theme).children(i18n::LANGUAGES.iter().enumerate().map(|(i, &(code, name))| {
            let view = view.clone();
            segment(theme, ("lang", i), code == current)
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.set_language(code, window, cx));
                })
                .child(name)
        })))
        .child(muted(theme, tr!("settings.language_help")))
}

fn appearance(theme: &Theme, cx: &mut Context<Configurator>) -> impl IntoElement {
    let current = Appearance::saved();
    let view = cx.entity().downgrade();
    card(theme)
        .gap_3()
        .child(card_title(tr!("settings.appearance")))
        .child(segmented(theme).children(Appearance::ALL.iter().enumerate().map(|(i, &choice)| {
            let view = view.clone();
            segment(theme, ("appearance", i), choice == current)
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |_, cx| choice.choose(window, cx));
                })
                .child(choice.label())
        })))
        .child(muted(theme, tr!("settings.appearance_help")))
}
