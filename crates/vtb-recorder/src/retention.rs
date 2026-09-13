//! Recording retention: rolling cleanup of old recordings so unattended
//! capture doesn't fill the disk (对标 blrec 的"空间不足自动删除旧录播").
//!
//! Deletes whole session directories oldest-first until either free space
//! recovers above the target or the age/count limits are satisfied. The
//! active session is never touched.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Delete sessions older than this. `None` disables age-based cleanup.
    pub max_age: Option<Duration>,
    /// Keep at most this many sessions per room. `None` disables it.
    pub max_sessions_per_room: Option<usize>,
    /// When free space is below this, delete oldest sessions until it
    /// recovers. `None` disables space-based cleanup.
    pub target_free_bytes: Option<u64>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age: None,
            max_sessions_per_room: None,
            target_free_bytes: None,
        }
    }
}

impl RetentionPolicy {
    pub fn is_noop(&self) -> bool {
        self.max_age.is_none()
            && self.max_sessions_per_room.is_none()
            && self.target_free_bytes.is_none()
    }
}

/// A recorded session directory: `<root>/room{id}/{stamp}`.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionDir {
    pub path: PathBuf,
    pub room_id: u64,
    pub modified: SystemTime,
    pub bytes: u64,
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            match e.metadata() {
                Ok(m) if m.is_dir() => total += dir_size(&e.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

/// Discover session directories under `root` (layout `room{id}/{stamp}`).
pub fn scan_sessions(root: &Path) -> Vec<SessionDir> {
    let mut out = vec![];
    let Ok(rooms) = std::fs::read_dir(root) else {
        return out;
    };
    for room in rooms.flatten() {
        if !room.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = room.file_name();
        let room_id = name
            .to_str()
            .and_then(|n| n.strip_prefix("room"))
            .and_then(|n| n.parse::<u64>().ok());
        let Some(room_id) = room_id else { continue };
        let Ok(sessions) = std::fs::read_dir(room.path()) else {
            continue;
        };
        for s in sessions.flatten() {
            if !s.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let path = s.path();
            let modified = s
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            out.push(SessionDir {
                bytes: dir_size(&path),
                path,
                room_id,
                modified,
            });
        }
    }
    out
}

/// Result of a cleanup pass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CleanupReport {
    pub deleted: Vec<PathBuf>,
    pub freed_bytes: u64,
}

/// Decide which sessions to delete under `policy`, protecting `keep`
/// (the active session). Pure planning — no filesystem mutation — so it is
/// deterministic and testable. `free_now` is the current free bytes;
/// `now` the reference time.
pub fn plan_cleanup(
    sessions: &[SessionDir],
    policy: &RetentionPolicy,
    keep: Option<&Path>,
    free_now: u64,
    now: SystemTime,
) -> Vec<PathBuf> {
    let mut victims: Vec<&SessionDir> = vec![];
    fn is_victim(s: &SessionDir, list: &[&SessionDir]) -> bool {
        list.iter().any(|v| v.path == s.path)
    }

    // Oldest first for space/age; per-room needs grouping.
    let mut by_age: Vec<&SessionDir> = sessions
        .iter()
        .filter(|s| keep.map(|k| k != s.path).unwrap_or(true))
        .collect();
    by_age.sort_by_key(|s| s.modified);

    // 1. Age-based.
    if let Some(max_age) = policy.max_age {
        for s in &by_age {
            if now
                .duration_since(s.modified)
                .map(|age| age > max_age)
                .unwrap_or(false)
                && !is_victim(s, &victims)
            {
                victims.push(s);
            }
        }
    }

    // 2. Per-room count: keep the N newest per room.
    if let Some(max_n) = policy.max_sessions_per_room {
        let mut rooms: std::collections::HashMap<u64, Vec<&SessionDir>> =
            std::collections::HashMap::new();
        for s in &by_age {
            rooms.entry(s.room_id).or_default().push(s);
        }
        for (_, mut list) in rooms {
            // Newest first; anything past max_n is a victim.
            list.sort_by_key(|s| std::cmp::Reverse(s.modified));
            for s in list.into_iter().skip(max_n) {
                if !is_victim(s, &victims) {
                    victims.push(s);
                }
            }
        }
    }

    // 3. Space-based: delete oldest until projected free ≥ target.
    if let Some(target) = policy.target_free_bytes {
        if free_now < target {
            let mut projected = free_now + victims.iter().map(|v| v.bytes).sum::<u64>();
            for s in &by_age {
                if projected >= target {
                    break;
                }
                if !is_victim(s, &victims) {
                    projected += s.bytes;
                    victims.push(s);
                }
            }
        }
    }

    // Return oldest-first for a tidy deletion order.
    victims.sort_by_key(|s| s.modified);
    victims.iter().map(|s| s.path.clone()).collect()
}

/// Run a cleanup pass: scan `root`, plan under `policy`, and delete the
/// planned session directories. `keep` (active session) is protected.
pub fn run_cleanup(
    root: &Path,
    policy: &RetentionPolicy,
    keep: Option<&Path>,
    free_space_fn: impl Fn(&Path) -> std::io::Result<u64>,
) -> CleanupReport {
    if policy.is_noop() {
        return CleanupReport::default();
    }
    let sessions = scan_sessions(root);
    let free_now = free_space_fn(root).unwrap_or(u64::MAX);
    let plan = plan_cleanup(&sessions, policy, keep, free_now, SystemTime::now());

    let sizes: std::collections::HashMap<&Path, u64> = sessions
        .iter()
        .map(|s| (s.path.as_path(), s.bytes))
        .collect();
    let mut report = CleanupReport::default();
    for path in plan {
        let bytes = sizes.get(path.as_path()).copied().unwrap_or(0);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => {
                report.freed_bytes += bytes;
                report.deleted.push(path);
            }
            Err(e) => tracing::warn!("retention: failed to delete {}: {e}", path.display()),
        }
    }
    report
}

