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
}

function guardName(level: number): string {
  return ["", "总督", "提督", "舰长"][level] ?? "";
}

function rowKey(ev: LiveEvent, i: number): string {
  return `${i}-${ev.kind}`;
}

export function DanmakuRow({
  ev,
  theme,
  translated,
}: {
  ev: LiveEvent;
  theme: DanmakuTheme;
  translated?: string;
}) {
  const tr = translated ? (
    <span className="dm-translated" data-testid="dm-translated">
      {translated}
    </span>
  ) : null;
  switch (ev.kind) {
    case "danmaku":
      return (
        <div className={`dm-row dm-anim-${theme.animation}`} data-testid="dm-row">
          {theme.showTime && (
            <span className="dm-time">
              {new Date(ev.timestamp).toLocaleTimeString()}
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
          <span className="dm-username">{ev.username}:</span>
          {ev.emoticon ? (
            <img className="dm-emoticon" src={ev.emoticon} alt={ev.text} />
          ) : (
            <span className="dm-text">{ev.text}</span>
          )}
          {tr}
        </div>
      );
    case "super_chat":
      return (
        <div className={`dm-row dm-sc dm-anim-${theme.animation}`} data-testid="dm-sc">
          <span className="dm-sc-price">¥{ev.price}</span>
          <span className="dm-username">{ev.username}</span>
          <span className="dm-text">{ev.text}</span>
          {tr}
        </div>
      );
    case "gift":
      return (
        <div className={`dm-row dm-gift dm-anim-${theme.animation}`} data-testid="dm-gift">
          <span className="dm-username">{ev.username}</span>
          <span className="dm-text">
            投喂 {ev.gift_name} ×{ev.count}
            {ev.total_price > 0 ? `（¥${ev.total_price.toFixed(1)}）` : ""}
          </span>
        </div>
      );
    case "guard_buy":
      return (
        <div className={`dm-row dm-guard dm-anim-${theme.animation}`} data-testid="dm-guard">
          <span className="dm-username">{ev.username}</span>
          <span className="dm-text">
            开通 {guardName(ev.guard_level)} ×{ev.count}
          </span>
        </div>
      );
    case "enter":
      return (
        <div className="dm-row dm-enter" data-testid="dm-enter">
          <span className="dm-text">{ev.username} 进入直播间</span>
        </div>
      );
    default:
      return null;
  }
}

export default function DanmakuList({
  events,
  theme,
  maxRows = 200,
  translations,
}: DanmakuListProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const shown = events.slice(-maxRows);

  useEffect(() => {
    bottomRef.current?.scrollIntoView?.({ behavior: "auto" });
  }, [events.length]);

  return (
    <div
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
        />
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
