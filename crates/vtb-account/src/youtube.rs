//! Google desktop OAuth and resumable uploads. Anonymous chat is independent.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

type Result<T> = std::result::Result<T, String>;
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const UPLOAD_URL: &str =
    "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status";

#[derive(Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}
impl ClientConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.client_id.ends_with(".apps.googleusercontent.com") || self.client_id.len() > 256 {
            return Err("请填写 Google Cloud 的桌面应用 OAuth 客户端 ID".into());
        }
        Ok(())
    }
}

pub struct Authorization {
    listener: tokio::net::TcpListener,
    state: String,
    verifier: String,
    redirect: String,
    pub url: String,
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}
fn network(e: reqwest::Error) -> String {
    e.without_url().to_string()
}
async fn response_json(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let body: Value = response.json().await.map_err(network)?;
    if !status.is_success() {
        let message = body["error"]["message"]
            .as_str()
            .or(body["error_description"].as_str())
            .unwrap_or("Google 请求失败");
        return Err(format!("Google {status}: {message}"));
    }
    Ok(body)
}
fn callback(target: &str, state: &str) -> Result<Option<String>> {
    let url =
        url::Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| "无效的授权回调")?;
    if url.path() != "/oauth/callback" {
        return Ok(None);
    }
    let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
    if params.get("state").map(|s| s.as_ref()) != Some(state) {
        return Ok(None);
    }
    if params.contains_key("error") {
        return Err("Google 授权未完成或已拒绝".into());
    }
    Ok(params.get("code").map(|s| s.to_string()))
}

async fn read_callback_request(reader: &mut (impl tokio::io::AsyncRead + Unpin)) -> Result<String> {
    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    while request.len() < 8192 {
        let n = reader.read(&mut chunk).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..n]);
        if request.windows(4).any(|part| part == b"\r\n\r\n") {
            return String::from_utf8(request).map_err(|_| "无效的授权回调编码".into());
        }
    }
    Err("授权回调不完整或过长".into())
}

