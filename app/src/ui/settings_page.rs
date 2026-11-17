//! Settings cards, one per section, laid out in as many columns as fit (Keys & indicators, Power,
//! and the lower part of Lighting).

use gpui::{div, prelude::*, px, AnyElement, Context};
use gpui_component::{h_flex, ActiveTheme as _};

use super::app::Configurator;
use super::controls;
use super::layout::{columns, GAP};
use super::widgets::{card, card_title, note};
use crate::settings::Section;

/// Narrowest a card gets before cards stack.
const MIN_CARD: f32 = 440.0;

pub fn render(app: &Configurator, sections: &'static [Section], width: f32, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    // As many columns as fit, but no more than there are sections: one section gets the full width.
    let (fit, _) = columns(width, MIN_CARD, GAP);
    let n = fit.min(sections.len()).max(1);
    let card_width = ((width - GAP * (n - 1) as f32) / n as f32).floor();
    h_flex()
        .flex_wrap()
        .items_start()
        .gap(px(GAP))
        .children(sections.iter().map(|section| {
            let rows = section.settings.iter().filter_map(|s| {
                if app.model.is_shown(s) {
                    Some(controls::row(app, s, cx))
                } else {
                    s.note().map(|text| div().py_3().child(note(&theme, text)).into_any_element())
                }
            });
            card(&theme).w(px(card_width)).child(card_title(section.title())).children(rows.collect::<Vec<_>>())
        }))
        .into_any_element()
}
