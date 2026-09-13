import { Innertube } from "youtubei.js/web";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { liveVideoId, normalizeItem, record, text, youtubeInput } from "./normalize";

type VideoInfo = Awaited<ReturnType<Innertube["getInfo"]>>;
export interface YoutubeRoom {
  input: string;
  video_id: string | null;
  channel_id: string | null;
  title: string;
  author: string;
  cover: string;
  live: boolean;
  upcoming: boolean;
  chat_available: boolean;
  connected: boolean;
}

export async function nativeFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const request = new Request(input, init);
  if (request.signal.aborted) throw new DOMException("Aborted", "AbortError");
  const result = await invoke<{ status: number; headers: Record<string, string>; body: string }>("youtube_http", {
    request: { url: request.url, method: request.method, headers: Object.fromEntries(request.headers.entries()),
      body: ["GET", "HEAD"].includes(request.method) ? null : await request.text() },
  });
  if (request.signal.aborted) throw new DOMException("Aborted", "AbortError");
  return new Response(result.body, { status: result.status, headers: result.headers });
}

let client: Promise<Innertube> | undefined;
function youtube(): Promise<Innertube> {
  client ??= Innertube.create({ fetch: nativeFetch, retrieve_player: false, lang: "en", location: "US" })
    .catch((e) => { client = undefined; throw e; });
  return client;
}

const cache = new Map<string, { expires: number; room: YoutubeRoom; info?: VideoInfo }>();
const active = new Map<string, Connection>();
const closing = new Map<string, Promise<void>>();

async function roomInfo(input: string, fresh = false): Promise<{ room: YoutubeRoom; info?: VideoInfo }> {
  const url = youtubeInput(input);
  const cached = cache.get(url);
  if (!fresh && cached && cached.expires > Date.now()) return { ...cached, room: { ...cached.room, connected: active.has(cached.room.video_id ?? "") } };
  const yt = await youtube();
  let videoId = new URL(url).searchParams.get("v");
  let channelId: string | null = null;
  let author = "";
  let cover = "";
  if (!videoId) {
    const endpoint = await yt.resolveURL(url.replace(/\/live$/, ""));
    channelId = endpoint.payload.browseId ?? null;
    if (!channelId) throw new Error("无法解析 YouTube 频道");
    const channel = await yt.getChannel(channelId);
    author = channel.metadata.title ?? "";
    cover = channel.metadata.avatar?.[0]?.url ?? "";
    if (channel.has_live_streams) {
      const feed = await channel.getLiveStreams();
      videoId = feed.videos.map(liveVideoId).find((id) => id != null) ?? null;
    }
  }
  let info: VideoInfo | undefined;
  if (videoId) info = await yt.getInfo(videoId);
  const basic = info?.basic_info;
  const room: YoutubeRoom = {
    input: url, video_id: videoId, channel_id: basic?.channel_id ?? channelId,
    title: basic?.title ?? author, author: basic?.author ?? author,
    cover: basic?.thumbnail?.[0]?.url ?? cover,
    live: Boolean(basic?.is_live), upcoming: Boolean(basic?.is_upcoming),
    chat_available: Boolean(info?.livechat && !info.livechat.is_replay), connected: active.has(videoId ?? ""),
  };
  if (cache.size >= 100) cache.delete(cache.keys().next().value!);
  cache.set(url, { expires: Date.now() + 25_000, room, info });
  return { room, info };
}

class Connection {
  readonly consumers = new Set<string>();
  private closed = false;
  private timer?: ReturnType<typeof setTimeout>;
  private queue: Promise<unknown> = Promise.resolve();
  private seen = new Set<string>();
  private continuation: string;
  private failures = 0;
  private state = "connecting";
  private message = "正在连接 YouTube 评论";
  constructor(readonly videoId: string, private info: VideoInfo) {
    this.continuation = info.livechat?.continuation ?? "";
  }
  async status(state = this.state, message = this.message) {
    this.state = state; this.message = message;
    await invoke("youtube_connection", { videoId: this.videoId, status: {
      video_id: this.videoId, title: this.info.basic_info.title, state, message, consumers: [...this.consumers],
    } });
  }
  private enqueue(task: () => Promise<unknown>) {
    this.queue = this.queue.then(() => this.closed ? undefined : task()).catch((e) => {
      if (!this.closed) void this.status("error", String(e));
    });
  }
  private action(value: unknown) {
    if (this.closed) return;
    const action = record(value);
    const type = text(action.type);
    if (["RemoveChatItemAction", "MarkChatItemAsDeletedAction", "MarkChatItemsByAuthorAsDeletedAction", "RemoveChatItemByAuthorAction"].includes(type)) {
      this.enqueue(() => invoke("youtube_delete", { videoId: this.videoId,
        messageId: text(action.target_item_id) || null,
        userId: text(action.external_channel_id) || null }));
      return;
    }
    if (type !== "AddChatItemAction" && type !== "ReplaceChatItemAction") return;
    const event = normalizeItem(action.item ?? action.replacement_item, this.videoId);
    if (!event?.source?.message_id) return;
    const id = event.source.message_id;
    if (type === "ReplaceChatItemAction") {
      this.seen.delete(id);
      this.enqueue(() => invoke("youtube_delete", { videoId: this.videoId, messageId: text(action.target_item_id), userId: null }));
    }
    if (this.seen.has(id)) return;
    this.seen.add(id);
    if (this.seen.size > 10_000) this.seen.delete(this.seen.values().next().value!);
    this.enqueue(() => invoke("youtube_event", { event, source: event.source }));
  }
  async start() {
    await this.status();
    if (!this.closed) void this.poll();
  }
  // Use the maintained upstream endpoint/parser with our own stoppable loop.
  // Await IPC delivery before polling again so busy chats have backpressure.
  private async poll() {
    if (this.closed) return;
    let delay = 2000;
    try {
      if (!this.continuation) throw new Error("YouTube 未返回评论续传标记");
      const response = await this.info.actions.execute("/live_chat/get_live_chat", { continuation: this.continuation, parse: true });
      if (this.closed) return;
      const contents = record(response.continuation_contents);
      const next = record(contents.continuation);
      if (!next.token) throw new Error("评论连接已结束，正在检查直播状态");
      this.continuation = text(next.token);
      const header = record(contents.header);
      if (contents.header) {
        const menu = record(header.view_selector).sub_menu_items;
        const live = Array.isArray(menu) ? record(menu[1]) : {};
        if (live.continuation && !live.selected) this.continuation = text(live.continuation);
        // Initial response includes old messages, excluded from live metrics.
      } else if (Array.isArray(contents.actions)) {
        for (const action of contents.actions) this.action(action);
        await this.queue;
      }
      if (this.closed) return;
      this.failures = 0;
      if (this.state !== "connected") await this.status("connected", "YouTube 评论已连接（匿名）");
      const timeout = Number(next.timeout_ms ?? next.timeout ?? 2000);
      delay = Number.isFinite(timeout) ? Math.max(1000, Math.min(10000, timeout)) : 2000;
    } catch (e) {
      if (this.closed) return;
      this.failures++;
      await this.status("reconnecting", String(e));
      delay = Math.min(30000, 2000 * 2 ** Math.min(this.failures, 4));
      if (this.failures >= 3) {
        try {
          const { room, info } = await roomInfo(this.videoId, true);
          if (this.closed) return;
          if (!room.live || !room.chat_available || !info) {
            this.consumers.clear();
            await retire(this, "直播已结束或评论已关闭"); return;
          }
          this.info = info; this.continuation = info.livechat?.continuation ?? "";
        } catch { /* Keep bounded retry while the service is unavailable. */ }
      }
    }
    if (!this.closed) this.timer = setTimeout(() => void this.poll(), delay);
  }
  async stop(message = "已断开") {
    this.closed = true; clearTimeout(this.timer);
    await this.queue;
    await this.status("stopped", message);
  }
}

