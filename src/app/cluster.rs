//! Opening the submit form, resolving its defaults, and submitting.

use super::{Action, App, Mode};
use crate::config;
use crate::history::{self, Record};
use crate::slurm::{self, Cluster};
use crate::submit::{SaveScope, SubmitForm};

impl App {
    /// Open the submit form for the selected recipe, detecting the cluster
    /// the first time it is needed.
    pub fn open_submit(&mut self) {
        let Some(namepath) = self.run_target() else {
            self.error("select a recipe to submit");
            return;
        };
        let Some(id) = self.selected_id else { return };

        if self.cluster.is_none() {
            self.cluster = Some(Cluster::detect());
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

        match slurm::submit(&form.base, &form.namepath, &form.settings) {
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
                    command: slurm::preview_command(&form.base, &form.namepath, &form.settings),
                    base: form.base.display().to_string(),
                    settings: form.settings.clone(),
                };
                let name = record.name.clone();

                let saved = self
                    .configs
                    .remember(&form.namepath, &form.settings)
                    .and_then(|()| self.history.append(record));
                match saved {
                    Ok(()) => self.info(format!("submitted {name} as job {job_id} — log {out}")),
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
