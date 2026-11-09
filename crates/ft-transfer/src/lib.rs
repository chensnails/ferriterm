use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ft_vfs::{join_path, OpenMode, Vfs, VfsFuture};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex, OwnedSemaphorePermit, Semaphore};

const CHUNK_SIZE: u32 = 256 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);
pub const DEFAULT_CONCURRENCY: usize = 6;
pub const MAX_CONCURRENCY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferStatus {
    Pending,
    Running,
    Paused,
    Done,
    Failed,
    Cancelled,
}

impl TransferStatus {
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            TransferStatus::Done | TransferStatus::Failed | TransferStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferJob {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub target_path: String,
    pub size: u64,
    pub transferred: u64,
    pub status: TransferStatus,
    pub error: Option<String>,
    pub speed: u64,
    pub eta_seconds: Option<u64>,
    pub is_directory: bool,
    pub files_done: u32,
    pub files_total: u32,
    /// Entries the copy deliberately left behind — symbolic links today.
    #[serde(default)]
    pub skipped: u32,
    /// Both ends are remote, so the bytes travel through this machine instead
    /// of straight from one server to the other.
    pub relay: bool,
    /// Which session (or mount, or nothing for local) each end was bound to.
    /// These are what make a job resumable after a restart.
    #[serde(default)]
    pub source_session: Option<String>,
    #[serde(default)]
    pub target_session: Option<String>,
    #[serde(default)]
    pub source_remote: bool,
    #[serde(default)]
    pub target_remote: bool,
}

impl TransferJob {
    fn new(id: String, name: String, source: &Endpoint, target: &Endpoint) -> Self {
        Self {
            id,
            name,
            source_path: source.path.clone(),
            target_path: target.path.clone(),
            size: 0,
            transferred: 0,
            status: TransferStatus::Pending,
            error: None,
            speed: 0,
            eta_seconds: None,
            is_directory: false,
            relay: source.remote && target.remote,
            files_done: 0,
            files_total: 0,
            skipped: 0,
            source_session: source.session.clone(),
            target_session: target.session.clone(),
            source_remote: source.remote,
            target_remote: target.remote,
        }
    }
}

#[derive(Clone)]
pub struct Endpoint {
    /// `None` only for a job restored from disk: it learns its VFS from the
    /// resolver when the user resumes it, if that session is connected by then.
    pub vfs: Option<Arc<dyn Vfs>>,
    pub path: String,
    /// True for a remote session or mount, false for the local disk.
    pub remote: bool,
    pub session: Option<String>,
}

impl Endpoint {
    pub fn new(vfs: Arc<dyn Vfs>, path: impl Into<String>) -> Self {
        Self {
            vfs: Some(vfs),
            path: path.into(),
            remote: false,
            session: None,
        }
    }

    /// An end point that has a path but no connection yet.
    pub fn deferred(session: Option<String>, path: impl Into<String>, remote: bool) -> Self {
        Self {
            vfs: None,
            path: path.into(),
            remote,
            session,
        }
    }

    pub fn with_session(mut self, session: Option<String>) -> Self {
        self.session = session;
        self
    }

    /// The VFS to copy through. A deferred end point has none until binding.
    pub fn bound(&self) -> Result<&Arc<dyn Vfs>, String> {
        self.vfs
            .as_ref()
            .ok_or_else(|| "this transfer is not connected to a session".to_string())
    }

    /// Marks the endpoint as remote, which is what makes a copy between two
    /// endpoints a relay through this machine.
    pub fn as_remote(mut self) -> Self {
        self.remote = true;
        self
    }

    /// Explicit form of [`Endpoint::as_remote`] for callers that already know
    /// which side they hold.
    pub fn with_remote(mut self, remote: bool) -> Self {
        self.remote = remote;
        self
    }
}

struct JobState {
    job: Mutex<TransferJob>,
    paused: AtomicBool,
    cancelled: AtomicBool,
    /// False for a job restored from disk: it has no task yet, so resuming it
    /// has to start one.
    started: AtomicBool,
    /// Concurrency slot. It is taken away while the job is paused, so a
    /// paused job never keeps a slot a waiting job could use.
    slot: Mutex<Option<OwnedSemaphorePermit>>,
    source: Endpoint,
    target: Endpoint,
}

/// Turns a stored session id back into a live filesystem, which is what makes
/// a restored job resumable once its host is connected again.
pub trait EndpointResolver: Send + Sync {
    /// `None` is the local disk.
    fn resolve<'a>(&'a self, session: Option<&'a str>) -> VfsFuture<'a, (Arc<dyn Vfs>, bool)>;
}

/// Where finished and paused jobs are remembered so a restart does not lose
/// them. Both calls are best-effort: a dead database must not stop a copy.
pub trait JobLog: Send + Sync {
    fn save<'a>(&'a self, job: &'a TransferJob) -> VfsFuture<'a, ()>;
    fn delete<'a>(&'a self, id: &'a str) -> VfsFuture<'a, ()>;
}

struct Inner {
    jobs: Mutex<HashMap<String, Arc<JobState>>>,
    events: broadcast::Sender<TransferJob>,
    semaphore: Arc<Semaphore>,
    limit: AtomicUsize,
    /// Permits a lower concurrency setting could not reclaim yet because
    /// they were in use; dropped as jobs release their slots.
    debt: AtomicUsize,
    resolver: Option<Arc<dyn EndpointResolver>>,
    log: Option<Arc<dyn JobLog>>,
}

#[derive(Clone)]
pub struct TransferManager {
    inner: Arc<Inner>,
}

impl Default for TransferManager {
    fn default() -> Self {
        Self::new(DEFAULT_CONCURRENCY)
    }
}

impl TransferManager {
    pub fn new(concurrency: usize) -> Self {
        Self::with_hooks(concurrency, None, None)
    }

