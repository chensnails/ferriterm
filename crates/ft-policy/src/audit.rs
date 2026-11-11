//! Tamper-evident audit trail.
//!
//! Every decision the gate hands out is appended to a monthly JSONL file, and
//! each record carries the hash of the one before it. Editing a line in the
//! middle breaks the chain at that point, which [`AuditLog::verify`] reports —
//! that is the whole point: the file is a log a person can check later without
//! trusting the app that wrote it.
//!
//! Records are written for every verdict, including the ones that were
//! refused, because "what almost ran" is as interesting as "what ran".

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Mode, Reason, Risk, Verdict};

/// How the gate ended up treating one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// Allowed by policy without a person in the loop.
    Auto,
    /// Approved for this run only.
    Once,
    /// Approved for the rest of the window.
    Session,
    /// Approved and written into the rule list.
    Always,
    /// The approval card was dismissed.
    Cancelled,
    /// Policy refused it, so nobody was asked.
    Denied,
}

impl Choice {
    pub fn id(self) -> &'static str {
        match self {
            Choice::Auto => "auto",
            Choice::Once => "once",
            Choice::Session => "session",
            Choice::Always => "always",
            Choice::Cancelled => "cancelled",
            Choice::Denied => "denied",
        }
    }

    pub fn ran(self) -> bool {
        matches!(
            self,
            Choice::Auto | Choice::Once | Choice::Session | Choice::Always
        )
    }
}

/// Where the decision was asked for. The chain is the same for all three; this
/// is what lets a person reading it tell "I ran that" from "an agent ran that".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// The window: the run button, or a command typed at the prompt.
    Ui,
    /// `ferriterm-cli`.
    Cli,
    /// An agent through `ferriterm-cli mcp`.
    Mcp,
}

impl Origin {
    pub fn id(self) -> &'static str {
        match self {
            Origin::Ui => "ui",
            Origin::Cli => "cli",
            Origin::Mcp => "mcp",
        }
    }
}

/// One decision, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// `{timestamp}#{sequence}`, unique within the file.
    pub id: String,
    /// UTC, second precision, `YYYY-MM-DDTHH:MM:SSZ`.
    pub ts: String,
    pub host: String,
    pub command: String,
    pub risk: Risk,
    pub reasons: Vec<Reason>,
    pub rule: Option<String>,
    pub verdict: Verdict,
    pub choice: Choice,
    pub mode: Mode,
    /// Absent in records written before this field existed. It stays out of the
    /// JSON entirely in that case, so replaying the hash of an old record gives
    /// back the same bytes it was hashed from — an old chain still verifies,
    /// while a record with the field removed no longer does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    pub prev_hash: String,
    pub hash: String,
}

/// Everything a record covers, minus the hash itself.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Body<'a> {
    id: &'a str,
    ts: &'a str,
    host: &'a str,
    command: &'a str,
    risk: Risk,
    reasons: &'a [Reason],
    rule: &'a Option<String>,
    verdict: Verdict,
    choice: Choice,
    mode: Mode,
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<Origin>,
}

fn hash_of(prev: &str, body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev.as_bytes());
    hasher.update(b"\n");
    hasher.update(body.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

impl AuditEntry {
    /// Recompute this entry's hash from its own contents. Used on write and on
    /// verification, so the same code guards both directions.
    pub fn compute_hash(&self) -> String {
        let body = Body {
            id: &self.id,
            ts: &self.ts,
            host: &self.host,
            command: &self.command,
            risk: self.risk,
            reasons: &self.reasons,
            rule: &self.rule,
            verdict: self.verdict,
            choice: self.choice,
            mode: self.mode,
            origin: self.origin,
        };
        let encoded = serde_json::to_string(&body).unwrap_or_default();
        hash_of(&self.prev_hash, &encoded)
    }

    /// Whether the stored hash still matches the contents.
    pub fn is_intact(&self) -> bool {
        self.compute_hash() == self.hash
    }
}

/// Seconds since the epoch, formatted as UTC.
pub fn timestamp(secs: u64) -> (String, String) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let iso = format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    );
    (iso, format!("{year:04}{month:02}"))
}

