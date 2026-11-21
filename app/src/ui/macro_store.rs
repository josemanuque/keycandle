//! The keyboard's macros and the keys that run them, read and written over USB.
//!
//! Macros run from the Fn layer: Fn + the chosen key plays the macro and the key keeps its normal
//! job. Keys whose Fn layer already does something (media, lighting, devices) aren't offered.

use anyhow::Result;

use crate::device::Keyboard;
use crate::keyboard;
use crate::macros::{self, Step};

/// QMK's "transparent" keycode: the Fn layer passes the key through (nothing assigned).
const KC_TRANSPARENT: u16 = 0x0001;

#[derive(Clone)]
pub struct Macros {
    /// Slots the firmware has, and the bytes they share.
    pub count: usize,
    pub size: usize,
    /// As stored on the keyboard.
    pub saved: Vec<Vec<Step>>,
    /// The key (index into the profile's keys) that runs each macro with Fn, if any.
    pub keys: Vec<Option<usize>>,
    /// Keys whose Fn layer already does something other than a macro.
    pub fn_taken: Vec<bool>,
}

impl Macros {
    pub fn read(kb: &Keyboard) -> Result<Self> {
        let profile = keyboard::current();
        let (count, size) = kb.macro_space()?;
        let saved = macros::decode_all(&kb.read_macros(size)?, count);
        let (_, cols) = profile.matrix;
        let layer = kb.read_layer(profile.fn_layer, profile.matrix.0, cols)?;
        let mut keys = vec![None; count];
        let mut fn_taken = vec![false; profile.keys.len()];
        for (i, key) in profile.keys.iter().enumerate() {
            let code = layer[key.row * cols + key.col];
            match macros::macro_of(code) {
                Some(n) if n < count => keys[n] = Some(i),
                _ => fn_taken[i] = code != KC_TRANSPARENT && code != 0,
            }
        }
        Ok(Self { count, size, saved, keys, fn_taken })
    }

    /// Example macros for preview mode (KEYCANDLE_PREVIEW), as if read from a keyboard.
    pub fn sample() -> Self {
        let profile = keyboard::current();
        let key = |label: &str| profile.keys.iter().position(|k| k.label == label);
        const ENTER: u8 = 0x28;
        const LEFT_CTRL: u8 = 0xE0;
        const LEFT_SHIFT: u8 = 0xE1;
        let mut saved = vec![Vec::new(); 16];
        saved[0] = vec![Step::Text("Best regards,".into()), Step::Key(ENTER), Step::Text("Alex".into())];
        saved[1] = vec![Step::Text("Thanks for the quick reply! I'll take a look today and get back to you.".into())];
        saved[2] = vec![Step::Shortcut { mods: vec![LEFT_CTRL, LEFT_SHIFT], key: 0x11 }, Step::Wait(400), Step::Text("standup notes".into()), Step::Key(ENTER)];
        let mut keys = vec![None; 16];
        keys[0] = key("J");
        keys[1] = key("K");
        keys[2] = key("N");
        Self { count: 16, size: 2048, saved, keys, fn_taken: vec![false; profile.keys.len()] }
    }

    /// Bytes the macros would take (each ends with a 0); one byte of the buffer is kept free.
    pub fn room(&self) -> usize {
        self.size.saturating_sub(1)
    }

    /// Store these macros on the keyboard (all of them: the firmware rewrites its macro area).
    pub fn save(&mut self, kb: &Keyboard, all: &[Vec<Step>]) -> Result<()> {
        kb.write_macros(&macros::encode_all(all), self.size)?;
        self.saved = all.to_vec();
        Ok(())
    }

    /// Run macro `n` with Fn + `key` (an index into the profile's keys), or with no key.
    pub fn assign(&mut self, kb: &Keyboard, n: usize, key: Option<usize>) -> Result<()> {
        let profile = keyboard::current();
        let layer = profile.fn_layer as u8;
        let place = |i: usize| (profile.keys[i].row as u8, profile.keys[i].col as u8);
        if let Some(old) = self.keys[n] {
            let (row, col) = place(old);
            kb.set_key(layer, row, col, KC_TRANSPARENT)?;
        }
        if let Some(new) = key {
            anyhow::ensure!(!self.fn_taken[new], "Fn + {} already does something", profile.keys[new].label);
            // Another macro on that key gives it up.
            for other in self.keys.iter_mut().filter(|k| **k == Some(new)) {
                *other = None;
            }
            let (row, col) = place(new);
            kb.set_key(layer, row, col, macros::macro_keycode(n))?;
        }
        self.keys[n] = key;
        Ok(())
    }

    /// Put macros and their keys back after a firmware update (which clears them).
    pub fn restore(&self, kb: &Keyboard) -> Result<()> {
        let mut now = Self::read(kb)?;
        if self.saved.iter().any(|m| !m.is_empty()) {
            now.save(kb, &self.saved)?;
        }
        let count = now.count;
        for (n, key) in self.keys.iter().enumerate().filter(|(n, _)| *n < count) {
            if key.is_some() {
                now.assign(kb, n, *key)?;
            }
        }
        Ok(())
    }
}