    pub fn with_hooks(
        concurrency: usize,
        resolver: Option<Arc<dyn EndpointResolver>>,
        log: Option<Arc<dyn JobLog>>,
    ) -> Self {
        let limit = concurrency.clamp(1, MAX_CONCURRENCY);
        Self {
            inner: Arc::new(Inner {
                jobs: Mutex::new(HashMap::new()),
                events: broadcast::channel(256).0,
                semaphore: Arc::new(Semaphore::new(limit)),
                limit: AtomicUsize::new(limit),
                debt: AtomicUsize::new(0),
                resolver,
                log,
            }),
        }
    }

    /// How many jobs may run at once.
    pub fn concurrency(&self) -> usize {
        self.inner.limit.load(Ordering::SeqCst)
    }

    /// Grows the queue immediately; shrinking takes the free permits back
    /// and settles the rest as running jobs finish.
    pub fn set_concurrency(&self, concurrency: usize) {
        let target = concurrency.clamp(1, MAX_CONCURRENCY);
        let previous = self.inner.limit.swap(target, Ordering::SeqCst);
        if target > previous {
            self.inner.semaphore.add_permits(target - previous);
        } else if target < previous {
            let wanted = previous - target;
            let removed = self.inner.semaphore.forget_permits(wanted);
            if removed < wanted {
                self.inner
                    .debt
                    .fetch_add(wanted - removed, Ordering::SeqCst);
            }
        }
    }

    async fn release_slot(&self, state: &JobState) {
        if state.slot.lock().await.take().is_some() {
            self.settle_debt();
        }
    }

    fn settle_debt(&self) {
        while self.inner.debt.load(Ordering::SeqCst) > 0 {
            if self.inner.semaphore.forget_permits(1) == 0 {
                return;
            }
            self.inner.debt.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TransferJob> {
        self.inner.events.subscribe()
    }

    pub async fn list(&self) -> Vec<TransferJob> {
        let jobs = self.inner.jobs.lock().await;
        let mut snapshots = Vec::with_capacity(jobs.len());
        for state in jobs.values() {
            snapshots.push(state.job.lock().await.clone());
        }
        drop(jobs);
        snapshots.sort_by(|a, b| a.id.cmp(&b.id));
        snapshots
    }

    pub async fn enqueue_file(
        &self,
        id: String,
        source: Endpoint,
        target: Endpoint,
        size: u64,
    ) -> String {
        let name = target
            .path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&target.path)
            .to_string();
        let mut job = TransferJob::new(id.clone(), name, &source, &target);
        job.size = size;
        job.files_total = 1;
        let state = Arc::new(JobState {
            job: Mutex::new(job),
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            started: AtomicBool::new(false),
            slot: Mutex::new(None),
            source,
            target,
        });
        self.register(state).await
    }

    pub async fn enqueue_tree(
        &self,
        id: String,
        source: Endpoint,
        target: Endpoint,
    ) -> Result<String, String> {
        let scan = scan_tree(source.bound()?.as_ref(), &source.path).await?;
        let name = source
            .path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&source.path)
            .to_string();
        let mut job = TransferJob::new(id.clone(), name, &source, &target);
        job.size = scan.total_size;
        job.files_total = scan.files;
        job.is_directory = true;
        let state = Arc::new(JobState {
            job: Mutex::new(job),
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            started: AtomicBool::new(false),
            slot: Mutex::new(None),
            source,
            target,
        });
        Ok(self.register(state).await)
    }

    /// Brings back the jobs a previous run left unfinished. They come back
    /// paused and unstarted: nothing reconnects on its own, and resuming one
    /// is what asks the resolver for its session.
    pub async fn restore(&self, jobs: Vec<TransferJob>) {
        for mut job in jobs {
            if job.status.is_finished() {
                continue;
            }
            job.status = TransferStatus::Paused;
            job.speed = 0;
            job.eta_seconds = None;
            job.error = Some("interrupted when the app closed; resume to continue".to_string());
            let state = Arc::new(JobState {
                job: Mutex::new(job.clone()),
                paused: AtomicBool::new(true),
                cancelled: AtomicBool::new(false),
                started: AtomicBool::new(false),
                slot: Mutex::new(None),
                source: Endpoint::deferred(
                    job.source_session.clone(),
                    &job.source_path,
                    job.source_remote,
                ),
                target: Endpoint::deferred(
                    job.target_session.clone(),
                    &job.target_path,
                    job.target_remote,
                ),
            });
            self.inner.jobs.lock().await.insert(job.id, state);
        }
    }

    async fn register(&self, state: Arc<JobState>) -> String {
        let id = state.job.lock().await.id.clone();
        self.inner
            .jobs
            .lock()
            .await
            .insert(id.clone(), state.clone());
        self.emit(&state).await;
        remember(&self.inner, &state).await;
        self.spawn(state);
        id
    }

