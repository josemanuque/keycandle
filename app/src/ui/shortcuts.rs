//! What Fn + a key does: a small card on Keys & indicators, and the full list in a dialog (also from
//! Help ▸ Keyboard shortcuts). From the keyboard's profile, with the F-row lock as it's set now and
//! the macros on their keys.

use gpui::{div, prelude::*, px, AnyElement, App, Context, FontWeight, SharedString, Window};
use gpui_component::{
    button::Button,
    h_flex, v_flex, ActiveTheme as _, Theme, WindowExt as _,
};

use super::app::Configurator;
use super::layout::GAP;
use super::widgets::{card, muted};
use crate::i18n;
use crate::keyboard;
use crate::tr;

/// The F-row order setting (0: F1-F12 first, 1: media first).
const FROW_ORDER: (u8, u8) = (0, 65);
/// The dialog's width, and its columns'.
const DIALOG_WIDTH: f32 = 920.0;
const COLUMNS: usize = 2;

/// A group of shortcuts: its title and (keys, what they do).
type Group = (String, Vec<(String, String)>);

/// Every group, as set now on the keyboard.
fn groups(app: &Configurator) -> Vec<Group> {
    let profile = keyboard::current();
    let mut groups: Vec<Group> = profile
        .fn_shortcuts
        .iter()
        .map(|(name, rows)| {
            let rows = rows.iter().map(|(k, what)| (k.to_string(), i18n::text(&format!("shortcuts.{what}")))).collect();
            (i18n::text(&format!("shortcuts.group.{name}")), rows)
        })
        .collect();

    // The number row while the F-row lock is on, as set now.
    let media_first = app.model.get(FROW_ORDER).is_some_and(|v| v.first() == Some(&1));
    let (plain, with_fn) =
        if media_first { (tr!("shortcuts.frow_media"), tr!("shortcuts.fkeys")) } else { (tr!("shortcuts.fkeys"), tr!("shortcuts.frow_media")) };
    groups.push((tr!("shortcuts.group.frow"), vec![("1 … =".into(), plain), ("Fn + 1 … =".into(), with_fn)]));

    // Macros on their keys.
    if let Some(m) = app.model.macros() {
        let names = crate::prefs::Prefs::load().macro_names;
        let rows: Vec<(String, String)> = m
            .keys
            .iter()
            .enumerate()
            .filter_map(|(n, key)| {
                let key = (*key)?;
                let name = names.get(n).filter(|s| !s.trim().is_empty()).cloned().unwrap_or_else(|| tr!("macros.default_name", number = n + 1));
                Some((format!("Fn + {}", profile.keys[key].label), name))
            })
            .collect();
        if !rows.is_empty() {
            groups.push((tr!("shortcuts.group.macros"), rows));
        }
    }
    groups
}

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

/// The groups in columns of `width` each.
fn list(theme: &Theme, groups: &[Group], column: f32) -> impl IntoElement {
    h_flex().flex_wrap().items_start().gap_x(px(GAP)).gap_y_5().children(groups.iter().map(|(title, rows)| {
        v_flex()
            .w(px(column))
            .gap_0p5()
            .child(div().pb_1().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child(title.to_uppercase()))
            .children(rows.iter().map(|(k, what)| {
                h_flex()
                    .gap_3()
                    .py_1()
                    .items_center()
                    .child(div().w(px(150.)).flex_none().child(keys(theme, k)))
                    .child(div().flex_1().min_w_0().text_sm().child(what.clone()))
            }))
    }))
}

/// Open the full list in a dialog.
pub fn open(app: &Configurator, window: &mut Window, cx: &mut App) {
    let groups = groups(app);
    window.open_dialog(cx, move |dialog, _, cx| {
        let theme = cx.theme().clone();
        let inner = DIALOG_WIDTH - 48.0;
        let column = ((inner - GAP * (COLUMNS - 1) as f32) / COLUMNS as f32).floor();
        dialog
            .w(px(DIALOG_WIDTH))
            .overlay_closable(true)
            .title(
                v_flex()
                    .gap_0p5()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(tr!("shortcuts.title")))
                    .child(muted(&theme, tr!("shortcuts.hint"))),
            )
            .child(list(&theme, &groups, column))
    });
}

/// The card on Keys & indicators: a line about it and the button that opens the list.
pub fn render(width: f32, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let view = cx.entity().downgrade();
    card(&theme)
        .w(px(width))
        .child(
            h_flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_4()
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(tr!("shortcuts.title")))
                        .child(muted(&theme, tr!("shortcuts.card_hint"))),
                )
                .child(Button::new("show-shortcuts").outline().label(tr!("shortcuts.show")).on_click(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| open(this, window, cx));
                })),
        )
        .into_any_element()
}
