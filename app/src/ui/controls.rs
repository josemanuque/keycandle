//! One control per setting, created from the catalogue in settings.rs, and the row that shows it.
//!
//! Controls write straight to the keyboard as they change (sliders while dragging).

use std::collections::HashMap;

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, SharedString, Window};
use gpui_component::{
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    h_flex,
    select::{Select, SelectEvent, SelectState},
    slider::{Slider, SliderEvent, SliderState},
    switch::Switch,
    v_flex, ActiveTheme as _, IndexPath,
};

use super::app::Configurator;
use super::model::Model;
use super::widgets::{muted, segment, segmented};
use crate::color;
use crate::settings::{self, value, Kind, Setting};
use crate::tr;

type ChoiceState = SelectState<Vec<SharedString>>;

/// Choices with at most this many options show as a segmented control (all options visible, one
/// click); longer ones as a dropdown, where a row of segments would get cramped.
const MAX_SEGMENTS: usize = 4;

/// The interactive part of one setting's row. Toggles and segments are stateless (drawn from the
/// value).
enum Control {
    Toggle,
    Segments,
    /// Windows or Mac per device (drawn from the value, like segments).
    Devices,
    Range(Entity<SliderState>),
    Choice(Entity<ChoiceState>),
    Color(Entity<ColorPickerState>),
}

#[derive(Default)]
pub struct Controls(HashMap<value::Id, Control>);

impl Controls {
    /// A control for every setting the keyboard reported, starting at its current value.
    pub fn build(model: &Model, window: &mut Window, cx: &mut Context<Configurator>) -> Self {
        let controls = settings::all()
            .filter_map(|setting| {
                let current = model.get(setting.value_id())?.to_vec();
                Some((setting.value_id(), control(setting, &current, window, cx)))
            })
            .collect();
        Self(controls)
    }

    /// Bring controls in line with values that changed on the keyboard itself (Fn keys, …).
    pub fn sync(&self, ids: &[value::Id], model: &Model, window: &mut Window, cx: &mut Context<Configurator>) {
        for id in ids {
            let Some(current) = model.get(*id) else { continue };
            match self.0.get(id) {
                Some(Control::Range(state)) => {
                    let v = current[0] as f32;
                    state.update(cx, |s, cx| s.set_value(v, window, cx));
                }
                Some(Control::Choice(state)) => {
                    let i = current[0] as usize;
                    state.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(i)), window, cx));
                }
                Some(Control::Color(state)) => {
                    let c = color::from_stored(current);
                    state.update(cx, |s, cx| s.set_value(c, window, cx));
                }
                _ => {} // toggles and segments are drawn from the model
            }
        }
    }

    /// Show a choice made elsewhere (e.g. the effect gallery) in its dropdown.
    pub fn show_choice(&self, setting: &Setting, index: usize, window: &mut Window, cx: &mut Context<Configurator>) {
        if let Some(Control::Choice(state)) = self.0.get(&setting.value_id()) {
            state.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(index)), window, cx));
        }
    }
}

fn control(setting: &'static Setting, current: &[u8], window: &mut Window, cx: &mut Context<Configurator>) -> Control {
    match setting.kind {
        Kind::Toggle => Control::Toggle,
        Kind::DeviceLayouts => Control::Devices,
        Kind::Range { min, max } => {
            let state = cx.new(|_| {
                SliderState::new().min(min as f32).max(max as f32).step(1.0).default_value(current[0] as f32)
            });
            cx.subscribe(&state, move |this, _, event: &SliderEvent, cx| {
                let SliderEvent::Change(v) = event else { return }; // live while dragging
                this.write(setting, &[v.start().round() as u8], cx);
            })
            .detach();
            Control::Range(state)
        }
        Kind::Choice(keys) if keys.len() <= MAX_SEGMENTS => Control::Segments,
        Kind::Choice(_) => {
            let options: Vec<SharedString> = setting.options().into_iter().map(Into::into).collect();
            let selected = (current[0] as usize).min(options.len() - 1);
            let state = cx.new(|cx| SelectState::new(options.clone(), Some(IndexPath::new(selected)), window, cx));
            cx.subscribe(&state, move |this, _, event: &SelectEvent<Vec<SharedString>>, cx| {
                let SelectEvent::Confirm(Some(choice)) = event else { return };
                if let Some(i) = options.iter().position(|o| o == choice) {
                    this.write(setting, &[i as u8], cx);
                }
            })
            .detach();
            Control::Choice(state)
        }
        Kind::Color => {
            let state = cx.new(|cx| ColorPickerState::new(window, cx).default_value(color::from_stored(current)));
            cx.subscribe(&state, move |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(picked)) = event else { return };
                this.write(setting, &color::to_stored(*picked), cx);
            })
            .detach();
            Control::Color(state)
        }
    }
}

