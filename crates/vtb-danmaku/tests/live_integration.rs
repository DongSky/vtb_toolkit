//! Live integration tests against real Bilibili endpoints.
//!
//! Ignored by default (network + external service). Run explicitly:
//! `cargo test -p vtb-danmaku --test live_integration -- --ignored --nocapture`

use vtb_danmaku::api::BiliApi;
use vtb_danmaku::{DanmakuClient, DanmakuClientConfig};

/// A large official room that exists permanently (bilibili 直播运营).
const TEST_ROOM: u64 = 1;

#[tokio::test]
#[ignore = "hits live bilibili API"]
async fn handshake_resolves_room_and_token() {
    let api = BiliApi::default_client().unwrap();
    let room = api.room_init(TEST_ROOM).await.unwrap();
    assert!(room.real_room_id > 0);

    let info = api.danmu_info(room.real_room_id).await.unwrap();
    assert!(!info.token.is_empty());
    assert!(!info.hosts.is_empty());
    println!(
        "room {} -> {} hosts, token len {}",
        room.real_room_id,
        info.hosts.len(),
        info.token.len()
    );
}

#[tokio::test]
#[ignore = "hits live bilibili websocket"]
async fn connect_and_receive_events() {
    let client = DanmakuClient::new(DanmakuClientConfig::anonymous(TEST_ROOM)).unwrap();
    let (mut rx, handle) = client.connect().await.unwrap();

    // Wait up to 45s for at least one event (heartbeat reply arrives ~30s,
    // popular rooms produce danmaku much sooner).
    let ev = tokio::time::timeout(std::time::Duration::from_secs(45), rx.recv())
        .await
        .expect("timed out waiting for any event")
        .expect("channel closed unexpectedly");
    println!("first event: {ev:?}");

    handle.abort();
}