    fn spawn(&self, state: Arc<JobState>) {
        let inner = self.inner.clone();
        let manager = self.clone();
        state.started.store(true, Ordering::SeqCst);
        tokio::spawn(async move {
            if !pause_gate(&inner, &state).await {
                manager.release_slot(&state).await;
                finish(
                    &inner,
                    &state,
                    TransferStatus::Cancelled,
                    Some("cancelled".into()),
                )
                .await;
                return;
            }
            set_status(&inner, &state, TransferStatus::Running, None).await;
            // A restored job has paths but no connection; this is where the
            // session is looked up again, and where resume fails loudly when
            // the host is not connected.
            let result = match bind(&inner, &state).await {
                Ok((source, target)) => {
                    if state.job.lock().await.is_directory {
                        copy_tree(&inner, &state, &source, &target).await
                    } else {
                        copy_single(&inner, &state, &source, &target).await
                    }
                }
                Err(error) => Err(error),
            };
            manager.release_slot(&state).await;
            match result {
                Ok(()) => {
                    finish(&inner, &state, TransferStatus::Done, None).await;
                }
                Err(error) => {
                    if state.cancelled.load(Ordering::SeqCst) {
                        finish(
                            &inner,
                            &state,
                            TransferStatus::Cancelled,
                            Some("cancelled".into()),
                        )
                        .await;
                    } else {
                        finish(&inner, &state, TransferStatus::Failed, Some(error)).await;
                    }
                }
            }
        });
    }

    pub async fn pause(&self, id: &str) {
        if let Some(state) = self.find(id).await {
            if !state.job.lock().await.status.is_finished() {
                state.paused.store(true, Ordering::SeqCst);
                set_status(&self.inner, &state, TransferStatus::Paused, None).await;
                remember(&self.inner, &state).await;
            }
        }
    }

    pub async fn resume(&self, id: &str) {
        if let Some(state) = self.find(id).await {
            if state.job.lock().await.status == TransferStatus::Paused {
                state.paused.store(false, Ordering::SeqCst);
                set_status(&self.inner, &state, TransferStatus::Running, None).await;
                remember(&self.inner, &state).await;
                // A restored job has no task waiting on the pause gate, so
                // resuming it means starting one.
                if !state.started.load(Ordering::SeqCst) {
                    self.spawn(state);
                }
            }
        }
    }

    pub async fn cancel(&self, id: &str) {
        if let Some(state) = self.find(id).await {
            state.cancelled.store(true, Ordering::SeqCst);
            state.paused.store(false, Ordering::SeqCst);
            state.slot.lock().await.take();
            if state.job.lock().await.status != TransferStatus::Running {
                finish(
                    &self.inner,
                    &state,
                    TransferStatus::Cancelled,
                    Some("cancelled".into()),
                )
                .await;
            }
        }
    }

    pub async fn retry(&self, id: &str) {
        let Some(state) = self.find(id).await else {
            return;
        };
        {
            let mut job = state.job.lock().await;
            if !matches!(
                job.status,
                TransferStatus::Failed | TransferStatus::Cancelled
            ) {
                return;
            }
            job.status = TransferStatus::Pending;
            job.error = None;
            job.speed = 0;
            job.eta_seconds = None;
            // The old byte count would make the next attempt start at a
            // progress it has not earned.
            job.transferred = 0;
            job.files_done = 0;
            job.skipped = 0;
        }
        state.cancelled.store(false, Ordering::SeqCst);
        state.paused.store(false, Ordering::SeqCst);
        self.emit(&state).await;
        remember(&self.inner, &state).await;
        self.spawn(state);
    }

    pub async fn remove(&self, id: &str) {
        if let Some(state) = self.inner.jobs.lock().await.remove(id) {
            state.cancelled.store(true, Ordering::SeqCst);
            recall(&self.inner, id).await;
        }
    }

    pub async fn clear_finished(&self) {
        let mut finished = Vec::new();
        {
            let jobs = self.inner.jobs.lock().await;
            for (id, state) in jobs.iter() {
                if state.job.lock().await.status.is_finished() {
                    finished.push(id.clone());
                }
            }
        }
        {
            let mut jobs = self.inner.jobs.lock().await;
            for id in &finished {
                jobs.remove(id);
            }
        }
        if let Some(log) = &self.inner.log {
            for id in &finished {
                let _ = log.delete(id).await;
            }
        }
    }

    async fn find(&self, id: &str) -> Option<Arc<JobState>> {
        self.inner.jobs.lock().await.get(id).cloned()
    }

