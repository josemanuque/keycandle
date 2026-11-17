//! Languages. All text is looked up by key in locales/<language>.yml (rust-i18n, compiled in);
//! English is the fallback for anything missing.
//!
//! The language follows the system unless the user picked one (kept in the app's preferences).
//! Adding a language: copy locales/en.yml to locales/<code>.yml, translate it, and add the code to
//! `LANGUAGES`.

use crate::prefs::Prefs;

/// (locale code, name in that language). Portuguese uses gpui-component's code so its own strings
/// follow too.
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("es", "Español"), ("pt-BR", "Português")];
const FALLBACK: &str = "en";

/// Pick the language at startup.
pub fn init() {
    let chosen = Prefs::load().language.filter(|code| is_supported(code));
    rust_i18n::set_locale(chosen.as_deref().unwrap_or_else(|| system_language()));
}

/// Switch language and remember the choice.
pub fn set(code: &str) {
    if !is_supported(code) {
        return;
    }
    rust_i18n::set_locale(code);
    let mut prefs = Prefs::load();
    prefs.language = Some(code.to_string());
    prefs.save();
}

pub fn current() -> String {
    rust_i18n::locale().to_string()
}

/// The system's language if we have it, else English.
fn system_language() -> &'static str {
    let system = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    LANGUAGES
        .iter()
        .map(|(code, _)| *code)
        .find(|code| system.starts_with(&code[..2]))
        .unwrap_or(FALLBACK)
}

fn is_supported(code: &str) -> bool {
    LANGUAGES.iter().any(|(c, _)| *c == code)
}

/// Text for a key (the key itself if it is missing everywhere, which shows up in review).
pub fn text(key: &str) -> String {
    rust_i18n::t!(key).into_owned()
}

/// Text for a key that may legitimately be absent (e.g. a setting without help).
pub fn optional(key: &str) -> Option<String> {
    let found = rust_i18n::t!(key);
    (found != key).then(|| found.into_owned())
}

/// Shorthand for the UI: `tr!("key")` or `tr!("key", name = value)`.
#[macro_export]
macro_rules! tr {
    ($key:expr) => {
        rust_i18n::t!($key).into_owned()
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        rust_i18n::t!($key, $($name = $value),+).into_owned()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every language has every key English has.
    #[test]
    fn languages_are_complete() {
        let keys = |code: &str| -> std::collections::BTreeSet<String> {
            let text = std::fs::read_to_string(format!("{}/locales/{code}.yml", env!("CARGO_MANIFEST_DIR"))).unwrap();
            let mut path: Vec<(usize, String)> = Vec::new();
            let mut out = std::collections::BTreeSet::new();
            for line in text.lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')) {
                let indent = line.len() - line.trim_start().len();
                let Some((name, rest)) = line.trim().split_once(':') else { continue };
                path.retain(|(i, _)| *i < indent);
                path.push((indent, name.trim().to_string()));
                if !rest.trim().is_empty() {
                    out.insert(path.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join("."));
                }
            }
            out
        };
        let english = keys("en");
        for (code, _) in LANGUAGES.iter().filter(|(c, _)| *c != "en") {
            let theirs = keys(code);
            let missing: Vec<_> = english.difference(&theirs).collect();
            assert!(missing.is_empty(), "{code} is missing {missing:?}");
        }
    }
}
