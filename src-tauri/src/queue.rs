//! Encode queue: one worker thread, one job at a time.
//!
//! The queue is "started" by `queue_start` or `queue_add(start: true)`. While
//! started, the worker takes pending jobs in order, including ones added while
//! it runs. When no pending job is left it stops until started again.

use crate::state::AppState;
use comfywebm_engine::run::{self, Cancel, RunError};
use comfywebm_engine::{naming, plan, EncodeSpec, Job, JobStatus, Progress};
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
}

#[derive(Default)]
pub struct Queue {
    inner: Mutex<Inner>,
    cv: Condvar,
}

/// Progress events are sent at most this often.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

impl Queue {
    pub fn list(&self) -> Vec<Job> {
        self.inner.lock().unwrap().jobs.clone()
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
                emit(&app, Some(job.clone()));
                taskbar(&app, Some(0.0));
                run_one(&app, &queue, job, &cancel);
                queue.finish_current();
                if !queue.has_pending() {
                    taskbar(&app, None);
                    attention(&app);
                }
            }
        })
        .expect("spawn queue thread");
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
        let opts = plan::Opts { display: true, zscale: state.zscale(), ..plan::Opts::new("", "") };
        let planned = plan::build(&job.spec, &info, &opts)?;
        let out = naming::output_path(
            &job.spec,
            &info,
            planned.out_height,
            settings.output_dir.as_deref(),
            &settings.name_template,
        );
        Ok((ffmpeg, info, out))
    })();
    let (ffmpeg, info, out) = match prepared {
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
    let res = run::run_job(&ffmpeg, &job.spec, &info, &out, state.zscale(), cancel, |p| {
        let job = queue.update(id, |j| j.progress = Some(p.clone()));
        if last_emit.elapsed() >= PROGRESS_INTERVAL {
            last_emit = Instant::now();
            taskbar(app, Some(p.fraction));
            emit(app, job);
        }
    });
    let updated = queue.update(id, |j| match res {
        Ok(outcome) => {
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
