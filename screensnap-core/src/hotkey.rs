//! The user-defined capture shortcut.
//!
//! A [`Hotkey`] is a set of modifiers plus one physical key, named by its W3C
//! `KeyboardEvent.code` (`"KeyA"`, `"Digit2"`, `"F5"`, `"Space"` …). That one
//! naming serves every side of the app:
//!
//! - the settings window's recorder ([`crate::ui::hotkey_recorder`]) builds one
//!   from an agg-gui key event with [`Hotkey::from_key_event`];
//! - the native shell registers it with the OS through
//!   [`Hotkey::to_accelerator`], whose text the `global-hotkey` crate parses
//!   (`"super+shift+Digit2"`);
//! - settings persist it in the same accelerator form (serde as a string);
//! - people read it through [`Hotkey::display`] (`⇧⌘2` on a Mac, `Ctrl+Shift+2`
//!   elsewhere). Labels only use glyphs agg-gui's bundled fonts can draw.
//!
//! agg-gui reports keys already translated through the keyboard layout, but
//! with Cmd or Ctrl held it hands back the key *without* modifiers (`'2'`, not
//! `'@'`), so mapping its characters back to physical codes is exact for
//! letters and digits; punctuation follows the US layout. That is why a hotkey
//! must hold Cmd or Ctrl unless it is a function key ([`Hotkey::is_valid`]) —
//! which also keeps a global shortcut from swallowing ordinary typing.

use std::fmt;
use std::str::FromStr;

use agg_gui::{Key, Modifiers};
use serde::{Deserialize, Serialize};

/// Modifier keys of a [`Hotkey`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HotkeyMods {
    /// Cmd on macOS, the Windows key on Windows, Super on Linux.
    pub cmd: bool,
    pub ctrl: bool,
    /// Option on macOS.
    pub alt: bool,
    pub shift: bool,
}

impl HotkeyMods {
    pub fn from_agg(m: Modifiers) -> Self {
        Self {
            cmd: m.meta,
            ctrl: m.ctrl,
            alt: m.alt,
            shift: m.shift,
        }
    }

    pub fn is_empty(&self) -> bool {
        !(self.cmd || self.ctrl || self.alt || self.shift)
    }
}

/// How [`Hotkey::display`] spells a shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayStyle {
    /// `⌥⇧⌘2` — macOS's symbol order. Control is spelled `Ctrl+` in
    /// front (`Ctrl+⇧⌘2`): agg-gui's bundled fonts have no `⌃` glyph.
    Mac,
    /// `Ctrl+Alt+Shift+Win+2`.
    Windows,
    /// `Ctrl+Alt+Shift+Super+2`.
    Linux,
}

impl DisplayStyle {
    /// The style of the OS this build targets.
    pub fn native() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// A global shortcut: modifiers plus one physical key code.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub mods: HotkeyMods,
    /// W3C `KeyboardEvent.code` name, e.g. `"Digit2"`. Always one of
    /// [`KEY_CODES`].
    pub code: String,
}

