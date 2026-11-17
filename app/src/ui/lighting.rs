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
use crate::settings::{self, effect_name, value, COLOR_EFFECTS, LIGHTING, MULTICOLOR_EFFECTS, TYPING_EFFECTS};
use crate::tr;

/// The two kinds of key effect.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectGroup {
    #[default]
    YourColor,
    Multicolor,
}

impl EffectGroup {
    const ALL: [EffectGroup; 2] = [EffectGroup::YourColor, EffectGroup::Multicolor];

    pub fn of(effect: u8) -> Self {
        if MULTICOLOR_EFFECTS.contains(&effect) { EffectGroup::Multicolor } else { EffectGroup::YourColor }
    }

    fn effects(self) -> &'static [u8] {
        match self {
            EffectGroup::YourColor => COLOR_EFFECTS,
            EffectGroup::Multicolor => MULTICOLOR_EFFECTS,
        }
    }

    fn key(self) -> &'static str {
        match self {
            EffectGroup::YourColor => "your_color",
            EffectGroup::Multicolor => "multicolor",
        }
    }

    fn label(self) -> String {
        crate::i18n::text(&format!("lighting.group.{}", self.key()))
    }

    fn hint(self) -> String {
        crate::i18n::text(&format!("lighting.group.{}_hint", self.key()))
    }

    fn icon(self) -> Icon {
        Icon::empty().path(match self {
            EffectGroup::YourColor => "icons/palette.svg",
            EffectGroup::Multicolor => "icons/rainbow.svg",
        })
    }
}

/// Smallest gallery thumbnail.
const MIN_THUMB: f32 = 120.0;
const THUMB_GAP: f32 = 10.0;
/// Card padding (p_5) and border on both sides, and the preview card's (p_4).
const CARD_PAD: f32 = 2.0 * (20.0 + 1.0);
const PREVIEW_PAD: f32 = 2.0 * (16.0 + 1.0);
/// Thumbnail padding (p_2) and border on both sides.
const THUMB_PAD: f32 = 2.0 * (8.0 + 1.0);

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
    let keys = effects::key_colors(effect as usize, &app.model.key_scene(t));
    let side = effects::side_colors(&app.model.side_scene(t));
    let scale = app.layout.preview_scale(width - PREVIEW_PAD);
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
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(effect_name(effect as usize)))
                    .child(div().text_color(theme.muted_foreground).child(if effect == 0 {
                        tr!("lighting.lighting_off")
                    } else {
                        format!("· {}", EffectGroup::of(effect).label())
                    })),
            )
        })
}

fn effect_picker(app: &Configurator, theme: &Theme, width: f32, cx: &mut Context<Configurator>) -> Div {
    let t = app.clock_ms();
    let scene = app.model.key_scene(t);
    let side = effects::side_colors(&app.model.side_scene(t));
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
    let thumb_scale = keyboard_scale(thumb_width - THUMB_PAD, 1.0);
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
            .child(preview::keyboard(thumb_scale, &effects::key_colors(effect as usize, &scene), &side))
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(div().flex_1().min_w_0().truncate().text_xs().font_weight(FontWeight::MEDIUM).child(effect_name(effect as usize)))
                    .when(TYPING_EFFECTS.contains(&effect), |el| el.child(badge(theme, tr!("lighting.typing")))),
            )
    });

    card(theme)
        .w(px(width))
        .gap_3()
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .gap_3()
                .child(card_title(tr!("lighting.effects")))
                .child(muted(theme, tr!("lighting.effects_hint")).truncate()),
        )
        .child(tabs)
        .child(muted(theme, group.hint()))
        .child(h_flex().w_full().flex_wrap().gap(px(THUMB_GAP)).children(thumbs.collect::<Vec<_>>()))
}
