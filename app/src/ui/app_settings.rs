//! Settings: the app's own preferences (language, appearance). The keyboard's settings are on the
//! other pages.

use gpui::{prelude::*, px, AnyElement, Context};
use gpui_component::{v_flex, ActiveTheme as _, Theme};

use super::app::Configurator;
use super::appearance::Appearance;
use super::layout::GAP;
use super::widgets::{card, card_title, muted, segment, segmented};
use crate::{i18n, tr};

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let width = app.layout.width.min(720.0);
    v_flex()
        .w(px(width))
        .gap(px(GAP))
        .child(language(&theme, cx))
        .child(appearance(&theme, cx))
        .into_any_element()
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
}
