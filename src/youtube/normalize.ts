import type { EventSource, LiveEvent } from "../types";

export function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" ? value as Record<string, unknown> : {};
}
export function text(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "string") return value;
  const runs = record(value).runs;
  if (Array.isArray(runs)) return runs.map((value) => {
    const run = record(value), emoji = record(run.emoji);
    if (emoji.is_custom) {
      const shortcuts = emoji.shortcuts;
      return Array.isArray(shortcuts) && shortcuts[0] ? String(shortcuts[0]) : "[表情]";
    }
    return String(run.text ?? "");
  }).join("");
  const result = String(value);
  return result === "[object Object]" ? "" : result;
}
function image(value: unknown): string | null {
  const list = Array.isArray(value) ? value : record(value).thumbnails;
  const url = Array.isArray(list) ? text(record(list[list.length - 1]).url) : "";
  return /^https:\/\//.test(url) ? url : null;
}

/** Keep ambiguous $/¥ as display-only currency, never assume CNY or USD. */
export function parseMoney(display: string): NonNullable<EventSource["money"]> {
  const currencies: [RegExp, string][] = [
    [/US\$|\bUSD\b/, "USD"], [/HK\$|\bHKD\b/, "HKD"], [/NT\$|\bTWD\b/, "TWD"],
    [/CA\$|\bCAD\b/, "CAD"], [/A\$|AU\$|\bAUD\b/, "AUD"], [/NZ\$|\bNZD\b/, "NZD"],
    [/S\$|SG\$|\bSGD\b/, "SGD"], [/MX\$|\bMXN\b/, "MXN"], [/JP¥|\bJPY\b/, "JPY"],
    [/CN¥|\bCNY\b|\bRMB\b/, "CNY"], [/€|\bEUR\b/, "EUR"], [/£|\bGBP\b/, "GBP"],
    [/₹|\bINR\b/, "INR"], [/₩|\bKRW\b/, "KRW"], [/\bBRL\b|R\$/, "BRL"],
  ];
  const currency = currencies.find(([re]) => re.test(display))?.[1] ?? null;
  const raw = display.match(/\d[\d.,\s\u00a0]*/)?.[0].replace(/\s/g, "").replace(/[.,]$/, "");
  let amount: number | null = null;
  if (raw) {
    const comma = raw.lastIndexOf(",");
    const dot = raw.lastIndexOf(".");
    let n = raw;
    if (comma >= 0 && dot >= 0) n = comma > dot ? raw.replace(/\./g, "").replace(",", ".") : raw.replace(/,/g, "");
    else if (comma >= 0) n = /,\d{1,2}$/.test(raw) ? raw.replace(",", ".") : raw.replace(/,/g, "");
    const parsed = Number(n);
    if (Number.isFinite(parsed) && parsed >= 0) amount = parsed;
  }
  return { display, currency, amount };
}

