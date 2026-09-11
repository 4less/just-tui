//! The Slurm job browser: what is queued, what has finished, and the log a
//! failed job left behind.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use crate::history::Record;
use crate::slurm::{self, Job, JobList, Logs, Usage};

/// Which states the list shows. Empty means all of them, which is both the
/// obvious reading and the one that survives a cluster reporting a state this
/// never thought of.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateFilter {
    chosen: BTreeSet<String>,
}

/// Offered in the filter window whether or not the queue happens to hold one
/// right now, so the list does not move about as jobs come and go.
const COMMON_STATES: [&str; 8] = [
    "RUNNING",
    "PENDING",
    "COMPLETED",
    "FAILED",
    "CANCELLED",
    "TIMEOUT",
    "OUT_OF_MEMORY",
    "NODE_FAIL",
];

impl StateFilter {
    pub fn is_all(&self) -> bool {
        self.chosen.is_empty()
    }

    pub fn holds(&self, state: &str) -> bool {
        self.chosen.contains(state)
    }

    pub fn toggle(&mut self, state: &str) {
        if !self.chosen.remove(state) {
            self.chosen.insert(state.to_owned());
        }
    }

    pub fn clear(&mut self) {
        self.chosen.clear();
    }

    fn keeps(&self, job: &Job) -> bool {
        self.is_all() || self.chosen.contains(job.state_word())
    }

    /// How the filter reads in the title: `all`, a state, or a count.
    pub fn label(&self) -> String {
        let mut names = self.chosen.iter();
        match (names.next(), self.chosen.len()) {
            (None, _) => "all".to_owned(),
            (Some(only), 1) => only.to_lowercase(),
            (_, count) => format!("{count} states"),
        }
    }

