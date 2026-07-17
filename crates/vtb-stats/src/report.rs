use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::{Result, StatsError};
use crate::tokenize::tokenize;
use crate::StatsDb;

/// A single SuperChat record in a session.
#[derive(Debug, Clone, Serialize)]
pub struct ScRecord {
    pub username: String,
    pub price: f64,
    pub text: String,
    pub received_at: DateTime<Utc>,
}

/// A single guard (舰长/提督/总督) purchase record in a session.
#[derive(Debug, Clone, Serialize)]
pub struct GuardRecord {
    pub username: String,
    /// 1 总督, 2 提督, 3 舰长.
    pub level: u8,
    pub count: u64,
    pub price: f64,
    pub received_at: DateTime<Utc>,
}

/// Aggregated statistics for one live session.
#[derive(Debug, Clone, Serialize)]
pub struct SessionReport {
    pub session_id: String,
    pub danmaku_count: u64,
    pub unique_chatters: u64,
    pub enter_count: u64,
    /// Sum of SuperChat prices (CNY).
    pub sc_total: f64,
    /// Sum of gift total prices (CNY).
    pub gift_total: f64,
    /// Total guard units purchased (sum of `count`).
    pub guard_count: u64,
    /// sc_total + gift_total + guard purchase prices.
    pub revenue_total: f64,
    /// Top 10 chatters by danmaku count: (username, count).
    pub top_chatters: Vec<(String, u64)>,
    pub superchats: Vec<ScRecord>,
    pub guards: Vec<GuardRecord>,
    /// Top 50 words by frequency across danmaku text: (word, count).
    pub word_freq: Vec<(String, u64)>,
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| StatsError::Timestamp(format!("{s}: {e}")))
}

impl StatsDb {
    /// Build the aggregated report for `session_id`.
    pub fn session_report(&self, session_id: &str) -> Result<SessionReport> {
        // --- Danmaku pass: count, unique chatters, top chatters, word freq ---
        let mut danmaku_count = 0u64;
        let mut chatters: HashMap<i64, (String, u64)> = HashMap::new();
        let mut freq: HashMap<String, u64> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT uid, username, text FROM events
                 WHERE session_id = ?1 AND kind = 'danmaku' ORDER BY id",
            )?;
            let rows = stmt.query_map([session_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                ))
            })?;
            for row in rows {
                let (uid, username, text) = row?;
                danmaku_count += 1;
                let entry = chatters.entry(uid).or_insert_with(|| (username.clone(), 0));
                entry.0 = username; // keep the latest seen username
                entry.1 += 1;
                for tok in tokenize(&text) {
                    *freq.entry(tok).or_insert(0) += 1;
                }
            }
        }
        let unique_chatters = chatters.len() as u64;

        let mut top_chatters: Vec<(String, u64)> = chatters.into_values().collect();
        top_chatters.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top_chatters.truncate(10);

        let mut word_freq: Vec<(String, u64)> = freq.into_iter().collect();
        word_freq.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        word_freq.truncate(50);

        // --- Simple aggregates ---
        let enter_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE session_id = ?1 AND kind = 'enter'",
            [session_id],
            |r| r.get(0),
        )?;
        let gift_total: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(price), 0) FROM events WHERE session_id = ?1 AND kind = 'gift'",
            [session_id],
            |r| r.get(0),
        )?;

        // --- SuperChats ---
        let mut superchats = Vec::new();
        let mut sc_total = 0.0f64;
        {
            let mut stmt = self.conn.prepare(
                "SELECT username, price, text, received_at FROM events
                 WHERE session_id = ?1 AND kind = 'super_chat' ORDER BY received_at, id",
            )?;
            let rows = stmt.query_map([session_id], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    r.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            })?;
            for row in rows {
                let (username, price, text, ts) = row?;
                sc_total += price;
                superchats.push(ScRecord {
                    username,
                    price,
                    text,
                    received_at: parse_ts(&ts)?,
                });
            }
        }

        // --- Guards ---
        let mut guards = Vec::new();
        let mut guard_count = 0u64;
        let mut guard_total = 0.0f64;
        {
            let mut stmt = self.conn.prepare(
                "SELECT username, level, count, price, received_at FROM events
                 WHERE session_id = ?1 AND kind = 'guard_buy' ORDER BY received_at, id",
            )?;
            let rows = stmt.query_map([session_id], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(2)?.unwrap_or(1),
                    r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                    r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                ))
            })?;
            for row in rows {
                let (username, level, count, price, ts) = row?;
                let count = count.max(0) as u64;
                guard_count += count;
                guard_total += price;
                guards.push(GuardRecord {
                    username,
                    level: level.clamp(0, u8::MAX as i64) as u8,
                    count,
                    price,
                    received_at: parse_ts(&ts)?,
                });
            }
        }

        Ok(SessionReport {
            session_id: session_id.to_string(),
            danmaku_count,
            unique_chatters,
            enter_count: enter_count.max(0) as u64,
            sc_total,
            gift_total,
            guard_count,
            revenue_total: sc_total + gift_total + guard_total,
            top_chatters,
            superchats,
            guards,
            word_freq,
        })
    }
}