/** Adapt upstream YTNodes without serializing their internal endpoint objects. */
export function normalizeItem(value: unknown, videoId: string): LiveEvent | null {
  const item = record(value);
  const type = text(item.type);
  const header = record(item.header);
  const author = item.author ? record(item.author) : { id: item.author_external_channel_id, name: header.author_name, thumbnails: header.author_photo };
  const id = text(item.id);
  if (!id) return null;
  const source: EventSource = {
    platform: "youtube", room_id: videoId, message_id: id,
    user_id: text(author.id) || null, avatar_url: image(author.thumbnails), event_type: type,
  };
  const millis = typeof item.timestamp === "number" ? item.timestamp : Number(item.timestamp_usec) / 1000;
  const timestamp = Number.isFinite(millis) && millis > 0 ? new Date(millis).toISOString() : new Date().toISOString();
  const base = { room_id: 0, uid: 0, username: text(author.name) || "YouTube 观众", timestamp, source };
  const message = text(item.message);
  const runs = record(item.message).runs;
  if (Array.isArray(runs) && runs.some((run) => record(record(run).emoji).is_custom)) {
    source.message_runs = runs.map((value) => {
      const run = record(value), emoji = record(run.emoji);
      return { text: emoji.is_custom ? (Array.isArray(emoji.shortcuts) ? String(emoji.shortcuts[0] || "[表情]") : "[表情]") : String(run.text ?? ""),
        emoji_url: emoji.is_custom ? image(emoji.image) : null };
    });
  }
  switch (type) {
    case "LiveChatTextMessage":
      return { ...base, kind: "danmaku", text: message, medal: null, guard_level: 0,
        is_admin: Boolean(author.is_moderator || author.is_creator || (Array.isArray(author.badges) && author.badges.some((b) => record(b).icon_type === "OWNER"))), emoticon: null };
    case "LiveChatPaidMessage":
    case "LiveChatPaidSticker": {
      source.money = parseMoney(text(item.purchase_amount));
      if (type === "LiveChatPaidSticker") source.sticker_url = image(item.sticker);
      return { ...base, kind: "super_chat", text: message || (type === "LiveChatPaidSticker" ? "Super Sticker" : ""),
        price: source.money.amount ?? 0, duration_secs: 0 };
    }
    case "LiveChatMembershipItem":
    case "LiveChatSponsorshipsGiftRedemptionAnnouncement": {
      source.membership = text(item.header_subtext) || text(item.header_primary_text) || message || "频道会员";
      return { ...base, kind: "guard_buy", guard_level: 0, count: 1, price: 0 };
    }
    case "LiveChatSponsorshipsGiftPurchaseAnnouncement": {
      const label = text(header.primary_text) || "赠送频道会员";
      source.membership = label;
      // Count is only trusted when the upstream parser exposes a numeric field.
      const count = typeof item.gift_count === "number" ? item.gift_count : Number(label.match(/gifted\s+(\d+)\s+/i)?.[1] ?? 1);
      return { ...base, kind: "gift", gift_id: 0, gift_name: label, count, total_price: 0 };
    }
    default: return null;
  }
}

export function youtubeInput(input: string): string {
  const value = input.trim();
  if (/^[\w-]{11}$/.test(value)) return `https://www.youtube.com/watch?v=${value}`;
  const url = new URL(value.startsWith("@") ? `https://www.youtube.com/${value}/live` : value);
  if (url.protocol !== "https:" || !["www.youtube.com", "youtube.com", "m.youtube.com", "youtu.be"].includes(url.hostname)
    || url.username || url.password || url.port) throw new Error("请输入 YouTube 直播/频道链接或 11 位视频 ID");
  if (url.hostname === "youtu.be") {
    const id = url.pathname.slice(1).split("/")[0];
    if (!/^[\w-]{11}$/.test(id)) throw new Error("无效的 YouTube 视频 ID");
    return `https://www.youtube.com/watch?v=${id}`;
  }
  const videoId = url.searchParams.get("v") ?? url.pathname.match(/^\/(?:live|shorts|embed)\/([\w-]{11})\/?$/)?.[1];
  if (videoId) {
    if (!/^[\w-]{11}$/.test(videoId)) throw new Error("无效的 YouTube 视频 ID");
    return `https://www.youtube.com/watch?v=${videoId}`;
  }
  if (!/^\/(?:@[^/]+|channel\/UC[\w-]+|c\/[^/]+|user\/[^/]+)(?:\/(?:live|streams))?\/?$/.test(url.pathname)) throw new Error("不支持的 YouTube 链接");
  return `https://www.youtube.com${url.pathname.replace(/\/(?:live|streams)\/?$/, "").replace(/\/$/, "")}/live`;
}

/** YouTube now uses LockupView in channel feeds alongside legacy Video nodes. */
export function liveVideoId(value: unknown): string | null {
  const node = record(value);
  const image = record(node.content_image);
  const overlays = Array.isArray(image.overlays) ? image.overlays : [];
  const badges = overlays.flatMap((o) => Array.isArray(record(o).badges) ? record(o).badges as unknown[] : []);
  const live = node.is_live || badges.some((b) => {
    const badge = record(b);
    return text(badge.badge_style).includes("LIVE") || text(badge.icon_name) === "LIVE" || text(badge.text) === "LIVE";
  });
  const id = text(node.id || node.content_id);
  return live && /^[\w-]{11}$/.test(id) ? id : null;
}
