//! Opening the submit form, resolving its defaults, and submitting.

use super::{Action, App, Mode};
use crate::batch;
use crate::chain::Chain;
use crate::config;
use crate::history::{self, Record};
use crate::just;
use crate::slurm::{self, Cluster, Settings};
use crate::submit::{ChainForm, ChainRow, Purpose, SaveScope, SubmitForm};

impl App {
    /// Take delivery of the cluster description, if it has arrived.
    pub fn poll_cluster(&mut self) {
        let Some(receiver) = self.detecting.as_ref() else {
            return;
        };
        if let Ok(cluster) = receiver.try_recv() {
            self.cluster = Some(cluster);
            self.detecting = None;
        }
    }

    /// Submit the selected recipe: the chain chooser when it has
    /// dependencies, the form alone when it does not.
    pub fn open_submit(&mut self) {
        let Some(namepath) = self.run_target() else {
            self.error("select a recipe to submit");
            return;
        };
        if !self.open_chain(&namepath, Purpose::Submit) {
            self.open_submit_single();
        }
    }

    /// Run the selected recipe through the chooser, if it has dependencies.
    /// Says whether it did; a recipe without any is left to the caller.
    pub fn open_run_chain(&mut self, extra: &str, dry: bool) -> bool {
        let Some(namepath) = self.run_target() else {
            return false;
        };
        self.open_chain(
            &namepath,
            Purpose::Run {
                extra: extra.to_owned(),
                dry,
            },
        )
    }

    /// The chooser for the selected recipe, when its chain has more than one
    /// link. A problem with the chain is reported and counts as opened, so
    /// the caller does not fall back to running it with the problem in it.
    fn open_chain(&mut self, namepath: &str, purpose: Purpose) -> bool {
        let Some(id) = self.selected_id else {
            return false;
        };
        let root = self.selected().map_or(0, |n| n.root_index);

        let chain = match Chain::build(&self.tree, root, id) {
            Ok(chain) => chain,
            Err(problem) => {
                self.error(problem);
                return true;
            }
        };
        if chain.len() < 2 {
            return false;
        }

        if purpose == Purpose::Submit {
            self.start_detecting();
        }
        let base = self.selected_source().working_dir.clone();
        let rows: Vec<ChainRow> = chain
            .links
            .into_iter()
            .map(|link| {
                let module_dirs = self.tree.module_dirs(link.id);
                let scope_dir = module_dirs.last().cloned().unwrap_or_else(|| base.clone());
                let resolved = self.resolve_settings(link.id, &link.namepath);
                ChainRow::new(link, resolved, base.clone(), scope_dir)
            })
            .collect();
        let skipped = self.configs.skipped(namepath).to_vec();

        self.chain = Some(ChainForm::new(namepath.to_owned(), purpose, rows, &skipped));
        self.mode = Mode::Chain;
        true
    }

    /// Open the submit form for the selected recipe on its own, detecting the
    /// cluster the first time it is needed.
    pub fn open_submit_single(&mut self) {
        let Some(namepath) = self.run_target() else {
            self.error("select a recipe to submit");
            return;
        };
        let Some(id) = self.selected_id else { return };

        self.start_detecting();
        let base = self.selected_source().working_dir.clone();
        let module_dirs = self.tree.module_dirs(id);
        let scope_dir = module_dirs.last().cloned().unwrap_or_else(|| base.clone());
        let resolved = self.resolve_settings(id, &namepath);

        self.form = Some(SubmitForm::new(namepath, resolved, base, scope_dir));
        self.mode = Mode::Submit;
    }

    /// Detection is four calls to the controller. The form opens on free
    /// text and the pick lists fill in when they answer.
    fn start_detecting(&mut self) {
        if self.cluster.is_none() && self.detecting.is_none() {
            let (sender, receiver) = std::sync::mpsc::channel();
            self.detecting = Some(receiver);
            crate::world::spawn(move || {
                let _ = sender.send(Cluster::detect());
            });
        }
    }

    /// Open the form for one row of the chain.
    pub fn open_chain_step(&mut self, row: usize) {
        let Some(chain) = self.chain.as_mut() else {
            return;
        };
        let Some(entry) = chain.rows.get(row) else {
            return;
        };
        let mut form = SubmitForm::new(
            entry.link.namepath.clone(),
            entry.resolved.clone(),
            entry.base.clone(),
            entry.scope_dir.clone(),
        );
        form.adopt(entry.settings.clone());
        chain.step = Some(row);
        chain.cursor = row;
        self.form = Some(form);
        self.mode = Mode::Submit;
    }

    /// Back from a row's form to the chooser, keeping nothing typed there.
    pub fn leave_chain_step(&mut self) {
        if let Some(chain) = self.chain.as_mut() {
            chain.step = None;
        }
        self.form = None;
        self.mode = Mode::Chain;
    }

