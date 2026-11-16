//! A page of settings cards, one per section (Keys & indicators, Power, and the lower part of
//! Lighting).

use gpui::{prelude::*, AnyElement, Context};
use gpui_component::{v_flex, ActiveTheme as _};

use super::app::Configurator;
use super::controls;
use super::widgets::{card, card_title};
use crate::settings::Section;

pub fn render(app: &Configurator, sections: &'static [Section], cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    v_flex()
        .gap_5()
        .children(sections.iter().map(|section| {
            card(&theme).child(card_title(section.title)).children(
                section
                    .settings
                    .iter()
                    .filter(|s| app.model.is_shown(s))
                    .map(|s| controls::row(app, s, cx)),
            )
        }))
        .into_any_element()
}
