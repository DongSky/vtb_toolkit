//! Embedded overlay pages: transparent, self-contained HTML for OBS
//! browser sources. Query params tune appearance without editing files:
//! `?size=18&width=420&lines=12` (danmaku) / `?size=30` (subtitle).

pub const DANMAKU_HTML: &str = include_str!("../assets/danmaku.html");
pub const CHAT_JS: &str = include_str!("../assets/chat.js");
pub const GUIDE_HTML: &str = include_str!("../../../public/guide/index.html");

pub const SUBTITLE_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>VTB 同传字幕条</title>
<style>
  html, body {
    margin: 0; padding: 0;
    background: transparent;
    overflow: hidden;
    font-family: system-ui, "PingFang SC", "Microsoft YaHei", sans-serif;
  }
  #bar {
    position: fixed; left: 0; right: 0; bottom: 0;
    display: flex; flex-direction: column; align-items: center;
    padding: 10px 24px 16px;
    box-sizing: border-box;
    text-align: center;
  }
  #translated {
    color: #fff;
    font-size: var(--fs, 30px);
    font-weight: 600;
    line-height: 1.35;
    text-shadow: 0 0 4px rgba(0,0,0,.95), 0 2px 3px rgba(0,0,0,.9);
  }
  #source {
    color: #cfe6ff;
    font-size: calc(var(--fs, 30px) * .62);
    line-height: 1.3;
    margin-top: 2px;
    text-shadow: 0 0 3px rgba(0,0,0,.95);
    opacity: .92;
  }
  .fade { transition: opacity .4s; opacity: 0 !important; }
</style>
</head>
<body>
<div id="bar">
  <div id="translated"></div>
  <div id="source"></div>
</div>
<script>
  const qs = new URLSearchParams(location.search);
  const requested = qs.get("lang") || navigator.language;
  const lang = /^en(?:-|$)/i.test(requested) ? "en" : /^ja(?:-|$)/i.test(requested) ? "ja" : "zh-CN";
  document.documentElement.lang = lang;
  document.title = lang === "en" ? "VTB Toolkit · Live subtitles" : lang === "ja" ? "VTB Toolkit · リアルタイム字幕" : "VTB Toolkit · 实时字幕";
  document.getElementById("bar").setAttribute("role", "log");
  document.getElementById("bar").setAttribute("aria-label", document.title);
  const bounded = (key, fallback, min, max) => {
    const n = Number(qs.get(key) ?? fallback);
    return Number.isFinite(n) ? Math.max(min, Math.min(max, n)) : fallback;
  };
  document.documentElement.style.setProperty("--fs", bounded("size", 30, 10, 120) + "px");
  const HOLD_MS = bounded("hold", 6000, 100, 600000);
  const t = document.getElementById("translated");
  const s = document.getElementById("source");
  let timer = null, socket, reconnect, closed = false, lastRoom = "";
  const room = qs.get("room");
  const platform = qs.get("platform") || "all";

  function show(m) {
    lastRoom = m.room_key || "";
    t.textContent = m.translated || m.text || "";
    s.textContent = m.translated ? (m.text || "") : "";
    t.classList.remove("fade");
    s.classList.remove("fade");
    clearTimeout(timer);
    timer = setTimeout(() => { t.classList.add("fade"); s.classList.add("fade"); }, HOLD_MS);
  }

  function connect() {
    if (closed) return;
    socket = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`);
    socket.onmessage = (e) => {
      let m; try { m = JSON.parse(e.data); } catch { return; }
      if (m.type === "subtitle_clear") { t.textContent = ""; s.textContent = ""; clearTimeout(timer); return; }
      if (room && m.room_key !== room) return;
      if (platform !== "all" && !(m.room_key || "").startsWith(platform + ":")) return;
      if (m.type === "subtitle") show(m);
      if (m.type === "subtitle_clear" || (m.type === "subtitle_clear_source" && lastRoom === m.room_key)) { t.textContent = ""; s.textContent = ""; clearTimeout(timer); }
    };
    socket.onclose = () => { if (!closed) reconnect = setTimeout(connect, 2000); };
    socket.onerror = () => socket.close();
  }
  window.addEventListener("pagehide", () => { closed = true; clearTimeout(reconnect); clearTimeout(timer); socket?.close(); });
  connect();
</script>
</body>
</html>
"#;
