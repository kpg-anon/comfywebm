//! Encode queue: one worker thread, one job at a time.
//!
//! The queue is "started" by `queue_start` or `queue_add(start: true)`. While
//! started, the worker takes pending jobs in order, including ones added while
//! it runs. When no pending job is left it stops until started again.
//! "Pause" stops it after the running job.
//!
//! The list is saved to `queue.json` in the app config dir on every change
//! other than progress. On load a job that was running is marked failed
//! ("Interrupted") and pending jobs wait for Start; nothing resumes by itself.

use crate::state::AppState;
use comfywebm_engine::run::{self, Cancel, RunError};
use comfywebm_engine::{naming, plan, EncodeSpec, Job, JobStatus, Progress};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Emitter, Manager, UserAttentionType};

#[derive(Default)]
struct Inner {
    jobs: Vec<Job>,
    next_id: u64,
    started: bool,
    current: Option<(u64, Arc<Cancel>)>,
    /// the app is exiting: the list on disk is final
    closing: bool,
}

#[derive(Default)]
pub struct Queue {
    inner: Mutex<Inner>,
    cv: Condvar,
    /// queue.json; None = not saved
    path: Option<PathBuf>,
    /// one writer at a time, so an older list never lands after a newer one
    save_lock: Mutex<()>,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    jobs: Vec<Job>,
}

pub const INTERRUPTED: &str = "Interrupted: comfywebm closed while this job was encoding. Retry starts it again.";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueState {
    /// the worker takes the next pending job when the running one ends
    pub started: bool,
}

/// Progress events are sent at most this often.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

