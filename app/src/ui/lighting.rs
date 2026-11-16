//! Lighting: a live preview of the current effect, a gallery of every effect playing in miniature
//! (click one to use it), then the lighting settings.

use gpui::{div, prelude::*, AnyElement, Context, Div};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Theme};

use super::app::Configurator;
use super::settings_page;
use super::widgets::{card, card_title, muted};
use crate::preview::{self, effects};
use crate::settings::{self, value, KEY_EFFECTS};

/// Gallery thumbnail size relative to the full preview.
const THUMB_SCALE: f32 = 0.24;
const THUMBS_PER_ROW: usize = 4;

/// The keyboard as it lights up with the current settings.
pub fn live_preview(app: &Configurator, theme: &Theme) -> Div {
    let t = app.clock_ms();
    let keys = effects::key_colours(app.model.byte(value::KEY_EFFECT) as usize, &app.model.key_scene(t));
    let side = effects::side_colours(&app.model.side_scene(t));
    card(theme).items_center().p_4().child(preview::keyboard(1.0, &keys, &side))
}

pub fn render(app: &Configurator, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    v_flex()
        .gap_5()
        .child(live_preview(app, &theme))
        .child(gallery(app, &theme, cx))
        .child(settings_page::render(app, crate::settings::LIGHTING, cx))
        .into_any_element()
}

fn gallery(app: &Configurator, theme: &Theme, cx: &mut Context<Configurator>) -> Div {
    let t = app.clock_ms();
    let scene = app.model.key_scene(t);
    let side = effects::side_colours(&app.model.side_scene(t));
    let current = app.model.byte(value::KEY_EFFECT) as usize;
    let view = cx.entity().downgrade();

    let thumbs: Vec<_> = (1..KEY_EFFECTS.len())
        .map(|effect| {
            let view = view.clone();
            let chosen = effect == current;
            v_flex()
                .id(("effect", effect))
                .flex_1()
                .p_2()
                .gap_1()
                .items_center()
                .rounded_md()
                .border_1()
                .border_color(if chosen { theme.ring } else { theme.border })
                .when(chosen, |el| el.bg(theme.accent))
                .cursor_pointer()
                .hover(|s| s.border_color(theme.ring))
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if let Some(setting) = settings::find(value::KEY_EFFECT) {
                            this.choose(setting, effect, window, cx);
                        }
                    });
                })
                .child(preview::keyboard(THUMB_SCALE, &effects::key_colours(effect, &scene), &side))
                .child(div().text_xs().child(KEY_EFFECTS[effect]))
        })
        .collect();

    let mut rows = Vec::new();
    let mut thumbs = thumbs.into_iter().peekable();
    while thumbs.peek().is_some() {
        let mut row = h_flex().gap_3();
        for _ in 0..THUMBS_PER_ROW {
            row = match thumbs.next() {
                Some(thumb) => row.child(thumb),
                None => row.child(div().flex_1()),
            };
        }
        rows.push(row);
    }

    card(theme)
        .gap_3()
        .child(card_title("Effects"))
        .child(muted(theme, "Shown with your colour and speed. Click one to use it."))
        .children(rows)
}