/// Every key code a hotkey may use, with its short label for display.
pub const KEY_CODES: &[(&str, &str)] = &[
    ("KeyA", "A"),
    ("KeyB", "B"),
    ("KeyC", "C"),
    ("KeyD", "D"),
    ("KeyE", "E"),
    ("KeyF", "F"),
    ("KeyG", "G"),
    ("KeyH", "H"),
    ("KeyI", "I"),
    ("KeyJ", "J"),
    ("KeyK", "K"),
    ("KeyL", "L"),
    ("KeyM", "M"),
    ("KeyN", "N"),
    ("KeyO", "O"),
    ("KeyP", "P"),
    ("KeyQ", "Q"),
    ("KeyR", "R"),
    ("KeyS", "S"),
    ("KeyT", "T"),
    ("KeyU", "U"),
    ("KeyV", "V"),
    ("KeyW", "W"),
    ("KeyX", "X"),
    ("KeyY", "Y"),
    ("KeyZ", "Z"),
    ("Digit0", "0"),
    ("Digit1", "1"),
    ("Digit2", "2"),
    ("Digit3", "3"),
    ("Digit4", "4"),
    ("Digit5", "5"),
    ("Digit6", "6"),
    ("Digit7", "7"),
    ("Digit8", "8"),
    ("Digit9", "9"),
    ("Minus", "-"),
    ("Equal", "="),
    ("BracketLeft", "["),
    ("BracketRight", "]"),
    ("Backslash", "\\"),
    ("Semicolon", ";"),
    ("Quote", "'"),
    ("Comma", ","),
    ("Period", "."),
    ("Slash", "/"),
    ("Backquote", "`"),
    ("Space", "Space"),
    ("Enter", "Return"),
    ("Tab", "Tab"),
    ("Backspace", "Delete"),
    ("Delete", "Fwd Del"),
    ("Home", "Home"),
    ("End", "End"),
    ("PageUp", "Page Up"),
    ("PageDown", "Page Down"),
    ("ArrowUp", "Up"),
    ("ArrowDown", "Down"),
    ("ArrowLeft", "Left"),
    ("ArrowRight", "Right"),
    ("F1", "F1"),
    ("F2", "F2"),
    ("F3", "F3"),
    ("F4", "F4"),
    ("F5", "F5"),
    ("F6", "F6"),
    ("F7", "F7"),
    ("F8", "F8"),
    ("F9", "F9"),
    ("F10", "F10"),
    ("F11", "F11"),
    ("F12", "F12"),
    ("F13", "F13"),
    ("F14", "F14"),
    ("F15", "F15"),
    ("F16", "F16"),
    ("F17", "F17"),
    ("F18", "F18"),
    ("F19", "F19"),
    ("F20", "F20"),
    ("PrintScreen", "Print Screen"),
];

/// Punctuation characters (US layout) and the physical key that types them.
const PUNCTUATION: &[(char, &str)] = &[
    ('-', "Minus"),
    ('=', "Equal"),
    ('[', "BracketLeft"),
    (']', "BracketRight"),
    ('\\', "Backslash"),
    (';', "Semicolon"),
    ('\'', "Quote"),
    (',', "Comma"),
    ('.', "Period"),
    ('/', "Slash"),
    ('`', "Backquote"),
    (' ', "Space"),
];

fn known_code(code: &str) -> Option<&'static str> {
    KEY_CODES.iter().find(|(c, _)| *c == code).map(|(c, _)| *c)
}

fn code_label(code: &str) -> &str {
    KEY_CODES
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(code, |(_, label)| label)
}

/// The physical key code for an agg-gui key, or `None` for keys a hotkey
/// cannot use (modifier keys pressed alone, media keys …).
pub fn code_for_key(key: &Key) -> Option<&'static str> {
    match key {
        Key::Char(c) => {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() {
                let name = format!("Key{}", c.to_ascii_uppercase());
                known_code(&name)
            } else if c.is_ascii_digit() {
                known_code(&format!("Digit{c}"))
            } else {
                PUNCTUATION
                    .iter()
                    .find(|(p, _)| *p == c)
                    .map(|(_, code)| *code)
            }
        }
        Key::Enter => Some("Enter"),
        Key::Tab => Some("Tab"),
        Key::Backspace => Some("Backspace"),
        Key::Delete => Some("Delete"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::PageUp => Some("PageUp"),
        Key::PageDown => Some("PageDown"),
        Key::ArrowUp => Some("ArrowUp"),
        Key::ArrowDown => Some("ArrowDown"),
        Key::ArrowLeft => Some("ArrowLeft"),
        Key::ArrowRight => Some("ArrowRight"),
        Key::Other(name) => known_code(name),
        Key::Insert | Key::Escape => None,
    }
}