impl Queue {
    /// Reads `path` (a missing or unreadable file gives an empty queue).
    pub fn load(path: PathBuf) -> Self {
        let mut jobs: Vec<Job> = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Saved>(&b).ok())
            .filter(|s| s.version == 1)
            .map(|s| s.jobs)
            .unwrap_or_default();
        for j in &mut jobs {
            if j.status == JobStatus::Running {
                j.status = JobStatus::Failed;
                j.error = Some(INTERRUPTED.into());
                j.progress = None;
                // the partial files of that run are ours: `<out>.part` and `<out>.best.part`
                if let Some(out) = &j.output {
                    let _ = std::fs::remove_file(format!("{out}.part"));
                    let _ = std::fs::remove_file(format!("{out}.best.part"));
                }
                j.output = None;
            }
        }
        let next_id = jobs.iter().map(|j| j.id).max().unwrap_or(0);
        Queue {
            inner: Mutex::new(Inner { jobs, next_id, ..Default::default() }),
            cv: Condvar::new(),
            path: Some(path),
            save_lock: Mutex::new(()),
        }
    }

    /// Writes the list to queue.json.
    fn save(&self) {
        let Some(path) = &self.path else { return };
        let _w = self.save_lock.lock().unwrap();
        let jobs = {
            let g = self.inner.lock().unwrap();
            if g.closing {
                return;
            }
            g.jobs.clone()
        };
        let Ok(json) = serde_json::to_vec(&Saved { version: 1, jobs }) else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    pub fn list(&self) -> Vec<Job> {
        self.inner.lock().unwrap().jobs.clone()
    }

    pub fn state(&self) -> QueueState {
        let g = self.inner.lock().unwrap();
        // the worker clears `started` only when it next looks for work, so an
        // idle queue with nothing waiting counts as stopped
        let busy = g.current.is_some() || g.jobs.iter().any(|j| j.status == JobStatus::Pending);
        QueueState { started: g.started && busy }
    }

    /// Stops after the running job.
    pub fn pause(&self) {
        self.inner.lock().unwrap().started = false;
    }

    /// Puts a finished job back in the list as pending, at the end.
    pub fn retry(&self, id: u64) -> Result<Job, String> {
        let job = {
            let mut g = self.inner.lock().unwrap();
            let pos = g.jobs.iter().position(|j| j.id == id).ok_or(format!("No job with id {id}."))?;
            if matches!(g.jobs[pos].status, JobStatus::Pending | JobStatus::Running) {
                return Err("The job has not finished.".into());
            }
            let mut j = g.jobs.remove(pos);
            j.status = JobStatus::Pending;
            j.output = None;
            j.progress = None;
            j.result = None;
            j.error = None;
            g.jobs.push(j.clone());
            self.cv.notify_all();
            j
        };
        self.save();
        Ok(job)
    }

    /// Moves job `id` to position `to` in the list (clamped).
    pub fn move_to(&self, id: u64, to: usize) -> Result<Vec<Job>, String> {
        let list = {
            let mut g = self.inner.lock().unwrap();
            let pos = g.jobs.iter().position(|j| j.id == id).ok_or(format!("No job with id {id}."))?;
            let j = g.jobs.remove(pos);
            let to = to.min(g.jobs.len());
            g.jobs.insert(to, j);
            g.jobs.clone()
        };
        self.save();
        Ok(list)
    }

    /// App exit: save the list as it is (a running job loads as interrupted),
    /// then kill the running ffmpeg so no child outlives the app.
    pub fn shutdown(&self) {
        self.save();
        let mut g = self.inner.lock().unwrap();
        g.closing = true;
        g.started = false;
        if let Some((_, c)) = &g.current {
            c.cancel();
        }
    }

    pub fn add(&self, spec: EncodeSpec, start: bool) -> Job {
        self.add_many(vec![spec], start).remove(0)
    }

    /// Adds the jobs in order under one lock, so the worker sees them all at once.
    pub fn add_many(&self, specs: Vec<EncodeSpec>, start: bool) -> Vec<Job> {
        let mut g = self.inner.lock().unwrap();
        let mut added = Vec::new();
        for spec in specs {
            g.next_id += 1;
            let job = Job {
                id: g.next_id,
                spec,
                status: JobStatus::Pending,
                output: None,
                progress: None,
                result: None,
                error: None,
            };
            g.jobs.push(job.clone());
            added.push(job);
        }
        if start && !added.is_empty() {
            g.started = true;
        }
        self.cv.notify_all();
        drop(g);
        self.save();
        added
    }

    fn has_pending(&self) -> bool {
        self.inner.lock().unwrap().jobs.iter().any(|j| j.status == JobStatus::Pending)
    }

    pub fn start(&self) {
        self.inner.lock().unwrap().started = true;
        self.cv.notify_all();
    }

    /// Kills a running job (the worker then reports it `canceled`) or removes
    /// a pending one. Finished jobs are left alone.
    pub fn cancel(&self, id: u64) -> Result<(), String> {
        let mut g = self.inner.lock().unwrap();
        let pos = g.jobs.iter().position(|j| j.id == id).ok_or(format!("No job with id {id}."))?;
        match g.jobs[pos].status {
            JobStatus::Pending => {
                g.jobs.remove(pos);
                drop(g);
                self.save();
                return Ok(());
            }
            JobStatus::Running => {
                if let Some((cid, c)) = &g.current {
                    if *cid == id {
                        c.cancel();
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Removes a finished (done, failed or canceled) job from the list.
    pub fn remove(&self, id: u64) -> Result<(), String> {
        let mut g = self.inner.lock().unwrap();
        let pos = g.jobs.iter().position(|j| j.id == id).ok_or(format!("No job with id {id}."))?;
        if matches!(g.jobs[pos].status, JobStatus::Pending | JobStatus::Running) {
            return Err("The job has not finished. Cancel it first.".into());
        }
        g.jobs.remove(pos);
        drop(g);
        self.save();
        Ok(())
    }

    /// Applies `f` to job `id` and returns the updated copy.
    fn update(&self, id: u64, f: impl FnOnce(&mut Job)) -> Option<Job> {
        let mut g = self.inner.lock().unwrap();
        let job = g.jobs.iter_mut().find(|j| j.id == id)?;
        f(job);
        Some(job.clone())
    }

    /// Blocks until the queue is started and has a pending job, marks it
    /// running and returns it with its cancel handle.
    fn next(&self) -> (Job, Arc<Cancel>) {
        let mut g = self.inner.lock().unwrap();
        loop {
            if g.started {
                if let Some(job) = g.jobs.iter_mut().find(|j| j.status == JobStatus::Pending) {
                    job.status = JobStatus::Running;
                    job.progress = Some(Progress {
                        fraction: 0.0,
                        pass: 1,
                        passes: 1,
                        attempt: 1,
                        speed: None,
                        eta_secs: None,
                    });
                    let job = job.clone();
                    let cancel = Arc::new(Cancel::new());
                    g.current = Some((job.id, cancel.clone()));
                    return (job, cancel);
                }
                g.started = false;
            }
            g = self.cv.wait(g).unwrap();
        }
    }

    fn finish_current(&self) {
        self.inner.lock().unwrap().current = None;
    }
}

fn emit(app: &AppHandle, job: Option<Job>) {
    if let Some(job) = job {
        let _ = app.emit("job-update", &job);
    }
}

pub fn spawn_worker(app: AppHandle) {
    std::thread::Builder::new()
        .name("encode-queue".into())
        .spawn(move || {
            let queue = app.state::<AppState>().queue.clone();
            loop {
                let (job, cancel) = queue.next();
                queue.save();
                emit(&app, Some(job.clone()));
                emit_state(&app, &queue);
                taskbar(&app, Some(0.0));
                run_one(&app, &queue, job, &cancel);
                queue.finish_current();
                queue.save();
                if !queue.has_pending() || !queue.state().started {
                    taskbar(&app, None);
                    attention(&app);
                }
                emit_state(&app, &queue);
            }
        })
        .expect("spawn queue thread");
}

/// `queue-state` event: whether the queue takes the next pending job.
pub fn emit_state(app: &AppHandle, queue: &Queue) {
    let _ = app.emit("queue-state", queue.state());
}

/// Probes a finished output and lists what does not match the plan: no
/// video, no audio when audio was asked for, another picture size, or a
/// duration off by more than 0.5 s and 3%.
fn verify(state: &AppState, out: &std::path::Path, spec: &EncodeSpec, size: (u32, u32), want: f64) -> Vec<String> {
    // ffprobe cannot decode animated WebP ("image data not found")
    if spec.codec == comfywebm_engine::Codec::Webp {
        return Vec::new();
    }
    let Some(ffprobe) = state.tools().0.ffprobe else { return Vec::new() };
    let info = match comfywebm_engine::probe::probe(&ffprobe, &out.to_string_lossy()) {
        Ok(i) => i,
        Err(e) => return vec![format!("ffprobe cannot read the output: {e}")],
    };
    let mut w = Vec::new();
    if spec.container == comfywebm_engine::Container::Audio {
        if info.audio.is_empty() {
            w.push("The output has no audio stream.".to_string());
        }
    } else if info.video.width == 0 || info.video.height == 0 {
        w.push("The output has no readable video stream.".to_string());
    } else if (info.video.width, info.video.height) != size {
        w.push(format!("The picture is {}x{}, planned {}x{}.", info.video.width, info.video.height, size.0, size.1));
    }
    // boomerang drops the sound on purpose
    if spec.audio.is_some() && spec.playback != comfywebm_engine::Playback::Boomerang && info.audio.is_empty() {
        w.push("Sound was selected, but the output has no audio stream.".to_string());
    }
    // a stream copy starts at a key frame, so its length differs by design
    if want > 0.0 && !spec.stream_copy && info.duration > 0.0 && (info.duration - want).abs() > 0.5_f64.max(want * 0.03) {
        w.push(format!("The output is {:.2} s long, planned {:.2} s.", info.duration, want));
    }
    w
}

/// Taskbar progress of the main window: Some(fraction) while a job runs, None to clear.
fn taskbar(app: &AppHandle, fraction: Option<f64>) {
    let Some(w) = app.get_webview_window("main") else { return };
    let state = match fraction {
        Some(f) => ProgressBarState {
            status: Some(ProgressBarStatus::Normal),
            progress: Some((f.clamp(0.0, 1.0) * 100.0).round() as u64),
        },
        None => ProgressBarState { status: Some(ProgressBarStatus::None), progress: None },
    };
    let _ = w.set_progress_bar(state);
}

/// The queue finished: ask for attention when the window is not focused.
fn attention(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    if !w.is_focused().unwrap_or(true) {
        let _ = w.request_user_attention(Some(UserAttentionType::Informational));
    }
}

fn run_one(app: &AppHandle, queue: &Queue, job: Job, cancel: &Cancel) {
    let id = job.id;
    let state = app.state::<AppState>();
    let prepared = (|| -> Result<_, String> {
        let ffmpeg = state.ffmpeg()?;
        let info = state.media(&job.spec.input)?;
        // read now, so a change to the output folder applies to jobs not yet started
        let settings = state.settings();
        // out height for the {height} token; the paths here are not used
        let sources = state.sources(&job.spec)?;
        let opts = plan::Opts { display: true, zscale: state.zscale(), sources: Some(&sources), ..plan::Opts::new("", "") };
        let planned = plan::build(&job.spec, &info, &opts)?;
        // the folder fixed when the job was queued, else the current setting
        let dir = match &job.spec.output_dir {
            Some(d) => Some(d.clone()),
            None => state.output_dir(),
        };
        let out = naming::output_path(&job.spec, &info, planned.out_height, dir.as_deref(), &settings.name_template);
        Ok((ffmpeg, info, out, planned))
    })();
    let (ffmpeg, info, out, planned) = match prepared {
        Ok(p) => p,
        Err(e) => {
            emit(
                app,
                queue.update(id, |j| {
                    j.status = JobStatus::Failed;
                    j.error = Some(e);
                }),
            );
            return;
        }
    };
    let out_s = out.to_string_lossy().into_owned();
    emit(app, queue.update(id, |j| j.output = Some(out_s)));

    let mut last_emit = Instant::now() - PROGRESS_INTERVAL;
    let sources = state.sources(&job.spec).unwrap_or_default();
    let res = run::run_job(&ffmpeg, &job.spec, &info, &out, state.zscale(), Some(&sources), cancel, |p| {
        let job = queue.update(id, |j| j.progress = Some(p.clone()));
        if last_emit.elapsed() >= PROGRESS_INTERVAL {
            last_emit = Instant::now();
            taskbar(app, Some(p.fraction));
            emit(app, job);
        }
    });
    let checks = match &res {
        Ok(o) => verify(&state, &o.output, &job.spec, o.out_size, planned.duration_out),
        Err(_) => Vec::new(),
    };
    let updated = queue.update(id, |j| match res {
        Ok(mut outcome) => {
            outcome.result.warnings = checks;
            j.status = JobStatus::Done;
            j.output = Some(outcome.output.to_string_lossy().into_owned());
            if let Some(p) = j.progress.as_mut() {
                p.fraction = 1.0;
                p.eta_secs = Some(0.0);
            }
            j.result = Some(outcome.result);
        }
        Err(RunError::Canceled) => j.status = JobStatus::Canceled,
        Err(RunError::Failed(e)) => {
            j.status = JobStatus::Failed;
            j.error = Some(e);
        }
    });
    emit(app, updated);
}
