//! Diagnostic: OBS overlay end-to-end with real danmaku.
//! Starts the overlay server, feeds it a managed danmaku connection from a
//! live room, connects a WS client (simulating the OBS page), and counts
//! messages arriving on the wire.

use futures_util::StreamExt;
use std::time::Duration;
use vtb_danmaku::api::BiliApi;
use vtb_danmaku::{spawn_managed, DanmakuClientConfig, ManagedEvent};

#[tokio::main]
async fn main() {
    let room: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(30);

    // 1. Overlay server.
    let publisher = vtb_overlay::publisher();
    let server = vtb_overlay::start(&publisher, 0).await.unwrap();
    println!("overlay 服务: {}", server.danmaku_url());

    // 2. Verify the page serves.
    let page = reqwest::get(server.danmaku_url()).await.unwrap();
    println!("弹幕页 HTTP {}", page.status());
    assert!(page.status().is_success());

    // 3. WS client (what the OBS browser source does).
    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://127.0.0.1:{}/ws",
        server.port
    ))
    .await
    .unwrap();
    let (_w, mut read) = ws.split();
    let ws_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let wc = ws_count.clone();
    let reader = tokio::spawn(async move {
        let mut first = true;
        while let Some(Ok(msg)) = read.next().await {
            if let Ok(text) = msg.into_text() {
                if first {
                    println!("WS 首条消息: {}", &text.chars().take(120).collect::<String>());
                    first = false;
                }
                wc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
    });

    // 4. Real danmaku → publisher.
    let api = BiliApi::default_client().unwrap();
    let real = api.room_init(room).await.unwrap().real_room_id;
    let (mut rx, stop) = spawn_managed(
        BiliApi::default_client().unwrap(),
        DanmakuClientConfig::anonymous(real),
    );
    println!("采集 {secs}s 真实弹幕 (room {real})…");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    let mut published = 0u32;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            ev = rx.recv() => match ev {
                Some(ManagedEvent::Live(live)) => {
                    publisher.publish(vtb_overlay::OverlayMessage::Danmaku { event: live, tid: None });
                    published += 1;
                }
                Some(_) => {}
                None => break,
            }
        }
    }
    let _ = stop.send(true);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let received = ws_count.load(std::sync::atomic::Ordering::SeqCst);
    println!("\n发布事件: {published}, WS 收到: {received}");
    reader.abort();
    server.shutdown().await;

    if published > 0 && received == published {
        println!("✅ OBS 叠加层端到端验证通过（发布=接收）");
    } else if received > 0 {
        println!("⚠️  收发不一致（可能 lag 丢弃）");
    } else {
        println!("❌ WS 未收到事件");
    }
}
