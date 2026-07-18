//! 粉丝画像: per-user notes (备注) and cross-session viewer profiles
//! (LAPLACE-style "认出老观众") computed from the events log.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use crate::db::StatsDb;
use crate::error::Result;

const NOTES_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS user_notes (
    key        TEXT PRIMARY KEY,
    uid        INTEGER,
    username   TEXT,
    note       TEXT NOT NULL,
    updated_at TEXT
);
";

/// Stable note key: real uid when known, otherwise the (masked) username —
/// anonymous danmaku connections report uid=0 for everyone.
pub fn note_key(uid: u64, username: &str) -> String {
    if uid != 0 {
        format!("uid:{uid}")
    } else {
        format!("name:{username}")
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct UserNote {
    pub uid: u64,
    pub username: String,
    pub note: String,
    pub updated_at: String,
}

/// Aggregated viewer history across every recorded session.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct UserProfile {
    pub uid: u64,
    pub username: String,
    pub danmaku_count: u64,
    pub sc_total: f64,
    pub gift_total: f64,
    pub guard_count: u64,
    pub sessions: u64,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub note: Option<String>,
}

impl StatsDb {
    fn ensure_notes_schema(&self) -> Result<()> {
        self.conn.execute_batch(NOTES_SCHEMA)?;
        Ok(())
    }

    /// Set (or clear, with an empty note) a user's 备注.
    pub fn set_note(
        &self,
        uid: u64,
        username: &str,
        note: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.ensure_notes_schema()?;
        let key = note_key(uid, username);
        if note.trim().is_empty() {
            self.conn
                .execute("DELETE FROM user_notes WHERE key = ?1", params![key])?;
        } else {
            self.conn.execute(
                "INSERT INTO user_notes (key, uid, username, note, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(key) DO UPDATE SET
                   username = excluded.username,
                   note = excluded.note,
                   updated_at = excluded.updated_at",
                params![key, uid as i64, username, note.trim(), now.to_rfc3339()],
            )?;
        }
        Ok(())
    }

    /// All notes (for hydrating the UI's tag map).
    pub fn list_notes(&self) -> Result<Vec<UserNote>> {
        self.ensure_notes_schema()?;
        let mut stmt = self.conn.prepare(
            "SELECT uid, username, note, updated_at FROM user_notes ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(UserNote {
                uid: r.get::<_, i64>(0)? as u64,
                username: r.get(1)?,
                note: r.get(2)?,
                updated_at: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Viewer history by uid (uid != 0) or username (anonymous data).
    pub fn user_profile(&self, uid: u64, username: &str) -> Result<UserProfile> {
        self.ensure_notes_schema()?;
        let (filter, arg): (&str, String) = if uid != 0 {
            ("uid = ?1", uid.to_string())
        } else {
            ("username = ?1", username.to_string())
        };
        let sql = format!(
            "SELECT
               COUNT(CASE WHEN kind = 'danmaku' THEN 1 END),
               COALESCE(SUM(CASE WHEN kind = 'super_chat' THEN price END), 0),
               COALESCE(SUM(CASE WHEN kind = 'gift' THEN price END), 0),
               COUNT(CASE WHEN kind = 'guard_buy' THEN 1 END),
               COUNT(DISTINCT session_id),
               MIN(received_at),
               MAX(received_at)
             FROM events WHERE {filter}"
        );
        let profile = self.conn.query_row(&sql, params![arg], |r| {
            Ok(UserProfile {
                uid,
                username: username.to_string(),
                danmaku_count: r.get::<_, i64>(0)? as u64,
                sc_total: r.get(1)?,
                gift_total: r.get(2)?,
                guard_count: r.get::<_, i64>(3)? as u64,
                sessions: r.get::<_, i64>(4)? as u64,
                first_seen: r.get(5)?,
                last_seen: r.get(6)?,
                note: None,
            })
        })?;
        let note = self
            .conn
            .query_row(
                "SELECT note FROM user_notes WHERE key = ?1",
                params![note_key(uid, username)],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        Ok(UserProfile { note, ..profile })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtb_common::{DanmakuMsg, GiftMsg, LiveEvent, SuperChatMsg};

    fn danmaku(uid: u64, name: &str, text: &str) -> LiveEvent {
        LiveEvent::Danmaku(DanmakuMsg {
            room_id: 1,
            uid,
            username: name.into(),
            text: text.into(),
            timestamp: Utc::now(),
            medal: None,
            guard_level: 0,
            is_admin: false,
            emoticon: None,
        })
    }

    #[test]
    fn note_roundtrip_update_and_clear() {
        let db = StatsDb::open_memory().unwrap();
        let now = Utc::now();
        db.set_note(42, "老观众", "三年舰长", now).unwrap();
        db.set_note(42, "老观众改名", "三年舰长，改过名", now).unwrap();
        let notes = db.list_notes().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].username, "老观众改名");
        assert_eq!(notes[0].note, "三年舰长，改过名");

        db.set_note(42, "老观众改名", "  ", now).unwrap();
        assert!(db.list_notes().unwrap().is_empty());
    }

    #[test]
    fn anonymous_notes_key_by_username() {
        let db = StatsDb::open_memory().unwrap();
        let now = Utc::now();
        db.set_note(0, "星***", "疑似老观众", now).unwrap();
        db.set_note(0, "另一位", "别的备注", now).unwrap();
        assert_eq!(db.list_notes().unwrap().len(), 2);
        let p = db.user_profile(0, "星***").unwrap();
        assert_eq!(p.note.as_deref(), Some("疑似老观众"));
    }

    #[test]
    fn profile_aggregates_across_sessions() {
        let db = StatsDb::open_memory().unwrap();
        let now = Utc::now();
        db.record("s1", &danmaku(7, "常客", "前排"), now).unwrap();
        db.record("s1", &danmaku(7, "常客", "哈哈哈"), now).unwrap();
        db.record("s2", &danmaku(7, "常客", "又来了"), now).unwrap();
        db.record(
            "s2",
            &LiveEvent::SuperChat(SuperChatMsg {
                room_id: 1,
                uid: 7,
                username: "常客".into(),
                text: "加油".into(),
                price: 30.0,
                timestamp: now,
                duration_secs: 60,
            }),
            now,
        )
        .unwrap();
        db.record(
            "s2",
            &LiveEvent::Gift(GiftMsg {
                room_id: 1,
                uid: 7,
                username: "常客".into(),
                gift_name: "小花花".into(),
                gift_id: 1,
                count: 10,
                total_price: 5.0,
                timestamp: now,
            }),
            now,
        )
        .unwrap();
        // Someone else's noise must not leak in.
        db.record("s2", &danmaku(8, "路人", "打扰了"), now).unwrap();

        db.set_note(7, "常客", "三连老哥", now).unwrap();
        let p = db.user_profile(7, "常客").unwrap();
        assert_eq!(p.danmaku_count, 3);
        assert_eq!(p.sc_total, 30.0);
        assert_eq!(p.gift_total, 5.0);
        assert_eq!(p.sessions, 2);
        assert_eq!(p.note.as_deref(), Some("三连老哥"));
        assert!(p.first_seen.is_some());
    }
}
