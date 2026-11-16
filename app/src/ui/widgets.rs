//! Small building blocks shared by the pages.

use gpui::{div, prelude::*, px, Div, ElementId, FontWeight, SharedString, Stateful};
use gpui_component::{v_flex, Theme};

/// A raised panel that groups related content.
pub fn card(theme: &Theme) -> Div {
    v_flex().p_5().gap_1().rounded_lg().bg(theme.secondary).border_1().border_color(theme.border)
}

/// A card's heading.
pub fn card_title(text: impl Into<SharedString>) -> Div {
    div().pb_2().font_weight(FontWeight::SEMIBOLD).child(text.into())
}

/// Secondary text.
pub fn muted(theme: &Theme, text: impl Into<SharedString>) -> Div {
    div().text_xs().text_color(theme.muted_foreground).child(text.into())
}

/// An explanation in a dashed box (why something isn't shown or does nothing).
pub fn note(theme: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_dashed()
        .border_color(theme.border)
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(text.into())
}

/// A small label next to something (e.g. "Typing", "Installed").
pub fn badge(theme: &Theme, text: impl Into<SharedString>) -> Div {
    div().flex_none().px_1p5().py(px(1.)).rounded_sm().bg(theme.accent).text_xs().text_color(theme.accent_foreground).child(text.into())
}

/// One option of a segmented control: a clickable pill, highlighted when chosen.
pub fn segment(theme: &Theme, id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .text_sm()
        .when(selected, |el| el.bg(theme.background).font_weight(FontWeight::SEMIBOLD))
        .when(!selected, |el| el.text_color(theme.muted_foreground).hover(|s| s.text_color(theme.foreground)))
}