    /// Enter on a row's form: keep its values and move on to the next row
    /// that needs asking about, or to the confirmation.
    pub fn accept_chain_step(&mut self) {
        let Some(form) = self.form.take() else { return };
        if let Some(problem) = form.plan_error.clone() {
            self.form = Some(form);
            self.error(format!("each: {problem}"));
            return;
        }
        let Some(chain) = self.chain.as_mut() else {
            return;
        };
        let step = chain.step.unwrap_or(0);
        chain.accept(form.settings);
        match chain.next_needing_form(step + 1) {
            Some(next) => self.open_chain_step(next),
            None => self.mode = Mode::ConfirmChain,
        }
    }

    /// Enter in the chooser. For a submission, walk the rows that have no
    /// settings, then ask for the `y`. For a run, run them.
    pub fn continue_chain(&mut self) -> Action {
        let Some(chain) = self.chain.as_ref() else {
            return Action::None;
        };
        if chain.checked() == 0 {
            self.error("nothing is checked — space toggles a row, a checks them all");
            return Action::None;
        }
        if chain.is_run() {
            return self.run_chain();
        }
        match chain.next_needing_form(0) {
            Some(row) => self.open_chain_step(row),
            None => self.mode = Mode::ConfirmChain,
        }
        Action::None
    }

    /// One `just --no-deps` per checked row, in order. What was unchecked is
    /// remembered the same way a submission remembers it.
    fn run_chain(&mut self) -> Action {
        let Some(chain) = self.chain.take() else {
            return Action::None;
        };
        self.mode = Mode::Normal;
        let source = self.selected_source();
        let dir = source.working_dir.clone();
        let commands = chain.run_commands(&just::file_args(source));
        let dry = matches!(chain.purpose, Purpose::Run { dry: true, .. });
        if let Err(err) = self.configs.remember_skipped(&chain.head, &chain.skipped()) {
            self.error(format!("could not save the chooser: {err}"));
        }
        Action::Run { commands, dir, dry }
    }

    /// Hand every checked row to sbatch in order, each told which of the
    /// earlier ones it waits on. A failure part way stops there: what is
    /// already queued stays queued and is named, so it can be cancelled.
    pub fn submit_chain(&mut self) {
        let Some(chain) = self.chain.take() else {
            return;
        };
        self.mode = Mode::Normal;

        let mut ids: Vec<Option<String>> = vec![None; chain.rows.len()];
        let mut first = String::new();
        for (index, row) in chain.rows.iter().enumerate() {
            if !row.checked {
                continue;
            }
            let after: Vec<String> = chain
                .waits_on(index)
                .into_iter()
                .filter_map(|upstream| ids[upstream].clone())
                .collect();
            match self.submit_link(row, &after, &first) {
                Ok(job_id) => {
                    if first.is_empty() {
                        first = job_id.clone();
                    }
                    ids[index] = Some(job_id);
                }
                Err(problem) => {
                    let queued: Vec<&str> = ids.iter().flatten().map(String::as_str).collect();
                    self.error(match queued.is_empty() {
                        true => format!("{}: {problem}", row.link.namepath),
                        false => format!(
                            "{}: {problem} — already queued: {}",
                            row.link.namepath,
                            queued.join(", ")
                        ),
                    });
                    return;
                }
            }
        }

        if let Err(err) = self.configs.remember_skipped(&chain.head, &chain.skipped()) {
            self.error(format!("submitted, but could not save the chooser: {err}"));
            return;
        }
        let queued: Vec<&str> = ids.iter().flatten().map(String::as_str).collect();
        self.info(format!(
            "submitted {} jobs: {}",
            queued.len(),
            queued.join(" → ")
        ));
    }

    /// One link: write its manifest if it has one, submit it after the jobs
    /// named, and record it the way a single submission is recorded.
    fn submit_link(
        &mut self,
        row: &ChainRow,
        after: &[String],
        chain: &str,
    ) -> Result<String, String> {
        let namepath = &row.link.namepath;
        let settings: &Settings = &row.settings;
        let plan = batch::plan(&row.base, namepath, settings)
            .map_err(|problem| format!("each: {problem}"))?;
        if let Some(plan) = plan.as_ref()
            && let Err(err) = batch::write(&row.base, plan)
        {
            return Err(format!("could not write the manifest: {err}"));
        }
        let batch = plan
            .as_ref()
            .map(|plan| (plan.manifest.as_path(), plan.count()));

        let job_id = match slurm::submit(&row.base, namepath, settings, batch, Some(after)) {
            slurm::Submission::Ok { job_id } => job_id,
            slurm::Submission::Failed { message } => return Err(format!("sbatch: {message}")),
        };

        let (out, err) = slurm::resolved_log_paths(namepath, settings, &job_id);
        let record = Record {
            job_id: job_id.clone(),
            namepath: namepath.clone(),
            name: slurm::job_name(namepath, settings),
            at: history::now(),
            when: history::timestamp(),
            out,
            err,
            command: slurm::preview_command(&row.base, namepath, settings, batch, Some(after)),
            base: row.base.display().to_string(),
            manifest: plan
                .as_ref()
                .map(|plan| plan.manifest.display().to_string())
                .unwrap_or_default(),
            tasks: plan.as_ref().map_or(0, |plan| plan.count()),
            after: after.to_vec(),
            chain: match chain.is_empty() {
                true => job_id.clone(),
                false => chain.to_owned(),
            },
            settings: settings.clone(),
        };
        self.configs
            .remember(namepath, settings)
            .and_then(|()| self.history.append(record))
            .map_err(|err| format!("queued as {job_id}, but could not save settings: {err}"))?;
        Ok(job_id)
    }

