//! Settings cards, one per section, laid out in as many columns as fit (Keys & indicators, Power,
//! and the lower part of Lighting). Wide sections take a row of their own, below the others.

use gpui::{div, prelude::*, px, AnyElement, Context};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::app::Configurator;
use super::controls;
use super::layout::{columns, GAP};
use super::widgets::{card, muted, note};
use crate::settings::{Section, Setting};

/// Narrowest a card gets before cards stack.
const MIN_CARD: f32 = 440.0;

pub fn render(app: &Configurator, sections: &'static [Section], width: f32, cx: &mut Context<Configurator>) -> AnyElement {
    let side_by_side: Vec<&'static Section> = sections.iter().filter(|s| !s.wide).collect();
    // As many columns as fit, but no more than there are sections: one section gets the full width.
    let (fit, _) = columns(width, MIN_CARD, GAP);
    let n = fit.min(side_by_side.len()).max(1);
    let card_width = ((width - GAP * (n - 1) as f32) / n as f32).floor();
    let cards: Vec<AnyElement> = side_by_side.into_iter().map(|section| section_card(app, section, card_width, cx)).collect();
    let wide: Vec<AnyElement> = sections.iter().filter(|s| s.wide).map(|section| section_card(app, section, width, cx)).collect();
    v_flex()
        .gap(px(GAP))
        .child(h_flex().flex_wrap().items_start().gap(px(GAP)).children(cards))
        .children(wide)
        .into_any_element()
}

fn section_card(app: &Configurator, section: &'static Section, width: f32, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let settings = section.settings;
    let mut rows: Vec<AnyElement> = Vec::new();
    for (i, s) in settings.iter().enumerate() {
        if s.inline {
            continue; // drawn in the row before it
        }
        let companion: Option<&'static Setting> = settings.get(i + 1).filter(|next| next.inline);
        if app.model.is_shown(s) {
            // Rows after the first are set apart by a line, as in the design.
            let row = controls::row(app, s, companion, cx);
            rows.push(if rows.is_empty() { row } else { div().border_t_1().border_color(theme.border).child(row).into_any_element() });
        } else if let Some(text) = s.note() {
            rows.push(div().py_3().child(note(&theme, text)).into_any_element());
        }
    }
    let title = h_flex()
        .justify_between()
        .items_center()
        .gap_4()
        .flex_wrap()
        .pb_2()
        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(section.title()))
        .when_some(section.hint(), |el, hint| el.child(muted(&theme, hint)));
    card(&theme).w(px(width)).child(title).children(rows).into_any_element()
}
