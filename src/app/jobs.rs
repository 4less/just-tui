//! The Slurm job browser: what is queued, what has finished, and the log a
//! failed job left behind.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use crate::history::Record;
use crate::slurm::{self, Job, JobList, Logs, Usage};

/// Which jobs the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobFilter {
    All,
    Active,
    Failed,
}

impl JobFilter {
    pub fn label(self) -> &'static str {
        match self {
            JobFilter::All => "all",
            JobFilter::Active => "running",
            JobFilter::Failed => "failed",
        }
    }

    fn next(self) -> Self {
        match self {
            JobFilter::All => JobFilter::Active,
            JobFilter::Active => JobFilter::Failed,
            JobFilter::Failed => JobFilter::All,
        }
    }

    fn keeps(self, job: &Job) -> bool {
        match self {
            JobFilter::All => true,
            JobFilter::Active => job.active(),
            JobFilter::Failed => job.failed(),
        }
    }
}

/// Which of a job's two log files is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Err,
    Out,
}

impl LogKind {
    pub fn label(self) -> &'static str {
        match self {
            LogKind::Err => "stderr",
            LogKind::Out => "stdout",
        }
    }

    pub fn other(self) -> Self {
        match self {
            LogKind::Err => LogKind::Out,
            LogKind::Out => LogKind::Err,
        }
    }
}

/// How far back `sacct` is asked to look, cycled with `d`.
const RANGES: [u32; 4] = [1, 7, 30, 90];

/// Longest tail held in memory for one log.
const LOG_LINES: usize = 4000;

/// How often the spinner steps while a log is being read.
const SPIN: Duration = Duration::from_millis(90);

/// How often the queue and the usage figures are re-asked for while the
/// overlay is open. The clock on screen ticks every second regardless: it is
/// carried forward locally, so this is only about fresh states and stats.
const REFRESH: Duration = Duration::from_secs(5);

/// A log read on a background thread, on its way back to the interface.
struct LoadedLog {
    /// The job and the file that were asked for, so a result that arrives
    /// after the cursor has moved on can be dropped.
    want: (String, LogKind),
    /// The file actually read: a job with nothing on stderr is shown its
    /// stdout instead.
    shown: LogKind,
    logs: Logs,
    lines: Vec<String>,
    truncated: bool,
}

/// Everything the jobs overlay draws.
pub struct JobsView {
    pub list: JobList,
    /// Indices into `list.jobs` passing the filter.
    pub visible: Vec<usize>,
    pub cursor: usize,
    pub filter: JobFilter,
    pub range: usize,
    pub logs: Logs,
    pub which: LogKind,
    pub lines: Vec<String>,
    pub truncated: bool,
    pub scroll: u16,
    /// Rows the log pane had last frame, so paging knows how far to go.
    pub height: u16,
    /// What each running job is using, by job id.
    pub usage: HashMap<String, Usage>,
    /// When the listing was last fetched, so the clock can run on from it.
    pub fetched: Instant,
    /// Whether the queue is re-asked on a timer.
    pub auto: bool,
    /// The job and file the loaded lines belong to.
    loaded: Option<(String, LogKind)>,
    /// What a background thread is reading, if anything.
    pending: Option<(String, LogKind)>,
    /// Which spinner frame is showing.
    pub frame: usize,
    sender: Sender<LoadedLog>,
    inbox: Receiver<LoadedLog>,
}

impl JobsView {
    /// A view over a listing that is already in hand, so the overlay can be
    /// drawn without a scheduler to ask.
    #[cfg(test)]
    pub fn with(list: JobList) -> Self {
        let mut view = Self::new();
        view.list = list;
        view.refilter();
        view
    }

    fn new() -> Self {
        let (sender, inbox) = channel();
        Self {
            list: JobList::default(),
            visible: Vec::new(),
            cursor: 0,
            filter: JobFilter::All,
            range: 1,
            logs: Logs::default(),
            which: LogKind::Err,
            lines: Vec::new(),
            truncated: false,
            scroll: 0,
            height: 10,
            usage: HashMap::new(),
            fetched: Instant::now(),
            auto: true,
            loaded: None,
            pending: None,
            frame: 0,
            sender,
            inbox,
        }
    }