    async fn emit(&self, state: &JobState) {
        let snapshot = state.job.lock().await.clone();
        let _ = self.inner.events.send(snapshot);
    }
}

/// Keeps a running job in step with pause/cancel and owns its concurrency
/// slot: the permit is dropped while the job is paused and taken again on
/// resume, so paused jobs never starve the queue.
async fn pause_gate(inner: &Inner, state: &JobState) -> bool {
    loop {
        if state.cancelled.load(Ordering::SeqCst) {
            state.slot.lock().await.take();
            return false;
        }
        if state.paused.load(Ordering::SeqCst) {
            state.slot.lock().await.take();
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }
        let mut slot = state.slot.lock().await;
        if slot.is_some() {
            return true;
        }
        if state.paused.load(Ordering::SeqCst) {
            continue;
        }
        match inner.semaphore.clone().acquire_owned().await {
            Ok(permit) => {
                *slot = Some(permit);
                return true;
            }
            Err(_) => return false,
        }
    }
}

async fn set_status(
    inner: &Inner,
    state: &JobState,
    status: TransferStatus,
    error: Option<String>,
) {
    {
        let mut job = state.job.lock().await;
        job.status = status;
        job.error = error;
        if status == TransferStatus::Running {
            job.speed = 0;
        }
    }
    emit_snapshot(inner, state).await;
}

async fn finish(inner: &Inner, state: &JobState, status: TransferStatus, error: Option<String>) {
    {
        let mut job = state.job.lock().await;
        job.status = status;
        job.error = error;
        job.speed = 0;
        job.eta_seconds = None;
        if status == TransferStatus::Done {
            job.transferred = job.size.max(job.transferred);
        }
    }
    state.started.store(false, Ordering::SeqCst);
    emit_snapshot(inner, state).await;
    remember(inner, state).await;
}

/// Writes the job's current shape to the log, so a restart can offer it back.
async fn remember(inner: &Inner, state: &JobState) {
    let Some(log) = &inner.log else {
        return;
    };
    let job = state.job.lock().await.clone();
    let _ = log.save(&job).await;
}

async fn recall(inner: &Inner, id: &str) {
    if let Some(log) = &inner.log {
        let _ = log.delete(id).await;
    }
}

/// Fills in the VFS of both ends, resolving sessions that a restored job only
/// knows by name. A job enqueued in this run is already bound and costs nothing.
async fn bind(inner: &Inner, state: &JobState) -> Result<(Endpoint, Endpoint), String> {
    Ok((
        bind_one(inner, &state.source).await?,
        bind_one(inner, &state.target).await?,
    ))
}

async fn bind_one(inner: &Inner, endpoint: &Endpoint) -> Result<Endpoint, String> {
    if endpoint.vfs.is_some() {
        return Ok(endpoint.clone());
    }
    let resolver = inner
        .resolver
        .as_ref()
        .ok_or_else(|| "this transfer has no session to reconnect to".to_string())?;
    let (vfs, remote) = resolver.resolve(endpoint.session.as_deref()).await?;
    Ok(Endpoint {
        vfs: Some(vfs),
        path: endpoint.path.clone(),
        remote,
        session: endpoint.session.clone(),
    })
}

async fn emit_snapshot(inner: &Inner, state: &JobState) {
    let snapshot = state.job.lock().await.clone();
    let _ = inner.events.send(snapshot);
}

struct Progress<'a> {
    inner: &'a Inner,
    state: &'a JobState,
    last_emit: Instant,
    last_bytes: u64,
    started: Instant,
}

impl<'a> Progress<'a> {
    fn new(inner: &'a Inner, state: &'a JobState) -> Self {
        Self {
            inner,
            state,
            last_emit: Instant::now(),
            last_bytes: 0,
            started: Instant::now(),
        }
    }

    async fn advance(&mut self, delta: u64) {
        let (transferred, size) = {
            let mut job = self.state.job.lock().await;
            job.transferred = job.transferred.saturating_add(delta);
            (job.transferred, job.size)
        };
        if self.last_emit.elapsed() < PROGRESS_INTERVAL {
            return;
        }
        let elapsed = self.started.elapsed().as_secs_f64().max(0.001);
        let speed = ((transferred - self.last_bytes) as f64 / elapsed) as u64;
        let eta = if speed > 0 && size > transferred {
            Some((size - transferred) / speed)
        } else if size > 0 && transferred >= size {
            Some(0)
        } else {
            None
        };
        {
            let mut job = self.state.job.lock().await;
            job.speed = speed;
            job.eta_seconds = eta;
        }
        self.last_emit = Instant::now();
        self.last_bytes = transferred;
        self.started = Instant::now();
        emit_snapshot(self.inner, self.state).await;
    }

    async fn file_done(&mut self) {
        self.state.job.lock().await.files_done += 1;
    }
}

async fn copy_single(
    inner: &Inner,
    state: &JobState,
    source: &Endpoint,
    target: &Endpoint,
) -> Result<(), String> {
    let mut progress = Progress::new(inner, state);
    copy_file(
        source.bound()?.as_ref(),
        &source.path,
        target.bound()?.as_ref(),
        &target.path,
        &mut progress,
    )
    .await?;
    progress.file_done().await;
    Ok(())
}

async fn copy_file(
    source: &dyn Vfs,
    source_path: &str,
    target: &dyn Vfs,
    target_path: &str,
    progress: &mut Progress<'_>,
) -> Result<(), String> {
    let source_entry = source.stat(source_path).await?;
    if let Some(parent) = ft_vfs::parent_path(target_path) {
        ensure_dir(target, &parent).await?;
    }

    let mut offset = 0u64;
    let mut truncate = true;
    if let Ok(existing) = target.stat(target_path).await {
        if existing.size == source_entry.size && !existing.is_dir {
            progress.advance(source_entry.size).await;
            return Ok(());
        }
        if existing.size < source_entry.size {
            offset = existing.size;
            truncate = false;
        }
    }

    let mut reader = source.open(source_path, OpenMode::Read).await?;
    let mut writer = target
        .open(target_path, OpenMode::Write { truncate })
        .await?;
    let mut flushed = offset;
    while offset < source_entry.size {
        if !pause_gate(progress.inner, progress.state).await {
            let _ = writer.close().await;
            let _ = reader.close().await;
            return Err("cancelled".to_string());
        }
        let len = CHUNK_SIZE.min((source_entry.size - offset).min(u64::from(u32::MAX)) as u32);
        let chunk = reader.read(offset, len).await?;
        if chunk.is_empty() {
            break;
        }
        writer.write(offset, &chunk).await?;
        offset += chunk.len() as u64;
        progress.advance(chunk.len() as u64).await;
        flushed = offset;
    }
    let missing = source_entry.size.saturating_sub(flushed);
    if missing > 0 {
        progress.advance(missing).await;
    }
    writer.close().await?;
    let _ = reader.close().await;
    // Keep the copy looking like the original: same permissions, same
    // modification time. Both are best-effort — the bytes are already there.
    let _ = target.chmod(target_path, source_entry.mode).await;
    let _ = target.set_times(target_path, source_entry.mtime).await;
    Ok(())
}