    /// The states worth offering: the usual ones, plus anything this cluster
    /// has actually reported, plus whatever is already chosen.
    pub fn options(&self, jobs: &[Job]) -> Vec<String> {
        let mut out: Vec<String> = COMMON_STATES.iter().map(|s| (*s).to_owned()).collect();
        for state in jobs
            .iter()
            .map(|job| job.state_word().to_owned())
            .chain(self.chosen.iter().cloned())
        {
            if !state.is_empty() && !out.contains(&state) {
                out.push(state);
            }
        }
        out
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

/// Work finished on a background thread, on its way back to the interface.
/// Everything that talks to the scheduler goes through here: none of it is
/// fast enough to do between a keystroke and the frame that answers it.
enum Fetched {
    /// A full listing: `squeue` and `sacct`.
    Listing(JobList),
    /// `squeue` alone, which is all a timed refresh needs.
    Queue(Option<Vec<Job>>),
    /// `sstat` for the running jobs.
    Usage(HashMap<String, Usage>),
    Log(LoadedLog),
}

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
    pub filter: StateFilter,
    /// Where the cursor sits in the filter window.
    pub filter_cursor: usize,
    /// Whether the log pane is shown at all; without it the queue has the
    /// whole screen.
    pub show_log: bool,
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
    sender: Sender<Fetched>,
    inbox: Receiver<Fetched>,
    /// A listing is being fetched, so a second is not started on top of it.
    fetching: bool,
    /// The job to put the cursor back on once that listing lands.
    keep: Option<String>,
    /// The selected task's line of the manifest, read when the selection
    /// changes rather than on every frame.
    pub task_args: Option<String>,
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
            filter: StateFilter::default(),
            filter_cursor: 0,
            show_log: true,
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
            fetching: false,
            keep: None,
            task_args: None,
        }
    }

    /// Whether a listing is on its way, so the list can say so instead of
    /// showing an empty pane.
    pub fn fetching(&self) -> bool {
        self.fetching
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
        let keep = self.filter.clone();
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
    /// Open the job browser. The overlay is up before Slurm has answered;
    /// asking it is the slow part and happens on a thread.
    pub fn open_jobs(&mut self) {
        if self.jobs.is_none() {
            self.jobs = Some(JobsView::new());
        }
        self.mode = Mode::Jobs;
        self.refresh_jobs();
    }

    /// Ask for a full listing — `squeue` and `sacct`. On a busy controller
    /// `sacct` over ninety days takes seconds, which is why nothing waits.
    pub fn refresh_jobs(&mut self) {
        self.history.reload();
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        if view.fetching {
            return;
        }
        view.keep = view.selected().map(|job| job.id.clone());
        view.fetching = true;
        view.fetched = Instant::now();

        let (days, sender) = (view.days(), view.sender.clone());
        std::thread::spawn(move || {
            let _ = sender.send(Fetched::Listing(slurm::fetch_jobs(days)));
        });
    }

    /// How long the event loop may wait for a key before the overlay wants
    /// redrawing. `None` means there is nothing to animate, so it can block.
    pub fn tick_interval(&self) -> Option<Duration> {
        let spinning = self
            .jobs
            .as_ref()
            .is_some_and(|view| view.loading() || view.fetching());
        match self.mode {
            // A second is enough for a ticking clock; a spinner needs more.
            Mode::Jobs | Mode::ConfirmJob | Mode::JobFilter => Some(match spinning {
                true => SPIN,
                false => Duration::from_secs(1),
            }),
            _ => None,
        }
    }

    /// Called when no key arrived: the clock is redrawn from the local one,
    /// and every so often the queue is asked for again — on a thread.
    pub fn tick(&mut self) {
        if let Some(view) = self.jobs.as_mut()
            && (view.loading() || view.fetching())
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

    /// Re-ask `squeue` only. Finished jobs cannot change, so `sacct` is left
    /// alone until the user asks for it with `r`.
    pub fn refresh_queue(&mut self) {
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        if view.fetching {
            return;
        }
        view.keep = view.selected().map(|job| job.id.clone());
        view.fetching = true;
        // Counted from the moment it was asked for, not the moment it came
        // back, or a slow controller would be asked again immediately.
        view.fetched = Instant::now();

        let sender = view.sender.clone();
        std::thread::spawn(move || {
            let _ = sender.send(Fetched::Queue(slurm::refresh_queue()));
        });
    }

    /// Ask `sstat` what the running jobs are using.
    pub fn refresh_usage(&mut self) {
        let Some(view) = self.jobs.as_ref() else {
            return;
        };
        let (ids, sender) = (view.running(), view.sender.clone());
        if ids.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            let _ = sender.send(Fetched::Usage(slurm::fetch_usage(&ids)));
        });
    }

    /// Put the cursor back on the job it was on, after a listing replaced the
    /// rows underneath it.
    fn restore_cursor(&mut self) {
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        view.refilter();
        if let Some(id) = view.keep.take()
            && let Some(position) = view
                .visible
                .iter()
                .position(|&index| view.list.jobs[index].id == id)
        {
            view.cursor = position;
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
        if !view.show_log {
            return;
        }
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
            let _ = sender.send(Fetched::Log(LoadedLog {
                want,
                shown,
                logs,
                lines,
                truncated,
            }));
        });
    }

    /// Take delivery of whatever the background threads have finished, and
    /// act on it. Never blocks: this runs before every frame.
    pub fn poll_background(&mut self) {
        let mut landed = Vec::new();
        if let Some(view) = self.jobs.as_mut() {
            while let Ok(fetched) = view.inbox.try_recv() {
                landed.push(fetched);
            }
        }
        if landed.is_empty() {
            return;
        }

        let mut listed = false;
        for fetched in landed {
            match fetched {
                Fetched::Listing(list) => {
                    if let Some(view) = self.jobs.as_mut() {
                        view.list = list;
                        view.usage.clear();
                        view.fetching = false;
                        view.loaded = None;
                    }
                    listed = true;
                }
                Fetched::Queue(fresh) => {
                    self.merge_queue(fresh);
                    listed = true;
                }
                Fetched::Usage(usage) => {
                    if let Some(view) = self.jobs.as_mut() {
                        view.usage = usage;
                    }
                }
                Fetched::Log(loaded) => self.take_log(loaded),
            }
        }

        if listed {
            self.restore_cursor();
            self.refresh_usage();
        }
        self.clamp_job_scroll();
        self.note_task_args();
        // The cursor has usually moved on by the time an answer arrives.
        self.request_job_log();
    }

    /// Fold a fresh `squeue` into the rows already on screen.
    fn merge_queue(&mut self, fresh: Option<Vec<Job>>) {
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        view.fetching = false;
        let Some(mut jobs) = fresh else {
            return;
        };
        // Anything that has left the queue since the last look keeps the row
        // sacct gave it, but stops being live.
        for old in &view.list.jobs {
            if !jobs.iter().any(|job| job.id == old.id) {
                let mut old = old.clone();
                old.live = false;
                jobs.push(old);
            }
        }
        jobs.sort_by_key(|job| std::cmp::Reverse(job.order()));
        view.list.jobs = jobs;
    }

    /// Apply a log that has been read, unless the cursor has left the job it
    /// belongs to.
    fn take_log(&mut self, loaded: LoadedLog) {
        let selected = self
            .jobs
            .as_ref()
            .and_then(|view| view.selected().map(|job| job.id.clone()));
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        // The loader is free again whatever the answer was worth.
        if view.pending.as_ref() == Some(&loaded.want) {
            view.pending = None;
        }
        if selected.as_deref() != Some(loaded.want.0.as_str()) || loaded.want.1 != view.which {
            return;
        }
        view.logs = loaded.logs;
        view.which = loaded.shown;
        view.lines = loaded.lines;
        view.truncated = loaded.truncated;
        view.loaded = Some((loaded.want.0, loaded.shown));
        view.scroll = u16::MAX; // start at the end, where the failure is
    }

    /// Read the selected array task's line of the manifest, once, when the
    /// selection changes — not once a frame, which on a shared filesystem is
    /// a stat and a read eleven times a second.
    pub fn note_task_args(&mut self) {
        let args = self.job_record().and_then(|record| {
            let job = self.jobs.as_ref()?.selected()?;
            let task = job.id.split_once('_')?.1.parse::<usize>().ok()?;
            if record.manifest.is_empty() {
                return None;
            }
            crate::batch::line(
                PathBuf::from(&record.base).as_path(),
                &record.manifest,
                task,
            )
        });
        if let Some(view) = self.jobs.as_mut() {
            view.task_args = args;
        }
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
        self.note_task_args();
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

    /// Open the filter window, with the cursor on the first state.
    pub fn open_job_filter(&mut self) {
        if let Some(view) = self.jobs.as_mut() {
            view.filter_cursor = 0;
        }
        self.mode = Mode::JobFilter;
    }

    /// The states the filter window lists, with how many jobs each holds.
    pub fn job_filter_options(&self) -> Vec<(String, usize)> {
        let Some(view) = self.jobs.as_ref() else {
            return Vec::new();
        };
        view.filter
            .options(&view.list.jobs)
            .into_iter()
            .map(|state| {
                let count = view
                    .list
                    .jobs
                    .iter()
                    .filter(|job| job.state_word() == state)
                    .count();
                (state, count)
            })
            .collect()
    }

    pub fn move_job_filter(&mut self, delta: isize) {
        let count = self.job_filter_options().len() as isize;
        if count == 0 {
            return;
        }
        if let Some(view) = self.jobs.as_mut() {
            view.filter_cursor = (view.filter_cursor as isize + delta).rem_euclid(count) as usize;
        }
    }

    /// Turn the highlighted state on or off. The list behind the window
    /// follows at once, so the effect is visible while choosing.
    pub fn toggle_job_filter(&mut self) {
        let options = self.job_filter_options();
        let Some(view) = self.jobs.as_mut() else {
            return;
        };
        let Some((state, _)) = options.get(view.filter_cursor) else {
            return;
        };
        view.filter.toggle(state);
        view.cursor = 0;
        view.refilter();
        self.request_job_log();
    }

    /// Back to showing everything.
    pub fn clear_job_filter(&mut self) {
        if let Some(view) = self.jobs.as_mut() {
            view.filter.clear();
            view.cursor = 0;
            view.refilter();
        }
        self.request_job_log();
    }

    /// Show or hide the log pane. Hidden, the queue has the whole window and
    /// no log is read at all.
    pub fn toggle_job_log_pane(&mut self) {
        let showing = match self.jobs.as_mut() {
            Some(view) => {
                view.show_log = !view.show_log;
                view.show_log
            }
            None => return,
        };
        if showing {
            self.request_job_log();
        }
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

        // Re-expand: the inputs may have changed since, and the manifest is
        // named after its contents, so an unchanged one writes the same file.
        let plan = crate::batch::plan(&base, &record.namepath, &record.settings)
            .map_err(|problem| format!("each: {problem}"))?;
        if let Some(plan) = plan.as_ref()
            && let Err(err) = crate::batch::write(&base, plan)
        {
            return Err(format!("could not write the manifest: {err}"));
        }
        let batch = plan
            .as_ref()
            .map(|plan| (plan.manifest.as_path(), plan.count()));

        match slurm::submit(&base, &record.namepath, &record.settings, batch) {
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
                    manifest: plan
                        .as_ref()
                        .map(|plan| plan.manifest.display().to_string())
                        .unwrap_or_default(),
                    tasks: plan.as_ref().map_or(0, |plan| plan.count()),
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
