//! Diagnostic: anonymous Twitch chat → LiveEvent pipeline.
//!
//! Usage: cargo run -p vtb-e2e --bin diag_twitch -- <channel> <secs>

use std::time::Duration;

#[tokio::main]
async fn main() {
    let channel = std::env::args().nth(1).unwrap_or_else(|| "shroud".into());
    let secs: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    println!("连接 Twitch #{channel}，采集 {secs}s 聊天…");
    let (mut rx, stop) = vtb_danmaku::twitch::spawn_twitch_chat(&channel);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    let mut count = 0u32;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            ev = rx.recv() => match ev {
                Some(vtb_common::LiveEvent::Danmaku(d)) => {
                    count += 1;
                    println!("[{}]{} {}: {}", d.uid, if d.is_admin {"(mod)"} else {""}, d.username, d.text);
                }
                Some(_) => {}
                None => { println!("连接关闭"); break; }
            }
        }
    }
    let _ = stop.send(true);
    println!("\n共 {count} 条聊天");
    if count == 0 {
        println!("⚠️  没有收到聊天（频道可能没开播/无人发言）");
    }
}