async fn ensure_dir(vfs: &dyn Vfs, path: &str) -> Result<(), String> {
    if path.is_empty() || path == "/" {
        return Ok(());
    }
    if vfs
        .stat(path)
        .await
        .map(|entry| entry.is_dir)
        .unwrap_or(false)
    {
        return Ok(());
    }
    let _ = vfs.mkdir(path).await;
    Ok(())
}

struct Scan {
    total_size: u64,
    files: u32,
    entries: Vec<ScanEntry>,
}

struct ScanEntry {
    relative: String,
    is_dir: bool,
    size: u64,
    mtime: u64,
    mode: u32,
    is_symlink: bool,
}

/// Walks a remote or local tree; entries are returned parents-first.
async fn scan_tree(vfs: &dyn Vfs, root: &str) -> Result<Scan, String> {
    let mut scan = Scan {
        total_size: 0,
        files: 0,
        entries: Vec::new(),
    };
    let mut pending = vec![(root.to_string(), String::new())];
    while let Some((path, relative)) = pending.pop() {
        let entries = vfs.list(&path, true).await?;
        for entry in entries {
            let child_relative = if relative.is_empty() {
                entry.name.clone()
            } else {
                join_path(&relative, &entry.name)
            };
            if entry.is_dir {
                scan.entries.push(ScanEntry {
                    relative: child_relative.clone(),
                    is_dir: true,
                    size: 0,
                    mtime: entry.mtime,
                    mode: entry.mode,
                    is_symlink: entry.is_symlink,
                });
                pending.push((entry.path.clone(), child_relative));
            } else {
                scan.total_size += entry.size;
                scan.files += 1;
                scan.entries.push(ScanEntry {
                    relative: child_relative,
                    is_dir: false,
                    size: entry.size,
                    mtime: entry.mtime,
                    mode: entry.mode,
                    is_symlink: entry.is_symlink,
                });
            }
        }
    }
    Ok(scan)
}

async fn copy_tree(
    inner: &Inner,
    state: &JobState,
    source: &Endpoint,
    target: &Endpoint,
) -> Result<(), String> {
    let scan = scan_tree(source.bound()?.as_ref(), &source.path).await?;
    {
        let mut job = state.job.lock().await;
        job.size = scan.total_size;
        job.files_total = scan.files;
    }
    emit_snapshot(inner, state).await;
    ensure_dir(target.bound()?.as_ref(), &target.path).await?;

    let mut progress = Progress::new(inner, state);
    let mut skipped = 0u32;
    for entry in scan.entries {
        if !pause_gate(inner, state).await {
            return Err("cancelled".to_string());
        }
        let source_path = join_path(&source.path, &entry.relative);
        let target_path = join_path(&target.path, &entry.relative);
        if entry.is_symlink {
            // Reading a link as a file copies whatever it points at, and a
            // self-referencing tree would never end. Leave it out and count it
            // so the queue can say the copy was not complete.
            skipped += 1;
            continue;
        }
        if entry.is_dir {
            ensure_dir(target.bound()?.as_ref(), &target_path).await?;
            copy_attributes(target.bound()?.as_ref(), &target_path, &entry).await;
        } else {
            copy_file(
                source.bound()?.as_ref(),
                &source_path,
                target.bound()?.as_ref(),
                &target_path,
                &mut progress,
            )
            .await?;
            progress.file_done().await;
        }
    }
    if skipped > 0 {
        state.job.lock().await.skipped = skipped;
        emit_snapshot(inner, state).await;
    }
    Ok(())
}

