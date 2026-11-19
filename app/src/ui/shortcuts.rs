//! Keys & indicators: a card listing what Fn + a key does, from the keyboard's profile, with the
//! F-row lock as it's set now and the macros on their keys.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Theme};

use super::app::Configurator;
use super::layout::{columns, GAP};
use super::widgets::{card, card_title, muted};
use crate::i18n;
use crate::keyboard;
use crate::tr;

/// The F-row order setting (0: F1-F12 first, 1: media first).
const FROW_ORDER: (u8, u8) = (0, 65);
/// Narrowest a group's column gets.
const MIN_COLUMN: f32 = 300.0;

/// Keys as key caps: "Fn + Q … T" -> [Fn] + [Q … T].
fn keys(theme: &Theme, text: &str) -> impl IntoElement {
    let parts: Vec<String> = text.split(" + ").map(str::to_string).collect();
    let last = parts.len() - 1;
    h_flex().flex_none().gap_1().items_center().children(parts.into_iter().enumerate().flat_map(move |(i, part)| {
        let cap = div()
            .px_1p5()
            .py(px(1.))
            .rounded_sm()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .child(SharedString::from(part))
            .into_any_element();
        let plus = (i < last).then(|| div().text_xs().text_color(theme.muted_foreground).child("+").into_any_element());
        std::iter::once(cap).chain(plus)
    }))
}

fn row(theme: &Theme, key_text: &str, what: String) -> AnyElement {
    h_flex()
        .gap_3()
        .py_1p5()
        .items_center()
        .child(div().w(px(150.)).flex_none().child(keys(theme, key_text)))
        .child(div().flex_1().min_w_0().text_sm().child(what))
        .into_any_element()
}

fn group(theme: &Theme, title: String, rows: Vec<AnyElement>, width: f32) -> AnyElement {
    v_flex()
        .w(px(width))
        .gap_0p5()
        .child(div().pb_1().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child(title.to_uppercase()))
        .children(rows)
        .into_any_element()
}

pub fn render(app: &Configurator, width: f32, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let profile = keyboard::current();
    // Inside the card's padding.
    let inner = width - 42.0;
    let (n, _) = columns(inner, MIN_COLUMN, GAP);
    let n = n.clamp(1, 3);
    let column = ((inner - GAP * (n - 1) as f32) / n as f32).floor();

    let mut groups: Vec<AnyElement> = profile
        .fn_shortcuts
        .iter()
        .map(|(name, rows)| {
            let rows = rows.iter().map(|(k, what)| row(&theme, k, i18n::text(&format!("shortcuts.{what}")))).collect();
            group(&theme, i18n::text(&format!("shortcuts.group.{name}")), rows, column)
        })
        .collect();

    // The number row while the F-row lock is on, as set now.
    let media_first = app.model.get(FROW_ORDER).is_some_and(|v| v.first() == Some(&1));
    let (plain, with_fn) = if media_first {
        (tr!("shortcuts.frow_media"), tr!("shortcuts.fkeys"))
    } else {
        (tr!("shortcuts.fkeys"), tr!("shortcuts.frow_media"))
    };
    groups.push(group(
        &theme,
        tr!("shortcuts.group.frow"),
        vec![row(&theme, "1 … =", plain), row(&theme, "Fn + 1 … =", with_fn)],
        column,
    ));

    // Macros on their keys.
    if let Some(m) = app.model.macros() {
        let names = crate::prefs::Prefs::load().macro_names;
        let rows: Vec<AnyElement> = m
            .keys
            .iter()
            .enumerate()
            .filter_map(|(n, key)| {
                let key = (*key)?;
                let name = names.get(n).filter(|s| !s.trim().is_empty()).cloned().unwrap_or_else(|| tr!("macros.default_name", number = n + 1));
                Some(row(&theme, &format!("Fn + {}", profile.keys[key].label), name))
            })
            .collect();
        if !rows.is_empty() {
            groups.push(group(&theme, tr!("shortcuts.group.macros"), rows, column));
        }
    }

    card(&theme)
        .w(px(width))
        .gap_3()
        .child(h_flex().justify_between().items_baseline().flex_wrap().gap_4().child(card_title(tr!("shortcuts.title"))).child(muted(&theme, tr!("shortcuts.hint"))))
        .child(h_flex().flex_wrap().items_start().gap_x(px(GAP)).gap_y_5().children(groups))
        .into_any_element()
}