    /// Merge the config chain for one recipe.
    fn resolve_settings(&mut self, id: usize, namepath: &str) -> config::Resolved {
        let dirs = config::scope_dirs(&self.project().working_dir, &self.tree.module_dirs(id));
        self.configs.resolve(&dirs, namepath)
    }

    /// Re-read the config files and refresh the open form from them.
    pub fn refresh_form(&mut self) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let namepath = form.namepath.clone();
        let root = self.selected().map_or(0, |n| n.root_index);
        let Some(id) = self.tree.find_namepath(root, &namepath) else {
            return;
        };

        self.configs.invalidate();
        let resolved = self.resolve_settings(id, &namepath);
        if let Some(form) = self.form.as_mut() {
            form.reset_to(resolved);
        }
    }

    /// Hand the job to sbatch, then remember what it was submitted with —
    /// once as the recipe's new default, once as a history record that keeps
    /// this job's settings for good.
    pub(super) fn submit_job(&mut self) {
        let Some(form) = self.form.take() else { return };
        self.mode = Mode::Normal;

        // The manifest has to be on disk before the first task starts.
        if let Some(plan) = form.plan.as_ref()
            && let Err(err) = batch::write(&form.base, plan)
        {
            self.error(format!("could not write the manifest: {err}"));
            return;
        }
        if let Some(problem) = form.plan_error.as_ref() {
            self.error(format!("each: {problem}"));
            return;
        }

        match slurm::submit(
            &form.base,
            &form.namepath,
            &form.settings,
            form.batch(),
            None,
        ) {
            slurm::Submission::Ok { job_id } => {
                let (out, err) = slurm::resolved_log_paths(&form.namepath, &form.settings, &job_id);
                let record = Record {
                    job_id: job_id.clone(),
                    namepath: form.namepath.clone(),
                    name: slurm::job_name(&form.namepath, &form.settings),
                    at: history::now(),
                    when: history::timestamp(),
                    out: out.clone(),
                    err,
                    command: slurm::preview_command(
                        &form.base,
                        &form.namepath,
                        &form.settings,
                        form.batch(),
                        None,
                    ),
                    base: form.base.display().to_string(),
                    manifest: form
                        .plan
                        .as_ref()
                        .map(|plan| plan.manifest.display().to_string())
                        .unwrap_or_default(),
                    tasks: form.plan.as_ref().map_or(0, |plan| plan.count()),
                    after: Vec::new(),
                    chain: String::new(),
                    settings: form.settings.clone(),
                };
                let name = record.name.clone();
                let tasks = record.tasks;

                let saved = self
                    .configs
                    .remember(&form.namepath, &form.settings)
                    .and_then(|()| self.history.append(record));
                match saved {
                    Ok(()) => self.info(match tasks {
                        0 => format!("submitted {name} as job {job_id} — log {out}"),
                        n => format!("submitted {name} as job {job_id} — {n} tasks"),
                    }),
                    Err(err) => self.error(format!(
                        "submitted {job_id}, but could not save settings: {err}"
                    )),
                }
            }
            slurm::Submission::Failed { message } => self.error(format!("sbatch: {message}")),
        }
    }

    /// Write the form's current values into a `.just-tui-cluster-config`.
    pub(super) fn save_scope(&mut self, scope: SaveScope) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let (dir, section) = form.save_target(scope, &self.project().working_dir);
        let settings = form.settings.clone();

        match self
            .configs
            .save_defaults(&dir, section.as_deref(), &settings)
        {
            Ok(path) => {
                let shown = crate::ui::shorten_home(&path.display().to_string());
                self.info(format!("saved {} defaults to {shown}", scope.label()));
                self.refresh_form();
            }
            Err(err) => self.error(format!("could not write config: {err}")),
        }
    }

    /// [`Self::save_scope`] in the shape the key handler wants.
    pub(super) fn save_scope_action(&mut self, scope: SaveScope) -> Action {
        self.save_scope(scope);
        Action::None
    }
}