fn is_function_key(code: &str) -> bool {
    code == "PrintScreen"
        || code
            .strip_prefix('F')
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

impl Hotkey {
    /// Build a hotkey from parts, or `None` when the code is unknown.
    pub fn new(mods: HotkeyMods, code: &str) -> Option<Self> {
        known_code(code).map(|code| Self {
            mods,
            code: code.to_string(),
        })
    }

    /// The default shortcut: ⇧⌘2 on macOS (unused by the system, next to its
    /// own ⇧⌘3/4/5), Ctrl+Shift+2 elsewhere (where the Windows key combos
    /// belong to the shell).
    pub fn default_for_platform() -> Self {
        let mods = if cfg!(target_os = "macos") {
            HotkeyMods {
                cmd: true,
                shift: true,
                ..HotkeyMods::default()
            }
        } else {
            HotkeyMods {
                ctrl: true,
                shift: true,
                ..HotkeyMods::default()
            }
        };
        Self {
            mods,
            code: "Digit2".to_string(),
        }
    }

    /// The hotkey a key press in the recorder describes, if it is one a
    /// global shortcut may use ([`Hotkey::is_valid`]).
    pub fn from_key_event(key: &Key, modifiers: Modifiers) -> Option<Self> {
        let code = code_for_key(key)?;
        let hotkey = Self {
            mods: HotkeyMods::from_agg(modifiers),
            code: code.to_string(),
        };
        hotkey.is_valid().then_some(hotkey)
    }

    /// Whether this may be a global shortcut: a function key (alone or with
    /// modifiers), or any key with Cmd or Ctrl held.
    pub fn is_valid(&self) -> bool {
        known_code(&self.code).is_some()
            && (is_function_key(&self.code) || self.mods.cmd || self.mods.ctrl)
    }

    /// The accelerator text the `global-hotkey` crate parses, and the form
    /// settings persist: `"super+shift+Digit2"`.
    pub fn to_accelerator(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.mods.cmd {
            parts.push("super");
        }
        if self.mods.ctrl {
            parts.push("control");
        }
        if self.mods.alt {
            parts.push("alt");
        }
        if self.mods.shift {
            parts.push("shift");
        }
        parts.push(&self.code);
        parts.join("+")
    }

    /// Human-readable form, e.g. `⇧⌘2` or `Ctrl+Shift+2`.
    pub fn display(&self, style: DisplayStyle) -> String {
        let key = code_label(&self.code);
        match style {
            DisplayStyle::Mac => {
                let mut s = String::new();
                if self.mods.ctrl {
                    s.push_str("Ctrl+");
                }
                if self.mods.alt {
                    s.push('\u{2325}');
                }
                if self.mods.shift {
                    s.push('\u{21e7}');
                }
                if self.mods.cmd {
                    s.push('\u{2318}');
                }
                s.push_str(key);
                s
            }
            DisplayStyle::Windows | DisplayStyle::Linux => {
                let mut parts: Vec<&str> = Vec::new();
                if self.mods.ctrl {
                    parts.push("Ctrl");
                }
                if self.mods.alt {
                    parts.push("Alt");
                }
                if self.mods.shift {
                    parts.push("Shift");
                }
                if self.mods.cmd {
                    parts.push(if style == DisplayStyle::Windows {
                        "Win"
                    } else {
                        "Super"
                    });
                }
                parts.push(key);
                parts.join("+")
            }
        }
    }
}

/// Why accelerator text could not be read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseHotkeyError(pub String);

impl fmt::Display for ParseHotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid hotkey: {}", self.0)
    }
}

impl std::error::Error for ParseHotkeyError {}

impl FromStr for Hotkey {
    type Err = ParseHotkeyError;

