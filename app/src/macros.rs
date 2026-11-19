//! Macros as the app shows them (steps: text, a shortcut, a key, a wait) and as QMK stores them: one
//! string per macro, each ended by a 0 byte, in the keyboard's macro buffer. In a string, text is
//! plain ASCII; a key step is 0x01 (SS_QMK_PREFIX), an action (1 tap, 2 press, 3 release) and a
//! keycode; a wait is 0x01 0x04, the milliseconds in digits, then '|'.

const PREFIX: u8 = 0x01;
const TAP: u8 = 0x01;
const DOWN: u8 = 0x02;
const UP: u8 = 0x03;
const DELAY: u8 = 0x04;
const DELAY_END: u8 = b'|';

/// Left Ctrl, Shift, Alt, GUI (Win / Cmd), then the right ones: HID usages 0xE0..0xE7.
pub const MODIFIERS: std::ops::RangeInclusive<u8> = 0xE0..=0xE7;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Typed as is. Only what a US layout types (ASCII); `typeable` says what fits.
    Text(String),
    /// Modifiers held (HID usages, in order) while a key is tapped: Ctrl + Shift + T.
    Shortcut { mods: Vec<u8>, key: u8 },
    /// One key tapped: Enter, an arrow, F5.
    Key(u8),
    /// A key held or let go on its own (from macros made elsewhere, e.g. in VIA).
    Press(u8),
    Release(u8),
    /// A pause, for apps that need time between steps.
    Wait(u16),
}

/// Whether a character can be typed by a macro (printable ASCII, tab, newline).
pub fn typeable(c: char) -> bool {
    c == '\n' || c == '\t' || (' '..='~').contains(&c)
}

/// One macro's bytes (without the 0 that ends it).
pub fn encode(steps: &[Step]) -> Vec<u8> {
    let mut out = Vec::new();
    let key = |out: &mut Vec<u8>, action: u8, code: u8| out.extend_from_slice(&[PREFIX, action, code]);
    for step in steps {
        match step {
            Step::Text(text) => out.extend(text.chars().filter(|c| typeable(*c)).map(|c| c as u8)),
            Step::Shortcut { mods, key: k } => {
                for m in mods {
                    key(&mut out, DOWN, *m);
                }
                key(&mut out, TAP, *k);
                for m in mods.iter().rev() {
                    key(&mut out, UP, *m);
                }
            }
            Step::Key(k) => key(&mut out, TAP, *k),
            Step::Press(k) => key(&mut out, DOWN, *k),
            Step::Release(k) => key(&mut out, UP, *k),
            Step::Wait(ms) => {
                out.extend_from_slice(&[PREFIX, DELAY]);
                out.extend_from_slice(ms.to_string().as_bytes());
                out.push(DELAY_END);
            }
        }
    }
    out
}

/// All macros' bytes, each ended by 0.
pub fn encode_all(macros: &[Vec<Step>]) -> Vec<u8> {
    macros.iter().flat_map(|m| encode(m).into_iter().chain([0])).collect()
}

/// One macro's steps. Presses, a tap and the matching releases become a Shortcut.
pub fn decode(bytes: &[u8]) -> Vec<Step> {
    let mut raw = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != PREFIX {
            match raw.last_mut() {
                Some(Step::Text(t)) => t.push(b as char),
                _ => raw.push(Step::Text((b as char).to_string())),
            }
            i += 1;
            continue;
        }
        let action = bytes.get(i + 1).copied().unwrap_or(0);
        if action == DELAY {
            let digits: String = bytes[i + 2..].iter().take_while(|c| c.is_ascii_digit()).map(|&c| c as char).collect();
            raw.push(Step::Wait(digits.parse().unwrap_or(0)));
            i += 2 + digits.len() + 1; // the digits, then '|'
            continue;
        }
        let code = bytes.get(i + 2).copied().unwrap_or(0);
        raw.push(match action {
            TAP => Step::Key(code),
            DOWN => Step::Press(code),
            _ => Step::Release(code),
        });
        i += 3;
    }
    group_shortcuts(raw)
}

/// Press(m1)..Press(mn) Key(k) Release(mn)..Release(m1), all modifiers -> Shortcut.
fn group_shortcuts(raw: Vec<Step>) -> Vec<Step> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let mods: Vec<u8> = raw[i..]
            .iter()
            .map_while(|s| match s {
                Step::Press(m) if MODIFIERS.contains(m) => Some(*m),
                _ => None,
            })
            .collect();
        if !mods.is_empty() {
            if let Some(Step::Key(k)) = raw.get(i + mods.len()) {
                let releases = &raw[i + mods.len() + 1..];
                let matched = mods.iter().rev().enumerate().all(|(j, m)| releases.get(j) == Some(&Step::Release(*m)));
                if matched {
                    out.push(Step::Shortcut { mods: mods.clone(), key: *k });
                    i += mods.len() * 2 + 1;
                    continue;
                }
            }
        }
        out.push(raw[i].clone());
        i += 1;
    }
    out
}

/// All macros in a buffer: `count` of them (empty ones too).
pub fn decode_all(buffer: &[u8], count: usize) -> Vec<Vec<Step>> {
    let mut macros: Vec<Vec<Step>> = buffer.split(|&b| b == 0).take(count).map(decode).collect();
    macros.resize(count, Vec::new());
    macros
}

