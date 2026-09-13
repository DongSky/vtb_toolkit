// TypeScript mirrors of the Rust vtb-common types (serde snake_case tagged).

export interface EventSource {
  platform: "bilibili" | "youtube" | "twitch";
  room_id: string;
  user_id?: string | null;
  message_id?: string | null;
  avatar_url?: string | null;
  money?: { currency: string | null; amount: number | null; display: string } | null;
  membership?: string | null;
  sticker_url?: string | null;
  event_type?: string | null;
  message_runs?: { text: string; emoji_url?: string | null }[];
}

export function roomKey(event: LiveEvent): string {
  return event.source ? `${event.source.platform}:${event.source.room_id}` : `bilibili:${event.room_id}`;
}

export interface ChatDelete {
  room_key: string;
  message_id?: string | null;
  user_id?: string | null;
}

export function isDeleted(event: LiveEvent, deletion: ChatDelete): boolean {
  return roomKey(event) === deletion.room_key && Boolean(
    (deletion.message_id && event.source?.message_id === deletion.message_id) ||
    (deletion.user_id && event.source?.user_id === deletion.user_id),
  );
}

export interface Medal {
  name: string;
  level: number;
  anchor_room_id: number;
}

export interface DanmakuMsg {
  room_id: number;
  uid: number;
  username: string;
  text: string;
  timestamp: string;
  medal: Medal | null;
  guard_level: number;
  is_admin: boolean;
  emoticon: string | null;
}

export interface SuperChatMsg {
  room_id: number;
  uid: number;
  username: string;
  text: string;
  price: number;
  timestamp: string;
  duration_secs: number;
}

export interface GiftMsg {
  room_id: number;
  uid: number;
  username: string;
  gift_name: string;
  gift_id: number;
  count: number;
  total_price: number;
  timestamp: string;
}

export interface GuardBuyMsg {
  room_id: number;
  uid: number;
  username: string;
  guard_level: number;
  count: number;
  price: number;
  timestamp: string;
}

export interface EnterMsg {
  room_id: number;
  uid: number;
  username: string;
  timestamp: string;
}

export type LiveEvent = (
  | ({ kind: "danmaku" } & DanmakuMsg)
  | ({ kind: "super_chat" } & SuperChatMsg)
  | ({ kind: "gift" } & GiftMsg)
  | ({ kind: "guard_buy" } & GuardBuyMsg)
  | ({ kind: "enter" } & EnterMsg)
  | ({ kind: "like" } & EnterMsg)
  | { kind: "live_start"; room_id: number; timestamp: string }
  | { kind: "live_end"; room_id: number; timestamp: string }
  | { kind: "watched_change"; room_id: number; count: number }
) & {
  /** Translation correlation id (present when 弹幕自动翻译 is running). */
  tid?: number;
  source?: EventSource | null;
};

export interface DanmakuTranslationPayload {
  tid: number;
  translated: string;
}

export interface RecorderEventPayload {
  kind: "started" | "stopped" | "error";
  room_id: number;
  output_dir?: string;
  total_bytes?: number;
  segments?: number;
  message?: string;
}

export interface JobProgressPayload {
  job_id: string;
  stage: string;
  fraction: number | null;
  message: string;
}

export interface JobDonePayload {
  job_id: string;
  ok: boolean;
  message: string;
  transcript_segments: number;
  translations: number;
  highlights: number;
  clips: number;
}