    /// Whether a log is still being read, so the pane can say so.
    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }

    /// Pretend a read is in flight, so the spinner can be drawn in a test
    /// without racing a thread that finishes in microseconds.
    #[cfg(test)]
    pub fn mark_loading(&mut self) {
        self.pending = Some((String::new(), self.which));
    }

    /// Seconds since the listing was fetched — how far a running job's clock
    /// has moved on from the elapsed time Slurm reported.
    pub fn since_fetch(&self) -> u64 {
        self.fetched.elapsed().as_secs()
    }

    /// Ids of the jobs `sstat` can say something about.
    fn running(&self) -> Vec<String> {
        self.list
            .jobs
            .iter()
            .filter(|job| job.state_word() == "RUNNING")
            .map(|job| job.id.clone())
            .collect()
    }

    pub fn days(&self) -> u32 {
        RANGES[self.range]
    }

    pub fn selected(&self) -> Option<&Job> {
        let index = *self.visible.get(self.cursor)?;
        self.list.jobs.get(index)
    }

    /// The file the log pane is showing, whichever way round it was found.
    pub fn shown_path(&self) -> Option<&PathBuf> {
        match self.which {
            LogKind::Err => self.logs.err.as_ref(),
            LogKind::Out => self.logs.out.as_ref(),
        }
    }

    fn refilter(&mut self) {
        let keep = self.filter;
        self.visible = self
            .list
            .jobs
            .iter()
            .enumerate()
            .filter(|(_, job)| keep.keeps(job))
            .map(|(index, _)| index)
            .collect();
        self.cursor = self.cursor.min(self.visible.len().saturating_sub(1));
    }
}

use super::{App, Mode};

impl App {
    /// Open the job browser, asking Slurm what it knows.
    pub fn open_jobs(&mut self) {
        if self.jobs.is_none() {
            self.jobs = Some(JobsView::new());
        }
        self.mode = Mode::Jobs;
        self.refresh_jobs();
    }

    /// Re-ask `squeue` and `sacct`, keeping the cursor on the same job.
    pub fn refresh_jobs(&mut self) {
        let Some(view) = self.jobs.as_ref() else {
            return;
        };
        let (days, keep) = (view.days(), view.selected().map(|job| job.id.clone()));

        let list = slurm::fetch_jobs(days);
        self.history.reload();

        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        view.list = list;
        view.refilter();
        if let Some(id) = keep
            && let Some(position) = view
                .visible
                .iter()
                .position(|&index| view.list.jobs[index].id == id)
        {
            view.cursor = position;
        }
        view.usage.clear();
        view.fetched = Instant::now();
        view.loaded = None;
        self.refresh_usage();
        self.request_job_log();
    }

    /// How long the event loop may wait for a key before the overlay wants
    /// redrawing. `None` means there is nothing to animate, so it can block.
    pub fn tick_interval(&self) -> Option<Duration> {
        let spinning = self.jobs.as_ref().is_some_and(JobsView::loading);
        match self.mode {
            // A second is enough for a ticking clock; a spinner needs more.
            Mode::Jobs | Mode::ConfirmJob => Some(match spinning {
                true => SPIN,
                false => Duration::from_secs(1),
            }),
            _ => None,
        }
    }

    /// Called when no key arrived: the clock is redrawn from the local one,
    /// and every so often the queue itself is re-asked.
    pub fn tick(&mut self) {
        if let Some(view) = self.jobs.as_mut()
            && view.loading()
        {
            view.frame = view.frame.wrapping_add(1);
        }
        let due = self
            .jobs
            .as_ref()
            .is_some_and(|view| view.auto && view.fetched.elapsed() >= REFRESH);
        if due {
            self.refresh_queue();
        }
    }

    /// Re-ask `squeue` and `sstat` only. Finished jobs cannot change, so
    /// `sacct` is left alone until the user asks for it with `r`.
    pub fn refresh_queue(&mut self) {
        let Some(fresh) = slurm::refresh_queue() else {
            if let Some(view) = self.jobs.as_mut() {
                view.fetched = Instant::now();
            }
            return;
        };
        let keep = self
            .jobs
            .as_ref()
            .and_then(|view| view.selected().map(|job| job.id.clone()));

        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        // Anything that has left the queue since the last look keeps the row
        // sacct gave it, but stops being live.
        let mut jobs = fresh;
        for old in &view.list.jobs {
            if !jobs.iter().any(|job| job.id == old.id) {
                let mut old = old.clone();
                old.live = false;
                jobs.push(old);
            }
        }
        jobs.sort_by_key(|job| std::cmp::Reverse(job.order()));
        view.list.jobs = jobs;
        view.fetched = Instant::now();
        view.refilter();
        if let Some(id) = keep
            && let Some(position) = view
                .visible
                .iter()
                .position(|&index| view.list.jobs[index].id == id)
        {
            view.cursor = position;
        }
        self.refresh_usage();
    }

