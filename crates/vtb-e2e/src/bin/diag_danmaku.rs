//! Diagnostic: connect to a room and print EVERY raw cmd seen at the
//! protocol layer, tallying which ones our parser converts vs drops.

use std::collections::BTreeMap;
use std::time::Duration;

use vtb_danmaku::api::BiliApi;
use vtb_danmaku::messages::parse_cmd;
use vtb_danmaku::protocol::{self, Operation};

#[tokio::main]
async fn main() {
    let room: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(320);
    let secs: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let api = BiliApi::default_client().unwrap();
    let room_info = api.room_init(room).await.unwrap();
    let real = room_info.real_room_id;
    let info = api.danmu_info(real).await.unwrap();
    let host = info.hosts.into_iter().next().unwrap();
    println!("real_room={real} host={} token_len={}", host.host, info.token.len());

    // rustls provider
    let _ = rustls::crypto::ring::default_provider().install_default();

    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let (ws, _) = tokio_tungstenite::connect_async(&host.wss_url()).await.unwrap();
    let (mut w, mut r) = ws.split();
    let auth = protocol::encode(
        Operation::Auth,
        protocol::ProtoVer::Json,
        &protocol::auth_body(0, real, &info.token, None),
    );
    w.send(Message::Binary(auth)).await.unwrap();

    // heartbeat keepalive
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(25));
        loop {
            t.tick().await;
            if w.send(Message::Binary(protocol::heartbeat())).await.is_err() {
                break;
            }
        }
    });

    let mut raw_cmds: BTreeMap<String, usize> = BTreeMap::new();
    let mut parsed_ok = 0usize;
    let mut parse_none = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            msg = r.next() => match msg {
                Some(Ok(Message::Binary(data))) => {
                    let frames = protocol::decode_packet(&data).unwrap_or_default();
                    for f in frames {
                        if f.operation != Operation::Message { continue; }
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&f.body) {
                            let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("<none>");
                            let base = cmd.split(':').next().unwrap_or(cmd).to_string();
                            *raw_cmds.entry(base.clone()).or_insert(0) += 1;
                            // Save any DANMU_MSG whose info[0][13] is an object (emoticon danmaku).
                            if base == "DANMU_MSG" {
                                if let Some(obj) = v.get("info").and_then(|i| i.get(0)).and_then(|m| m.get(13)) {
                                    if obj.is_object() {
                                        let _ = std::fs::write("/tmp/emo_sample.json", serde_json::to_string_pretty(&v).unwrap());
                                    }
                                }
                            }
                            match parse_cmd(real, &v) {
                                Some(ev) => {
                                    parsed_ok += 1;
                                    print_event(&ev);
                                }
                                None => parse_none += 1,
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            }
        }
    }

    println!("\n=== {secs}s 内收到的原始 cmd 统计 ===");
    let mut total = 0;
    for (cmd, n) in &raw_cmds {
        total += n;
        println!("  {n:4}  {cmd}");
    }
    println!("总 message 帧: {total}");
    println!("解析成功(转为LiveEvent): {parsed_ok}");
    println!("解析忽略(返回None): {parse_none}");
}

fn print_event(ev: &vtb_common::LiveEvent) {
    use vtb_common::LiveEvent::*;
    match ev {
        Danmaku(d) => println!(
            "  弹幕 [{}]{} {}: {}",
            d.username,
            d.medal.as_ref().map(|m| format!(" {}·{}", m.name, m.level)).unwrap_or_default(),
            if d.guard_level > 0 { format!("(舰{})", d.guard_level) } else { String::new() },
            d.text
        ),
        Enter(e) => println!("  进场 {}", e.username),
        SuperChat(s) => println!("  SC ¥{} {}: {}", s.price, s.username, s.text),
        Gift(g) => println!("  礼物 {} 送 {}×{}", g.username, g.gift_name, g.count),
        GuardBuy(g) => println!("  上舰 {} 开通 guard{}", g.username, g.guard_level),
        Like(l) => println!("  点赞 {}", l.username),
        WatchedChange { count, .. } => println!("  看过 {count}"),
        _ => {}
    }
}
