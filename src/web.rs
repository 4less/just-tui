//! The browser front end.
//!
//! The same [`App`] the terminal drives, rendered into the page by ratzilla
//! and fed the browser's key events through [`crate::input`]. Everything it
//! would ask the operating system for is answered by [`crate::world::demo`],
//! so the interface here is the real one — only the cluster is canned.

use std::cell::RefCell;
use std::rc::Rc;

use ratzilla::WebRenderer;
use ratzilla::backend::dom::DomBackend;
use ratzilla::ratatui::Terminal;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::app::{Action, App};
use crate::source::SourceCache;
use crate::{input, just, ui};

/// Entry point: called once the page has loaded.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    run().map_err(|err| wasm_bindgen::JsValue::from_str(&err))
}

/// The part of the URL after `#`, lowercased.
fn fragment() -> Option<String> {
    let hash = ratzilla::web_sys::window()?.location().hash().ok()?;
    let hash = hash.trim_start_matches('#').to_ascii_lowercase();
    (!hash.is_empty()).then_some(hash)
}

/// Everything that can go wrong here is worth saying in one voice, rather
/// than three kinds of error the page cannot tell apart.
fn run() -> Result<(), String> {
    let sources = vec![just::load(std::path::Path::new("."), None).map_err(|err| err.to_string())?];
    let mut app = App::new(sources, SourceCache::default());
    app.info("browser demo — the cluster is canned, everything else is the real thing");

    // `#jobs` opens the job browser straight away, so the documentation can
    // link to the screen it is describing.
    if fragment().as_deref() == Some("jobs") {
        app.open_jobs();
    }

    let app = Rc::new(RefCell::new(app));
    // The backend appends its grid to the page; CSS puts it between the
    // header and the footer.
    let backend = DomBackend::new().map_err(|err| format!("no terminal in the page: {err}"))?;
    let mut terminal = Terminal::new(backend).map_err(|err| err.to_string())?;

    let handler = app.clone();
    let listening = terminal.on_key_event(move |event| {
        let mut app = handler.borrow_mut();
        let action = app.handle_key(input::from_web(&event));
        // The two actions that hand the screen to another program have no
        // meaning in a page; the rest happen inside the application.
        match action {
            Action::Run { .. } => app.error("running a recipe needs a terminal — try `s` instead"),
            Action::Edit { .. } => app.error("$EDITOR is not available in the browser"),
            Action::View { path } => app.info(format!("would open {} in $PAGER", path.display())),
            Action::Copy(_) => app.info("copied"),
            Action::Quit | Action::None => {}
        }
    });
    listening.map_err(|err| format!("could not listen for keys: {err}"))?;

    terminal.draw_web(move |frame| {
        let mut app = app.borrow_mut();
        app.poll_background();
        app.tick();
        ui::draw(frame, &mut app);
    });

    Ok(())
}
