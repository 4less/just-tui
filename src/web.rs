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

/// The element the interface is drawn into.
const TERMINAL: &str = "terminal";

/// Listen for keys on the document itself.
///
/// The backend offers this, but binds to the grid element — which has to be
/// focused to receive anything, and is replaced whenever the grid is rebuilt,
/// taking the listener with it. On the document there is nothing to focus and
/// nothing to lose. It also leaves us free to stop the page acting on a key
/// itself: an arrow would otherwise move the cursor and scroll the window.
fn listen_for_keys<F>(mut handler: F) -> Result<(), String>
where
    F: FnMut(input::KeyEvent) + 'static,
{
    use ratzilla::web_sys::wasm_bindgen::JsCast;
    use ratzilla::web_sys::wasm_bindgen::prelude::Closure;

    let document = ratzilla::web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "no document".to_owned())?;

    let closure = Closure::wrap(Box::new(move |event: ratzilla::web_sys::KeyboardEvent| {
        let key = input::from_web(&event);
        if key.code == input::KeyCode::Other {
            return;
        }
        if !input::is_the_page_s_business(key.code) {
            event.prevent_default();
        }
        handler(key);
    }) as Box<dyn FnMut(_)>);

    document
        .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
        .map_err(|_| "could not listen for keys".to_owned())?;
    // The listener outlives this function, so the closure must too.
    closure.forget();
    Ok(())
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
    // Into the page's own element, which has a fixed height: the backend
    // measures its parent to decide how many rows and columns there are, and
    // measuring the body would grow the grid to fit its own output.
    let backend =
        DomBackend::new_by_id(TERMINAL).map_err(|err| format!("no terminal in the page: {err}"))?;
    let mut terminal = Terminal::new(backend).map_err(|err| err.to_string())?;

    let handler = app.clone();
    listen_for_keys(move |event| {
        let mut app = handler.borrow_mut();
        let action = app.handle_key(event);
        // The two actions that hand the screen to another program have no
        // meaning in a page; the rest happen inside the application.
        match action {
            Action::Run { .. } => app.error("running a recipe needs a terminal — try `s` instead"),
            Action::Edit { .. } => app.error("$EDITOR is not available in the browser"),
            Action::View { path } => app.info(format!("would open {} in $PAGER", path.display())),
            Action::Copy(_) => app.info("copied"),
            Action::Quit | Action::None => {}
        }
    })?;

    terminal.draw_web(move |frame| {
        let mut app = app.borrow_mut();
        app.poll_background();
        app.tick();
        ui::draw(frame, &mut app);
    });

    Ok(())
}
