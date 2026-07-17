use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use vtb_common::LiveEvent;

use crate::error::Result;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS events (
    id          INTEGER PRIMARY KEY,
    session_id  TEXT NOT NULL,
    room_id     INTEGER,
    received_at TEXT,
    kind        TEXT,
    uid         INTEGER,
    username    TEXT,
    text        TEXT,
    price       REAL,
    count       INTEGER,
    level       INTEGER
);
CREATE INDEX IF NOT EXISTS idx_events_session_kind ON events(session_id, kind);
";

/// SQLite-backed event log for live-session statistics.
pub struct StatsDb {
    pub(crate) conn: Connection,
}

/// One row's worth of normalized data extracted from a [`LiveEvent`].
struct EventRow<'a> {
    room_id: u64,
    kind: &'static str,
    uid: u64,
    username: &'a str,
    text: Option<&'a str>,
    price: Option<f64>,
    count: Option<i64>,
    level: Option<i64>,
}

impl StatsDb {
    /// Open (creating if needed) a stats database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Self::init(Connection::open(path)?)
    }

    /// Open an in-memory database (useful for tests).
    pub fn open_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Record a live event under `session_id`.
    ///
    /// `WatchedChange` / `LiveStart` / `LiveEnd` are ignored.
    pub fn record(
        &self,
        session_id: &str,
        ev: &LiveEvent,
        received_at: DateTime<Utc>,
    ) -> Result<()> {
        let row = match ev {
            LiveEvent::Danmaku(d) => EventRow {
                room_id: d.room_id,
                kind: "danmaku",
                uid: d.uid,
                username: &d.username,
                text: Some(&d.text),
                price: None,
                count: None,
                level: None,
            },
            LiveEvent::SuperChat(s) => EventRow {
                room_id: s.room_id,
                kind: "super_chat",
                uid: s.uid,
                username: &s.username,
                text: Some(&s.text),
                price: Some(s.price),
                count: None,
                level: None,
            },
            LiveEvent::Gift(g) => EventRow {
                room_id: g.room_id,
                kind: "gift",
                uid: g.uid,
                username: &g.username,
                text: Some(&g.gift_name),
                price: Some(g.total_price),
                count: Some(g.count as i64),
                level: None,
            },
            LiveEvent::GuardBuy(g) => EventRow {
                room_id: g.room_id,
                kind: "guard_buy",
                uid: g.uid,
                username: &g.username,
                text: None,
                price: Some(g.price),
                count: Some(g.count as i64),
                level: Some(g.guard_level as i64),
            },
            LiveEvent::Enter(e) => EventRow {
                room_id: e.room_id,
                kind: "enter",
                uid: e.uid,
                username: &e.username,
                text: None,
                price: None,
                count: None,
                level: None,
            },
            LiveEvent::Like(l) => EventRow {
                room_id: l.room_id,
                kind: "like",
                uid: l.uid,
                username: &l.username,
                text: None,
                price: None,
                count: None,
                level: None,
            },
            LiveEvent::WatchedChange { .. }
            | LiveEvent::LiveStart { .. }
            | LiveEvent::LiveEnd { .. } => return Ok(()),
        };

        self.conn.execute(
            "INSERT INTO events (session_id, room_id, received_at, kind, uid, username, text, price, count, level)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                session_id,
                row.room_id as i64,
                received_at.to_rfc3339(),
                row.kind,
                row.uid as i64,
                row.username,
                row.text,
                row.price,
                row.count,
                row.level,
            ],
        )?;
        Ok(())
    }
}