impl Authorization {
    pub async fn new(config: &ClientConfig) -> Result<Self> {
        config.validate()?;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| e.to_string())?;
        let redirect = format!(
            "http://127.0.0.1:{}/oauth/callback",
            listener.local_addr().map_err(|e| e.to_string())?.port()
        );
        let verifier = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
        let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
        url.query_pairs_mut().extend_pairs([
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("response_type", "code"),
            ("scope", "https://www.googleapis.com/auth/youtube.upload"),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ]);
        Ok(Self {
            listener,
            state,
            verifier,
            redirect,
            url: url.into(),
        })
    }

    /// The listener and pending exchange disappear when this future is cancelled.
    pub async fn finish(self, config: &ClientConfig) -> Result<String> {
        let wait = async {
            loop {
                let (mut socket, _) = self.listener.accept().await.map_err(|e| e.to_string())?;
                let request = match tokio::time::timeout(
                    Duration::from_secs(5),
                    read_callback_request(&mut socket),
                )
                .await
                {
                    Ok(Ok(request)) => request,
                    _ => continue,
                };
                let mut words = request.lines().next().unwrap_or("").split_whitespace();
                let method = words.next();
                let target = words.next().unwrap_or("");
                if method != Some("GET") {
                    continue;
                }
                let result = callback(target, &self.state);
                let accepted = !matches!(result, Ok(None));
                let body = if accepted {
                    "Authorization received. Return to VTB Toolkit."
                } else {
                    "Unknown request."
                };
                let reply = format!("HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",if accepted {"200 OK"} else {"404 Not Found"},body.len());
                let _ = socket.write_all(reply.as_bytes()).await;
                if let Some(code) = result? {
                    return Ok::<_, String>(code);
                }
            }
        };
        let code = tokio::time::timeout(Duration::from_secs(300), wait)
            .await
            .map_err(|_| "授权等待超时，请重试")??;
        let response = client()?
            .post(TOKEN_URL)
            .form(&[
                ("client_id", config.client_id.as_str()),
                ("client_secret", config.client_secret.as_str()),
                ("code", code.as_str()),
                ("code_verifier", self.verifier.as_str()),
                ("redirect_uri", self.redirect.as_str()),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(network)?;
        response_json(response).await?["refresh_token"]
            .as_str()
            .map(str::to_string)
            .ok_or("授权未返回刷新凭据，请重新授权".into())
    }
}

pub async fn access_token(config: &ClientConfig, refresh: &str) -> Result<String> {
    let response = client()?
        .post(TOKEN_URL)
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(network)?;
    response_json(response).await?["access_token"]
        .as_str()
        .map(str::to_string)
        .ok_or("请重新授权 YouTube 投稿".into())
}

#[derive(Deserialize)]
pub struct UploadOptions {
    pub file: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub privacy: String,
    pub made_for_kids: bool,
}
impl UploadOptions {
    pub fn validate(&self) -> Result<()> {
        if self.title.trim().is_empty()
            || self.title.chars().count() > 100
            || self.title.contains(['<', '>'])
        {
            return Err("标题需为 1–100 字且不能含尖括号".into());
        }
        if self.description.len() > 5000 {
            return Err("描述超过 5000 字节".into());
        }
        if !["private", "unlisted", "public"].contains(&self.privacy.as_str()) {
            return Err("无效的可见性".into());
        }
        if self.tags.iter().map(String::len).sum::<usize>() > 500 {
            return Err("标签总长度超过 500 字节".into());
        }
        Ok(())
    }
}
fn upload_location(value: &str) -> Result<url::Url> {
    let url = url::Url::parse(value).map_err(|_| "无效上传会话")?;
    if url.scheme() != "https"
        || url.host_str() != Some("www.googleapis.com")
        || !url.path().starts_with("/upload/youtube/")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Google 返回了非预期的上传会话地址".into());
    }
    Ok(url)
}
fn acknowledged(response: &reqwest::Response, total: u64) -> Result<u64> {
    let Some(range) = response.headers().get("range") else {
        return Ok(0);
    };
    let last = range
        .to_str()
        .ok()
        .and_then(|s| s.strip_prefix("bytes=0-"))
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("上传进度响应无效")?;
    last.checked_add(1)
        .filter(|n| *n <= total)
        .ok_or("上传进度超出文件大小".into())
}