    /// Ask `sstat` what the running jobs are using.
    pub fn refresh_usage(&mut self) {
        let Some(view) = self.jobs.as_ref() else {
            return;
        };
        let usage = slurm::fetch_usage(&view.running());
        if let Some(view) = self.jobs.as_mut() {
            view.usage = usage;
        }
    }

    /// Stop or restart the timer, for a cluster where asking is expensive.
    pub fn toggle_job_auto(&mut self) {
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        view.auto = !view.auto;
        let on = view.auto;
        self.info(match on {
            true => "refreshing every 5s",
            false => "auto-refresh off — r reloads",
        });
    }

    /// Ask for the selected job's log, on a thread. Finding it can mean
    /// waiting on `scontrol` and walking a large `logs/` tree, neither of
    /// which the cursor should have to wait for.
    pub fn request_job_log(&mut self) {
        let hints = vec![self.project().working_dir.clone()];
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        let Some(job) = view.selected().cloned() else {
            view.logs = Logs::default();
            view.lines.clear();
            view.loaded = None;
            view.pending = None;
            return;
        };

        let want = (job.id.clone(), view.which);
        // Already read, or a read is already out. Only one loader runs at a
        // time: holding the cursor key down would otherwise put a thread and
        // a walk of the whole `logs/` tree behind every row it passed over.
        // Whatever ends up selected is asked for once this one comes back.
        if view.loaded.as_ref() == Some(&want) || view.pending.is_some() {
            return;
        }
        view.pending = Some(want.clone());
        view.frame = 0;

        let sender = view.sender.clone();
        std::thread::spawn(move || {
            let logs = slurm::find_logs(&job, &hints);
            // A job that wrote nothing to stderr is better read from stdout.
            let shown = match want.1 {
                LogKind::Err if logs.err.is_none() && logs.out.is_some() => LogKind::Out,
                asked => asked,
            };
            let path = match shown {
                LogKind::Err => logs.err.clone(),
                LogKind::Out => logs.out.clone(),
            };
            let (lines, truncated) = match &path {
                Some(path) => slurm::tail(path, LOG_LINES),
                None => (
                    vec![match &logs.note {
                        Some(note) => note.clone(),
                        None => format!("no {} file for this job", shown.label()),
                    }],
                    false,
                ),
            };
            let _ = sender.send(LoadedLog {
                want,
                shown,
                logs,
                lines,
                truncated,
            });
        });
    }

    /// Take delivery of anything the loader has finished, dropping results
    /// for a job the cursor has already left. Never blocks.
    pub fn poll_background(&mut self) {
        let selected = self
            .jobs
            .as_ref()
            .and_then(|view| view.selected().map(|job| job.id.clone()));
        let Some(view) = self.jobs.as_mut() else {
            return;
        };

        while let Ok(loaded) = view.inbox.try_recv() {
            // The loader is free again whatever the answer was worth.
            if view.pending.as_ref() == Some(&loaded.want) {
                view.pending = None;
            }
            let current = (selected.as_deref() == Some(loaded.want.0.as_str()))
                && loaded.want.1 == view.which;
            if !current {
                continue;
            }
            view.logs = loaded.logs;
            view.which = loaded.shown;
            view.lines = loaded.lines;
            view.truncated = loaded.truncated;
            view.loaded = Some((loaded.want.0, loaded.shown));
            view.scroll = u16::MAX; // start at the end, where the failure is
        }
        self.clamp_job_scroll();
        // The cursor has usually moved on by the time an answer arrives.
        self.request_job_log();
    }

    pub fn move_job(&mut self, delta: isize) {
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        if view.visible.is_empty() {
            return;
        }
        let count = view.visible.len() as isize;
        view.cursor = (view.cursor as isize + delta).rem_euclid(count) as usize;
        self.request_job_log();
    }

    pub fn scroll_job_log(&mut self, delta: i32) {
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        view.scroll =
            (i64::from(view.scroll) + i64::from(delta)).clamp(0, i64::from(u16::MAX)) as u16;
        self.clamp_job_scroll();
    }

    /// Keep the last line reachable but not scrollable past.
    pub fn clamp_job_scroll(&mut self) {
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        let max = view.lines.len().saturating_sub(view.height.max(1) as usize);
        view.scroll = view.scroll.min(max as u16);
    }

    pub fn toggle_job_log(&mut self) {
        if let Some(view) = self.jobs.as_mut() {
            view.which = view.which.other();
        }
        self.request_job_log();
    }

    pub fn cycle_job_filter(&mut self) {
        if let Some(view) = self.jobs.as_mut() {
            view.filter = view.filter.next();
            view.cursor = 0;
            view.refilter();
        }
        self.request_job_log();
    }

