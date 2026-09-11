//! Opening the submit form, resolving its defaults, and submitting.

use super::{Action, App, Mode};
use crate::batch;
use crate::config;
use crate::history::{self, Record};
use crate::slurm::{self, Cluster};
use crate::submit::{SaveScope, SubmitForm};

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

    /// Open the submit form for the selected recipe, detecting the cluster
    /// the first time it is needed.
    pub fn open_submit(&mut self) {
        let Some(namepath) = self.run_target() else {
            self.error("select a recipe to submit");
            return;
        };
        let Some(id) = self.selected_id else { return };

        // Detection is four calls to the controller. The form opens on free
        // text and the pick lists fill in when they answer.
        if self.cluster.is_none() && self.detecting.is_none() {
            let (sender, receiver) = std::sync::mpsc::channel();
            self.detecting = Some(receiver);
            std::thread::spawn(move || {
                let _ = sender.send(Cluster::detect());
            });
        }

        let base = self.selected_source().working_dir.clone();
        let module_dirs = self.tree.module_dirs(id);
        let scope_dir = module_dirs.last().cloned().unwrap_or_else(|| base.clone());
        let resolved = self.resolve_settings(id, &namepath);

        self.form = Some(SubmitForm::new(namepath, resolved, base, scope_dir));
        self.mode = Mode::Submit;
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

        match slurm::submit(&form.base, &form.namepath, &form.settings, form.batch()) {
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
                    ),
                    base: form.base.display().to_string(),
                    manifest: form
                        .plan
                        .as_ref()
                        .map(|plan| plan.manifest.display().to_string())
                        .unwrap_or_default(),
                    tasks: form.plan.as_ref().map_or(0, |plan| plan.count()),
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
