//! Diagnostic: managed danmaku connection against a live room — prints
//! connection states and events; verifies auto-reconnect works live.

use std::time::Duration;
use vtb_danmaku::api::BiliApi;
use vtb_danmaku::{spawn_managed, ConnState, DanmakuClientConfig, ManagedEvent};

#[tokio::main]
async fn main() {
    let room: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(40);

    let api = BiliApi::default_client().unwrap();
    let (mut rx, stop) = spawn_managed(api, DanmakuClientConfig::anonymous(room));

    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    let mut n_events = 0;
    let mut n_connects = 0;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            ev = rx.recv() => match ev {
                Some(ManagedEvent::State(s)) => {
                    if matches!(s, ConnState::Connected { .. }) { n_connects += 1; }
                    println!("[state] {s:?}");
                }
                Some(ManagedEvent::Live(ev)) => {
                    n_events += 1;
                    if n_events <= 5 {
                        println!("[event] {ev:?}");
                    }
                }
                None => break,
            }
        }
    }
    stop.send(true).unwrap();
    // Drain until Stopped.
    while let Some(ev) = rx.recv().await {
        if matches!(ev, ManagedEvent::State(ConnState::Stopped)) {
            println!("[state] Stopped");
            break;
        }
    }
    println!("\n连接次数: {n_connects}, 事件数: {n_events}");
    if n_connects >= 1 && n_events > 0 {
        println!("✅ managed 客户端正常");
    }
}
