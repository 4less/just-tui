//! Key events, from whichever front end is driving.
//!
//! Natively these are crossterm's own types. Crossterm does not build for
//! WebAssembly — it reaches for mio and signal handling — so the browser gets
//! a stand-in with the same shape, and the browser's own key events are
//! translated into it. Everything above this line is then written once.

#[cfg(not(target_arch = "wasm32"))]
pub use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[cfg(target_arch = "wasm32")]
pub use browser::{KeyCode, KeyEvent, KeyModifiers};

#[cfg(target_arch = "wasm32")]
mod browser {
    /// The keys this application actually looks at.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum KeyCode {
        Char(char),
        Enter,
        Esc,
        Tab,
        BackTab,
        Backspace,
        Delete,
        Home,
        End,
        Left,
        Right,
        Up,
        Down,
        PageUp,
        PageDown,
        F(u8),
        /// Anything else, which every handler ignores.
        Other,
    }

    /// Only Control is ever tested for, but the shape matches crossterm's so
    /// the handlers read the same in both builds.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct KeyModifiers(u8);

    impl KeyModifiers {
        pub const NONE: Self = Self(0);
        pub const CONTROL: Self = Self(1);
        pub const SHIFT: Self = Self(2);
        pub const ALT: Self = Self(4);

        pub fn contains(self, other: Self) -> bool {
            self.0 & other.0 == other.0
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct KeyEvent {
        pub code: KeyCode,
        pub modifiers: KeyModifiers,
    }

    impl KeyEvent {
        pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
            Self { code, modifiers }
        }
    }

    /// Translate a browser `keydown` into what the handlers expect.
    ///
    /// Taken straight from the DOM event rather than through the backend: the
    /// backend binds its listener to the grid element, which is replaced when
    /// the grid is rebuilt, and the listener goes with it.
    pub fn from_web(event: &ratzilla::web_sys::KeyboardEvent) -> KeyEvent {
        let key = event.key();
        let mut chars = key.chars();

        let code = match (chars.next(), chars.next()) {
            // A printable key reports itself as a one-character string.
            (Some(c), None) => KeyCode::Char(c),
            _ => match key.as_str() {
                "Enter" => KeyCode::Enter,
                "Escape" => KeyCode::Esc,
                // The browser has no BackTab; shift-tab is a shifted tab.
                "Tab" if event.shift_key() => KeyCode::BackTab,
                "Tab" => KeyCode::Tab,
                "Backspace" => KeyCode::Backspace,
                "Delete" => KeyCode::Delete,
                "Home" => KeyCode::Home,
                "End" => KeyCode::End,
                "ArrowLeft" => KeyCode::Left,
                "ArrowRight" => KeyCode::Right,
                "ArrowUp" => KeyCode::Up,
                "ArrowDown" => KeyCode::Down,
                "PageUp" => KeyCode::PageUp,
                "PageDown" => KeyCode::PageDown,
                other => match other.strip_prefix('F').and_then(|n| n.parse().ok()) {
                    Some(n) => KeyCode::F(n),
                    None => KeyCode::Other,
                },
            },
        };

        let mut modifiers = KeyModifiers::NONE;
        if event.ctrl_key() {
            modifiers = KeyModifiers(modifiers.0 | KeyModifiers::CONTROL.0);
        }
        if event.shift_key() {
            modifiers = KeyModifiers(modifiers.0 | KeyModifiers::SHIFT.0);
        }
        if event.alt_key() {
            modifiers = KeyModifiers(modifiers.0 | KeyModifiers::ALT.0);
        }
        KeyEvent::new(code, modifiers)
    }

    /// Keys the page would otherwise act on itself — scrolling, mostly.
    pub fn is_the_page_s_business(code: KeyCode) -> bool {
        !matches!(
            code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Enter
                | KeyCode::Backspace
                | KeyCode::Char(' ')
                | KeyCode::F(_)
        )
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::{from_web, is_the_page_s_business};
