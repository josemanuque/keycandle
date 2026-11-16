//! Small building blocks shared by the pages.

use gpui::{div, prelude::*, Div, FontWeight, SharedString};
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
