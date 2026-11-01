//! Interface translations. Strings live in assets/lang/*.json, embedded at compile time.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// Languages available in the interface.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// Code stored in the settings file.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }

    /// Name shown in the language switch, written in that language.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        match code {
            "en" => Some(Lang::En),
            "fr" => Some(Lang::Fr),
            _ => None,
        }
    }
}

static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
static FR: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Current language. Stored as a number so any thread can read it.
static CURRENT: AtomicU8 = AtomicU8::new(Lang::En as u8);

fn parse(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).expect("invalid language file")
}

fn table(lang: Lang) -> &'static HashMap<String, String> {
    match lang {
        Lang::En => EN.get_or_init(|| parse(include_str!("../assets/lang/en.json"))),
        Lang::Fr => FR.get_or_init(|| parse(include_str!("../assets/lang/fr.json"))),
    }
}

/// Sets the language used by `t` and `tf` from now on.
pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// The language currently in use.
pub fn lang() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::Fr as u8 {
        Lang::Fr
    } else {
        Lang::En
    }
}

/// Text for `key` in the current language. Falls back to English, then to the key itself.
pub fn t(key: &'static str) -> &'static str {
    table(lang())
        .get(key)
        .or_else(|| table(Lang::En).get(key))
        .map(String::as_str)
        .unwrap_or(key)
}

/// Text for `key` with placeholders such as `{version}` replaced by the given values.
pub fn tf(key: &'static str, args: &[(&str, &str)]) -> String {
    let mut text = t(key).to_owned();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Names between braces in a text, e.g. `{version}`.
    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
            .collect()
    }

    #[test]
    fn every_english_key_is_translated() {
        for key in table(Lang::En).keys() {
            assert!(table(Lang::Fr).contains_key(key), "missing in fr: {key}");
        }
    }

    #[test]
    fn no_french_key_is_orphaned() {
        for key in table(Lang::Fr).keys() {
            assert!(table(Lang::En).contains_key(key), "missing in en: {key}");
        }
    }

    #[test]
    fn translations_use_the_same_placeholders() {
        for (key, en) in table(Lang::En) {
            let fr = &table(Lang::Fr)[key];
            assert_eq!(
                placeholders(en),
                placeholders(fr),
                "placeholders differ: {key}"
            );
        }
    }
}
