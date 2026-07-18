//! Embedded overlay pages: transparent, self-contained HTML for OBS
//! browser sources. Query params tune appearance without editing files:
//! `?size=18&width=420&lines=12` (danmaku) / `?size=30` (subtitle).

pub const DANMAKU_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>VTB 弹幕叠加层</title>
<style>
  html, body {
    margin: 0; padding: 0;
    background: transparent;
    overflow: hidden;
    font-family: system-ui, "PingFang SC", "Microsoft YaHei", sans-serif;
  }
  #list {
    display: flex; flex-direction: column; justify-content: flex-end;
    gap: 6px;
    box-sizing: border-box;
    padding: 8px;
    height: 100vh;
    width: var(--w, 420px);
  }
  .row {
    color: #fff;
    font-size: var(--fs, 18px);
    line-height: 1.35;
    text-shadow: 0 0 3px rgba(0,0,0,.9), 0 1px 2px rgba(0,0,0,.9);
    word-break: break-word;
    animation: in .18s ease-out;
  }
  .row .u { color: #9fd0ff; margin-right: 6px; }
  .row .medal {
    background: rgba(50,120,200,.85); color:#fff; border-radius: 3px;
    padding: 0 4px; margin-right: 6px; font-size: .8em;
  }
  .row.sc { background: rgba(255,140,0,.92); border-radius: 6px; padding: 4px 8px; }
  .row.gift .t { color: #ffb3d0; }
  .row.guard .t { color: #d9b3ff; }
  .row.enter { opacity: .6; font-size: calc(var(--fs, 18px) * .85); }
  .row .tr {
    display: block;
    color: #ffe9a8;
    font-size: calc(var(--fs, 18px) * .88);
    margin-top: 1px;
  }
  @keyframes in { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; } }
</style>
</head>
<body>
<div id="list"></div>
<script>
  const qs = new URLSearchParams(location.search);
  const MAX = parseInt(qs.get("lines") || "14", 10);
  document.documentElement.style.setProperty("--fs", (qs.get("size") || 18) + "px");
  document.documentElement.style.setProperty("--w", (qs.get("width") || 420) + "px");
  const list = document.getElementById("list");

  function guardName(l) { return ["", "总督", "提督", "舰长"][l] || ""; }

  function row(ev) {
    const div = document.createElement("div");
    div.className = "row";
    if (ev.kind === "danmaku") {
      let html = "";
      if (ev.medal) html += `<span class="medal">${esc(ev.medal.name)}·${ev.medal.level}</span>`;
      html += `<span class="u">${esc(ev.username)}</span><span class="t">${esc(ev.text)}</span>`;
      div.innerHTML = html;
    } else if (ev.kind === "super_chat") {
      div.className = "row sc";
      div.innerHTML = `<b>¥${ev.price}</b> <span class="u">${esc(ev.username)}</span>${esc(ev.text)}`;
    } else if (ev.kind === "gift") {
      div.className = "row gift";
      div.innerHTML = `<span class="u">${esc(ev.username)}</span><span class="t">投喂 ${esc(ev.gift_name)} ×${ev.count}</span>`;
    } else if (ev.kind === "guard_buy") {
      div.className = "row guard";
      div.innerHTML = `<span class="u">${esc(ev.username)}</span><span class="t">开通${guardName(ev.guard_level)}!</span>`;
    } else if (ev.kind === "enter") {
      div.className = "row enter";
      div.textContent = `${ev.username} 进入直播间`;
      return div;
    } else {
      return null;
    }
    return div;
  }

  function esc(s) {
    return String(s ?? "").replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));
  }

  function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws`);
    ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.type === "danmaku_translation") {
        const row = list.querySelector(`[data-tid="${m.tid}"]`);
        if (row) {
          const tr = document.createElement("span");
          tr.className = "tr";
          tr.textContent = m.translated;
          row.appendChild(tr);
        }
        return;
      }
      if (m.type !== "danmaku") return;
      const div = row(m);
      if (!div) return;
      if (m.tid != null) div.dataset.tid = m.tid;
      list.appendChild(div);
      while (list.children.length > MAX) list.removeChild(list.firstChild);
    };
    ws.onclose = () => setTimeout(connect, 2000);
  }
  connect();
</script>
</body>
</html>
"#;

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
  document.documentElement.style.setProperty("--fs", (qs.get("size") || 30) + "px");
  const HOLD_MS = parseInt(qs.get("hold") || "6000", 10);
  const t = document.getElementById("translated");
  const s = document.getElementById("source");
  let timer = null;

  function show(m) {
    t.textContent = m.translated || m.text || "";
    s.textContent = m.translated ? (m.text || "") : "";
    t.classList.remove("fade");
    s.classList.remove("fade");
    clearTimeout(timer);
    timer = setTimeout(() => { t.classList.add("fade"); s.classList.add("fade"); }, HOLD_MS);
  }

  function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws`);
    ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.type === "subtitle") show(m);
      if (m.type === "subtitle_clear") { t.textContent = ""; s.textContent = ""; }
    };
    ws.onclose = () => setTimeout(connect, 2000);
  }
  connect();
</script>
</body>
</html>
"#;
