//! Diagnostic: send a danmaku with the logged-in account (keychain creds
//! saved by the app's QR login). Verifies the csrf(bili_jct) POST path.
//!
//! Usage: cargo run -p vtb-e2e --bin diag_danmaku_send -- <room> "<text>"

use vtb_account::CredentialStore;

#[tokio::main]
async fn main() {
    let room: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(320);
    let text = std::env::args().nth(2).unwrap_or_else(|| "test".into());

    let store = vtb_account::KeyringStore::default();
    let creds = match store.load() {
        Ok(Some(c)) => c,
        _ => {
            eprintln!("❌ 钥匙串没有登录凭据 — 先在 App 里扫码登录");
            std::process::exit(1);
        }
    };
    if creds.bili_jct.is_empty() {
        eprintln!("❌ 凭据缺少 bili_jct，请重新登录");
        std::process::exit(1);
    }
    println!("uid={} 发送到 room {room}: {text:?}", creds.dede_user_id);

    let client = vtb_account::build_client(Some(&creds)).unwrap();
    let api = vtb_danmaku::api::BiliApi::new(client);
    let real = api.room_init(room).await.unwrap().real_room_id;
    match api.send_danmaku(real, &text, &creds.bili_jct).await {
        Ok(()) => println!("✅ 发送成功 (real room {real})"),
        Err(e) => {
            println!("❌ 发送失败: {e}");
            std::process::exit(1);
        }
    }
}
