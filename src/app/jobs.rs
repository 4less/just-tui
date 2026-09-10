//! The Slurm job browser: what is queued, what has finished, and the log a
//! failed job left behind.

use std::path::PathBuf;

use crate::history::Record;
use crate::slurm::{self, Job, JobList, Logs};

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
    /// The job and file the loaded lines belong to.
    loaded: Option<(String, LogKind)>,
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
            loaded: None,
        }
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
        view.loaded = None;
        self.load_job_log();
    }

    /// Read the selected job's log, unless it is already in hand.
    pub fn load_job_log(&mut self) {
        let hints = vec![self.project().working_dir.clone()];
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        let Some(job) = view.selected().cloned() else {
            view.logs = Logs::default();
            view.lines.clear();
            view.loaded = None;
            return;
        };
        if view.loaded.as_ref() == Some(&(job.id.clone(), view.which)) {
            return;
        }

        // The paths are per job; only the choice of file changes with `Tab`.
        if view.loaded.as_ref().map(|(id, _)| id.as_str()) != Some(job.id.as_str()) {
            view.logs = slurm::find_logs(&job, &hints);
            // A job that wrote nothing to stderr is better read from stdout.
            if view.logs.err.is_none() && view.logs.out.is_some() {
                view.which = LogKind::Out;
            }
        }

        match view.shown_path() {
            Some(path) => {
                let (lines, truncated) = slurm::tail(path, LOG_LINES);
                view.lines = lines;
                view.truncated = truncated;
            }
            None => {
                view.lines = vec![match &view.logs.note {
                    Some(note) => note.clone(),
                    None => format!("no {} file for this job", view.which.label()),
                }];
                view.truncated = false;
            }
        }
        view.scroll = u16::MAX; // start at the end, where the failure is
        view.loaded = Some((job.id, view.which));
        self.clamp_job_scroll();
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
        self.load_job_log();
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
        self.load_job_log();
    }

    pub fn cycle_job_filter(&mut self) {
        if let Some(view) = self.jobs.as_mut() {
            view.filter = view.filter.next();
            view.cursor = 0;
            view.refilter();
        }
        self.load_job_log();
    }

    pub fn cycle_job_range(&mut self) {
        if let Some(view) = self.jobs.as_mut() {
            view.range = (view.range + 1) % RANGES.len();
        }
        self.refresh_jobs();
    }

    /// The recorded submission behind the selected job, if just-tui made it.
    pub fn job_record(&self) -> Option<&Record> {
        let job = self.jobs.as_ref()?.selected()?;
        self.history.for_job(&job.id)
    }
}
