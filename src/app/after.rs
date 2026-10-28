//! Picking queued jobs for a submission to wait on: what `squeue` lists,
//! which rows are ticked, and the form the ids go back to.

use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, Sender, channel};

use super::{App, Mode};
use crate::slurm::{self, Job};

/// One row of the picker: a job of yours still on the queue, or an id picked
/// earlier whose job has left it since.
pub struct AfterRow {
    /// What `--dependency` is given. For the not-yet-started remainder of an
    /// array that is the array as a whole, since its own id shrinks as
    /// tasks start.
    pub id: String,
    /// Nothing for an id that is no longer queued.
    pub job: Option<Job>,
}

/// The picker: the queue as last fetched, the ids ticked, and the cursor.
pub struct AfterPick {
    pub rows: Vec<AfterRow>,
    pub chosen: BTreeSet<String>,
    pub cursor: usize,
    /// Why the list is empty, when it is.
    pub note: Option<String>,
    fetching: bool,
    sender: Sender<Option<Vec<Job>>>,
    inbox: Receiver<Option<Vec<Job>>>,
}

impl AfterPick {
    /// An empty picker with `chosen` already ticked, waiting for the queue.
    fn new(chosen: &[String]) -> Self {
        let (sender, inbox) = channel();
        let mut pick = Self {
            rows: Vec::new(),
            chosen: chosen.iter().cloned().collect(),
            cursor: 0,
            note: None,
            fetching: false,
            sender,
            inbox,
        };
        pick.rebuild(Vec::new());
        pick
    }

    /// A picker over a queue already in hand, so it can be driven without a
    /// scheduler to ask.
    #[cfg(test)]
    pub fn with(jobs: Vec<Job>, chosen: &[String]) -> Self {
        let mut pick = Self::new(chosen);
        pick.take(Some(jobs));
        pick
    }

    /// Whether the queue is still being asked for.
    pub fn fetching(&self) -> bool {
        self.fetching
    }

    /// Take delivery of a queue listing, or of `squeue` not answering.
    fn take(&mut self, fresh: Option<Vec<Job>>) {
        self.fetching = false;
        match fresh {
            Some(jobs) => {
                self.rebuild(jobs);
                self.note = self
                    .rows
                    .is_empty()
                    .then(|| "no jobs of yours are queued".to_owned());
            }
            None => {
                self.rebuild(Vec::new());
                self.note = Some("squeue did not answer".to_owned());
            }
        }
    }

    /// One row per queued job, then one per ticked id the queue no longer
    /// holds, so it can still be unticked. On a refresh the cursor stays on
    /// the id it was on where it can; the first listing starts at the top.
    fn rebuild(&mut self, jobs: Vec<Job>) {
        let was_on = self
            .rows
            .get(self.cursor)
            .filter(|_| self.rows.iter().any(|row| row.job.is_some()))
            .map(|row| row.id.clone());
        let mut rows: Vec<AfterRow> = Vec::new();
        for job in jobs {
            let id = job.dependency_id().to_owned();
            if rows.iter().any(|row| row.id == id) {
                continue;
            }
            rows.push(AfterRow { id, job: Some(job) });
        }
        for id in &self.chosen {
            if !rows.iter().any(|row| &row.id == id) {
                rows.push(AfterRow {
                    id: id.clone(),
                    job: None,
                });
            }
        }
        self.rows = rows;
        self.cursor = was_on
            .and_then(|id| self.rows.iter().position(|row| row.id == id))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let count = self.rows.len() as isize;
        if count > 0 {
            self.cursor = (self.cursor as isize + delta).rem_euclid(count) as usize;
        }
    }

    pub fn toggle(&mut self) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if !self.chosen.remove(&row.id) {
            self.chosen.insert(row.id.clone());
        }
    }

    /// Tick or untick every row. Unticking also lets go of ids whose jobs
    /// have left the queue, which is the only way to be rid of them.
    pub fn set_all(&mut self, on: bool) {
        match on {
            true => self
                .chosen
                .extend(self.rows.iter().map(|row| row.id.clone())),
            false => self.chosen.clear(),
        }
    }

    pub fn is_chosen(&self, id: &str) -> bool {
        self.chosen.contains(id)
    }

    /// The ticked ids in the order the rows show them: the queue's, newest
    /// first, then anything no longer on it.
    pub fn chosen_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .rows
            .iter()
            .filter(|row| self.chosen.contains(&row.id))
            .map(|row| row.id.clone())
            .collect();
        ids.extend(
            self.chosen
                .iter()
                .filter(|id| !self.rows.iter().any(|row| &row.id == *id))
                .cloned(),
        );
        ids
    }
}

impl App {
    /// Open the picker over the form, ticked the way the form stands, and
    /// ask the queue on a thread so the window is up before it answers.
    pub fn open_after_pick(&mut self) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        self.after_pick = Some(AfterPick::new(&form.after));
        self.mode = Mode::AfterPick;
        self.refresh_after_pick();
    }

    /// Ask `squeue` again, on a thread.
    pub fn refresh_after_pick(&mut self) {
        let Some(pick) = self.after_pick.as_mut() else {
            return;
        };
        if pick.fetching {
            return;
        }
        pick.fetching = true;
        let sender = pick.sender.clone();
        crate::world::spawn(move || {
            let _ = sender.send(slurm::refresh_queue());
        });
    }

    /// Take delivery of a queue listing the picker asked for. Never blocks.
    pub fn poll_after_pick(&mut self) {
        let Some(pick) = self.after_pick.as_mut() else {
            return;
        };
        while let Ok(fresh) = pick.inbox.try_recv() {
            pick.take(fresh);
        }
    }

    /// Enter: the form waits on what is ticked.
    pub fn accept_after_pick(&mut self) {
        let Some(pick) = self.after_pick.take() else {
            return;
        };
        self.mode = Mode::Submit;
        let ids = pick.chosen_ids();
        let message = match ids.len() {
            0 => "waits on no other job".to_owned(),
            1 => format!("waits on job {}", ids[0]),
            n => format!("waits on {n} jobs: {}", ids.join(", ")),
        };
        if let Some(form) = self.form.as_mut() {
            form.after = ids;
        }
        self.info(message);
    }

    /// Esc: back to the form as it was.
    pub fn leave_after_pick(&mut self) {
        self.after_pick = None;
        self.mode = Mode::Submit;
    }
}
