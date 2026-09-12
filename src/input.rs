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

    /// Translate what the browser reports into what the handlers expect.
    pub fn from_web(event: &ratzilla::event::KeyEvent) -> KeyEvent {
        use ratzilla::event::KeyCode as Web;

        let code = match event.code {
            Web::Char(c) => KeyCode::Char(c),
            Web::Enter => KeyCode::Enter,
            Web::Esc => KeyCode::Esc,
            // The browser has no BackTab; shift-tab arrives as a shifted tab.
            Web::Tab if event.shift => KeyCode::BackTab,
            Web::Tab => KeyCode::Tab,
            Web::Backspace => KeyCode::Backspace,
            Web::Delete => KeyCode::Delete,
            Web::Home => KeyCode::Home,
            Web::End => KeyCode::End,
            Web::Left => KeyCode::Left,
            Web::Right => KeyCode::Right,
            Web::Up => KeyCode::Up,
            Web::Down => KeyCode::Down,
            Web::PageUp => KeyCode::PageUp,
            Web::PageDown => KeyCode::PageDown,
            Web::F(n) => KeyCode::F(n),
            _ => KeyCode::Other,
        };

        let mut modifiers = KeyModifiers::NONE;
        if event.ctrl {
            modifiers = KeyModifiers(modifiers.0 | KeyModifiers::CONTROL.0);
        }
        if event.shift {
            modifiers = KeyModifiers(modifiers.0 | KeyModifiers::SHIFT.0);
        }
        if event.alt {
            modifiers = KeyModifiers(modifiers.0 | KeyModifiers::ALT.0);
        }
        KeyEvent::new(code, modifiers)
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::from_web;
