//! Interface translations. Strings live in assets/lang/*.json, embedded at compile time.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// Languages available in the interface.
/// To add one: a variant here, a JSON file in `assets/lang/`, a flag in `assets/flags/`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    En,
    Fr,
    Es,
    De,
    It,
}

impl Lang {
    /// Every language, in the order of the language menu. The position of a
    /// language in this list is also its number in `CURRENT`.
    pub const ALL: [Lang; 5] = [Lang::En, Lang::Fr, Lang::Es, Lang::De, Lang::It];

    /// Code stored in the settings file.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::Es => "es",
            Lang::De => "de",
            Lang::It => "it",
        }
    }

    /// Name shown in the language switch, written in that language.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
            Lang::Es => "Español",
            Lang::De => "Deutsch",
            Lang::It => "Italiano",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|lang| lang.code() == code)
    }
}

static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
static DE: OnceLock<HashMap<String, String>> = OnceLock::new();
static IT: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Current language, as its position in `Lang::ALL`. Stored as a number so any thread can read it.
static CURRENT: AtomicU8 = AtomicU8::new(0);

fn parse(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).expect("invalid language file")
}

fn table(lang: Lang) -> &'static HashMap<String, String> {
    match lang {
        Lang::En => EN.get_or_init(|| parse(include_str!("../assets/lang/en.json"))),
        Lang::Fr => FR.get_or_init(|| parse(include_str!("../assets/lang/fr.json"))),
        Lang::Es => ES.get_or_init(|| parse(include_str!("../assets/lang/es.json"))),
        Lang::De => DE.get_or_init(|| parse(include_str!("../assets/lang/de.json"))),
        Lang::It => IT.get_or_init(|| parse(include_str!("../assets/lang/it.json"))),
    }
}

/// Sets the language used by `t` and `tf` from now on.
pub fn set_lang(lang: Lang) {
    let index = Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0);
    CURRENT.store(index as u8, Ordering::Relaxed);
}

/// The language currently in use.
pub fn lang() -> Lang {
    let index = CURRENT.load(Ordering::Relaxed) as usize;
    Lang::ALL.get(index).copied().unwrap_or_default()
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
        for lang in Lang::ALL {
            for key in table(Lang::En).keys() {
                assert!(
                    table(lang).contains_key(key),
                    "missing in {}: {key}",
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn no_translated_key_is_orphaned() {
        for lang in Lang::ALL {
            for key in table(lang).keys() {
                assert!(
                    table(Lang::En).contains_key(key),
                    "unknown key in {}: {key}",
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn translations_use_the_same_placeholders() {
        for lang in Lang::ALL {
            for (key, en) in table(Lang::En) {
                let other = &table(lang)[key];
                assert_eq!(
                    placeholders(en),
                    placeholders(other),
                    "placeholders differ in {}: {key}",
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn no_translation_is_empty() {
        for lang in Lang::ALL {
            for (key, text) in table(lang) {
                assert!(
                    !text.trim().is_empty(),
                    "empty text in {}: {key}",
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn language_codes_round_trip() {
        for lang in Lang::ALL {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
        }
        assert_eq!(Lang::from_code("xx"), None);
    }

    #[test]
    fn the_current_language_can_be_set_and_read() {
        // Only this test changes the language, so it cannot disturb the others.
        for lang in Lang::ALL {
            set_lang(lang);
            assert_eq!(super::lang(), lang);
        }
        set_lang(Lang::En);
    }
}
