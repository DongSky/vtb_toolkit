import { getLocale } from "../i18n";
import { t, useLocale } from "../i18n";
import { useEffect, useRef } from "react";
import type { LiveEvent } from "../types";
import type { DanmakuTheme } from "../themes";

export interface DanmakuListProps {
  events: LiveEvent[];
  theme: DanmakuTheme;
  /** Cap rendered rows (older rows dropped). */
  maxRows?: number;
  /** Async translations keyed by event tid (弹幕自动翻译). */
  translations?: Record<number, string>;
  /** Viewer 备注 tags, keyed by note_key (uid:N / name:U). */
  notes?: Record<string, string>;
  /** Click a username → edit that viewer's note. */
  onUserClick?: (uid: number, username: string, userKey?: string) => void;
}

/** Matches the backend's note_key (vtb-stats::note_key). */
export function noteKey(uid: number, username: string, source?: LiveEvent["source"]): string {
  if (source?.user_id) return `${source.platform}:${source.user_id}`;
  return uid !== 0 ? `uid:${uid}` : `name:${username}`;
}

function guardName(level: number): string {
  return t(["", "总督", "提督", "舰长"][level] ?? "");
}

function rowKey(ev: LiveEvent, i: number): string {
  return `${i}-${ev.kind}`;
}

export function DanmakuRow({
  ev,
  theme,
  translated,
  note,
  onUserClick,
}: {
  ev: LiveEvent;
  theme: DanmakuTheme;
  translated?: string;
  note?: string;
  onUserClick?: (uid: number, username: string, userKey?: string) => void;
}) {
  useLocale();
  const tr = translated ? (
    <span className="dm-translated" data-testid="dm-translated">
      {translated}
    </span>
  ) : null;
  const noteTag = note ? (
    <span className="dm-note" data-testid="dm-note" title={note}>
      {note}
    </span>
  ) : null;
  const userProps = (uid: number, username: string) =>
    onUserClick
      ? {
          onClick: () => ev.source?.user_id ? onUserClick(uid, username, `${ev.source.platform}:${ev.source.user_id}`) : onUserClick(uid, username),
          style: { cursor: "pointer" } as React.CSSProperties,
          title: t("点击编辑备注"),
        }
      : {};
  const identity = ev.source ? <><span className="dm-platform">{ev.source.platform === "youtube" ? "YouTube" : ev.source.platform}</span>{ev.source.avatar_url && <img src={ev.source.avatar_url} alt="" width={24} height={24} style={{ borderRadius: "50%" }} />}</> : null;
  switch (ev.kind) {
    case "danmaku":
      return (
        <div className={`dm-row dm-anim-${theme.animation}`} data-testid="dm-row">
          {identity}
          {theme.showTime && (
            <span className="dm-time">
              {new Date(ev.timestamp).toLocaleTimeString(getLocale())}
            </span>
          )}
          {theme.showMedal && ev.medal && (
            <span className="dm-medal">
              {ev.medal.name}·{ev.medal.level}
            </span>
          )}
          {ev.guard_level > 0 && (
            <span className="dm-guard-tag">{guardName(ev.guard_level)}</span>
          )}
          {noteTag}
          <span className="dm-username" {...userProps(ev.uid, ev.username)}>
            {ev.username}:
          </span>
          {ev.emoticon ? (
            <img className="dm-emoticon" src={ev.emoticon} alt={ev.text} />
          ) : (
            <span className="dm-text"><MessageContent ev={ev} /></span>
          )}
          {tr}
        </div>
      );
    case "super_chat":
      return (
        <div className={`dm-row dm-sc dm-anim-${theme.animation}`} data-testid="dm-sc">
          <span className="dm-sc-price">{ev.source?.money?.display ?? `¥${ev.price}`}</span>
          {identity}{noteTag}
          {ev.source?.sticker_url && <img src={ev.source.sticker_url} alt="Super Sticker" width={64} height={64} />}
          <span className="dm-username" {...userProps(ev.uid, ev.username)}>{ev.username}</span>
          <span className="dm-text"><MessageContent ev={ev} /></span>
          {tr}
        </div>
      );
    case "gift":
      return (
        <div className={`dm-row dm-gift dm-anim-${theme.animation}`} data-testid="dm-gift">
          {identity}{noteTag}
          <span className="dm-username" {...userProps(ev.uid, ev.username)}>{ev.username}</span>
          <span className="dm-text">
            {ev.source ? ev.gift_name : t("投喂 {0} ×{1}", ev.gift_name, ev.count)}
            {ev.total_price > 0 ? `（¥${ev.total_price.toFixed(1)}）` : ""}
          </span>
        </div>
      );
    case "guard_buy":
      return (
        <div className={`dm-row dm-guard dm-anim-${theme.animation}`} data-testid="dm-guard">
          {identity}{noteTag}
          <span className="dm-username" {...userProps(ev.uid, ev.username)}>{ev.username}</span>
          <span className="dm-text">
            {ev.source?.membership ?? t("开通 {0} ×{1}", guardName(ev.guard_level), ev.count)}
          </span>
        </div>
      );
    case "enter":
      return (
        <div className="dm-row dm-enter" data-testid="dm-enter">
          <span className="dm-text">{ev.username} {t("进入直播间")}</span>
        </div>
      );
    default:
      return null;
  }
}

function MessageContent({ ev }: { ev: LiveEvent & { text: string } }) {
  useLocale();
  return <>{ev.source?.message_runs?.length ? ev.source.message_runs.map((run, index) =>
    run.emoji_url?.startsWith("https://") ? <img key={index} src={run.emoji_url} alt={run.text} title={run.text} style={{ width: "1.4em", height: "1.4em", verticalAlign: "middle" }} /> : run.text,
  ) : ev.text}</>;
}

export default function DanmakuList({
  events,
  theme,
  maxRows = 200,
  translations,
  notes,
  onUserClick,
}: DanmakuListProps) {
  useLocale();
  const listRef = useRef<HTMLDivElement>(null);
  const shown = events.slice(-maxRows);

  useEffect(() => {
    const list = listRef.current;
    if (list) list.scrollTop = list.scrollHeight;
  }, [events, translations]);

  return (
    <div
      ref={listRef}
      className="dm-list"
      data-testid="dm-list"
      style={theme.vars as React.CSSProperties}
    >
      {shown.map((ev, i) => (
        <DanmakuRow
          key={rowKey(ev, i)}
          ev={ev}
          theme={theme}
          translated={
            ev.tid !== undefined ? translations?.[ev.tid] : undefined
          }
          note={
            "username" in ev && "uid" in ev
              ? notes?.[noteKey(ev.uid, ev.username, ev.source)]
              : undefined
          }
          onUserClick={onUserClick}
        />
      ))}
    </div>
  );
}
