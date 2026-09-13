import { afterEach, expect, it, vi } from "vitest";
import script from "../../crates/vtb-overlay/assets/chat.js?raw";
import pages from "../../crates/vtb-overlay/src/pages.rs?raw";

class Socket {
  static current: Socket;
  onmessage?: (event: { data: string }) => void;
  close() {}
  constructor() { Socket.current = this; }
  send(value: object) { this.onmessage?.({ data: JSON.stringify(value) }); }
}
afterEach(() => { window.dispatchEvent(new Event("pagehide")); vi.unstubAllGlobals(); window.history.replaceState({}, "", "/"); });
function page(query: string) {
  window.history.replaceState({}, "", `/?${query}`);
  document.body.innerHTML = '<div id="list"></div><div id="status"></div>';
  vi.stubGlobal("WebSocket", Socket);
  // Execute the exact script served by the Rust overlay, without Tauri.
  new Function(script)();
  return Socket.current;
}
const chat = (id: string, text = "hello") => ({ type: "danmaku", kind: "danmaku", room_id: 0, username: "name", text, tid: 7, source: { platform: "youtube", room_id: "abcdefghijk", user_id: "UC1", message_id: id } });
it.each([["en", "Live chat", "Showing live chat"], ["ja", "ライブチャット", "チャット表示中"], ["zh-CN", "直播评论", "正在显示直播评论"]])("localizes %s overlay chrome without changing chat", (lang, title, status) => {
  const ws = page(`lang=${lang}`);
  ws.send(chat("locale", "连接"));
  expect(document.documentElement.lang).toBe(lang);
  expect(document.title).toContain(title);
  expect(document.getElementById("list")).toHaveAttribute("aria-label", title);
  expect(document.querySelector(".text")).toHaveTextContent("连接");
  expect(document.getElementById("status")).toHaveTextContent(status);
  expect(document.body).not.toHaveClass("preview");
});
it("renders custom emoji safely alongside text", () => {
  const ws = page("");
  ws.send({ ...chat("emoji"), source: { ...chat("emoji").source, message_runs: [{text:"<b>hi</b>"},{text:":smile:",emoji_url:"https://example.org/smile.png"},{text:"unsafe",emoji_url:"javascript:alert(1)"}] } });
  expect(document.querySelector(".text b")).toBeNull();
  expect(document.querySelector(".emoji")).toHaveAttribute("alt", ":smile:");
  expect(document.querySelector(".text")?.textContent).toContain("unsafe");
  expect(document.querySelectorAll(".emoji")).toHaveLength(1);
});
it("filters rooms, escapes chat text, attaches translations, deduplicates and deletes", () => {
  const ws = page("platform=youtube&room=youtube%3Aabcdefghijk&lines=2");
  ws.send({ ...chat("other"), source: { ...chat("other").source, room_id: "other-room" } });
  ws.send(chat("1", '<img src=x onerror="alert(1)">')); ws.send(chat("1"));
  expect(document.querySelectorAll(".row")).toHaveLength(1);
  expect(document.querySelector(".text")?.textContent).toContain("<img");
  expect(document.querySelector(".text img")).toBeNull();
  ws.send({ type: "danmaku_translation", tid: 7, translated: "翻译" });
  expect(document.querySelector(".translation")?.textContent).toBe("翻译");
  ws.send({ type: "chat_delete", room_key: "youtube:abcdefghijk", message_id: "1" });
  expect(document.querySelectorAll(".row")).toHaveLength(0);
});
it("displays both sites, caps rows and keeps paid currency text", () => {
  const ws = page("lines=2&size=9999");
  ws.send(chat("1")); ws.send({ type: "danmaku", kind: "danmaku", room_id: 42, username: "B站", text: "弹幕" });
  ws.send({ ...chat("2"), kind: "super_chat", source: { ...chat("2").source, money: { display: "HK$20.00" } } });
  expect(document.querySelectorAll(".row")).toHaveLength(2);
  expect(document.querySelector(".price")?.textContent).toBe("HK$20.00");
  expect(document.documentElement.style.getPropertyValue("--dm-font-size")).toBe("72px");
});

it("filters subtitles by source and clears global or matching source messages", () => {
  window.history.replaceState({}, "", "/?room=youtube%3Aabcdefghijk&size=NaN");
  const html = pages.split('pub const SUBTITLE_HTML: &str = r#"')[1].split('"#;')[0];
  document.body.innerHTML = html;
  vi.stubGlobal("WebSocket", Socket);
  new Function(document.querySelector("script")!.textContent!)();
  const ws = Socket.current;
  ws.send({type:"subtitle",room_key:"bilibili:42",text:"other"});
  expect(document.querySelector("#translated")).toBeEmptyDOMElement();
  ws.send({type:"subtitle",room_key:"youtube:abcdefghijk",text:"hello",translated:"你好"});
  expect(document.querySelector("#translated")).toHaveTextContent("你好");
  ws.send({type:"subtitle_clear_source",room_key:"bilibili:42"});
  expect(document.querySelector("#translated")).toHaveTextContent("你好");
  ws.send({type:"subtitle_clear"});
  expect(document.querySelector("#translated")).toBeEmptyDOMElement();
  expect(document.documentElement.style.getPropertyValue("--fs")).toBe("30px");
});
