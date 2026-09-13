//! Markdown / CSV exporters for [`SessionReport`].

use std::fmt::Write as _;

use crate::report::SessionReport;

/// Escape a value for use inside a Markdown table cell.
fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn guard_level_name(level: u8, locale: &str) -> &'static str {
    match level {
        1 => label(locale, "总督", "Governor", "総督"),
        2 => label(locale, "提督", "Admiral", "提督"),
        3 => label(locale, "舰长", "Captain", "艦長"),
        _ => label(locale, "未知", "Unknown", "不明"),
    }
}

/// Render a session report as a Markdown document.
pub fn export_markdown(r: &SessionReport) -> String {
    export_markdown_localized(r, "zh-CN")
}

fn label<'a>(locale: &str, zh: &'a str, en: &'a str, ja: &'a str) -> &'a str {
    match locale {
        "en" => en,
        "ja" => ja,
        _ => zh,
    }
}

/// Localize report labels while retaining viewer text, amounts and identifiers.
pub fn export_markdown_localized(r: &SessionReport, locale: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{}{}{}",
        label(
            locale,
            "# 场次统计报告:",
            "# Session report: ",
            "# 配信統計レポート："
        ),
        md_cell(&r.session_id),
        label(locale, "\n", "\n", "\n")
    );

    let _ = writeln!(
        s,
        "{}",
        label(locale, "## 汇总\n", "## Summary\n", "## 集計\n")
    );
    let _ = writeln!(
        s,
        "{}",
        label(
            locale,
            "| 指标 | 数值 |",
            "| Metric | Value |",
            "| 項目 | 値 |"
        )
    );
    let _ = writeln!(s, "| --- | --- |");
    let _ = writeln!(
        s,
        "{}{}{}",
        label(locale, "| 弹幕数 | ", "| Messages | ", "| チャット数 | "),
        r.danmaku_count,
        label(locale, " |", " |", " |")
    );
    let _ = writeln!(
        s,
        "{}{}{}",
        label(
            locale,
            "| 独立发言人数 | ",
            "| Unique viewers | ",
            "| 発言者数 | "
        ),
        r.unique_chatters,
        label(locale, " |", " |", " |")
    );
    let _ = writeln!(
        s,
        "{}{}{}",
        label(locale, "| 进场次数 | ", "| Entries | ", "| 入室数 | "),
        r.enter_count,
        label(locale, " |", " |", " |")
    );
    let _ = writeln!(
        s,
        "{}{:.2}{}",
        label(
            locale,
            "| SC 总额 | ¥",
            "| Bilibili Super Chat (CNY) | ¥",
            "| Bilibili SC 合計（CNY） | ¥"
        ),
        r.sc_total,
        label(locale, " |", " |", " |")
    );
    let _ = writeln!(
        s,
        "{}{:.2}{}",
        label(
            locale,
            "| 礼物总额 | ¥",
            "| Bilibili gifts (CNY) | ¥",
            "| Bilibili ギフト合計（CNY） | ¥"
        ),
        r.gift_total,
        label(locale, " |", " |", " |")
    );
    let _ = writeln!(
        s,
        "{}{}{}",
        label(
            locale,
            "| 舰长数 | ",
            "| Membership count | ",
            "| メンバー加入数 | "
        ),
        r.guard_count,
        label(locale, " |", " |", " |")
    );
    let _ = writeln!(
        s,
        "{}{:.2}{}",
        label(
            locale,
            "| 总营收 | ¥",
            "| Revenue (CNY) | ¥",
            "| 収益（CNY） | ¥"
        ),
        r.revenue_total,
        label(locale, " |", " |", " |")
    );
    for (currency, amount) in &r.revenue_by_currency {
        if currency != "CNY" {
            let _ = writeln!(
                s,
                "{}{currency}{}{amount:.2}{}",
                label(locale, "| 营收 (", "| Revenue (", "| 収益 ("),
                label(locale, ") | ", ") | ", ") | "),
                label(locale, " |", " |", " |")
            );
        }
    }
    if r.unpriced_paid_events > 0 {
        let _ = writeln!(
            s,
            "{}{}{}",
            label(
                locale,
                "| 币种不明或金额未公开的事件 | ",
                "| Undisclosed currency or amount | ",
                "| 通貨・金額が不明のイベント | "
            ),
            r.unpriced_paid_events,
            label(
                locale,
                "（未折算） |",
                " (not converted) |",
                "（換算なし） |"
            )
        );
    }

    let _ = writeln!(
        s,
        "{}",
        label(
            locale,
            "\n## SuperChat 记录\n",
            "\n## Super Chat history\n",
            "\n## Super Chat 履歴\n"
        )
    );
    if r.superchats.is_empty() {
        let _ = writeln!(s, "{}", label(locale, "(无)", "(None)", "（なし）"));
    } else {
        let _ = writeln!(
            s,
            "{}",
            label(
                locale,
                "| 时间 | 用户 | 金额 | 内容 |",
                "| Time (UTC) | Viewer | Amount | Message |",
                "| 時刻（UTC） | 視聴者 | 金額 | 内容 |"
            )
        );
        let _ = writeln!(s, "| --- | --- | --- | --- |");
        for sc in &r.superchats {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} |",
                sc.received_at.format("%Y-%m-%d %H:%M:%S"),
                md_cell(&sc.username),
                md_cell(&sc.display),
                md_cell(&sc.text),
            );
        }
    }

    let _ = writeln!(
        s,
        "{}",
        label(
            locale,
            "\n## 舰长记录\n",
            "\n## Membership history\n",
            "\n## メンバー加入履歴\n"
        )
    );
    if r.guards.is_empty() {
        let _ = writeln!(s, "{}", label(locale, "(无)", "(None)", "（なし）"));
    } else {
        let _ = writeln!(
            s,
            "{}",
            label(
                locale,
                "| 时间 | 用户 | 等级 | 数量 | 金额 |",
                "| Time (UTC) | Viewer | Tier | Count | Amount |",
                "| 時刻（UTC） | 視聴者 | レベル | 数量 | 金額 |"
            )
        );
        let _ = writeln!(s, "| --- | --- | --- | --- | --- |");
        for g in &r.guards {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} |",
                g.received_at.format("%Y-%m-%d %H:%M:%S"),
                md_cell(&g.username),
                md_cell(
                    g.membership
                        .as_deref()
                        .unwrap_or(guard_level_name(g.level, locale))
                ),
                g.count,
                md_cell(&g.display),
            );
        }
    }

    let _ = writeln!(
        s,
        "{}",
        label(
            locale,
            "\n## TOP 弹幕用户\n",
            "\n## Top chatters\n",
            "\n## 発言数ランキング\n"
        )
    );
    if r.top_chatters.is_empty() {
        let _ = writeln!(s, "{}", label(locale, "(无)", "(None)", "（なし）"));
    } else {
        let _ = writeln!(
            s,
            "{}",
            label(
                locale,
                "| 用户 | 弹幕数 |",
                "| Viewer | Messages |",
                "| 視聴者 | チャット数 |"
            )
        );
        let _ = writeln!(s, "| --- | --- |");
        for (name, n) in &r.top_chatters {
            let _ = writeln!(s, "| {} | {} |", md_cell(name), n);
        }
    }

    s
}

/// Escape a single CSV field (RFC 4180 style).
fn csv_field(f: &str) -> String {
    if f.contains(',') || f.contains('"') || f.contains('\n') || f.contains('\r') {
        format!("\"{}\"", f.replace('"', "\"\""))
    } else {
        f.to_string()
    }
}

/// Export the SuperChat list as CSV (`username,price,text,received_at`).
pub fn export_superchats_csv(r: &SessionReport) -> String {
    let extended = r
        .superchats
        .iter()
        .any(|s| s.currency.as_deref() != Some("CNY"));
    let mut s = String::from(if extended {
        "username,price,text,received_at,currency,display\n"
    } else {
        "username,price,text,received_at\n"
    });
    for sc in &r.superchats {
        let _ = write!(
            s,
            "{},{:.2},{},{}",
            csv_field(&sc.username),
            sc.price,
            csv_field(&sc.text),
            csv_field(&sc.received_at.to_rfc3339()),
        );
        if extended {
            let _ = write!(
                s,
                ",{},{}",
                csv_field(sc.currency.as_deref().unwrap_or("unknown")),
                csv_field(&sc.display)
            );
        }
        s.push('\n');
    }
    s
}
