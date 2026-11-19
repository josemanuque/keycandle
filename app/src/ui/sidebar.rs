//! The sidebar: app name, the sections, and the connection state.

use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, rgb, white, AnyElement, Context, FontWeight, Hsla, KeyDownEvent, Rgba, Role, Window};
use gpui_component::{h_flex, tooltip::Tooltip, v_flex, ActiveTheme as _, Icon, IconName};

use super::app::Configurator;
use super::chrome;
use super::model::{Connection, Model};
use crate::btlink::Status;
use super::page::Page;
use super::widgets::muted;
use crate::tr;

/// The sidebar's width (gpui-component's default); with only icons, see `Backdrop::rail_width`.
pub const WIDTH: f32 = 255.0;

/// How long the status dot takes to grow into a mark, and to shrink back.
const MARK_GROW: Duration = Duration::from_millis(260);
const MARK_SHRINK: Duration = Duration::from_millis(220);

/// How something ended: a check or an X.
#[derive(Clone, Copy, PartialEq)]
pub enum Outcome {
    Done,
    Failed,
}

/// The mark as it should be drawn right now.
#[derive(Clone, Copy)]
pub struct Mark {
    outcome: Outcome,
    color: Rgba,
    /// 0 = still the dot, 1 = the full mark (a little more while it overshoots).
    scale: f32,
    /// Still animating: draw again next frame.
    pub moving: bool,
}

/// The dot turning into a mark when something finishes. The same result arriving while its mark is
/// up keeps it up rather than playing it again, so dragging a slider doesn't make it flicker.
#[derive(Default)]
pub struct StatusMark {
    current: Option<(Outcome, Rgba)>,
    started: Option<Instant>,
    ended: Option<Instant>,
}

impl StatusMark {
    /// Call on every draw, with the result to show now, if any.
    pub fn update(&mut self, wanted: Option<(Outcome, Rgba)>, now: Instant) -> Option<Mark> {
        match wanted {
            Some(_) => {
                let still_up = self.ended.is_none_or(|ended| now.duration_since(ended) < MARK_SHRINK);
                if self.current != wanted || !still_up {
                    self.started = Some(now);
                }
                self.current = wanted;
                self.ended = None;
            }
            None => {
                self.current?;
                let ended = *self.ended.get_or_insert(now);
                if now.duration_since(ended) >= MARK_SHRINK {
                    *self = Self::default();
                    return None;
                }
            }
        }
        let (outcome, color) = self.current?;
        let part = |since: Instant, length: Duration| (now.duration_since(since).as_secs_f32() / length.as_secs_f32()).min(1.0);
        let grow = part(self.started?, MARK_GROW);
        // Grows past full size and settles back.
        let t = grow - 1.0;
        let grown = 1.0 + 2.7 * t * t * t + 1.7 * t * t;
        let left = self.ended.map_or(1.0, |ended| (1.0 - part(ended, MARK_SHRINK)).powi(2));
        Some(Mark { outcome, color, scale: grown * left, moving: grow < 1.0 || self.ended.is_some() })
    }
}

/// The result the status should show right now, if any.
pub fn outcome(model: &Model) -> Option<(Outcome, Rgba)> {
    match model.activity() {
        Status::Saved => Some((Outcome::Done, connection_text(model.connection(), None).0)),
        Status::Received => Some((Outcome::Done, YELLOW)),
        Status::Failed(_) => Some((Outcome::Failed, RED)),
        _ => None,
    }
}

/// The status dot, or the mark it turns into. Always the same room, so the text beside it stays put.
fn status_mark(dot: Rgba, mark: Option<Mark>) -> impl IntoElement {
    let scale = mark.map_or(0.0, |m| m.scale);
    let icon = match mark.map(|m| m.outcome) {
        Some(Outcome::Failed) => IconName::Close,
        _ => IconName::Check,
    };
    div().size_3p5().flex_none().flex().items_center().justify_center().child(
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(8.0 + 6.0 * scale))
            .rounded_full()
            .bg(mark.map_or(dot, |m| m.color))
            .when(scale > 0.05, |el| {
                el.child(Icon::new(icon).size(px(10.0 * scale)).text_color(rgb(0x0F1115)).opacity(scale.min(1.0)))
            }),
    )
}

/// Colors of the selected section.
#[derive(Clone, Copy)]
struct Selection {
    fill: Hsla,
    text: Hsla,
}

