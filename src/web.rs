//! The browser front end.
//!
//! The same [`App`] the terminal drives, rendered into the page by ratzilla
//! and fed the browser's key events through [`crate::input`]. Everything it
//! would ask the operating system for is answered by [`crate::world::demo`],
//! so the interface here is the real one — only the cluster is canned.

use std::cell::RefCell;
use std::rc::Rc;

use ratzilla::backend::canvas::{CanvasBackend, CanvasBackendOptions};
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

/// Call something on a timer, for as long as the page is open.
fn every(millis: i32, work: Rc<dyn Fn()>) -> Result<(), String> {
    use ratzilla::web_sys::wasm_bindgen::JsCast;
    use ratzilla::web_sys::wasm_bindgen::prelude::Closure;

    let window = ratzilla::web_sys::window().ok_or_else(|| "no window".to_owned())?;
    let closure = Closure::wrap(Box::new(move || work()) as Box<dyn FnMut()>);
    window
        .set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            millis,
        )
        .map_err(|_| "could not start the clock".to_owned())?;
    closure.forget();
    Ok(())
}

/// The element the interface is drawn into.
const TERMINAL: &str = "terminal";

/// Where the build stamp, and then what the page knows about itself, goes.
const STAMP: &str = "stamp";

/// The pixel size the interface is drawn at: the element it lives in.
fn canvas_size() -> (u32, u32) {
    let element = ratzilla::web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(TERMINAL));
    match element {
        Some(element) => {
            let rect = element.get_bounding_client_rect();
            (
                rect.width().max(320.0) as u32,
                rect.height().max(240.0) as u32,
            )
        }
        None => (1200, 720),
    }
}

/// Which build this is — the same string `--version` prints natively. Shown
/// in the page so a stale bundle is obvious rather than baffling.
fn show(text: &str) {
    let element = ratzilla::web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(STAMP));
    if let Some(element) = element {
        element.set_text_content(Some(text));
    }
}

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

/// The part of the URL after `?`, lowercased.
fn query() -> Option<String> {
    let search = ratzilla::web_sys::window()?.location().search().ok()?;
    let search = search.trim_start_matches('?').to_ascii_lowercase();
    (!search.is_empty()).then_some(search)
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
    // A canvas, not a grid of elements. The DOM backend builds a <span> per
    // cell and rebuilds them every frame — on a full screen that is thousands
    // of nodes sixty times a second, and a single frame took long enough for
    // the browser to kill the script, which left the interface frozen on
    // whatever it had drawn first.
    let backend = CanvasBackend::new_with_options(
        CanvasBackendOptions::new()
            .grid_id(TERMINAL)
            .size(canvas_size()),
    )
    .map_err(|err| format!("no terminal in the page: {err}"))?;
    let mut terminal = Terminal::new(backend).map_err(|err| err.to_string())?;

    show(&format!(
        "{} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("JUST_TUI_COMMIT")
    ));

    // Drawing is driven from here rather than handed to the backend's own
    // animation loop. That loop re-arms itself at the end of a frame, so one
    // frame that does not finish stops it for good — and nothing after that,
    // no key and no background result, ever reaches the screen again.
    //
    // Here the interface is redrawn when something has happened: a key, or
    // the tick that moves the clock and collects finished background work.
    let terminal = Rc::new(RefCell::new(terminal));
    let redraw: Rc<dyn Fn()> = {
        let (terminal, app) = (terminal.clone(), app.clone());
        Rc::new(move || {
            let mut app = app.borrow_mut();
            app.poll_background();
            app.tick();
            let _ = terminal
                .borrow_mut()
                .draw(|frame| ui::draw(frame, &mut app));
        })
    };

    // `?debug` reports each key and what it selected, for when the interface
    // and the keyboard disagree about whether anything happened.
    let debug = query().as_deref() == Some("debug");

    let handler = app.clone();
    let on_key = redraw.clone();
    listen_for_keys(move |event| {
        {
            let mut app = handler.borrow_mut();
            let action = app.handle_key(event);
            // The two actions that hand the screen to another program have no
            // meaning in a page; the rest happen inside the application.
            match action {
                Action::Run { .. } => {
                    app.error("running a recipe needs a terminal — try `s` instead")
                }
                Action::Edit { .. } => app.error("$EDITOR is not available in the browser"),
                Action::View { path } => {
                    app.info(format!("would open {} in $PAGER", path.display()))
                }
                Action::Copy(_) => app.info("copied"),
                Action::Quit | Action::None => {}
            }
            if debug {
                let selected = app
                    .selected()
                    .map(|node| node.name.clone())
                    .unwrap_or_else(|| "nothing".to_owned());
                show(&format!("{:?} -> {selected}", event.code));
            }
        }
        on_key();
    })?;

    // A running job's clock moves every second, and background work lands
    // between keys; ten times a second covers both without busy-drawing.
    every(100, redraw.clone())?;
    redraw();
    Ok(())
}