/// Permissions and timestamps are a courtesy: a backend that cannot store them
/// (S3, WebDAV) must not fail the copy that already delivered the bytes.
async fn copy_attributes(target: &dyn Vfs, path: &str, entry: &ScanEntry) {
    let _ = target.chmod(path, entry.mode).await;
    let _ = target.set_times(path, entry.mtime).await;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncActionKind {
    CreateDir,
    CopyFile,
    DeleteFile,
    DeleteDir,
}

/// Which tree receives an action. A one-way sync only ever writes to the
/// target; a two-way sync pulls missing entries back into the source as well.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncSide {
    #[default]
    Target,
    Source,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncAction {
    pub kind: SyncActionKind,
    #[serde(default)]
    pub side: SyncSide,
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictReason {
    /// Same name on both sides, different size or modification time.
    ContentDiffers,
    /// One side is a file, the other a directory.
    TypeMismatch,
}

/// A pair of entries that both trees own but that disagree. Two-way sync never
/// resolves these on its own: the caller picks a side (or skips the entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub path: String,
    pub reason: ConflictReason,
    pub source_size: u64,
    pub source_mtime: u64,
    pub target_size: u64,
    pub target_mtime: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    #[serde(default)]
    pub source_root: String,
    #[serde(default)]
    pub target_root: String,
    #[serde(default)]
    pub two_way: bool,
    pub actions: Vec<SyncAction>,
    pub up_files: usize,
    pub up_bytes: u64,
    pub down_files: usize,
    pub down_bytes: u64,
    pub create_dirs: usize,
    pub delete_entries: usize,
    #[serde(default)]
    pub conflicts: Vec<SyncConflict>,
}

impl SyncPlan {
    fn roots(source_root: &str, target_root: &str, two_way: bool) -> Self {
        Self {
            source_root: source_root.to_string(),
            target_root: target_root.to_string(),
            two_way,
            ..Self::default()
        }
    }
}

/// Compares a source tree with a target tree, producing the actions needed to
/// make the target match the source.
pub async fn preview_sync(
    source: &dyn Vfs,
    source_root: &str,
    target: &dyn Vfs,
    target_root: &str,
    delete_extra: bool,
) -> Result<SyncPlan, String> {
    let source_scan = scan_tree(source, source_root).await?;
    let mut plan = SyncPlan::roots(source_root, target_root, false);
    let mut source_relatives: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in &source_scan.entries {
        source_relatives.insert(entry.relative.clone());
        let target_path = join_path(target_root, &entry.relative);
        let existing = target.stat(&target_path).await.ok();
        if entry.is_dir {
            if !existing.map(|entry| entry.is_dir).unwrap_or(false) {
                plan.actions.push(SyncAction {
                    kind: SyncActionKind::CreateDir,
                    side: SyncSide::Target,
                    path: entry.relative.clone(),
                    size: 0,
                });
                plan.create_dirs += 1;
            }
        } else {
            let needs_copy = match &existing {
                None => true,
                Some(existing) => {
                    !existing.is_dir
                        && (existing.size != entry.size || existing.mtime != entry.mtime)
                }
            };
            if needs_copy {
                plan.actions.push(SyncAction {
                    kind: SyncActionKind::CopyFile,
                    side: SyncSide::Target,
                    path: entry.relative.clone(),
                    size: entry.size,
                });
                plan.up_bytes += entry.size;
                plan.up_files += 1;
            }
        }
    }

    if delete_extra {
        let mut deletions: Vec<(usize, SyncAction)> = Vec::new();
        collect_target_extras(target, target_root, "", &source_relatives, &mut deletions).await?;
        deletions.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        for (_, action) in deletions {
            plan.delete_entries += 1;
            plan.actions.push(action);
        }
    }

    Ok(plan)
}

async fn collect_target_extras(
    target: &dyn Vfs,
    target_root: &str,
    relative: &str,
    source_relatives: &std::collections::HashSet<String>,
    out: &mut Vec<(usize, SyncAction)>,
) -> Result<(), String> {
    let path = if relative.is_empty() {
        target_root.to_string()
    } else {
        join_path(target_root, relative)
    };
    let entries = target.list(&path, true).await?;
    for entry in entries {
        let child = if relative.is_empty() {
            entry.name.clone()
        } else {
            join_path(relative, &entry.name)
        };
        if !source_relatives.contains(&child) {
            let depth = child.matches('/').count();
            out.push((
                depth,
                SyncAction {
                    kind: if entry.is_dir {
                        SyncActionKind::DeleteDir
                    } else {
                        SyncActionKind::DeleteFile
                    },
                    side: SyncSide::Target,
                    path: child,
                    size: entry.size,
                },
            ));
        } else if entry.is_dir {
            Box::pin(collect_target_extras(
                target,
                target_root,
                &child,
                source_relatives,
                out,
            ))
            .await?;
        }
    }
    Ok(())
}

fn push_entry_action(plan: &mut SyncPlan, entry: &ScanEntry, side: SyncSide) {
    if entry.is_dir {
        plan.actions.push(SyncAction {
            kind: SyncActionKind::CreateDir,
            side,
            path: entry.relative.clone(),
            size: 0,
        });
        plan.create_dirs += 1;
    } else {
        plan.actions.push(SyncAction {
            kind: SyncActionKind::CopyFile,
            side,
            path: entry.relative.clone(),
            size: entry.size,
        });
        match side {
            SyncSide::Target => {
                plan.up_bytes += entry.size;
                plan.up_files += 1;
            }
            SyncSide::Source => {
                plan.down_bytes += entry.size;
                plan.down_files += 1;
            }
        }
    }
}

/// Compares both trees in both directions. Entries owned by one side only are
/// copied to the other; entries that exist on both sides but disagree are
/// reported as conflicts and left for the caller to resolve.
pub async fn preview_sync_two_way(
    source: &dyn Vfs,
    source_root: &str,
    target: &dyn Vfs,
    target_root: &str,
) -> Result<SyncPlan, String> {
    let source_scan = scan_tree(source, source_root).await?;
    let target_scan = scan_tree(target, target_root).await?;
    let mut plan = SyncPlan::roots(source_root, target_root, true);
    let mut target_entries: HashMap<String, ScanEntry> = target_scan
        .entries
        .into_iter()
        .map(|entry| (entry.relative.clone(), entry))
        .collect();

    let mut pending: Vec<(usize, ScanEntry, SyncSide)> = Vec::new();
    for entry in source_scan.entries {
        match target_entries.remove(&entry.relative) {
            None => {
                let depth = entry.relative.matches('/').count();
                pending.push((depth, entry, SyncSide::Target));
            }
            Some(other) => {
                if entry.is_dir && other.is_dir {
                    continue;
                }
                let reason = if entry.is_dir != other.is_dir {
                    ConflictReason::TypeMismatch
                } else if entry.size == other.size && entry.mtime == other.mtime {
                    continue;
                } else {
                    ConflictReason::ContentDiffers
                };
                plan.conflicts.push(SyncConflict {
                    path: entry.relative.clone(),
                    reason,
                    source_size: entry.size,
                    source_mtime: entry.mtime,
                    target_size: other.size,
                    target_mtime: other.mtime,
                });
            }
        }
    }
    for (_, entry) in target_entries {
        let depth = entry.relative.matches('/').count();
        pending.push((depth, entry, SyncSide::Source));
    }

    // Parents before children, so a directory is always created before the
    // entries it holds on that side.
    pending.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.is_dir.cmp(&a.1.is_dir)));
    for (_, entry, side) in pending {
        push_entry_action(&mut plan, &entry, side);
    }
    plan.conflicts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(plan)
}

