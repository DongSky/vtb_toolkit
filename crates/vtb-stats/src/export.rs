//! Markdown / CSV exporters for [`SessionReport`].

use std::fmt::Write as _;

use crate::report::SessionReport;

/// Escape a value for use inside a Markdown table cell.
fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn guard_level_name(level: u8) -> &'static str {
    match level {
        1 => "总督",
        2 => "提督",
        3 => "舰长",
        _ => "未知",
    }
}

/// Render a session report as a Markdown document.
pub fn export_markdown(r: &SessionReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# 场次统计报告:{}\n", md_cell(&r.session_id));

    let _ = writeln!(s, "## 汇总\n");
    let _ = writeln!(s, "| 指标 | 数值 |");
    let _ = writeln!(s, "| --- | --- |");
    let _ = writeln!(s, "| 弹幕数 | {} |", r.danmaku_count);
    let _ = writeln!(s, "| 独立发言人数 | {} |", r.unique_chatters);
    let _ = writeln!(s, "| 进场次数 | {} |", r.enter_count);
    let _ = writeln!(s, "| SC 总额 | ¥{:.2} |", r.sc_total);
    let _ = writeln!(s, "| 礼物总额 | ¥{:.2} |", r.gift_total);
    let _ = writeln!(s, "| 舰长数 | {} |", r.guard_count);
    let _ = writeln!(s, "| 总营收 | ¥{:.2} |", r.revenue_total);

    let _ = writeln!(s, "\n## SuperChat 记录\n");
    if r.superchats.is_empty() {
        let _ = writeln!(s, "(无)");
    } else {
        let _ = writeln!(s, "| 时间 | 用户 | 金额 | 内容 |");
        let _ = writeln!(s, "| --- | --- | --- | --- |");
        for sc in &r.superchats {
            let _ = writeln!(
                s,
                "| {} | {} | ¥{:.2} | {} |",
                sc.received_at.format("%Y-%m-%d %H:%M:%S"),
                md_cell(&sc.username),
                sc.price,
                md_cell(&sc.text),
            );
        }
    }

    let _ = writeln!(s, "\n## 舰长记录\n");
    if r.guards.is_empty() {
        let _ = writeln!(s, "(无)");
    } else {
        let _ = writeln!(s, "| 时间 | 用户 | 等级 | 数量 | 金额 |");
        let _ = writeln!(s, "| --- | --- | --- | --- | --- |");
        for g in &r.guards {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | ¥{:.2} |",
                g.received_at.format("%Y-%m-%d %H:%M:%S"),
                md_cell(&g.username),
                guard_level_name(g.level),
                g.count,
                g.price,
            );
        }
    }

    let _ = writeln!(s, "\n## TOP 弹幕用户\n");
    if r.top_chatters.is_empty() {
        let _ = writeln!(s, "(无)");
    } else {
        let _ = writeln!(s, "| 用户 | 弹幕数 |");
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
    let mut s = String::from("username,price,text,received_at\n");
    for sc in &r.superchats {
        let _ = writeln!(
            s,
            "{},{:.2},{},{}",
            csv_field(&sc.username),
            sc.price,
            csv_field(&sc.text),
            csv_field(&sc.received_at.to_rfc3339()),
        );
    }
    s
}
