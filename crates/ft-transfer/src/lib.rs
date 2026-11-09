use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ft_vfs::{join_path, OpenMode, Vfs};
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
            files_done: 0,
            files_total: 0,
        }
    }
}

#[derive(Clone)]
pub struct Endpoint {
    pub vfs: Arc<dyn Vfs>,
    pub path: String,
}

impl Endpoint {
    pub fn new(vfs: Arc<dyn Vfs>, path: impl Into<String>) -> Self {
        Self {
            vfs,
            path: path.into(),
        }
    }
}

struct JobState {
    job: Mutex<TransferJob>,
    paused: AtomicBool,
    cancelled: AtomicBool,
    /// Concurrency slot. It is taken away while the job is paused, so a
    /// paused job never keeps a slot a waiting job could use.
    slot: Mutex<Option<OwnedSemaphorePermit>>,
    source: Endpoint,
    target: Endpoint,
}

struct Inner {
    jobs: Mutex<HashMap<String, Arc<JobState>>>,
    events: broadcast::Sender<TransferJob>,
    semaphore: Arc<Semaphore>,
    limit: AtomicUsize,
    /// Permits a lower concurrency setting could not reclaim yet because
    /// they were in use; dropped as jobs release their slots.
    debt: AtomicUsize,
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
        let limit = concurrency.clamp(1, MAX_CONCURRENCY);
        Self {
            inner: Arc::new(Inner {
                jobs: Mutex::new(HashMap::new()),
                events: broadcast::channel(256).0,
                semaphore: Arc::new(Semaphore::new(limit)),
                limit: AtomicUsize::new(limit),
                debt: AtomicUsize::new(0),
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
                self.inner.debt.fetch_add(wanted - removed, Ordering::SeqCst);
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
        let scan = scan_tree(source.vfs.as_ref(), &source.path).await?;
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
            slot: Mutex::new(None),
            source,
            target,
        });
        Ok(self.register(state).await)
    }

    async fn register(&self, state: Arc<JobState>) -> String {
        let id = state.job.lock().await.id.clone();
        self.inner
            .jobs
            .lock()
            .await
            .insert(id.clone(), state.clone());
        self.emit(&state).await;
        self.spawn(state);
        id
    }

    fn spawn(&self, state: Arc<JobState>) {
        let inner = self.inner.clone();
        let manager = self.clone();
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
            let result = if state.job.lock().await.is_directory {
                copy_tree(&inner, &state).await
            } else {
                copy_single(&inner, &state).await
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
            }
        }
    }

    pub async fn resume(&self, id: &str) {
        if let Some(state) = self.find(id).await {
            if state.job.lock().await.status == TransferStatus::Paused {
                state.paused.store(false, Ordering::SeqCst);
                set_status(&self.inner, &state, TransferStatus::Running, None).await;
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
        }
        state.cancelled.store(false, Ordering::SeqCst);
        state.paused.store(false, Ordering::SeqCst);
        self.emit(&state).await;
        self.spawn(state);
    }

    pub async fn remove(&self, id: &str) {
        if let Some(state) = self.inner.jobs.lock().await.remove(id) {
            state.cancelled.store(true, Ordering::SeqCst);
        }
    }

    pub async fn clear_finished(&self) {
        let mut jobs = self.inner.jobs.lock().await;
        let mut finished = Vec::new();
        for (id, state) in jobs.iter() {
            if state.job.lock().await.status.is_finished() {
                finished.push(id.clone());
            }
        }
        for id in finished {
            jobs.remove(&id);
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
    emit_snapshot(inner, state).await;
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

async fn copy_single(inner: &Inner, state: &JobState) -> Result<(), String> {
    let mut progress = Progress::new(inner, state);
    copy_file(
        state.source.vfs.as_ref(),
        &state.source.path,
        state.target.vfs.as_ref(),
        &state.target.path,
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
                });
            }
        }
    }
    Ok(scan)
}

async fn copy_tree(inner: &Inner, state: &JobState) -> Result<(), String> {
    let scan = scan_tree(state.source.vfs.as_ref(), &state.source.path).await?;
    {
        let mut job = state.job.lock().await;
        job.size = scan.total_size;
        job.files_total = scan.files;
    }
    emit_snapshot(inner, state).await;
    ensure_dir(state.target.vfs.as_ref(), &state.target.path).await?;

    let mut progress = Progress::new(inner, state);
    for entry in scan.entries {
        if !pause_gate(inner, state).await {
            return Err("cancelled".to_string());
        }
        let source_path = join_path(&state.source.path, &entry.relative);
        let target_path = join_path(&state.target.path, &entry.relative);
        if entry.is_dir {
            ensure_dir(state.target.vfs.as_ref(), &target_path).await?;
        } else {
            copy_file(
                state.source.vfs.as_ref(),
                &source_path,
                state.target.vfs.as_ref(),
                &target_path,
                &mut progress,
            )
            .await?;
            progress.file_done().await;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncActionKind {
    CreateDir,
    CopyFile,
    DeleteFile,
    DeleteDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncAction {
    pub kind: SyncActionKind,
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub actions: Vec<SyncAction>,
    pub upload_bytes: u64,
    pub create_dirs: usize,
    pub copy_files: usize,
    pub delete_entries: usize,
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
    let mut plan = SyncPlan::default();
    let mut source_relatives: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in &source_scan.entries {
        source_relatives.insert(entry.relative.clone());
        let target_path = join_path(target_root, &entry.relative);
        let existing = target.stat(&target_path).await.ok();
        if entry.is_dir {
            if !existing.map(|entry| entry.is_dir).unwrap_or(false) {
                plan.actions.push(SyncAction {
                    kind: SyncActionKind::CreateDir,
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
                    path: entry.relative.clone(),
                    size: entry.size,
                });
                plan.upload_bytes += entry.size;
                plan.copy_files += 1;
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

/// Applies a previously previewed plan: creates directories, enqueues copies
/// and deletes entries missing from the source.
pub async fn apply_sync(
    manager: &TransferManager,
    source: &Endpoint,
    target: &Endpoint,
    plan: &SyncPlan,
    id_prefix: &str,
) -> Result<(), String> {
    for (index, action) in plan.actions.iter().enumerate() {
        let target_path = join_path(&target.path, &action.path);
        match action.kind {
            SyncActionKind::CreateDir => ensure_dir(target.vfs.as_ref(), &target_path).await?,
            SyncActionKind::CopyFile => {
                let source_path = join_path(&source.path, &action.path);
                manager
                    .enqueue_file(
                        format!("{id_prefix}-{index}"),
                        Endpoint::new(source.vfs.clone(), source_path),
                        Endpoint::new(target.vfs.clone(), target_path),
                        action.size,
                    )
                    .await;
            }
            SyncActionKind::DeleteFile => target.vfs.remove_file(&target_path).await?,
            SyncActionKind::DeleteDir => target.vfs.remove_dir_all(&target_path).await?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ft_vfs::LocalFs;

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
        assert_eq!(plan.copy_files, 1);
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
}
