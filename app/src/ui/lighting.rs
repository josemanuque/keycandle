//! Lighting: a live preview of the current effect, the effect picker (two groups of animated
//! thumbnails), and the lighting settings.
//!
//! Wide windows: preview and settings on the left, the effect picker beside them. Narrow
//! windows: one column, the picker right under the preview.

use gpui::{div, prelude::*, px, AnyElement, Context, Div, FontWeight};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Theme};

use super::app::Configurator;
use super::layout::{columns, keyboard_scale, GAP};
use super::settings_page;
use super::widgets::{badge, card, card_title, muted, segment, segmented};
use crate::preview::{self, effects};
use crate::settings::{self, value, COLOUR_EFFECTS, KEY_EFFECTS, LIGHTING, MULTICOLOUR_EFFECTS, TYPING_EFFECTS};

/// The two kinds of key effect.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectGroup {
    #[default]
    YourColour,
    Multicolour,
}

impl EffectGroup {
    const ALL: [EffectGroup; 2] = [EffectGroup::YourColour, EffectGroup::Multicolour];

    pub fn of(effect: u8) -> Self {
        if MULTICOLOUR_EFFECTS.contains(&effect) { EffectGroup::Multicolour } else { EffectGroup::YourColour }
    }

    fn effects(self) -> &'static [u8] {
        match self {
            EffectGroup::YourColour => COLOUR_EFFECTS,
            EffectGroup::Multicolour => MULTICOLOUR_EFFECTS,
        }
    }

    fn label(self) -> &'static str {
        match self {
            EffectGroup::YourColour => "Your colour",
            EffectGroup::Multicolour => "Multicolour",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            EffectGroup::YourColour => "These use the colour you pick: solid, or shades and waves of it.",
            EffectGroup::Multicolour => "These make their own colours (rainbows, rain, heat), so the colour isn't used.",
        }
    }

    fn icon(self) -> Icon {
        Icon::empty().path(match self {
            EffectGroup::YourColour => "icons/palette.svg",
            EffectGroup::Multicolour => "icons/rainbow.svg",
        })
    }
}

/// Smallest gallery thumbnail.
const MIN_THUMB: f32 = 150.0;
const THUMB_GAP: f32 = 10.0;
/// Card padding (p_5) on both sides, and the preview card's (p_4).
const CARD_PAD: f32 = 2.0 * 20.0;
const PREVIEW_PAD: f32 = 2.0 * 16.0;

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let layout = app.layout;
    if layout.is_wide() {
        let (left, right) = layout.split(0.58);
        h_flex()
            .items_start()
            .gap(px(GAP))
            .child(
                v_flex()
                    .w(px(left))
                    .gap(px(GAP))
                    .child(live_preview(app, &theme, left, true))
                    .child(settings_page::render(app, LIGHTING, left, cx)),
            )
            .child(effect_picker(app, &theme, right, cx))
            .into_any_element()
    } else {
        let width = layout.width;
        v_flex()
            .gap(px(GAP))
            .child(live_preview(app, &theme, width, true))
            .child(effect_picker(app, &theme, width, cx))
            .child(settings_page::render(app, LIGHTING, width, cx))
            .into_any_element()
    }
}

/// The keyboard as it lights up with the current settings, `width` px wide.
pub fn live_preview(app: &Configurator, theme: &Theme, width: f32, caption: bool) -> Div {
    let t = app.clock_ms();
    let effect = app.model.byte(value::KEY_EFFECT);
    let keys = effects::key_colours(effect as usize, &app.model.key_scene(t));
    let side = effects::side_colours(&app.model.side_scene(t));
    let scale = keyboard_scale(width - PREVIEW_PAD, 1.4);
    card(theme)
        .w(px(width))
        .items_center()
        .p_4()
        .gap_2()
        .child(preview::keyboard(scale, &keys, &side))
        .when(caption, |el| {
            el.child(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(KEY_EFFECTS.get(effect as usize).copied().unwrap_or("?")))
                    .child(div().text_color(theme.muted_foreground).child(if effect == 0 {
                        "Lighting off".to_string()
                    } else {
                        format!("· {}", EffectGroup::of(effect).label())
                    })),
            )
        })
}

fn effect_picker(app: &Configurator, theme: &Theme, width: f32, cx: &mut Context<Configurator>) -> Div {
    let t = app.clock_ms();
    let scene = app.model.key_scene(t);
    let side = effects::side_colours(&app.model.side_scene(t));
    let current = app.model.byte(value::KEY_EFFECT);
    let group = app.effect_group;
    let view = cx.entity().downgrade();

    let tabs = segmented(theme).children(EffectGroup::ALL.map(|g| {
        let view = view.clone();
        segment(theme, ("group", g as usize), g == group)
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.effect_group = g;
                    cx.notify();
                });
            })
            .child(g.icon().size_4())
            .child(g.label())
            .child(div().text_xs().opacity(0.7).child(g.effects().len().to_string()))
    }));

    let (_, thumb_width) = columns(width - CARD_PAD, MIN_THUMB, THUMB_GAP);
    let thumb_scale = keyboard_scale(thumb_width - 16.0, 1.0);
    let thumbs = group.effects().iter().map(|&effect| {
        let view = view.clone();
        let chosen = effect == current;
        v_flex()
            .id(("effect", effect as usize))
            .w(px(thumb_width))
            .p_2()
            .gap_1p5()
            .rounded_md()
            .border_1()
            .border_color(if chosen { theme.ring } else { theme.border })
            .bg(if chosen { theme.accent } else { theme.background })
            .cursor_pointer()
            .hover(|s| s.border_color(theme.ring))
            .on_click(move |_, window, cx| {
                let _ = view.update(cx, |this, cx| this.choose(&settings::KEY_EFFECT, effect as usize, window, cx));
            })
            .child(preview::keyboard(thumb_scale, &effects::key_colours(effect as usize, &scene), &side))
            .child(
                h_flex()
                    .justify_between()
                    .gap_1()
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).child(KEY_EFFECTS[effect as usize]))
                    .when(TYPING_EFFECTS.contains(&effect), |el| el.child(badge(theme, "Typing"))),
            )
    });

    card(theme)
        .w(px(width))
        .gap_3()
        .child(h_flex().justify_between().items_center().child(card_title("Effects")).child(muted(theme, "Shown with your colour and speed")))
        .child(tabs)
        .child(muted(theme, group.hint()))
        .child(h_flex().flex_wrap().gap(px(THUMB_GAP)).children(thumbs.collect::<Vec<_>>()))
}