    /// Parse [`Hotkey::to_accelerator`]'s form (modifier names are
    /// case-insensitive; `cmd`/`command`/`meta` are accepted for `super`,
    /// `ctrl` for `control`, `option` for `alt`).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseHotkeyError(s.to_string());
        let mut mods = HotkeyMods::default();
        let mut code = None;
        for part in s.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "super" | "cmd" | "command" | "meta" => mods.cmd = true,
                "control" | "ctrl" => mods.ctrl = true,
                "alt" | "option" => mods.alt = true,
                "shift" => mods.shift = true,
                _ if code.is_none() => code = Some(known_code(part).ok_or_else(err)?),
                _ => return Err(err()),
            }
        }
        let code = code.ok_or_else(err)?;
        let hotkey = Self {
            mods,
            code: code.to_string(),
        };
        if hotkey.is_valid() {
            Ok(hotkey)
        } else {
            Err(err())
        }
    }
}

impl Serialize for Hotkey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_accelerator())
    }
}

impl<'de> Deserialize<'de> for Hotkey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(cmd: bool, ctrl: bool, alt: bool, shift: bool) -> Modifiers {
        Modifiers {
            meta: cmd,
            ctrl,
            alt,
            shift,
        }
    }

    #[test]
    fn digits_and_letters_map_to_physical_codes() {
        let h = Hotkey::from_key_event(&Key::Char('2'), mods(true, false, false, true)).unwrap();
        assert_eq!(h.code, "Digit2");
        assert_eq!(h.to_accelerator(), "super+shift+Digit2");
        let h = Hotkey::from_key_event(&Key::Char('A'), mods(false, true, false, false)).unwrap();
        assert_eq!(h.code, "KeyA");
        let h = Hotkey::from_key_event(&Key::Char('/'), mods(true, false, false, false)).unwrap();
        assert_eq!(h.code, "Slash");
    }

    #[test]
    fn plain_keys_need_cmd_or_ctrl_but_function_keys_do_not() {
        assert!(
            Hotkey::from_key_event(&Key::Char('a'), mods(false, false, false, false)).is_none()
        );
        assert!(Hotkey::from_key_event(&Key::Char('a'), mods(false, false, true, true)).is_none());
        let f5 = Hotkey::from_key_event(&Key::Other("F5".into()), Modifiers::default()).unwrap();
        assert_eq!(f5.to_accelerator(), "F5");
        assert!(Hotkey::from_key_event(
            &Key::Other("Shift".into()),
            mods(true, false, false, true)
        )
        .is_none());
        assert!(Hotkey::from_key_event(&Key::Escape, mods(true, false, false, false)).is_none());
    }

    #[test]
    fn accelerator_round_trips() {
        for text in [
            "super+shift+Digit2",
            "control+alt+KeyS",
            "F13",
            "super+PrintScreen",
        ] {
            let h: Hotkey = text.parse().unwrap();
            assert_eq!(h.to_accelerator(), text);
        }
        let h: Hotkey = "Cmd+Shift+Digit4".parse().unwrap();
        assert_eq!(h.to_accelerator(), "super+shift+Digit4");
        assert!("shift+KeyA".parse::<Hotkey>().is_err());
        assert!("super+Nope".parse::<Hotkey>().is_err());
        assert!("super+KeyA+KeyB".parse::<Hotkey>().is_err());
    }

    #[test]
    fn display_styles() {
        let h: Hotkey = "control+alt+shift+super+Digit2".parse().unwrap();
        assert_eq!(
            h.display(DisplayStyle::Mac),
            "Ctrl+\u{2325}\u{21e7}\u{2318}2"
        );
        assert_eq!(h.display(DisplayStyle::Windows), "Ctrl+Alt+Shift+Win+2");
        assert_eq!(h.display(DisplayStyle::Linux), "Ctrl+Alt+Shift+Super+2");
    }

    #[test]
    fn serde_uses_accelerator_text() {
        let h = Hotkey::default_for_platform();
        let json = serde_json::to_string(&h).unwrap();
        let back: Hotkey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
        assert!(serde_json::from_str::<Hotkey>("\"KeyA\"").is_err());
    }
}