/// Howard Hinnant's days-to-civil algorithm, valid well outside our range.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { y + 1 } else { y }, month, day)
}

/// Result of walking a month's chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub month: String,
    pub entries: usize,
    /// `true` when every record hashes correctly and links to its predecessor.
    pub ok: bool,
    /// 1-based position of the first record that does not verify.
    pub broken_at: Option<usize>,
    /// `hash` or `link` — which check failed.
    pub broken_reason: Option<String>,
}

/// A directory of monthly audit files.
#[derive(Debug, Clone)]
pub struct AuditLog {
    dir: PathBuf,
}

impl AuditLog {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn ensure_dir(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)
    }

    fn file_for(&self, month: &str) -> PathBuf {
        self.dir.join(format!("audit-{month}.jsonl"))
    }

    /// Months with a file, oldest first.
    pub fn months(&self) -> std::io::Result<Vec<String>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut months: Vec<String> = fs::read_dir(&self.dir)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                let rest = name.strip_prefix("audit-")?;
                let month = rest.strip_suffix(".jsonl")?;
                if month.len() == 6 && month.chars().all(|ch| ch.is_ascii_digit()) {
                    Some(month.to_string())
                } else {
                    None
                }
            })
            .collect();
        months.sort();
        Ok(months)
    }

    fn read_lines(&self, month: &str) -> std::io::Result<Vec<String>> {
        let path = self.file_for(month);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(path)?;
        Ok(BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter(|line| !line.trim().is_empty())
            .collect())
    }

    pub fn read_month(&self, month: &str) -> std::io::Result<Vec<AuditEntry>> {
        let mut out = Vec::new();
        for line in self.read_lines(month)? {
            if let Ok(entry) = serde_json::from_str::<AuditEntry>(&line) {
                out.push(entry);
            }
        }
        Ok(out)
    }

    /// Hash of the newest record anywhere in the log, so a new month continues
    /// the chain instead of starting a second, unrelated one.
    fn last_hash(&self) -> std::io::Result<String> {
        for month in self.months()?.iter().rev() {
            let lines = self.read_lines(month)?;
            if let Some(last) = lines.last() {
                if let Ok(entry) = serde_json::from_str::<AuditEntry>(last) {
                    return Ok(entry.hash);
                }
            }
        }
        Ok(String::new())
    }

    /// Append one decision. `entry.hash` and `prev_hash` are filled in here.
    pub fn append(
        &self,
        entry: &mut AuditEntry,
        now: u64,
    ) -> std::io::Result<PathBuf> {
        self.ensure_dir()?;
        let (iso, month) = timestamp(now);
        let previous = self.last_hash()?;
        let month_file = self.file_for(&month);
        let sequence = self.read_lines(&month)?.len() + 1;

        entry.ts = iso;
        entry.id = format!("{}#{sequence}", entry.ts);
        entry.prev_hash = previous;
        entry.hash = entry.compute_hash();

        let line = serde_json::to_string(entry).unwrap_or_default();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&month_file)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(month_file)
    }

    pub fn verify(&self, month: &str) -> std::io::Result<VerifyReport> {
        let lines = self.read_lines(month)?;
        let mut previous = self.previous_of(month)?;
        for (index, line) in lines.iter().enumerate() {
            let position = index + 1;
            let Ok(entry) = serde_json::from_str::<AuditEntry>(line) else {
                return Ok(VerifyReport {
                    month: month.to_string(),
                    entries: lines.len(),
                    ok: false,
                    broken_at: Some(position),
                    broken_reason: Some("parse".into()),
                });
            };
            if !entry.is_intact() {
                return Ok(VerifyReport {
                    month: month.to_string(),
                    entries: lines.len(),
                    ok: false,
                    broken_at: Some(position),
                    broken_reason: Some("hash".into()),
                });
            }
            if entry.prev_hash != previous {
                return Ok(VerifyReport {
                    month: month.to_string(),
                    entries: lines.len(),
                    ok: false,
                    broken_at: Some(position),
                    broken_reason: Some("link".into()),
                });
            }
            previous = entry.hash;
        }
        Ok(VerifyReport {
            month: month.to_string(),
            entries: lines.len(),
            ok: true,
            broken_at: None,
            broken_reason: None,
        })
    }

    /// The hash the first record of `month` must point at: the last record of
    /// the month before it, or nothing for the first month in the log.
    fn previous_of(&self, month: &str) -> std::io::Result<String> {
        let months = self.months()?;
        let Some(position) = months.iter().position(|item| item == month) else {
            return Ok(String::new());
        };
        if position == 0 {
            return Ok(String::new());
        }
        let earlier = &months[position - 1];
        let lines = self.read_lines(earlier)?;
        match lines.last() {
            Some(last) => Ok(serde_json::from_str::<AuditEntry>(last)
                .map(|entry| entry.hash)
                .unwrap_or_default()),
            None => Ok(String::new()),
        }
    }

    /// Delete whole months beyond `keep`, oldest first. Returns what went.
    pub fn prune(&self, keep: usize) -> std::io::Result<Vec<String>> {
        let months = self.months()?;
        if months.len() <= keep {
            return Ok(Vec::new());
        }
        let mut removed = Vec::new();
        for month in &months[..months.len() - keep] {
            let path = self.file_for(month);
            if fs::remove_file(&path).is_ok() {
                removed.push(month.clone());
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("ft-policy-{tag}-{nanos}"))
    }

    fn entry(command: &str, choice: Choice) -> AuditEntry {
        AuditEntry {
            id: String::new(),
            ts: String::new(),
            host: "web-1".into(),
            command: command.into(),
            risk: Risk::R2,
            reasons: vec![Reason::Unknown],
            rule: None,
            verdict: Verdict::Ask,
            choice,
            mode: Mode::Auto,
            origin: None,
            prev_hash: String::new(),
            hash: String::new(),
        }
    }

    #[test]
    fn civil_dates_are_right() {
        // 2024-02-29T00:00:00Z
        let (iso, month) = timestamp(1_709_164_800);
        assert_eq!(iso, "2024-02-29T00:00:00Z");
        assert_eq!(month, "202402");
        // 1970-01-01T00:00:00Z
        assert_eq!(timestamp(0).0, "1970-01-01T00:00:00Z");
        // 2026-11-11T10:00:00Z
        assert_eq!(timestamp(1_794_391_200).0, "2026-11-11T10:00:00Z");
    }

    #[test]
    fn appends_and_verifies() {
        let dir = temp_dir("append");
        let log = AuditLog::new(&dir);
        for (index, command) in ["ls", "rm build.log", "cp a b"].iter().enumerate() {
            let mut item = entry(command, Choice::Auto);
            log.append(&mut item, 1_794_391_200 + index as u64).expect("append");
        }
        let report = log.verify("202611").expect("verify");
        assert!(report.ok);
        assert_eq!(report.entries, 3);
        let entries = log.read_month("202611").expect("read");
        assert_eq!(entries[1].prev_hash, entries[0].hash);
        assert!(entries[1].is_intact());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn editing_a_record_breaks_the_chain() {
        let dir = temp_dir("tamper");
        let log = AuditLog::new(&dir);
        for index in 0..3 {
            let mut item = entry("ls", Choice::Auto);
            log.append(&mut item, 1_794_391_200 + index).expect("append");
        }
        let path = log.file_for("202611");
        let text = fs::read_to_string(&path).expect("read");
        let edited = text.replacen("\"command\":\"ls\"", "\"command\":\"rm -rf /\"", 1);
        fs::write(&path, edited).expect("write");

        let report = log.verify("202611").expect("verify");
        assert!(!report.ok);
        assert_eq!(report.broken_at, Some(1));
        assert_eq!(report.broken_reason.as_deref(), Some("hash"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_a_record_breaks_the_link() {
        let dir = temp_dir("remove");
        let log = AuditLog::new(&dir);
        for index in 0..3 {
            let mut item = entry("ls", Choice::Auto);
            log.append(&mut item, 1_794_391_200 + index).expect("append");
        }
        let path = log.file_for("202611");
        let lines: Vec<String> = fs::read_to_string(&path)
            .expect("read")
            .lines()
            .map(str::to_string)
            .collect();
        fs::write(&path, format!("{}\n{}\n", lines[0], lines[2])).expect("write");

        let report = log.verify("202611").expect("verify");
        assert!(!report.ok);
        assert_eq!(report.broken_at, Some(2));
        assert_eq!(report.broken_reason.as_deref(), Some("link"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_new_month_links_to_the_previous_one() {
        let dir = temp_dir("month");
        let log = AuditLog::new(&dir);
        let mut first = entry("ls", Choice::Auto);
        log.append(&mut first, 1_794_391_200).expect("append"); // 2026-11-11
        let mut second = entry("ls", Choice::Auto);
        // 2026-12-07, the next month
        log.append(&mut second, 1_796_600_000).expect("append");
        let months = log.months().expect("months");
        assert_eq!(months, vec!["202611".to_string(), "202612".to_string()]);
        let later = log.read_month(&months[1]).expect("read");
        assert_eq!(later[0].prev_hash, first.hash);
        assert!(log.verify(&months[1]).expect("verify").ok);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prune_keeps_the_newest_months() {
        let dir = temp_dir("prune");
        let log = AuditLog::new(&dir);
        for index in 0..4 {
            let mut item = entry("ls", Choice::Auto);
            // one month apart
            log.append(&mut item, 1_794_391_200 + index * 30 * 86_400)
                .expect("append");
        }
        let before = log.months().expect("months");
        assert!(before.len() >= 3);
        let removed = log.prune(2).expect("prune");
        assert!(!removed.is_empty());
        assert_eq!(log.months().expect("months").len(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_record_without_an_origin_still_verifies() {
        // What every record written before the field existed looks like. Its
        // hash was taken over a body that never mentioned the origin, so the
        // body must still serialise without one.
        let dir = temp_dir("legacy-origin");
        let log = AuditLog::new(&dir);
        for index in 0..2 {
            let mut item = entry("ls", Choice::Auto);
            log.append(&mut item, 1_794_391_200 + index).expect("append");
        }
        let text = fs::read_to_string(log.file_for("202611")).expect("read");
        assert!(!text.contains("origin"), "an absent origin must not be written");
        assert!(log.verify("202611").expect("verify").ok);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_origin_is_covered_by_the_hash() {
        let dir = temp_dir("origin-tamper");
        let log = AuditLog::new(&dir);
        for index in 0..2 {
            let mut item = entry("ls", Choice::Auto);
            item.origin = Some(Origin::Mcp);
            log.append(&mut item, 1_794_391_200 + index).expect("append");
        }
        let path = log.file_for("202611");
        let text = fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"origin\":\"mcp\""));

        // Rewriting who asked is as much a lie as rewriting what ran.
        fs::write(&path, text.replacen("\"origin\":\"mcp\"", "\"origin\":\"ui\"", 1)).expect("write");
        let report = log.verify("202611").expect("verify");
        assert!(!report.ok);
        assert_eq!(report.broken_at, Some(1));
        assert_eq!(report.broken_reason.as_deref(), Some("hash"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropping_the_origin_field_breaks_the_chain() {
        // The cheap way out: delete the field so the record looks old. The hash
        // covers it, so the record stops verifying instead of passing as legacy.
        let dir = temp_dir("origin-drop");
        let log = AuditLog::new(&dir);
        let mut item = entry("ls", Choice::Auto);
        item.origin = Some(Origin::Cli);
        log.append(&mut item, 1_794_391_200).expect("append");

        let path = log.file_for("202611");
        let text = fs::read_to_string(&path).expect("read");
        fs::write(&path, text.replace(",\"origin\":\"cli\"", "")).expect("write");
        let report = log.verify("202611").expect("verify");
        assert!(!report.ok);
        assert_eq!(report.broken_reason.as_deref(), Some("hash"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn choice_reports_whether_it_ran() {
        assert!(Choice::Auto.ran());
        assert!(Choice::Always.ran());
        assert!(!Choice::Cancelled.ran());
        assert!(!Choice::Denied.ran());
    }
}