/// Cleanup for one platform source; only our manifested sessions are eligible.
pub fn run_source_cleanup(root: &Path, policy: &RetentionPolicy, keep: &Path) -> CleanupReport {
    if policy.is_noop() {
        return CleanupReport::default();
    }
    let Ok(canonical_root) = root.canonicalize() else {
        return CleanupReport::default();
    };
    let sessions: Vec<SessionDir> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && e.path().join("recording.meta.json").is_file()
        })
        .map(|e| SessionDir {
            path: e.path(),
            room_id: 0,
            modified: e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH),
            bytes: dir_size(&e.path()),
        })
        .collect();
    let victims = plan_cleanup(
        &sessions,
        policy,
        Some(keep),
        crate::disk::free_space(root).unwrap_or(u64::MAX),
        SystemTime::now(),
    );
    let mut report = CleanupReport::default();
    for path in victims {
        let Ok(resolved) = path.canonicalize() else {
            continue;
        };
        if resolved == canonical_root || !resolved.starts_with(&canonical_root) {
            continue;
        }
        let bytes = dir_size(&path);
        if std::fs::remove_dir_all(&path).is_ok() {
            report.freed_bytes += bytes;
            report.deleted.push(path);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(root: &Path, room: u64, stamp: &str, bytes: u64, age_secs: u64) -> SessionDir {
        let path = root.join(format!("room{room}")).join(stamp);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("rec.flv"), vec![0u8; bytes as usize]).unwrap();
        SessionDir {
            path,
            room_id: room,
            modified: SystemTime::now() - Duration::from_secs(age_secs),
            bytes,
        }
    }

    #[test]
    fn scan_finds_sessions_with_room_and_size() {
        let dir = tempfile::tempdir().unwrap();
        session(dir.path(), 320, "20260718-100000", 100, 0);
        session(dir.path(), 320, "20260718-110000", 200, 0);
        session(dir.path(), 999, "20260718-120000", 50, 0);
        // Non-room dirs ignored.
        std::fs::create_dir_all(dir.path().join("notaroom")).unwrap();

        let mut found = scan_sessions(dir.path());
        found.sort_by_key(|s| s.bytes);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].bytes, 50);
        assert_eq!(found[0].room_id, 999);
        assert_eq!(found[2].bytes, 200);
    }

    #[test]
    fn noop_policy_deletes_nothing() {
        let policy = RetentionPolicy::default();
        assert!(policy.is_noop());
        let plan = plan_cleanup(&[], &policy, None, 0, SystemTime::now());
        assert!(plan.is_empty());
    }

    #[test]
    fn age_policy_deletes_old_only() {
        let dir = tempfile::tempdir().unwrap();
        let old = session(dir.path(), 1, "old", 10, 3 * 86400);
        let fresh = session(dir.path(), 1, "fresh", 10, 60);
        let sessions = vec![old.clone(), fresh.clone()];
        let policy = RetentionPolicy {
            max_age: Some(Duration::from_secs(86400)),
            ..Default::default()
        };
        let plan = plan_cleanup(&sessions, &policy, None, u64::MAX, SystemTime::now());
        assert_eq!(plan, vec![old.path]);
    }

    #[test]
    fn per_room_keeps_newest_n() {
        let dir = tempfile::tempdir().unwrap();
        let s1 = session(dir.path(), 7, "s1", 10, 300);
        let s2 = session(dir.path(), 7, "s2", 10, 200);
        let s3 = session(dir.path(), 7, "s3", 10, 100);
        let other = session(dir.path(), 8, "o1", 10, 500);
        let sessions = vec![s1.clone(), s2.clone(), s3.clone(), other.clone()];
        let policy = RetentionPolicy {
            max_sessions_per_room: Some(2),
            ..Default::default()
        };
        let plan = plan_cleanup(&sessions, &policy, None, u64::MAX, SystemTime::now());
        // room 7 keeps s3,s2 (newest); s1 deleted. room 8 keeps its only one.
        assert_eq!(plan, vec![s1.path]);
    }

    #[test]
    fn space_policy_deletes_oldest_until_target_met() {
        let dir = tempfile::tempdir().unwrap();
        let s1 = session(dir.path(), 1, "s1", 100, 300); // oldest
        let s2 = session(dir.path(), 1, "s2", 100, 200);
        let s3 = session(dir.path(), 1, "s3", 100, 100); // newest
        let sessions = vec![s1.clone(), s2.clone(), s3.clone()];
        let policy = RetentionPolicy {
            target_free_bytes: Some(150),
            ..Default::default()
        };
        // free=0, need 150 → delete s1(+100=100), s2(+100=200≥150) stop.
        let plan = plan_cleanup(&sessions, &policy, None, 0, SystemTime::now());
        assert_eq!(plan, vec![s1.path, s2.path]);
    }

    #[test]
    fn active_session_is_never_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let active = session(dir.path(), 1, "active", 100, 3 * 86400);
        let sessions = vec![active.clone()];
        let policy = RetentionPolicy {
            max_age: Some(Duration::from_secs(60)),
            target_free_bytes: Some(u64::MAX),
            ..Default::default()
        };
        let plan = plan_cleanup(&sessions, &policy, Some(&active.path), 0, SystemTime::now());
        assert!(plan.is_empty(), "active session must be protected");
    }

    #[test]
    fn run_cleanup_actually_deletes_dirs() {
        // run_cleanup re-scans real on-disk mtimes, so drive it with a
        // space policy (age-independent) and keep the newest via `keep`.
        let dir = tempfile::tempdir().unwrap();
        let a = session(dir.path(), 1, "a", 100, 0);
        let b = session(dir.path(), 1, "b", 100, 0);
        // free=0, target=150 → must delete until +150 freed: both (each 100).
        let policy = RetentionPolicy {
            target_free_bytes: Some(150),
            ..Default::default()
        };
        let report = run_cleanup(dir.path(), &policy, Some(&b.path), |_| Ok(0));
        assert_eq!(report.deleted, vec![a.path.clone()]);
        assert_eq!(report.freed_bytes, 100);
        assert!(!a.path.exists());
        assert!(b.path.exists(), "protected active session kept");
    }
}
