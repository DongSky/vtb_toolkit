/* Standalone OBS/browser page. No Tauri or third-party runtime dependency. */
(() => {
  "use strict";
  const qs = new URLSearchParams(location.search);
  const requested = qs.get("lang") || navigator.language;
  const lang = /^en(?:-|$)/i.test(requested) ? "en" : /^ja(?:-|$)/i.test(requested) ? "ja" : "zh-CN";
  const labels = {
    "zh-CN": ["直播评论", "正在连接本地服务…", "本地服务已连接 · 等待直播评论", "本地服务已连接 · 正在显示直播评论", "连接断开，正在重连…", "表情", "进入直播间", "会员", "总督", "提督", "舰长", "开通"],
    en: ["Live chat", "Connecting to local server…", "Connected · Waiting for live chat", "Connected · Showing live chat", "Disconnected · Reconnecting…", "Emoji", "joined the stream", "Member", "Governor", "Admiral", "Captain", "Joined"],
    ja: ["ライブチャット", "ローカルサーバーに接続中…", "接続済み・チャット待機中", "接続済み・チャット表示中", "切断されました・再接続中…", "絵文字", "が入室しました", "メンバー", "総督", "提督", "艦長", "加入"]
  }[lang];
  document.documentElement.lang = lang;
  document.title = `VTB Toolkit · ${labels[0]}`;
  const bounded = (key, fallback, min, max) => {
    const n = Number(qs.get(key) ?? fallback);
    return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : fallback;
  };
  const max = Math.floor(bounded("lines", 14, 1, 200));
  const hold = bounded("hold", 0, 0, 600000);
  const list = document.getElementById("list");
  const status = document.getElementById("status");
  list.setAttribute("aria-label", labels[0]);
  status.textContent = labels[1];
  const style = document.documentElement.style;
  style.setProperty("--width", bounded("width", 420, 200, 3840) + "px");
  style.setProperty("--dm-font-size", bounded("size", 20, 10, 72) + "px");
  if (qs.get("preview") === "1") document.body.classList.add("preview");
  const room = qs.get("room") || "";
  const platform = qs.get("platform") || "all";
  let showTime = false, showMedal = true;
  try {
    const theme = JSON.parse(qs.get("theme") || "null");
    if (theme && theme.vars) {
      const names = ["--dm-bg", "--dm-text", "--dm-username", "--dm-medal-bg", "--dm-sc-bg", "--dm-gift", "--dm-guard", "--dm-line-gap", "--dm-radius", "--dm-font-family"];
      for (const name of names) if (typeof theme.vars[name] === "string") style.setProperty(name, theme.vars[name]);
      showTime = Boolean(theme.showTime); showMedal = Boolean(theme.showMedal);
      if (theme.animation === "none") list.style.setProperty("--animation", "none");
    }
  } catch { /* malformed optional theme uses safe defaults */ }
  const span = (parent, cls, value) => {
    const el = document.createElement("span"); el.className = cls; el.textContent = String(value ?? ""); parent.appendChild(el); return el;
  };
  const picture = (parent, cls, value) => {
    if (typeof value !== "string" || !/^https:\/\//.test(value)) return;
    const el = document.createElement("img"); el.className = cls; el.src = value; el.alt = ""; el.referrerPolicy = "no-referrer"; parent.appendChild(el);
  };
  const sourceKey = (m) => m.source ? `${m.source.platform}:${m.source.room_id}` : `bilibili:${m.room_id}`;
  function message(parent, m) {
    const content = span(parent, "text", "");
    const runs = m.source?.message_runs;
    if (!Array.isArray(runs) || !runs.length) { content.textContent = m.text || ""; return; }
    for (const run of runs) {
      if (typeof run.emoji_url === "string" && run.emoji_url.startsWith("https://")) {
        picture(content, "emoji", run.emoji_url);
        content.lastChild.alt = run.text || labels[5];
      } else content.appendChild(document.createTextNode(String(run.text || "")));
    }
  }
  function removeMatches(m) {
    for (const row of [...list.children]) {
      if (row.dataset.room === m.room_key && ((m.message_id && row.dataset.message === m.message_id) || (m.user_id && row.dataset.user === m.user_id))) row.remove();
    }
  }
  function render(m) {
    if (m.type === "chat_delete") { removeMatches(m); return; }
    if (m.type === "danmaku_translation") {
      for (const row of list.children) if (row.dataset.tid === String(m.tid)) {
        const tr = row.querySelector(".translation") || span(row, "translation", ""); tr.textContent = m.translated;
      }
      return;
    }
    if (m.type !== "danmaku") return;
    const key = sourceKey(m), source = m.source || {};
    const site = source.platform || "bilibili";
    if ((platform !== "all" && site !== platform) || (room && room !== key)) return;
    if (!["danmaku", "super_chat", "gift", "guard_buy", "enter"].includes(m.kind)) return;
    const row = document.createElement("div"); row.className = "row";
    row.dataset.room = key;
    if (source.message_id) row.dataset.message = source.message_id;
    if (source.user_id) row.dataset.user = source.user_id;
    if (m.tid != null) row.dataset.tid = String(m.tid);
    if (source.message_id) for (const existing of list.children) if (existing.dataset.room === key && existing.dataset.message === source.message_id) return;
    if (showTime && m.timestamp) span(row, "time", new Date(m.timestamp).toLocaleTimeString(lang));
    if (qs.get("badges") !== "0") span(row, "platform", site === "youtube" ? "YouTube" : site === "twitch" ? "Twitch" : "Bilibili");
    if (qs.get("avatars") !== "0") picture(row, "avatar", source.avatar_url);
    if (showMedal && m.medal) span(row, "medal", `${m.medal.name} · ${m.medal.level}`);
    span(row, "user", m.username);
    if (m.kind === "super_chat") {
      row.classList.add("sc"); span(row, "price", source.money?.display || (site === "bilibili" ? `¥${m.price}` : "Super Chat"));
      message(row, m); picture(row, "sticker", source.sticker_url);
    } else if (m.kind === "gift") {
      row.classList.add("gift"); span(row, "text", source.platform ? m.gift_name : `${m.gift_name} ×${m.count}`);
    } else if (m.kind === "guard_buy") {
      row.classList.add("guard"); span(row, "text", source.membership || `${labels[11]} ${labels[7 + (m.guard_level >= 1 && m.guard_level <= 3 ? m.guard_level : 0)]} ×${m.count}`);
    } else if (m.kind === "enter") span(row, "text", labels[6]);
    else { message(row, m); picture(row, "sticker", m.emoticon); }
    list.appendChild(row);
    status.textContent = labels[3];
    while (list.children.length > max) list.firstChild.remove();
    if (hold > 0) setTimeout(() => row.remove(), hold);
  }
  let socket, timer, closed = false;
  function connect() {
    if (closed) return;
    socket = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`);
    socket.onopen = () => { status.textContent = labels[2]; };
    socket.onmessage = (event) => { try { render(JSON.parse(event.data)); } catch { /* bad packet does not break the stream */ } };
    socket.onclose = () => { status.textContent = labels[4]; if (!closed) timer = setTimeout(connect, 2000); };
    socket.onerror = () => socket.close();
  }
  window.addEventListener("pagehide", () => { closed = true; clearTimeout(timer); socket?.close(); });
  connect();
})();