/// QMK's keycode for "run macro n" (QK_MACRO).
pub const fn macro_keycode(n: usize) -> u16 {
    0x7700 + n as u16
}

/// The macro a keycode runs, if it's a macro key.
pub fn macro_of(keycode: u16) -> Option<usize> {
    (0x7700..=0x777F).contains(&keycode).then(|| (keycode - 0x7700) as usize)
}

/// A key's name (HID usage), as shown in steps.
pub fn key_name(code: u8) -> String {
    let named = match code {
        0x04..=0x1D => return ((b'A' + code - 0x04) as char).to_string(),
        0x1E..=0x26 => return ((b'1' + code - 0x1E) as char).to_string(),
        0x27 => "0",
        0x28 => "Enter",
        0x29 => "Esc",
        0x2A => "Backspace",
        0x2B => "Tab",
        0x2C => "Space",
        0x2D => "-",
        0x2E => "=",
        0x2F => "[",
        0x30 => "]",
        0x31 => "\\",
        0x33 => ";",
        0x34 => "'",
        0x35 => "`",
        0x36 => ",",
        0x37 => ".",
        0x38 => "/",
        0x39 => "Caps Lock",
        0x3A..=0x45 => return format!("F{}", code - 0x39),
        0x46 => "Print Screen",
        0x47 => "Scroll Lock",
        0x48 => "Pause",
        0x49 => "Insert",
        0x4A => "Home",
        0x4B => "Page Up",
        0x4C => "Delete",
        0x4D => "End",
        0x4E => "Page Down",
        0x4F => "→",
        0x50 => "←",
        0x51 => "↓",
        0x52 => "↑",
        0xE0 | 0xE4 => "Ctrl",
        0xE1 | 0xE5 => "Shift",
        0xE2 | 0xE6 => "Alt",
        0xE3 | 0xE7 => "Win",
        _ => return format!("key {code:#04x}"),
    };
    named.to_string()
}

/// A key the app saw pressed (GPUI's key name), as a HID usage.
pub fn usage_of(key: &str) -> Option<u8> {
    let c = key.chars().next()?;
    if key.chars().count() == 1 {
        let c = c.to_ascii_lowercase();
        return Some(match c {
            'a'..='z' => 0x04 + (c as u8 - b'a'),
            '1'..='9' => 0x1E + (c as u8 - b'1'),
            '0' => 0x27,
            '-' => 0x2D,
            '=' => 0x2E,
            '[' => 0x2F,
            ']' => 0x30,
            '\\' => 0x31,
            ';' => 0x33,
            '\'' => 0x34,
            '`' => 0x35,
            ',' => 0x36,
            '.' => 0x37,
            '/' => 0x38,
            ' ' => 0x2C,
            _ => return None,
        });
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=12).contains(n)) {
        return Some(0x39 + n);
    }
    Some(match key {
        "enter" => 0x28,
        "escape" => 0x29,
        "backspace" => 0x2A,
        "tab" => 0x2B,
        "space" => 0x2C,
        "delete" => 0x4C,
        "insert" => 0x49,
        "home" => 0x4A,
        "end" => 0x4D,
        "pageup" => 0x4B,
        "pagedown" => 0x4E,
        "right" => 0x4F,
        "left" => 0x50,
        "down" => 0x51,
        "up" => 0x52,
        _ => return None,
    })
}

/// Bytes the macros take, with each one's ending 0.
pub fn size_of(macros: &[Vec<Step>]) -> usize {
    encode_all(macros).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let steps = vec![
            Step::Text("Best regards,".into()),
            Step::Key(0x28),
            Step::Shortcut { mods: vec![0xE0, 0xE1], key: 0x17 },
            Step::Wait(250),
            Step::Press(0xE1),
            Step::Release(0xE1),
        ];
        assert_eq!(decode(&encode(&steps)), steps);
    }

    #[test]
    fn bytes_as_via_writes_them() {
        // Ctrl + C: press Ctrl, tap C, release Ctrl; then a 50 ms wait.
        let steps = [Step::Shortcut { mods: vec![0xE0], key: 0x06 }, Step::Wait(50)];
        assert_eq!(encode(&steps), [1, 2, 0xE0, 1, 1, 0x06, 1, 3, 0xE0, 1, 4, b'5', b'0', b'|']);
    }

    #[test]
    fn buffers_split_into_macros() {
        let all = encode_all(&[vec![Step::Text("hi".into())], vec![], vec![Step::Key(0x28)]]);
        let macros = decode_all(&all, 4);
        assert_eq!(macros.len(), 4);
        assert_eq!(macros[0], [Step::Text("hi".into())]);
        assert!(macros[1].is_empty() && macros[3].is_empty());
        assert_eq!(macros[2], [Step::Key(0x28)]);
    }

    #[test]
    fn untypeable_text_is_left_out() {
        assert_eq!(encode(&[Step::Text("Café".into())]), b"Caf");
    }

    #[test]
    fn keys_from_the_app() {
        assert_eq!(usage_of("t"), Some(0x17));
        assert_eq!(usage_of("f5"), Some(0x3E));
        assert_eq!(usage_of("enter"), Some(0x28));
        assert_eq!(key_name(0x3E), "F5");
        assert_eq!(macro_of(macro_keycode(3)), Some(3));
    }
}