    pub fn cycle_job_range(&mut self) {
        if let Some(view) = self.jobs.as_mut() {
            view.range = (view.range + 1) % RANGES.len();
        }
        self.refresh_jobs();
    }

    /// What the selected job is using right now, if `sstat` knew.
    pub fn job_usage(&self) -> Option<(&Job, Usage)> {
        let view = self.jobs.as_ref()?;
        let job = view.selected()?;
        Some((job, *view.usage.get(&job.id)?))
    }

    /// The recorded submission behind the selected job, if just-tui made it.
    pub fn job_record(&self) -> Option<&Record> {
        let job = self.jobs.as_ref()?.selected()?;
        self.history.for_job(&job.id)
    }
}

/// Something that touches the queue, waiting to be confirmed.
pub struct PendingAction {
    pub id: String,
    pub name: String,
    /// Cancel the job that is there now.
    pub kill: bool,
    /// Put the same job back on the queue.
    pub resubmit: bool,
}

impl PendingAction {
    pub fn title(&self) -> &'static str {
        match (self.kill, self.resubmit) {
            (true, true) => " Kill this job and run it again? ",
            (true, false) => " Kill this job? ",
            _ => " Submit this job again? ",
        }
    }
}

impl App {
    /// Ask before anything reaches the scheduler: `scancel` cannot be taken
    /// back, and a stray keystroke should not queue work twice.
    pub fn ask_job_action(&mut self, kill: bool, resubmit: bool) {
        let Some(job) = self.jobs.as_ref().and_then(|view| view.selected()).cloned() else {
            return;
        };
        if kill && !job.active() {
            self.error(format!("job {} has already finished", job.id));
            return;
        }
        // Submitting again on its own is for a job that has stopped. While one
        // is still queued or running, the thing wanted is nearly always to
        // replace it rather than to end up with two.
        if resubmit && !kill && job.active() {
            self.error(format!(
                "job {} is still {} — X kills it and submits it again",
                job.id,
                job.state_word().to_lowercase()
            ));
            return;
        }
        if resubmit && self.job_record().is_none() {
            self.error("only jobs submitted from just-tui can be submitted again");
            return;
        }

        self.pending = Some(PendingAction {
            id: job.id.clone(),
            name: job.name.clone(),
            kill,
            resubmit,
        });
        self.mode = Mode::ConfirmJob;
    }

    pub fn abandon_job_action(&mut self) {
        self.pending = None;
        self.mode = Mode::Jobs;
    }

    /// Carry it out: kill first when asked to, then submit again when asked
    /// to. A failed kill stops the whole thing — resubmitting alongside a job
    /// that is still running is never what was meant.
    pub fn confirm_job_action(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.mode = Mode::Jobs;

        if pending.kill {
            if let Err(err) = slurm::cancel(&pending.id) {
                self.error(format!("scancel {}: {err}", pending.id));
                return;
            }
            if !pending.resubmit {
                self.info(format!("cancelled job {}", pending.id));
                self.refresh_queue();
                return;
            }
        }

        match (self.resubmit(&pending.id), pending.kill) {
            (Ok(new_id), true) => self.info(format!(
                "cancelled {} and submitted it again as {new_id}",
                pending.id
            )),
            (Ok(new_id), false) => {
                self.info(format!("submitted {} again as job {new_id}", pending.name))
            }
            (Err(err), true) => self.error(format!("cancelled {}, but {err}", pending.id)),
            (Err(err), false) => self.error(err),
        }
        self.refresh_queue();
    }

    /// Submit a recorded job again, exactly as it went out the first time.
    fn resubmit(&mut self, id: &str) -> Result<String, String> {
        let Some(record) = self.history.for_job(id).cloned() else {
            return Err("it was not submitted from just-tui".to_owned());
        };
        // Older records predate the directory being kept; the project is the
        // only sensible guess, and is right for anything but a global recipe.
        let base = match record.base.is_empty() {
            true => self.project().working_dir.clone(),
            false => PathBuf::from(&record.base),
        };

        match slurm::submit(&base, &record.namepath, &record.settings) {
            slurm::Submission::Failed { message } => Err(format!("sbatch: {message}")),
            slurm::Submission::Ok { job_id } => {
                let (out, err) =
                    slurm::resolved_log_paths(&record.namepath, &record.settings, &job_id);
                let again = Record {
                    job_id: job_id.clone(),
                    at: crate::history::now(),
                    when: crate::history::timestamp(),
                    out,
                    err,
                    ..record
                };
                if let Err(problem) = self.history.append(again) {
                    return Err(format!("could not record {job_id}: {problem}"));
                }
                Ok(job_id)
            }
        }
    }
}
