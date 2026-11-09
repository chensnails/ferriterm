-- Transfer queue, so a job that was still running when the app closed can be
-- offered back instead of vanishing. The payload is the job as the UI saw it.
CREATE TABLE IF NOT EXISTS transfer_jobs (
    id TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
