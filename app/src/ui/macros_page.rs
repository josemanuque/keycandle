//! Macros: text and key sequences the keyboard plays on Fn + a key.
//!
//! Edits stay in the app until Save (the keyboard rewrites its whole macro area on each save, so
//! not on every keystroke). Names stay on this computer: the keyboard has no room for them. The key
//! is set at once. All of it needs the USB cable; the macros themselves run over Bluetooth too.

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, FocusHandle, FontWeight, KeyDownEvent, SharedString, Window};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};

use super::app::Configurator;
use super::controls;
use super::widgets::{badge, card, muted, note};
use crate::keyboard;
use crate::macros::{self, key_name, typeable, usage_of, Step};
use crate::prefs::Prefs;
use crate::settings::MACRO_MATCH_OS;
use crate::tr;

/// The page's own state (the draft being edited, and its inputs).
pub struct MacroUi {
    /// What the page shows and edits; `None` until the keyboard's macros are read.
    draft: Option<Vec<Vec<Step>>>,
    dirty: bool,
    selected: usize,
    choosing_key: bool,
    recording: bool,
    /// A text step being edited in the text box, by index.
    editing: Option<usize>,
    error: Option<String>,
    names: Vec<String>,
    name_input: Entity<InputState>,
    text_input: Entity<InputState>,
    try_input: Entity<InputState>,
    capture: FocusHandle,
}

impl MacroUi {
    pub fn new(window: &mut Window, cx: &mut Context<Configurator>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx));
        let text_input = cx.new(|cx| InputState::new(window, cx).placeholder(tr!("macros.text_placeholder")));
        let try_input = cx.new(|cx| InputState::new(window, cx).placeholder(tr!("macros.try_placeholder")));
        cx.subscribe(&name_input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let name = input.read(cx).value().to_string();
                this.macro_ui.rename(name);
            }
        })
        .detach();
        cx.subscribe_in(&text_input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.macro_ui.add_text(window, cx);
                cx.notify();
            }
        })
        .detach();
        Self {
            draft: None,
            dirty: false,
            selected: 0,
            choosing_key: false,
            recording: false,
            editing: None,
            error: None,
            names: if super::model::preview() {
                ["Email sign-off", "Quick reply", "Standup notes"].map(String::from).to_vec()
            } else {
                Prefs::load().macro_names
            },
            name_input,
            text_input,
            try_input,
            capture: cx.focus_handle(),
        }
    }

    fn name(&self, n: usize) -> String {
        self.names.get(n).filter(|s| !s.trim().is_empty()).cloned().unwrap_or_else(|| tr!("macros.default_name", number = n + 1))
    }

    fn rename(&mut self, name: String) {
        let n = self.selected;
        if self.names.len() <= n {
            self.names.resize(n + 1, String::new());
        }
        if self.names[n] != name {
            self.names[n] = name;
            let mut prefs = Prefs::load();
            prefs.macro_names = self.names.clone();
            prefs.save();
        }
    }

    fn steps_mut(&mut self) -> Option<&mut Vec<Step>> {
        let n = self.selected;
        self.draft.as_mut().and_then(|d| d.get_mut(n))
    }

    fn push(&mut self, step: Step) {
        let Some(steps) = self.steps_mut() else { return };
        // Typing in a row goes into one text step.
        match (steps.last_mut(), &step) {
            (Some(Step::Text(t)), Step::Text(more)) => t.push_str(more),
            _ => steps.push(step),
        }
        self.dirty = true;
    }

    /// The text box: add its text as a step, or replace the text step being edited.
    fn add_text(&mut self, window: &mut Window, cx: &mut Context<Configurator>) {
        let text: String = self.text_input.read(cx).value().chars().filter(|c| typeable(*c)).collect();
        if text.is_empty() {
            return;
        }
        match self.editing.take() {
            Some(i) => {
                if let Some(Step::Text(t)) = self.steps_mut().and_then(|s| s.get_mut(i)) {
                    *t = text;
                    self.dirty = true;
                }
            }
            None => self.push(Step::Text(text)),
        }
        self.text_input.update(cx, |s, cx| s.set_value("", window, cx));
    }

    fn select(&mut self, n: usize, window: &mut Window, cx: &mut Context<Configurator>) {
        self.selected = n;
        self.choosing_key = false;
        self.editing = None;
        let name = self.name(n);
        self.name_input.update(cx, |s, cx| s.set_value(name, window, cx));
    }
}

