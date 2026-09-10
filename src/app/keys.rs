//! Key handling. Every mode gets its own handler; the normal-mode one
//! returns the [`Action`]s the event loop carries out.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, App, Mode, Pane};
use crate::submit::SaveScope;
use crate::tree::Kind;

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        self.status = None;
        match self.mode {
            Mode::Search => return self.handle_search_key(key),
            Mode::Args => return self.handle_args_key(key),
            Mode::Submit => return self.handle_submit_key(key),
            Mode::ConfigPick => return self.handle_config_pick_key(key),
            Mode::Jobs => return self.handle_jobs_key(key),
            Mode::ConfirmJob => return self.handle_confirm_key(key),
            Mode::History => return self.handle_history_key(key),
            Mode::Help => {
                self.mode = Mode::Normal;
                return Action::None;
            }
            Mode::Normal => {}
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('c') => Action::Quit,
                KeyCode::Char('d') => {
                    self.scroll(10);
                    Action::None
                }
                KeyCode::Char('u') => {
                    self.scroll(-10);
                    Action::None
                }
                _ => Action::None,
            };
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.refresh_visible();
                    Action::None
                } else {
                    Action::Quit
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll(1);
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll(-1);
                Action::None
            }
            KeyCode::Char('J') => {
                self.move_to_recipe(true);
                Action::None
            }
            KeyCode::Char('K') => {
                self.move_to_recipe(false);
                Action::None
            }
            KeyCode::Char('g') | KeyCode::Home => {
                match self.focus {
                    Pane::Tree => self.set_cursor(0),
                    Pane::Doc => self.doc_scroll = 0,
                    Pane::Code => self.code_scroll = 0,
                }
                Action::None
            }
            KeyCode::Char('G') | KeyCode::End => {
                match self.focus {
                    Pane::Tree => self.set_cursor(self.visible.len().saturating_sub(1)),
                    _ => self.scroll_target(i32::MAX / 4),
                }
                Action::None
            }
            KeyCode::PageDown => {
                self.scroll(10);
                Action::None
            }
            KeyCode::PageUp => {
                self.scroll(-10);
                Action::None
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if self.focus == Pane::Tree {
                    self.collapse();
                } else {
                    self.focus = Pane::Tree;
                }
                Action::None
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.expand();
                Action::None
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self
                    .selected()
                    .is_some_and(|n| n.is_container() || n.kind == Kind::Alias)
                {
                    if self.selected().is_some_and(|n| n.kind == Kind::Alias) {
                        let target = self
                            .selected()
                            .and_then(|n| n.alias_target.clone())
                            .unwrap_or_default();
                        let root = self.selected().map_or(0, |n| n.root_index);
                        if let Some(id) = self.tree.find_namepath(root, &target) {
                            self.selected_id = Some(id);
                            self.refresh_visible();
                        }
                    } else {
                        self.toggle_expand();
                    }
                    Action::None
                } else {
                    self.build_run("", false)
                }
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                Action::None
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                Action::None
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.focus = Pane::Tree;
                Action::None
            }
            KeyCode::Char('r') => self.build_run("", false),
            KeyCode::Char('n') => self.build_run("", true),
            KeyCode::Char('a') => {
                if self.run_target().is_some() {
                    self.args_input.clear();
                    self.mode = Mode::Args;
                } else {
                    self.error("select a recipe first");
                }
                Action::None
            }
            KeyCode::Char('p') => {
                self.toggle_private();
                Action::None
            }
            KeyCode::Char('m') => {
                self.toggle_groups();
                Action::None
            }
            KeyCode::Char('e') => {
                self.set_all_expanded(true);
                Action::None
            }
            KeyCode::Char('c') => {
                self.set_all_expanded(false);
                Action::None
            }
            KeyCode::Char('w') => {
                self.wrap_code = !self.wrap_code;
                self.code_scroll = 0;
                Action::None
            }
            KeyCode::Char('v') => {
                self.vertical_split = !self.vertical_split;
                Action::None
            }
            KeyCode::Char('f') => {
                self.zoom = !self.zoom;
                if self.zoom && self.focus == Pane::Tree {
                    self.focus = Pane::Code;
                }
                Action::None
            }
            KeyCode::Char('R') => {
                self.reload();
                Action::None
            }
            KeyCode::Char('y') => match self.selected().and_then(|n| n.info.as_ref()) {
                Some(info) => Action::Copy(info.code.clone()),
                None => {
                    self.error("nothing to copy");
                    Action::None
                }
            },
            KeyCode::Char('o') => {
                let target = self
                    .selected()
                    .and_then(|n| n.source.clone().map(|p| (p, n.info.as_ref())))
                    .map(|(path, info)| (path, info.map_or(1, |i| i.start_line)));
                match target {
                    Some((path, line)) => Action::Edit { path, line },
                    None => {
                        self.error("no source file for this entry");
                        Action::None
                    }
                }
            }
            KeyCode::Char('s') => {
                self.open_submit();
                Action::None
            }
            KeyCode::Char('S') => {
                self.open_jobs();
                Action::None
            }
            KeyCode::Char('H') => {
                self.open_history(None);
                Action::None
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.search.clear();
                self.mode = Mode::Normal;
                self.refresh_visible();
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                if self.selected().is_none_or(|n| n.kind != Kind::Recipe) {
                    self.select_first_recipe();
                }
            }
            KeyCode::Backspace => {
                self.search.pop();
                self.refresh_visible();
            }
            KeyCode::Down => self.move_cursor(1),
            KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char(c) => {
                self.search.push(c);
                self.selected_id = None;
                self.refresh_visible();
                self.select_first_recipe();
            }
            _ => {}
        }
        Action::None
    }

    fn handle_submit_key(&mut self, key: KeyEvent) -> Action {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return Action::None;
        }
        // Keys that need the app, not just the form.
        match key.code {
            KeyCode::Esc => {
                self.form = None;
                self.mode = Mode::Normal;
                return Action::None;
            }
            KeyCode::Enter => {
                self.submit_job();
                return Action::None;
            }
            KeyCode::F(2) => return self.save_scope_action(SaveScope::Module),
            KeyCode::F(3) => return self.save_scope_action(SaveScope::Project),
            KeyCode::F(4) => return self.save_scope_action(SaveScope::Recipe),
            KeyCode::F(5) => {
                self.mode = Mode::ConfigPick;
                return Action::None;
            }
            KeyCode::F(6) => {
                let namepath = self.form.as_ref().map(|form| form.namepath.clone());
                self.open_history(namepath);
                return Action::None;
            }
            KeyCode::Left | KeyCode::Right => {
                let delta = if key.code == KeyCode::Left { -1 } else { 1 };
                if let (Some(form), Some(cluster)) = (self.form.as_mut(), self.cluster.as_ref()) {
                    form.cycle(cluster, delta);
                }
                return Action::None;
            }
            _ => {}
        }

        let Some(form) = self.form.as_mut() else {
            return Action::None;
        };
        match key.code {
            KeyCode::Up | KeyCode::BackTab => form.move_field(-1),
            KeyCode::Down | KeyCode::Tab => form.move_field(1),
            KeyCode::Delete => form.clear_field(),
            KeyCode::Backspace => form.pop_char(),
            KeyCode::Char(c) => form.push_char(c),
            _ => {}
        }
        Action::None
    }

    fn handle_config_pick_key(&mut self, key: KeyEvent) -> Action {
        let Some(form) = self.form.as_mut() else {
            self.mode = Mode::Normal;
            return Action::None;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Submit,
            KeyCode::Up | KeyCode::Char('k') => form.move_scope(-1),
            KeyCode::Down | KeyCode::Char('j') => form.move_scope(1),
            KeyCode::Enter => {
                let dir = form.selected_scope();
                self.mode = Mode::Submit;
                return match self.configs.ensure_file(&dir) {
                    Ok(path) => Action::Edit { path, line: 1 },
                    Err(err) => {
                        self.error(format!("could not open config: {err}"));
                        Action::None
                    }
                };
            }
            _ => {}
        }
        Action::None
    }

    fn handle_args_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                Action::None
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                let extra = self.args_input.clone();
                self.build_run(&extra, false)
            }
            KeyCode::Backspace => {
                self.args_input.pop();
                Action::None
            }
            KeyCode::Char(c) => {
                self.args_input.push(c);
                Action::None
            }
            _ => Action::None,
        }
    }

    /// The job browser. Up and down move through jobs; the log pane is paged
    /// rather than scrolled line by line, since it is read from the end.
    fn handle_jobs_key(&mut self, key: KeyEvent) -> Action {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => return Action::Quit,
                KeyCode::Char('d') => self.scroll_job_log(10),
                KeyCode::Char('u') => self.scroll_job_log(-10),
                _ => {}
            }
            return Action::None;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_job(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_job(-1),
            KeyCode::PageDown => self.scroll_job_log(10),
            KeyCode::PageUp => self.scroll_job_log(-10),
            KeyCode::Char('g') | KeyCode::Home => self.scroll_job_log(i32::MIN / 4),
            KeyCode::Char('G') | KeyCode::End => self.scroll_job_log(i32::MAX / 4),
            KeyCode::Tab | KeyCode::Char('t') => self.toggle_job_log(),
            KeyCode::Char('f') => self.cycle_job_filter(),
            KeyCode::Char('d') => self.cycle_job_range(),
            KeyCode::Char('r') => self.refresh_jobs(),
            KeyCode::Char('p') => self.toggle_job_auto(),
            KeyCode::Char('x') => self.ask_job_action(true, false),
            KeyCode::Char('X') => self.ask_job_action(true, true),
            KeyCode::Char('s') => self.ask_job_action(false, true),
            KeyCode::Char('u') => return self.reuse_job_settings(),
            KeyCode::Enter | KeyCode::Char('o') => {
                return match self.jobs.as_ref().and_then(|view| view.shown_path()) {
                    Some(path) => Action::View { path: path.clone() },
                    None => {
                        self.error("no log file to open for this job");
                        Action::None
                    }
                };
            }
            KeyCode::Char('y') => {
                return match self.jobs.as_ref().and_then(|view| view.shown_path()) {
                    Some(path) => Action::Copy(path.display().to_string()),
                    None => Action::None,
                };
            }
            _ => {}
        }
        Action::None
    }

    /// Jump from a job to the recipe that submitted it, with the settings it
    /// used loaded back into the form.
    fn reuse_job_settings(&mut self) -> Action {
        let Some(record) = self.job_record().cloned() else {
            self.error("this job was not submitted from just-tui");
            return Action::None;
        };
        self.mode = Mode::Normal;
        if !self.select_namepath(&record.namepath) {
            self.error(format!(
                "{} is not in this justfile any more",
                record.namepath
            ));
            return Action::None;
        }
        self.open_submit();
        if let Some(form) = self.form.as_mut() {
            form.settings = record.settings.clone();
        }
        self.info(format!("settings from job {}", record.job_id));
        Action::None
    }

    /// Touching the queue takes a deliberate `y`: Enter is too easy to lean on.
    fn handle_confirm_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => self.confirm_job_action(),
            _ => self.abandon_job_action(),
        }
        Action::None
    }

    fn handle_history_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = self.picker.as_ref().map_or(Mode::Normal, |pick| pick.from);
                self.picker = None;
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => self.move_history(1),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => self.move_history(-1),
            KeyCode::Enter => self.apply_history(),
            _ => {}
        }
        Action::None
    }
}