/// `compact`: icons only (narrow windows).
pub fn render(
    app: &Configurator,
    compact: bool,
    mark: Option<Mark>,
    window: &Window,
    cx: &mut Context<Configurator>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let native = app.backdrop.is_translucent();
    // Room for the window buttons, which sit in the sidebar when there's no title bar.
    let title_bar = app.backdrop.title_bar_height();
    let inset = app.backdrop.sidebar_inset();
    let rail = app.backdrop.rail_width();
    // macOS: the system's accent color, gray while another window is active. Elsewhere: the theme's.
    let selection = match (native, window.is_window_active()) {
        (true, true) => Selection { fill: chrome::accent_color().unwrap_or(theme.primary), text: white() },
        (true, false) => Selection { fill: theme.foreground.opacity(0.14), text: theme.foreground },
        (false, _) => Selection { fill: theme.sidebar_accent, text: theme.sidebar_accent_foreground },
    };
    let view = cx.entity().downgrade();
    let rows: Vec<AnyElement> = Page::ALL.into_iter().enumerate().map(|(index, page)| {
        let selected = page == app.page;
        let view = view.clone();
        let title = page.title();
        let row = h_flex()
            .id(("section", index))
            .role(Role::TreeItem)
            .aria_label(title.clone())
            .aria_selected(selected)
            .flex_none()
            .items_center()
            .text_color(if selected { selection.text } else { theme.sidebar_foreground })
            .when(selected, |el| el.bg(selection.fill))
            // Native sidebars don't light up under the pointer.
            .when(!native && !selected, |el| el.hover(|el| el.bg(theme.sidebar_accent.opacity(0.8))))
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| this.open(page, cx));
            });
        if compact {
            // As wide as the window buttons above, and centered under them.
            row.justify_center()
                .w(px(if native { 60. } else { 36. }))
                .h(px(if native { 38. } else { 36. }))
                .rounded(px(if native { 10. } else { 8. }))
                .child(page.icon().size_5())
                .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
                .into_any_element()
        } else {
            row.w_full()
                .h_7()
                .px_2()
                .gap_2()
                .rounded(px(8.))
                .when(native, |el| el.text_size(px(13.)))
                .when(!native, |el| el.text_sm())
                .child(page.icon().size_4())
                .child(div().min_w_0().truncate().child(title))
                .into_any_element()
        }
    }).collect();

    let (dot, state, detail) = connection_text(app.model.connection(), app.model.write_error());
    // What's happening right now takes the place of the usual line.
    let activity = app.model.activity();
    let dot = activity_color(&activity).unwrap_or(dot);
    let detail = activity_label(&activity).or(detail);
    let status_dot = status_mark(dot, mark);
    // On hover: the whole status when only the dot fits, and what Bluetooth can't do.
    let tip = if compact {
        Some(detail.as_ref().map_or_else(|| state.clone(), |d| format!("{state}: {d}")))
    } else {
        app.model.on_bluetooth().then(|| tr!("connection.bluetooth_note"))
    };
    // Text lines up with the rows' icons.
    let text_inset = px(inset + 8.);
    let status = div()
        .id("connection-status")
        .flex_none()
        .w_full()
        .pt_2()
        .pb_4()
        .child(if compact {
            h_flex().w_full().justify_center().child(status_dot).into_any_element()
        } else {
            v_flex()
                .px(text_inset)
                .gap_1()
                .child(h_flex().gap_1p5().items_center().child(status_dot).child(state))
                .when_some(detail, |el, detail| el.child(muted(&theme, detail)))
                .into_any_element()
        })
        .when_some(tip, |el, tip| el.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)));

    // The ring shows only when the keyboard brought the focus here, not after a click.
    let focused = app.sidebar_focus.is_focused(window) && window.last_input_was_keyboard();
    v_flex()
        .id("sidebar")
        .track_focus(&app.sidebar_focus.clone().tab_index(0).tab_stop(true))
        .relative()
        .flex_none()
        .h_full()
        .w(px(if compact { rail } else { WIDTH }))
        .overflow_hidden()
        .bg(theme.tokens.sidebar)
        .text_color(theme.sidebar_foreground)
        .border_r_1()
        .border_color(theme.sidebar_border)
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            let step = match event.keystroke.key.as_str() {
                "up" => -1,
                "down" => 1,
                _ => return,
            };
            if event.keystroke.modifiers.modified() {
                return;
            }
            let _ = view.update(cx, |this, cx| {
                let current = Page::ALL.iter().position(|page| *page == this.page).unwrap_or(0) as isize;
                let next = (current + step).clamp(0, Page::ALL.len() as isize - 1) as usize;
                this.open(Page::ALL[next], cx);
            });
            cx.stop_propagation();
        })
        .child(chrome::title_strip(title_bar))
        .when(!compact, |el| {
            el.child(
                v_flex()
                    .flex_none()
                    .px(text_inset)
                    .pb_3()
                    .when(!native, |el| el.pt_3())
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(tr!("app.name")))
                    .child(muted(&theme, tr!("app.subtitle"))),
            )
        })
        .child(
            v_flex()
                .id("sections")
                .role(Role::Tree)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .when(compact, |el| el.items_center().gap_1().pt_2())
                .when(!compact, |el| el.px(px(inset)).gap_0p5())
                .children(rows),
        )
        .child(status)
        .when(focused, |el| {
            el.child(div().absolute().top(title_bar).left_1().right_1().bottom_1().rounded_md().border_2().border_color(theme.ring))
        })
}

const YELLOW: Rgba = Rgba { r: 0.984, g: 0.749, b: 0.141, a: 1.0 };
const RED: Rgba = Rgba { r: 0.973, g: 0.443, b: 0.443, a: 1.0 };