/// A setting's row: label and help on the left, its control on the right. `companion`: a setting
/// drawn in the same row, before the control (a toggle's color, or its level while it's on).
pub fn row(app: &Configurator, setting: &'static Setting, companion: Option<&'static Setting>, cx: &mut Context<Configurator>) -> AnyElement {
    let id = setting.value_id();
    let theme = cx.theme().clone();
    let help = setting.help();

    if let Some(Control::Devices) = app.controls.0.get(&id) {
        return device_layouts(app, setting, cx);
    }

    if let Some(Control::Segments) = app.controls.0.get(&id) {
        // As in the design: label above, the options across the full width, help below.
        let current = app.model.byte(id);
        let names = setting.options();
        let view = cx.entity().downgrade();
        let segments = setting.display_order().into_iter().map(|value| {
            let view = view.clone();
            let name = names.get(value as usize).cloned().unwrap_or_default();
            segment(&theme, SharedString::from(format!("seg-{}-{}-{value}", id.0, id.1)), value == current)
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.write(setting, &[value], cx));
                })
                .child(name)
        });
        return v_flex()
            .gap_2()
            .py_3()
            .child(div().text_sm().text_color(theme.muted_foreground).child(setting.label()))
            .child(segmented(&theme).children(segments))
            .when_some(help, |el, help| el.child(muted(&theme, help)))
            .into_any_element();
    }

    let label = v_flex()
        .gap_0p5()
        .flex_1()
        .min_w(px(180.))
        .child(div().child(setting.label()))
        .when_some(help, |el, help| el.child(muted(&theme, help)));

    let control = control_element(app, setting, cx);
    // A color is always there to pick; a level only while its toggle is on.
    let companion = companion.filter(|c| matches!(c.kind, Kind::Color) || app.model.is_shown(c)).map(|c| {
        let element = control_element(app, c, cx);
        match c.kind {
            Kind::Range { .. } => h_flex()
                .gap_3()
                .items_center()
                .child(div().text_sm().text_color(theme.muted_foreground).child(c.label()))
                .child(element)
                .into_any_element(),
            _ => element,
        }
    });
    h_flex()
        .gap_6()
        .py_3()
        .items_center()
        .flex_wrap()
        .child(label)
        .child(h_flex().gap_3().items_center().children(companion).child(control))
        .into_any_element()
}

/// The control alone, for a setting drawn by `row`.
fn control_element(app: &Configurator, setting: &'static Setting, cx: &mut Context<Configurator>) -> AnyElement {
    let id = setting.value_id();
    let theme = cx.theme().clone();
    match app.controls.0.get(&id) {
        Some(Control::Toggle) => switch(app, setting, cx),
        Some(Control::Range(state)) => {
            let max = match setting.kind {
                Kind::Range { max, .. } => max.max(1) as u32,
                _ => 255,
            };
            let value = app.model.byte(id) as u32;
            let shown = if app.raw_values { value.to_string() } else { format!("{}%", (value * 100 + max / 2) / max) };
            h_flex()
                .gap_3()
                .w(px(300.))
                .flex_shrink(1.)
                .min_w(px(160.))
                .child(div().flex_1().child(Slider::new(state)))
                .child(div().w(px(40.)).text_right().text_color(theme.muted_foreground).child(shown))
                .into_any_element()
        }
        Some(Control::Choice(state)) => div().w(px(300.)).child(Select::new(state)).into_any_element(),
        Some(Control::Color(state)) => ColorPicker::new(state).into_any_element(),
        Some(Control::Segments | Control::Devices) => muted(&theme, tr!("controls.not_available")).into_any_element(),
        None => muted(&theme, tr!("controls.not_available")).into_any_element(),
    }
}

