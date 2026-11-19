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
use super::widgets::{badge, muted, segment, segmented};
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

/// A setting's row: label and help on the left, its control on the right.
pub fn row(app: &Configurator, setting: &'static Setting, cx: &mut Context<Configurator>) -> AnyElement {
    let id = setting.value_id();
    let theme = cx.theme().clone();
    let help = setting.help();

    if let Some(Control::Devices) = app.controls.0.get(&id) {
        return device_layouts(app, setting, cx);
    }

    if let Some(Control::Segments) = app.controls.0.get(&id) {
        // As in the design: label above, the options across the full width, help below.
        let current = app.model.byte(id) as usize;
        let view = cx.entity().downgrade();
        let segments = setting.options().into_iter().enumerate().map(|(i, option)| {
            let view = view.clone();
            segment(&theme, SharedString::from(format!("seg-{}-{}-{i}", id.0, id.1)), i == current)
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.write(setting, &[i as u8], cx));
                })
                .child(option)
        });
        return v_flex()
            .gap_2()
            .py_3()
            .child(div().child(setting.label()))
            .child(segmented(&theme).children(segments))
            .when_some(help, |el, help| el.child(muted(&theme, help)))
            .into_any_element();
    }

    let label = v_flex()
        .gap_0p5()
        .flex_1()
        .min_w_0()
        .child(div().child(setting.label()))
        .when_some(help, |el, help| el.child(muted(&theme, help)));

    let control: AnyElement = match app.controls.0.get(&id) {
        Some(Control::Toggle) => {
            let view = cx.entity().downgrade();
            Switch::new(SharedString::from(format!("toggle-{}-{}", id.0, id.1)))
                .checked(app.model.byte(id) != 0)
                .on_click(move |&checked, _, cx| {
                    let _ = view.update(cx, |this, cx| this.write(setting, &[checked as u8], cx));
                })
                .into_any_element()
        }
        Some(Control::Range(state)) => h_flex()
            .gap_3()
            .w(px(300.))
            .flex_shrink(1.)
            .min_w(px(160.))
            .child(div().flex_1().child(Slider::new(state)))
            .child(div().w(px(32.)).text_right().text_color(theme.muted_foreground).child(app.model.byte(id).to_string()))
            .into_any_element(),
        Some(Control::Choice(state)) => div().w(px(300.)).child(Select::new(state)).into_any_element(),
        Some(Control::Color(state)) => ColorPicker::new(state).into_any_element(),
        Some(Control::Segments | Control::Devices) => unreachable!("drawn above"),
        None => muted(&theme, tr!("controls.not_available")).into_any_element(),
    };
    h_flex().gap_6().py_3().items_center().child(label).child(control).into_any_element()
}

/// The devices the keyboard connects to, in the bit order of the DeviceLayouts value.
const DEVICES: usize = 6;
const WIRED: usize = 5;

/// One row per device (Bluetooth 1..5 and the cable), each with Windows / Mac.
fn device_layouts(app: &Configurator, setting: &'static Setting, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let id = setting.value_id();
    let mask = app.model.byte(id);
    // The device in use, when the keyboard says (USB with firmware v0.8+).
    let in_use = app.model.state().map(|s| if s.bluetooth { s.slot as usize } else { WIRED });
    let view = cx.entity().downgrade();
    let rows = (0..DEVICES).map(|device| {
        let name = if device == WIRED { tr!("devices.wired") } else { tr!("devices.bluetooth", number = device + 1) };
        let mac = mask >> device & 1 != 0;
        let options = [(false, tr!("devices.windows")), (true, tr!("devices.mac"))].map(|(is_mac, label)| {
            let view = view.clone();
            let new = if is_mac { mask | 1 << device } else { mask & !(1 << device) };
            segment(&theme, SharedString::from(format!("layout-{device}-{is_mac}")), mac == is_mac)
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.write(setting, &[new], cx));
                })
                .child(label)
        });
        h_flex()
            .gap_4()
            .py_2()
            .items_center()
            .child(
                h_flex()
                    .gap_2()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .child(div().child(name))
                    .when(in_use == Some(device), |el| el.child(badge(&theme, tr!("devices.in_use")))),
            )
            .child(div().w(px(240.)).flex_shrink(1.).min_w(px(150.)).child(segmented(&theme).children(options)))
    });
    v_flex()
        .gap_1()
        .py_3()
        .child(div().child(setting.label()))
        .when_some(setting.help(), |el, help| el.child(muted(&theme, help)))
        .child(v_flex().pt_2().children(rows))
        .into_any_element()
}