/// A recorded key press, as a step: plain typing is text, anything else a key or shortcut. On a Mac,
/// Cmd is stored as Ctrl (and Ctrl as Cmd), so that "match the device" plays it right on both.
fn recorded(event: &KeyDownEvent) -> Option<Step> {
    let ks = &event.keystroke;
    let m = &ks.modifiers;
    if !m.control && !m.alt && !m.platform {
        if let Some(c) = ks.key_char.as_deref().filter(|s| s.chars().count() == 1).and_then(|s| s.chars().next()) {
            if typeable(c) && c != '\t' && c != '\n' {
                return Some(Step::Text(c.to_string()));
            }
        }
    }
    let key = usage_of(&ks.key)?;
    let (ctrl, gui) = if cfg!(target_os = "macos") { (m.platform, m.control) } else { (m.control, m.platform) };
    let mods: Vec<u8> = [(ctrl, 0xE0), (m.shift, 0xE1), (m.alt, 0xE2), (gui, 0xE3)].into_iter().filter(|(on, _)| *on).map(|(_, u)| u).collect();
    Some(if mods.is_empty() { Step::Key(key) } else { Step::Shortcut { mods, key } })
}

/// How a modifier reads here: on a Mac the stored Ctrl is played as Cmd (see `recorded`).
fn mod_name(code: u8) -> String {
    match (code, cfg!(target_os = "macos")) {
        (0xE0 | 0xE4, true) => "Cmd".into(),
        (0xE3 | 0xE7, true) => "Ctrl".into(),
        _ => key_name(code),
    }
}

/// A step, in words: its kind and what it does.
fn describe(step: &Step) -> (String, String) {
    match step {
        Step::Text(t) => (tr!("macros.step_text"), format!("“{}”", t.replace('\n', "⏎"))),
        Step::Shortcut { mods, key } => {
            let names: Vec<String> = mods.iter().map(|m| mod_name(*m)).chain([key_name(*key)]).collect();
            (tr!("macros.step_shortcut"), names.join(" + "))
        }
        Step::Key(k) => (tr!("macros.step_key"), key_name(*k)),
        Step::Press(k) => (tr!("macros.step_hold"), mod_name(*k)),
        Step::Release(k) => (tr!("macros.step_release"), mod_name(*k)),
        Step::Wait(ms) => (tr!("macros.step_wait"), format!("{ms} ms")),
    }
}

/// Examples to start from: text and sequences (one shortcut alone is no macro).
fn templates() -> Vec<(String, String, Vec<Step>)> {
    let ctrl = |key: u8| Step::Shortcut { mods: vec![0xE0], key };
    vec![
        (tr!("macros.tpl_signoff"), tr!("macros.tpl_signoff_what"), vec![Step::Text("Best regards,".into()), Step::Key(0x28), Step::Text("Name".into())]),
        (tr!("macros.tpl_duplicate"), tr!("macros.tpl_duplicate_what"), vec![
            Step::Key(0x4A),
            Step::Shortcut { mods: vec![0xE1], key: 0x4D },
            ctrl(0x06),
            Step::Key(0x4D),
            Step::Key(0x28),
            ctrl(0x19),
        ]),
        (tr!("macros.tpl_link"), tr!("macros.tpl_link_what"), vec![ctrl(0x1B), Step::Text("[".into()), ctrl(0x19), Step::Text("]()".into()), Step::Key(0x50)]),
        (tr!("macros.tpl_commit"), tr!("macros.tpl_commit_what"), vec![Step::Text("git commit -m \"\"".into()), Step::Key(0x50)]),
        (tr!("macros.tpl_form"), tr!("macros.tpl_form_what"), vec![Step::Text("name@example.com".into()), Step::Key(0x2B)]),
        (tr!("macros.tpl_reply"), tr!("macros.tpl_reply_what"), vec![
            Step::Text("Thanks for reaching out! I'll get back to you by tomorrow.".into()),
        ]),
    ]
}

/// Gap between keys in the picker, in px (gap_1).
const PICKER_GAP: f32 = 4.;
/// A key's width per unit at most, in px.
const PICKER_UNIT_MAX: f32 = 34.;