/// A toggle's switch (also used outside the settings cards: the battery card).
pub fn switch(app: &Configurator, setting: &'static Setting, cx: &mut Context<Configurator>) -> AnyElement {
    let id = setting.value_id();
    if !matches!(app.controls.0.get(&id), Some(Control::Toggle)) {
        return muted(cx.theme(), tr!("controls.not_available")).into_any_element();
    }
    let view = cx.entity().downgrade();
    Switch::new(SharedString::from(format!("toggle-{}-{}", id.0, id.1)))
        .checked(app.model.byte(id) != 0)
        .on_click(move |&checked, _, cx| {
            let _ = view.update(cx, |this, cx| this.write(setting, &[checked as u8], cx));
        })
        .into_any_element()
}

/// Narrowest a device card gets, and the space between them.
const MIN_DEVICE_CARD: f32 = 140.0;
const DEVICE_GAP: f32 = 10.0;

/// A card per device (the cable, then Bluetooth 1..5), each with Windows / Mac; the one in use is
/// outlined.
fn device_layouts(app: &Configurator, setting: &'static Setting, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let id = setting.value_id();
    let mask = app.model.byte(id);
    // The device in use, when the keyboard says (USB with firmware v0.8+, Bluetooth with v0.9+).
    let profile = crate::keyboard::current();
    let wired = profile.wired_device();
    let in_use = app.model.state().copied().or(app.model.bluetooth_state()).map(|s| if s.bluetooth { s.slot as usize } else { wired });
    let view = cx.entity().downgrade();
    // Even rows: 6 across, or 3 by 2, or 2 by 3, never a lone card stretched along a row of its own.
    // The section spans the page; the card's padding and border come off.
    let inner = app.layout.width - 42.0;
    let columns = [6.0f32, 3.0, 2.0].into_iter().find(|n| (inner - DEVICE_GAP * (n - 1.0)) / n >= MIN_DEVICE_CARD).unwrap_or(1.0);
    let card_width = ((inner - DEVICE_GAP * (columns - 1.0)) / columns).floor();
    let cards = std::iter::once(wired).chain(0..wired).map(|device| {
        let (name, key) = if device == wired {
            (tr!("devices.wired_short"), tr!("devices.wired_key"))
        } else {
            (tr!("devices.bluetooth_short", number = device + 1), tr!("devices.bluetooth_key", key = profile.bluetooth_device_keys[device]))
        };
        let used = in_use == Some(device);
        let detail = if used { format!("{key} · {}", tr!("devices.in_use")) } else { key };
        let mac = mask >> device & 1 != 0;
        let options = [(false, tr!("devices.win")), (true, tr!("devices.mac"))].map(|(is_mac, label)| {
            let view = view.clone();
            let new = if is_mac { mask | 1 << device } else { mask & !(1 << device) };
            segment(&theme, SharedString::from(format!("layout-{device}-{is_mac}")), mac == is_mac)
                .min_w(px(0.))
                .px_2()
                .py_1()
                .text_xs()
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.write(setting, &[new], cx));
                })
                .child(label)
        });
        v_flex()
            .w(px(card_width))
            .gap_1()
            .p_3()
            .rounded_lg()
            .bg(theme.background)
            .border_1()
            .border_color(if used { theme.ring } else { theme.border })
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).truncate().child(name))
            .child(muted(&theme, detail))
            .child(div().pt_1().child(segmented(&theme).children(options)))
    });
    v_flex()
        .gap_3()
        .py_2()
        .child(h_flex().flex_wrap().gap(px(DEVICE_GAP)).children(cards))
        .when_some(setting.help(), |el, help| el.child(muted(&theme, help)))
        .into_any_element()
}