/// The dot's color while something is going on. Idle and saved keep the connection's own color:
/// blue on Bluetooth, green on the cable.
fn activity_color(activity: &Status) -> Option<Rgba> {
    match activity {
        Status::Idle | Status::Saved => None,
        Status::Sending(_) => Some(rgb(0x60A5FA)),
        Status::Receiving | Status::Received => Some(YELLOW),
        Status::Failed(_) => Some(RED),
    }
}

/// What's going on, in words.
fn activity_label(activity: &Status) -> Option<String> {
    match activity {
        Status::Idle => None,
        Status::Sending(count) => Some(tr!("sync.sending", count = count)),
        Status::Receiving => Some(tr!("sync.receiving")),
        Status::Received => Some(tr!("sync.received")),
        Status::Saved => Some(tr!("sync.saved")),
        Status::Failed(error) => Some(tr!("sync.failed", error = error)),
    }
}

/// Icons-only sidebar: no room for words, so transfers and failures show in a small pop-up next to it.
pub fn activity_popup(app: &Configurator, cx: &mut Context<Configurator>) -> Option<AnyElement> {
    let activity = app.model.activity();
    // Not for finished work: the check on the rail says it, without a pop-up on every slider step.
    if matches!(activity, Status::Saved | Status::Received) {
        return None;
    }
    let color = activity_color(&activity)?;
    let text = activity_label(&activity)?;
    let theme = cx.theme();
    Some(
        h_flex()
            .absolute()
            .left(px(app.backdrop.rail_width() + 12.))
            .bottom_3()
            .max_w(px(360.))
            .gap_2()
            .items_center()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .text_sm()
            .shadow_md()
            .child(div().size_2().flex_none().rounded_full().bg(color))
            .child(div().min_w_0().child(text))
            .into_any_element(),
    )
}

/// Status dot, headline and detail for the connection.
fn connection_text(connection: &Connection, write_error: Option<&str>) -> (Rgba, String, Option<String>) {
    let (green, amber, red) = (rgb(0x4ADE80), rgb(0xFBBF24), rgb(0xF87171));
    match connection {
        Connection::Usb { protocol } => {
            let detail = match write_error {
                Some(error) => tr!("connection.write_failed", error = error),
                None => tr!("connection.usb", version = protocol),
            };
            (green, tr!("connection.connected"), Some(detail))
        }
        Connection::FlashMode => (amber, tr!("connection.flash_mode"), Some(tr!("connection.flash_mode_detail"))),
        Connection::Stock => (amber, tr!("connection.stock"), Some(tr!("connection.stock_detail"))),
        Connection::Unplugged => (red, tr!("connection.not_connected"), Some(tr!("connection.unplugged"))),
        Connection::Missing(reason) => (red, tr!("connection.not_connected"), Some(reason.clone())),
        Connection::NotFound => (red, tr!("connection.not_connected"), None),
        Connection::Bluetooth => (rgb(0x60A5FA), tr!("connection.bluetooth"), Some(tr!("connection.bluetooth_detail"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const DONE: Option<(Outcome, Rgba)> = Some((Outcome::Done, YELLOW));

    fn scale(mark: Option<Mark>) -> f32 {
        mark.expect("a mark").scale
    }

    #[test]
    fn mark_grows_settles_and_goes() {
        let t0 = Instant::now();
        let mut mark = StatusMark::default();
        assert!(mark.update(None, t0).is_none());
        assert!(scale(mark.update(DONE, t0)) < 0.05); // starts as the dot
        let full = mark.update(DONE, t0 + MARK_GROW).unwrap();
        assert!((full.scale - 1.0).abs() < 0.01);
        assert!(!full.moving); // no redraws needed while it just stays up
        let end = t0 + MS(900);
        let shrinking = mark.update(None, end).unwrap();
        assert!(shrinking.moving);
        assert!(scale(mark.update(None, end + MS(110))) < 1.0);
        assert!(mark.update(None, end + MARK_SHRINK).is_none());
    }

    #[test]
    fn the_same_result_does_not_replay_while_up() {
        let t0 = Instant::now();
        let mut mark = StatusMark::default();
        mark.update(DONE, t0);
        mark.update(DONE, t0 + MARK_GROW);
        let later = t0 + MS(800);
        mark.update(None, later);
        assert!((scale(mark.update(DONE, later + MS(100))) - 1.0).abs() < 0.01);
    }

    #[test]
    fn a_later_or_different_result_plays() {
        let t0 = Instant::now();
        let mut mark = StatusMark::default();
        mark.update(DONE, t0);
        mark.update(None, t0 + MS(500));
        assert!(mark.update(None, t0 + MS(500) + MARK_SHRINK).is_none());
        let later = t0 + MS(900);
        assert!(scale(mark.update(DONE, later)) < 0.05);
        // A failure right after a success starts its own mark, an X.
        let failed = mark.update(Some((Outcome::Failed, RED)), later + MS(400)).unwrap();
        assert!(failed.scale < 0.05 && failed.outcome == Outcome::Failed);
    }
}