/// Bounded memory, 8 MiB chunks; retry interrupted requests by querying the
/// server's committed range. Dropping the future cancels local transfer.
pub async fn upload(
    token: &str,
    options: &UploadOptions,
    progress: impl Fn(u64, u64) + Send,
) -> Result<String> {
    options.validate()?;
    let mut file = tokio::fs::File::open(Path::new(&options.file))
        .await
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().await.map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("请选择非空视频文件".into());
    }
    let total = metadata.len();
    let http = client()?;
    let response = http.post(UPLOAD_URL).bearer_auth(token)
        .header("X-Upload-Content-Length",total).header("X-Upload-Content-Type","application/octet-stream")
        .json(&json!({"snippet":{"title":options.title,"description":options.description,"tags":options.tags,"categoryId":"22"},"status":{"privacyStatus":options.privacy,"selfDeclaredMadeForKids":options.made_for_kids}}))
        .send().await.map_err(network)?;
    if !response.status().is_success() {
        return response_json(response)
            .await
            .and(Err("无法创建上传会话".into()));
    }
    let location = upload_location(
        response
            .headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .ok_or("Google 未返回上传会话地址")?,
    )?;
    let mut offset = 0;
    let mut failures = 0u32;
    let mut query = false;
    loop {
        let response = if query {
            http.put(location.clone())
                .bearer_auth(token)
                .header("Content-Length", 0)
                .header("Content-Range", format!("bytes */{total}"))
                .send()
                .await
        } else {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| e.to_string())?;
            let length = (total - offset).min(8 * 1024 * 1024) as usize;
            if length == 0 {
                return Err("上传结束但服务未确认，请在 YouTube Studio 核实".into());
            }
            let mut bytes = vec![0; length];
            file.read_exact(&mut bytes)
                .await
                .map_err(|e| e.to_string())?;
            http.put(location.clone())
                .bearer_auth(token)
                .header("Content-Type", "application/octet-stream")
                .header(
                    "Content-Range",
                    format!("bytes {offset}-{}/{total}", offset + length as u64 - 1),
                )
                .body(bytes)
                .send()
                .await
        };
        match response {
            Ok(response) if response.status().is_success() => {
                let value = response_json(response).await?;
                let id = value["id"].as_str().ok_or("上传响应缺少视频 ID")?;
                progress(total, total);
                return Ok(id.to_string());
            }
            Ok(response) if response.status().as_u16() == 308 => {
                let committed = acknowledged(&response, total)?;
                if committed <= offset && !query {
                    failures += 1;
                } else if committed > offset {
                    failures = 0;
                }
                offset = committed;
                progress(offset, total);
                query = false;
            }
            Ok(response)
                if response.status().is_server_error() || response.status().as_u16() == 429 =>
            {
                failures += 1;
                query = true;
            }
            Ok(response) => {
                return response_json(response).await.and(Err("上传失败".into()));
            }
            Err(_) => {
                failures += 1;
                query = true;
            }
        }
        if failures > 5 {
            return Err("多次重试仍无法完成上传，请在 YouTube Studio 检查后重试".into());
        }
        if failures > 0 {
            tokio::time::sleep(Duration::from_secs(2u64.pow(failures.min(5)))).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn callback_accepts_fragmented_http_headers_and_rejects_truncation() {
        let (mut sender, mut receiver) = tokio::io::duplex(16);
        let send = tokio::spawn(async move {
            sender
                .write_all(b"GET /oauth/callback?state=ok&code=abc HTTP/1.1\r\n")
                .await
                .unwrap();
            tokio::task::yield_now().await;
            sender.write_all(b"Host: localhost\r\n\r\n").await.unwrap();
        });
        let request = read_callback_request(&mut receiver).await.unwrap();
        send.await.unwrap();
        assert_eq!(
            callback(request.split_whitespace().nth(1).unwrap(), "ok").unwrap(),
            Some("abc".into())
        );
        assert!(
            read_callback_request(&mut b"GET /oauth/callback".as_slice())
                .await
                .is_err()
        );
        assert!(read_callback_request(&mut vec![b'x'; 8192].as_slice())
            .await
            .is_err());
    }
    #[test]
    fn callback_requires_matching_state_and_path() {
        assert_eq!(
            callback("/oauth/callback?state=ok&code=secret", "ok").unwrap(),
            Some("secret".into())
        );
        assert_eq!(
            callback("/oauth/callback?state=wrong&code=secret", "ok").unwrap(),
            None
        );
        assert_eq!(callback("/favicon.ico", "ok").unwrap(), None);
        assert!(callback("/oauth/callback?state=ok&error=access_denied", "ok").is_err());
    }
    #[test]
    fn session_url_never_sends_token_to_a_different_origin() {
        assert!(upload_location(
            "https://www.googleapis.com/upload/youtube/v3/videos?upload_id=abc"
        )
        .is_ok());
        for url in [
            "http://www.googleapis.com/upload/youtube/v3/videos",
            "https://www.googleapis.com.evil.test/upload/youtube/v3/videos",
            "https://user@www.googleapis.com/upload/youtube/v3/videos",
        ] {
            assert!(upload_location(url).is_err());
        }
    }
    #[tokio::test]
    async fn desktop_authorization_uses_pkce_and_only_upload_scope() {
        let config = ClientConfig {
            client_id: "example.apps.googleusercontent.com".into(),
            client_secret: String::new(),
        };
        let auth = Authorization::new(&config).await.unwrap();
        let url = url::Url::parse(&auth.url).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(
            params["scope"],
            "https://www.googleapis.com/auth/youtube.upload"
        );
        assert_eq!(
            params["code_challenge"],
            URL_SAFE_NO_PAD.encode(Sha256::digest(auth.verifier.as_bytes()))
        );
    }
}