/// The px per key unit that fits every row of the picker (keys and gaps) into `room` px.
fn picker_unit(keys: &[keyboard::rk71::layout::Key], room: f32) -> f32 {
    (0..5)
        .filter_map(|row| {
            let (units, count) = keys.iter().filter(|k| k.row == row).fold((0f32, 0usize), |(u, c), k| (u + k.w, c + 1));
            (count > 0).then(|| (room - (count - 1) as f32 * PICKER_GAP) / units)
        })
        .fold(PICKER_UNIT_MAX, f32::min)
        .max(16.)
        .floor()
}

pub fn render(app: &mut Configurator, window: &mut Window, cx: &mut Context<Configurator>) -> AnyElement {
    let theme = cx.theme().clone();
    let width = app.layout.width;
    let Some(stored) = app.model.macros().cloned() else {
        let view = cx.entity().downgrade();
        let update = app.model.macros_need_update();
        return v_flex()
            .w(px(width))
            .gap_3()
            .child(note(&theme, if update { tr!("macros.needs_update") } else { tr!("macros.needs_cable") }))
            .when(update, |el| {
                el.child(h_flex().child(Button::new("open-firmware").primary().label(tr!("macros.open_firmware")).on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.open(super::page::Page::Firmware, cx));
                })))
            })
            .into_any_element();
    };
    // Load the keyboard's macros into the editor (again after they change and nothing is unsaved).
    if app.macro_ui.draft.is_none() || (!app.macro_ui.dirty && app.macro_ui.draft.as_ref() != Some(&stored.saved)) {
        app.macro_ui.draft = Some(stored.saved.clone());
        let n = app.macro_ui.selected.min(stored.count.saturating_sub(1));
        app.macro_ui.select(n, window, cx);
    }
    let ui = &app.macro_ui;
    let draft = ui.draft.clone().unwrap_or_default();
    let used = macros::size_of(&draft);
    let room = stored.room();
    let over = used > room;
    let view = cx.entity().downgrade();
    let profile = keyboard::current();
    let key_label = |n: usize| stored.keys.get(n).copied().flatten().map(|i| format!("Fn + {}", profile.keys[i].label));

    // ---- the list ----
    let shown: Vec<usize> = (0..stored.count).filter(|n| !draft[*n].is_empty() || *n == ui.selected || stored.keys[*n].is_some()).collect();
    let list = v_flex()
        .w(px(280.))
        .flex_none()
        .gap_1()
        .children(shown.iter().map(|&n| {
            let selected = n == ui.selected;
            let summary = draft[n].iter().map(|s| describe(s).1).collect::<Vec<_>>().join(" · ");
            let view = view.clone();
            v_flex()
                .id(("macro", n))
                .gap_0p5()
                .px_3()
                .py_2p5()
                .rounded_lg()
                .cursor_pointer()
                .border_1()
                .border_color(if selected { theme.ring } else { gpui::transparent_black() })
                .when(selected, |el| el.bg(theme.accent))
                .when(!selected, |el| el.hover(|el| el.bg(theme.muted)))
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.macro_ui.select(n, window, cx);
                        cx.notify();
                    });
                })
                .child(
                    h_flex()
                        .justify_between()
                        .gap_2()
                        .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).child(ui.name(n)))
                        .when_some(key_label(n), |el, k| el.child(badge(&theme, k))),
                )
                .child(muted(&theme, if summary.is_empty() { tr!("macros.empty") } else { summary }).truncate())
        }))
        .child({
            let view = view.clone();
            let free = (0..stored.count).find(|n| draft[*n].is_empty() && stored.keys[*n].is_none() && !shown.contains(n));
            Button::new("new-macro")
                .ghost()
                .icon(Icon::empty().path("icons/plus.svg"))
                .label(tr!("macros.new"))
                .disabled(free.is_none())
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if let Some(n) = free {
                            this.macro_ui.select(n, window, cx);
                            cx.notify();
                        }
                    });
                })
        })
        .child(muted(&theme, tr!("macros.slots", used = shown.iter().filter(|n| !draft[**n].is_empty()).count(), total = stored.count)));

    // ---- the editor ----
    let n = ui.selected;
    let steps = draft.get(n).cloned().unwrap_or_default();
    let steps_view: Vec<AnyElement> = steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let (kind, what) = describe(step);
            let is_text = matches!(step, Step::Text(_));
            let editing = ui.editing == Some(i);
            let (up, down, remove, edit) = (view.clone(), view.clone(), view.clone(), view.clone());
            let text = if let Step::Text(t) = step { t.clone() } else { String::new() };
            h_flex()
                .gap_3()
                .px_3()
                .py_2()
                .rounded_lg()
                .bg(theme.background)
                .border_1()
                .border_color(if editing { theme.ring } else { theme.border })
                .child(div().w(px(76.)).flex_none().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child(kind))
                .child(div().flex_1().min_w_0().truncate().font_family("monospace").text_sm().child(what))
                .when(is_text, |el| {
                    el.child(Button::new(("edit", i)).ghost().xsmall().label(tr!("macros.edit")).on_click(move |_, window, cx| {
                        let text = text.clone();
                        let _ = edit.update(cx, |this, cx| {
                            this.macro_ui.editing = Some(i);
                            this.macro_ui.text_input.update(cx, |s, cx| s.set_value(text, window, cx));
                            cx.notify();
                        });
                    }))
                })
                .child(Button::new(("up", i)).ghost().xsmall().icon(IconName::ChevronUp).disabled(i == 0).on_click(move |_, _, cx| {
                    let _ = up.update(cx, |this, cx| {
                        if let Some(s) = this.macro_ui.steps_mut() {
                            s.swap(i - 1, i);
                            this.macro_ui.dirty = true;
                        }
                        cx.notify();
                    });
                }))
                .child(Button::new(("down", i)).ghost().xsmall().icon(IconName::ChevronDown).disabled(i + 1 == steps.len()).on_click(
                    move |_, _, cx| {
                        let _ = down.update(cx, |this, cx| {
                            if let Some(s) = this.macro_ui.steps_mut() {
                                s.swap(i, i + 1);
                                this.macro_ui.dirty = true;
                            }
                            cx.notify();
                        });
                    },
                ))
                .child(Button::new(("remove", i)).ghost().xsmall().icon(IconName::Close).on_click(move |_, _, cx| {
                    let _ = remove.update(cx, |this, cx| {
                        if let Some(s) = this.macro_ui.steps_mut() {
                            s.remove(i);
                            this.macro_ui.dirty = true;
                            this.macro_ui.editing = None;
                        }
                        cx.notify();
                    });
                }))
                .into_any_element()
        })
        .collect();

    let add_text = view.clone();
    let record = view.clone();
    let wait = |ms: u16, id: &'static str| {
        let view = view.clone();
        Button::new(id).outline().small().label(tr!("macros.add_wait", ms = ms)).on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| {
                this.macro_ui.push(Step::Wait(ms));
                cx.notify();
            });
        })
    };
    let recording = ui.recording;
    let capture = ui.capture.clone();
    let recorder = div()
        .id("recorder")
        .track_focus(&capture)
        .when(recording, |el| {
            let view = view.clone();
            el.on_key_down(move |event: &KeyDownEvent, _, cx| {
                let Some(step) = recorded(event) else { return };
                let _ = view.update(cx, |this, cx| {
                    this.macro_ui.push(step);
                    cx.notify();
                });
                cx.stop_propagation();
            })
        })
        .px_3()
        .py_2()
        .rounded_lg()
        .border_1()
        .border_dashed()
        .border_color(if recording { theme.danger } else { theme.border })
        .text_sm()
        .text_color(if recording { theme.foreground } else { theme.muted_foreground })
        .child(if recording { tr!("macros.recording_hint") } else { tr!("macros.record_hint") });

    let key_text = key_label(n).unwrap_or_else(|| tr!("macros.no_key"));
    let choose = view.clone();
    let clear = view.clone();
    let header = h_flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_3()
        .child(div().w(px(260.)).child(Input::new(&ui.name_input)))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(muted(&theme, tr!("macros.runs_on")))
                .child(badge(&theme, key_text))
                .child(Button::new("choose-key").outline().small().label(tr!("macros.choose_key")).on_click(move |_, _, cx| {
                    let _ = choose.update(cx, |this, cx| {
                        this.macro_ui.choosing_key = !this.macro_ui.choosing_key;
                        cx.notify();
                    });
                }))
                .child(Button::new("clear-macro").ghost().small().label(tr!("macros.clear")).disabled(steps.is_empty()).on_click(move |_, _, cx| {
                    let _ = clear.update(cx, |this, cx| {
                        if let Some(s) = this.macro_ui.steps_mut() {
                            s.clear();
                            this.macro_ui.dirty = true;
                        }
                        cx.notify();
                    });
                })),
        );

    // The editor's width, less its padding and the picker's: the keys shrink to fit it.
    let editor_width = (width - 280. - 16.).max(420.);
    let key_unit = picker_unit(&profile.keys, editor_width - 2. * 20. - 2. * 16. - 4.);
    let key_picker = ui.choosing_key.then(|| {
        let rows = (0..5).map(|row| {
            h_flex().gap_1().children(profile.keys.iter().enumerate().filter(|(_, k)| k.row == row).map(|(i, k)| {
                let taken = stored.fn_taken[i];
                let mine = stored.keys[n] == Some(i);
                let view = view.clone();
                div()
                    .id(("pick", i))
                    .h(px((key_unit * 0.88).round()))
                    .w(px(k.w * key_unit))
                    .flex_none()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .text_xs()
                    .border_1()
                    .border_color(if mine { theme.ring } else { theme.border })
                    .when(mine, |el| el.bg(theme.primary).text_color(theme.primary_foreground))
                    .when(taken, |el| el.text_color(theme.muted_foreground.opacity(0.5)))
                    .when(!taken && !mine, |el| el.cursor_pointer().hover(|el| el.bg(theme.muted)))
                    .when(!taken, |el| {
                        el.on_click(move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.macro_ui.error = this.model.assign_macro(n, Some(i)).err().map(|e| format!("{e:#}"));
                                this.macro_ui.choosing_key = false;
                                cx.notify();
                            });
                        })
                    })
                    .child(k.label)
            }))
        });
        let none = view.clone();
        v_flex()
            .gap_2()
            .p_4()
            .rounded_lg()
            .bg(theme.background)
            .border_1()
            .border_color(theme.ring)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(tr!("macros.pick_title")))
                    .child(Button::new("no-key").ghost().small().label(tr!("macros.no_key")).on_click(move |_, _, cx| {
                        let _ = none.update(cx, |this, cx| {
                            this.macro_ui.error = this.model.assign_macro(n, None).err().map(|e| format!("{e:#}"));
                            this.macro_ui.choosing_key = false;
                            cx.notify();
                        });
                    })),
            )
            .child(v_flex().gap_1().items_center().py_1().children(rows))
            .child(muted(&theme, tr!("macros.pick_help")))
    });

    let save = view.clone();
    let discard = view.clone();
    let save_bar = h_flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_3()
        .pt_3()
        .border_t_1()
        .border_color(theme.border)
        .child(muted(&theme, tr!("macros.storage", used = used, room = room)).when(over, |el| el.text_color(theme.danger)))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .when(ui.dirty, |el| el.child(muted(&theme, tr!("macros.unsaved"))))
                .child(Button::new("discard").ghost().label(tr!("macros.discard")).disabled(!ui.dirty).on_click(move |_, window, cx| {
                    let _ = discard.update(cx, |this, cx| {
                        this.macro_ui.draft = None;
                        this.macro_ui.dirty = false;
                        this.macro_ui.editing = None;
                        let _ = window;
                        cx.notify();
                    });
                }))
                .child(Button::new("save").primary().label(tr!("macros.save")).disabled(!ui.dirty || over).on_click(move |_, _, cx| {
                    let _ = save.update(cx, |this, cx| {
                        let all = this.macro_ui.draft.clone().unwrap_or_default();
                        match this.model.save_macros(&all) {
                            Ok(()) => {
                                this.macro_ui.dirty = false;
                                this.macro_ui.error = None;
                            }
                            Err(e) => this.macro_ui.error = Some(format!("{e:#}")),
                        }
                        cx.notify();
                    });
                })),
        );

    let editor = card(&theme)
        .flex_1()
        .min_w(px(420.))
        .gap_4()
        .child(header)
        .children(key_picker)
        .child(if steps_view.is_empty() { note(&theme, tr!("macros.no_steps")).into_any_element() } else { v_flex().gap_2().children(steps_view).into_any_element() })
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(div().flex_1().child(Input::new(&ui.text_input)))
                .child(Button::new("add-text").outline().label(if ui.editing.is_some() { tr!("macros.update_text") } else { tr!("macros.add_text") }).on_click(
                    move |_, window, cx| {
                        let _ = add_text.update(cx, |this, cx| {
                            this.macro_ui.add_text(window, cx);
                            cx.notify();
                        });
                    },
                )),
        )
        .child(
            h_flex()
                .flex_wrap()
                .gap_2()
                .items_center()
                .child(
                    Button::new("record")
                        .when(recording, |b| b.danger())
                        .when(!recording, |b| b.outline())
                        .icon(Icon::empty().path(if recording { "icons/square.svg" } else { "icons/circle-dot.svg" }))
                        .label(if recording { tr!("macros.stop_recording") } else { tr!("macros.record") })
                        .on_click(move |_, window, cx| {
                            let _ = record.update(cx, |this, cx| {
                                this.macro_ui.recording = !this.macro_ui.recording;
                                if this.macro_ui.recording {
                                    this.macro_ui.capture.focus(window, cx);
                                }
                                cx.notify();
                            });
                        }),
                )
                .child(wait(100, "wait-100"))
                .child(wait(500, "wait-500"))
                .child(wait(1000, "wait-1000")),
        )
        .child(recorder)
        .child(
            h_flex()
                .gap_6()
                .pt_3()
                .border_t_1()
                .border_color(theme.border)
                .items_center()
                .child(v_flex().flex_1().gap_0p5().child(div().child(MACRO_MATCH_OS.label())).when_some(MACRO_MATCH_OS.help(), |el, h| el.child(muted(&theme, h))))
                .child(controls::switch(app, &MACRO_MATCH_OS, cx)),
        )
        .child(
            v_flex()
                .gap_2()
                .pt_3()
                .border_t_1()
                .border_color(theme.border)
                .child(muted(&theme, tr!("macros.try", key = key_label(n).unwrap_or_else(|| tr!("macros.no_key")))))
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&ui.try_input)))
                        .child({
                            let view = view.clone();
                            let unsaved = ui.dirty || steps.is_empty();
                            Button::new("play")
                                .outline()
                                .icon(Icon::empty().path("icons/play.svg"))
                                .label(tr!("macros.play"))
                                .disabled(unsaved)
                                .tooltip(if unsaved { tr!("macros.play_save_first") } else { tr!("macros.play_help") })
                                .on_click(move |_, window, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        // Into the box, not wherever the focus was.
                                        this.macro_ui.try_input.update(cx, |s, cx| s.focus(window, cx));
                                        this.macro_ui.error = this.model.play_macro(n).err().map(|e| format!("{e:#}"));
                                        cx.notify();
                                    });
                                })
                        }),
                ),
        )
        .when_some(ui.error.clone(), |el, e| el.child(note(&theme, e)))
        .child(save_bar);

    // ---- examples ----
    let tpl = templates();
    let examples = card(&theme)
        .w(px(width))
        .gap_3()
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .flex_wrap()
                .gap_4()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(tr!("macros.examples")))
                .child(muted(&theme, tr!("macros.examples_hint"))),
        )
        .child(h_flex().flex_wrap().gap_2p5().children(tpl.into_iter().enumerate().map(|(i, (name, what, steps))| {
            let view = view.clone();
            let free = (0..stored.count).find(|n| draft[*n].is_empty() && stored.keys[*n].is_none());
            let label = SharedString::from(name.clone());
            v_flex()
                .id(("template", i))
                .w(px(220.))
                .gap_1()
                .p_3()
                .rounded_lg()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .when(free.is_some(), |el| el.cursor_pointer().hover(|el| el.border_color(theme.ring)))
                .on_click(move |_, window, cx| {
                    let Some(slot) = free else { return };
                    let (name, steps) = (name.clone(), steps.clone());
                    let _ = view.update(cx, |this, cx| {
                        if let Some(d) = this.macro_ui.draft.as_mut() {
                            d[slot] = steps;
                            this.macro_ui.dirty = true;
                        }
                        this.macro_ui.select(slot, window, cx);
                        this.macro_ui.rename(name.clone());
                        this.macro_ui.name_input.update(cx, |s, cx| s.set_value(name, window, cx));
                        this.macro_ui.choosing_key = true;
                        cx.notify();
                    });
                })
                .child(div().font_weight(FontWeight::SEMIBOLD).child(label))
                .child(muted(&theme, what))
        })));

    v_flex()
        .w(px(width))
        .gap_4()
        .child(h_flex().items_start().gap_4().child(list).child(editor))
        .child(examples)
        .into_any_element()
}

