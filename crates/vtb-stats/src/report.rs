use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::{Result, StatsError};
use crate::tokenize::tokenize;
use crate::StatsDb;

/// A single SuperChat record in a session.
#[derive(Debug, Clone, Serialize)]
pub struct ScRecord {
    pub currency: Option<String>,
    pub display: String,
    pub username: String,
    pub price: f64,
    pub text: String,
    pub received_at: DateTime<Utc>,
}

/// A single guard (舰长/提督/总督) purchase record in a session.
#[derive(Debug, Clone, Serialize)]
pub struct GuardRecord {
    pub membership: Option<String>,
    pub display: String,
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
    /// Amounts are never summed across currencies. Legacy totals below are CNY only.
    pub revenue_by_currency: std::collections::BTreeMap<String, f64>,
    pub unpriced_paid_events: u64,
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
        let mut chatters: HashMap<String, (String, u64)> = HashMap::new();
        let mut freq: HashMap<String, u64> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT COALESCE(user_key, CASE WHEN uid != 0 THEN 'uid:' || uid ELSE 'name:' || username END), username, text FROM events
                 WHERE session_id = ?1 AND kind = 'danmaku' ORDER BY id",
            )?;
            let rows = stmt.query_map([session_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
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
            "SELECT COALESCE(SUM(price), 0) FROM events WHERE session_id = ?1 AND kind = 'gift' AND currency = 'CNY'",
            [session_id],
            |r| r.get(0),
        )?;

        // --- SuperChats ---
        let mut superchats = Vec::new();
        let mut sc_total = 0.0f64;
        {
            let mut stmt = self.conn.prepare(
                "SELECT username, price, text, received_at, currency, price_display FROM events
                 WHERE session_id = ?1 AND kind = 'super_chat' ORDER BY received_at, id",
            )?;
            let rows = stmt.query_map([session_id], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    r.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })?;
            for row in rows {
                let (username, price, text, ts, currency, display) = row?;
                if currency.as_deref() == Some("CNY") {
                    sc_total += price;
                }
                superchats.push(ScRecord {
                    display: display.unwrap_or_else(|| format!("¥{price:.2}")),
                    currency,
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
                "SELECT username, level, count, price, received_at, source_json, price_display, currency FROM events
                 WHERE session_id = ?1 AND kind = 'guard_buy' ORDER BY received_at, id",
            )?;
            let rows = stmt.query_map([session_id], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(2)?.unwrap_or(1),
                    r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                    r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            })?;
            for row in rows {
                let (username, level, count, price, ts, source_json, display, currency) = row?;
                let count = count.max(0) as u64;
                guard_count += count;
                if currency.as_deref() == Some("CNY") {
                    guard_total += price;
                }
                let source: Option<vtb_common::EventSource> =
                    source_json.and_then(|s| serde_json::from_str(&s).ok());
                guards.push(GuardRecord {
                    membership: source.as_ref().and_then(|s| s.membership.clone()),
                    display: display.unwrap_or_else(|| {
                        if source.is_some() {
                            "金额未公开".into()
                        } else {
                            format!("¥{price:.2}")
                        }
                    }),
                    username,
                    level: level.clamp(0, u8::MAX as i64) as u8,
                    count,
                    price,
                    received_at: parse_ts(&ts)?,
                });
            }
        }

        let mut stmt = self.conn.prepare("SELECT currency, SUM(price) FROM events WHERE session_id=?1 AND kind IN ('super_chat','gift','guard_buy') AND currency IS NOT NULL AND price IS NOT NULL GROUP BY currency")?;
        let revenue_by_currency = stmt
            .query_map([session_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let unpriced_paid_events: u64 = self.conn.query_row("SELECT COUNT(*) FROM events WHERE session_id=?1 AND kind IN ('super_chat','gift','guard_buy') AND (currency IS NULL OR price IS NULL)", [session_id], |r| r.get(0))?;
        Ok(SessionReport {
            revenue_by_currency,
            unpriced_paid_events,
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
