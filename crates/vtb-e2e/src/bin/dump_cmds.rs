//! Dump raw JSON of specific cmds to understand their real payload shape.

use std::time::Duration;

use vtb_danmaku::api::BiliApi;
use vtb_danmaku::protocol::{self, Operation};

#[tokio::main]
async fn main() {
    let room: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(40);
    let want: Vec<String> = std::env::args().skip(3).collect();

    let api = BiliApi::default_client().unwrap();
    let real = api.room_init(room).await.unwrap().real_room_id;
    let info = api.danmu_info(real).await.unwrap();
    let host = info.hosts.into_iter().next().unwrap();
    let _ = rustls::crypto::ring::default_provider().install_default();

    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let (ws, _) = tokio_tungstenite::connect_async(&host.wss_url()).await.unwrap();
    let (mut w, mut r) = ws.split();
    w.send(Message::Binary(protocol::encode(
        Operation::Auth,
        protocol::ProtoVer::Json,
        &protocol::auth_body(0, real, &info.token, None),
    )))
    .await
    .unwrap();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(25));
        loop {
            t.tick().await;
            if w.send(Message::Binary(protocol::heartbeat())).await.is_err() {
                break;
            }
        }
    });

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            msg = r.next() => match msg {
                Some(Ok(Message::Binary(data))) => {
                    for f in protocol::decode_packet(&data).unwrap_or_default() {
                        if f.operation != Operation::Message { continue; }
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&f.body) {
                            let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("").split(':').next().unwrap_or("").to_string();
                            if (want.is_empty() || want.contains(&cmd)) && seen.insert(cmd.clone()) {
                                println!("\n===== {cmd} =====");
                                println!("{}", serde_json::to_string_pretty(&v).unwrap());
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            }
        }
    }
}