/// Applies a previously previewed plan: creates directories, enqueues copies
/// and deletes entries, honouring the side each action belongs to.
pub async fn apply_sync(
    manager: &TransferManager,
    source: &Endpoint,
    target: &Endpoint,
    plan: &SyncPlan,
    id_prefix: &str,
) -> Result<(), String> {
    for (index, action) in plan.actions.iter().enumerate() {
        let (dest, from) = match action.side {
            SyncSide::Target => (target, source),
            SyncSide::Source => (source, target),
        };
        let dest_path = join_path(&dest.path, &action.path);
        match action.kind {
            SyncActionKind::CreateDir => ensure_dir(dest.bound()?.as_ref(), &dest_path).await?,
            SyncActionKind::CopyFile => {
                if let Some(parent) = ft_vfs::parent_path(&dest_path) {
                    ensure_dir(dest.bound()?.as_ref(), &parent).await?;
                }
                manager
                    .enqueue_file(
                        format!("{id_prefix}-{index}"),
                        Endpoint::new(from.bound()?.clone(), join_path(&from.path, &action.path))
                            .with_remote(from.remote)
                            .with_session(from.session.clone()),
                        Endpoint::new(dest.bound()?.clone(), dest_path)
                            .with_remote(dest.remote)
                            .with_session(dest.session.clone()),
                        action.size,
                    )
                    .await;
            }
            SyncActionKind::DeleteFile => dest.bound()?.remove_file(&dest_path).await?,
            SyncActionKind::DeleteDir => dest.bound()?.remove_dir_all(&dest_path).await?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ft_vfs::{LocalFs, Vfs};

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ft-transfer-{tag}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    async fn wait_done(manager: &TransferManager, id: &str) -> TransferJob {
        for _ in 0..500 {
            let jobs = manager.list().await;
            if let Some(job) = jobs.iter().find(|job| job.id == id) {
                if job.status.is_finished() {
                    return job.clone();
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("transfer {id} did not finish");
    }

    #[tokio::test]
    async fn copies_file_and_resumes_partial() {
        let root = temp_dir("file");
        let source = root.join("source.txt");
        std::fs::write(&source, b"hello transfer").unwrap();
        let target = root.join("target.txt");
        std::fs::write(&target, b"hello ").unwrap();

        let manager = TransferManager::new(2);
        let id = manager
            .enqueue_file(
                "one".to_string(),
                Endpoint::new(Arc::new(LocalFs), source.to_string_lossy()),
                Endpoint::new(Arc::new(LocalFs), target.to_string_lossy()),
                14,
            )
            .await;
        let job = wait_done(&manager, &id).await;
        assert_eq!(job.status, TransferStatus::Done);
        assert_eq!(std::fs::read(&target).unwrap(), b"hello transfer");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn copies_directory_tree() {
        let root = temp_dir("tree");
        let src = root.join("src");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("a.txt"), b"aaaa").unwrap();
        std::fs::write(src.join("nested/b.txt"), b"bbbbbb").unwrap();
        let dst = root.join("dst");

        let manager = TransferManager::default();
        let id = manager
            .enqueue_tree(
                "tree".to_string(),
                Endpoint::new(Arc::new(LocalFs), src.to_string_lossy()),
                Endpoint::new(Arc::new(LocalFs), dst.to_string_lossy()),
            )
            .await
            .unwrap();
        let job = wait_done(&manager, &id).await;
        assert_eq!(job.status, TransferStatus::Done);
        assert_eq!(job.files_done, 2);
        assert_eq!(std::fs::read(dst.join("nested/b.txt")).unwrap(), b"bbbbbb");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A copy should keep the source's timestamp and mode, so a moved tree
    /// still looks like the tree it came from.
    #[tokio::test]
    async fn keeps_the_source_timestamp() {
        let root = temp_dir("attrs");
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let file = src.join("stamped.txt");
        std::fs::write(&file, b"stamped").unwrap();

        // Far enough in the past that "now" cannot pass for it.
        const WHEN: u64 = 1_600_000_000;
        let fs = LocalFs;
        fs.set_times(&file.to_string_lossy(), WHEN).await.unwrap();

        let dst = root.join("dst");
        let manager = TransferManager::default();
        let id = manager
            .enqueue_tree(
                "attrs".to_string(),
                Endpoint::new(Arc::new(LocalFs), src.to_string_lossy()),
                Endpoint::new(Arc::new(LocalFs), dst.to_string_lossy()),
            )
            .await
            .unwrap();
        assert_eq!(wait_done(&manager, &id).await.status, TransferStatus::Done);

        let copied = std::fs::metadata(dst.join("stamped.txt")).unwrap();
        let mtime = copied
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(mtime, WHEN);
        assert_eq!(std::fs::read(dst.join("stamped.txt")).unwrap(), b"stamped");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn previews_and_deletes_extra_entries() {
        let root = temp_dir("sync");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("keep.txt"), b"keep").unwrap();
        std::fs::write(dst.join("stale.txt"), b"stale").unwrap();

        let local = Arc::new(LocalFs);
        let plan = preview_sync(
            local.as_ref(),
            src.to_string_lossy().as_ref(),
            local.as_ref(),
            dst.to_string_lossy().as_ref(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(plan.up_files, 1);
        assert_eq!(plan.delete_entries, 1);

        let source = Endpoint::new(local.clone(), src.to_string_lossy());
        let target = Endpoint::new(local, dst.to_string_lossy());
        let manager = TransferManager::default();
        apply_sync(&manager, &source, &target, &plan, "sync")
            .await
            .unwrap();
        for _ in 0..200 {
            if manager
                .list()
                .await
                .iter()
                .all(|job| job.status.is_finished())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(dst.join("keep.txt").exists());
        assert!(!dst.join("stale.txt").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn two_way_sync_copies_both_directions_and_reports_conflicts() {
        let root = temp_dir("twoway");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("only-source.txt"), b"up").unwrap();
        std::fs::write(dst.join("only-target.txt"), b"down").unwrap();
        std::fs::write(src.join("clash.txt"), b"left").unwrap();
        std::fs::write(dst.join("clash.txt"), b"right-side").unwrap();

        let local = Arc::new(LocalFs);
        let src_str = src.to_string_lossy().to_string();
        let dst_str = dst.to_string_lossy().to_string();
        let plan = preview_sync_two_way(local.as_ref(), &src_str, local.as_ref(), &dst_str)
            .await
            .unwrap();
        assert!(plan.two_way);
        assert_eq!(plan.up_files, 1);
        assert_eq!(plan.down_files, 1);
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].path, "clash.txt");
        assert_eq!(plan.conflicts[0].reason, ConflictReason::ContentDiffers);
        assert_eq!(plan.actions.len(), 2);

        let source = Endpoint::new(local.clone(), src_str);
        let target = Endpoint::new(local, dst_str);
        let manager = TransferManager::default();
        apply_sync(&manager, &source, &target, &plan, "twoway")
            .await
            .unwrap();
        for _ in 0..200 {
            if manager
                .list()
                .await
                .iter()
                .all(|job| job.status.is_finished())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(std::fs::read(dst.join("only-source.txt")).unwrap(), b"up");
        assert_eq!(std::fs::read(src.join("only-target.txt")).unwrap(), b"down");
        assert_eq!(std::fs::read(src.join("clash.txt")).unwrap(), b"left");
        assert_eq!(std::fs::read(dst.join("clash.txt")).unwrap(), b"right-side");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The promise of the whole feature: a job written down before the app
    /// closed comes back paused, and resuming it finishes the copy once its
    /// session can be resolved again.
    struct SessionLookup;

    impl EndpointResolver for SessionLookup {
        fn resolve<'a>(&'a self, session: Option<&'a str>) -> VfsFuture<'a, (Arc<dyn Vfs>, bool)> {
            Box::pin(async move {
                let vfs: Arc<dyn Vfs> = Arc::new(LocalFs);
                Ok((vfs, session.is_some()))
            })
        }
    }

    #[tokio::test]
    async fn resumes_a_job_restored_from_disk() {
        let root = temp_dir("restore");
        let src = root.join("a.txt");
        std::fs::write(&src, b"payload").unwrap();
        let first = root.join("first.txt");

        // A run that finished, only to be turned into the record a restart
        // would find: saved while unfinished, with a session id but no VFS.
        let before = TransferManager::new(1);
        let id = before
            .enqueue_file(
                "job".to_string(),
                Endpoint::new(Arc::new(LocalFs), src.to_string_lossy())
                    .with_session(Some("session-1".to_string())),
                Endpoint::new(Arc::new(LocalFs), first.to_string_lossy()),
                7,
            )
            .await;
        assert_eq!(wait_done(&before, &id).await.status, TransferStatus::Done);
        let mut stored = before.list().await.remove(0);
        assert_eq!(stored.source_session.as_deref(), Some("session-1"));

        // The second run knows nothing about those bytes yet.
        let second = root.join("second.txt");
        std::fs::remove_file(&first).unwrap();
        stored.status = TransferStatus::Pending;
        stored.transferred = 0;
        stored.target_path = second.to_string_lossy().into_owned();

        let after = TransferManager::with_hooks(1, Some(Arc::new(SessionLookup)), None);
        after.restore(vec![stored]).await;
        let restored = after.list().await.remove(0);
        assert_eq!(restored.status, TransferStatus::Paused);
        assert!(restored.error.is_some());

        after.resume("job").await;
        let job = wait_done(&after, "job").await;
        assert_eq!(job.status, TransferStatus::Done);
        assert_eq!(std::fs::read(&second).unwrap(), b"payload");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