async function connect(input: string, consumer = "ui", signal: AbortSignal) {
  const { room, info } = await roomInfo(input);
  signal.throwIfAborted();
  if (!room.live || !info || !room.video_id) throw new Error(room.upcoming ? "直播尚未开始" : "当前没有正在直播的场次");
  if (!room.chat_available) throw new Error("该直播未开放评论");
  const id = room.video_id;
  await closing.get(id);
  signal.throwIfAborted();
  let conn = active.get(id);
  const isNew = !conn;
  if (!conn) { conn = new Connection(id, info); active.set(id, conn); }
  const added = !conn.consumers.has(consumer);
  conn.consumers.add(consumer);
  if (added) signal.addEventListener("abort", () => { void disconnect(id, consumer); }, { once: true });
  try {
    if (isNew) await conn.start(); else await conn.status();
    signal.throwIfAborted();
    return { ...room, connected: true };
  } catch (e) {
    if (added) await disconnect(id, consumer);
    throw e;
  }
}

async function retire(conn: Connection, message?: string) {
  if (active.get(conn.videoId) !== conn) return;
  active.delete(conn.videoId);
  const stopping = conn.stop(message);
  closing.set(conn.videoId, stopping);
  try { await stopping; } finally { if (closing.get(conn.videoId) === stopping) closing.delete(conn.videoId); }
}

async function disconnect(videoId: string, consumer = "ui") {
  const conn = active.get(videoId);
  if (!conn) return;
  conn.consumers.delete(consumer);
  if (!conn.consumers.size) await retire(conn);
  else await conn.status();
}

let bootstrap: Promise<void> | undefined;
/** Once per app runtime, independent of whichever panel is visible. */
export function startYoutubeBridge(): Promise<void> {
  bootstrap ??= (async () => {
    const requests = new Map<number, AbortController>();
    await listen<number>("youtube://cancel", ({ payload }) => requests.get(payload)?.abort());
    await listen<{ id: number; method: string; args: Record<string, unknown> }>("youtube://request", ({ payload }) => {
      const { id, method, args } = payload;
      const controller = new AbortController();
      requests.set(id, controller);
      void (async () => {
        try {
          let value: unknown;
          switch (method) {
            case "info": value = (await roomInfo(text(args.input), Boolean(args.fresh))).room; break;
            case "connect": value = await connect(text(args.input), text(args.consumer) || "ui", controller.signal); break;
            case "disconnect": value = await disconnect(text(args.video_id), text(args.consumer) || "ui"); break;
            default: throw new Error("不支持的 YouTube 操作");
          }
          const accepted = await invoke<boolean>("youtube_reply", { id, value: value ?? null, error: null });
          if (!accepted) controller.abort();
        } catch (e) { await invoke("youtube_reply", { id, value: null, error: String(e) }); }
        finally { requests.delete(id); }
      })();
    });
    const previous = await invoke<{ video_id: string; title: string; consumers: string[] }[]>("youtube_bridge_ready");
    // A WebView reload must restore recording consumers as well as previews.
    for (const entry of Array.isArray(previous) ? previous : []) {
      if (!entry.consumers?.length) continue;
      void (async () => {
        try {
          for (const consumer of entry.consumers) await connect(entry.video_id, consumer, new AbortController().signal);
        } catch (e) {
          await invoke("youtube_connection", { videoId: entry.video_id, status: { ...entry, state: "error", message: `评论恢复失败：${String(e)}` } });
        }
      })();
    }
  })();
  return bootstrap;
}
