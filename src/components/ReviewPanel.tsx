import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { usePersisted } from "../hooks/usePersisted";
import WordCloud from "./WordCloud";
import CoverPanel from "./CoverPanel";

interface Curve {
  window_ms: number;
  total_ms: number;
  density: number[] | null;
  keyword: number[] | null;
  gift: number[] | null;
  audio: number[] | null;
}

interface HighlightItem {
  start_ms: number;
  end_ms: number;
  score: number;
  reason: string;
  title: string | null;
}

interface ReviewData {
  signals: Curve | null;
  highlights: HighlightItem[];
  clips: string[];
  video: string | null;
}

interface TimelineMarker {
  at_ms: number;
  kind: "auto" | "manual";
  source: string;
  note: string | null;
}

interface TimelineEvent {
  at_ms: number;
  kind: string;
  label: string;
}

interface TimelineData {
  markers: TimelineMarker[];
  events: TimelineEvent[];
}

function fmt(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** Normalize an array to 0..1 (max-based). */
function norm(v: number[] | null): number[] {
  if (!v || v.length === 0) return [];
  const max = Math.max(...v, 1e-9);
  return v.map((x) => x / max);
}

export default function ReviewPanel() {
  const [dir, setDir] = usePersisted("review.dir", "");
  const [data, setData] = useState<ReviewData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sel, setSel] = useState<{ start: number; end: number } | null>(null);
  const [cursor, setCursor] = useState<number | null>(null);
  const [exportMsg, setExportMsg] = useState<string | null>(null);
  const [report, setReport] = useState<Record<string, unknown> | null>(null);
  const [reportMsg, setReportMsg] = useState<string | null>(null);
  const [uploadMsg, setUploadMsg] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [timeline, setTimeline] = useState<TimelineData | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const dragStart = useRef<number | null>(null);

  const load = async () => {
    setError(null);
    setData(null);
    setSel(null);
    try {
      setData(await invoke<ReviewData>("review_load", { dir }));
      try {
        setReport(
          (await invoke<Record<string, unknown>>("stats_report", {
            sessionDir: dir,
          })) ?? null,
        );
      } catch {
        setReport(null);
      }
      try {
        setTimeline(await invoke<TimelineData>("marker_timeline", { dir }));
      } catch {
        setTimeline(null);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const uploadClip = async (file: string) => {
    setUploadMsg("上传中…");
    try {
      const r = await invoke<{ ok: boolean; output: string }>("biliup_upload", {
        file,
        title: `【切片】${file.split("/").pop()}`,
        tags: ["虚拟主播", "切片"],
      });
      setUploadMsg(r.ok ? "投稿成功" : `投稿失败: ${r.output.slice(0, 300)}`);
    } catch (e) {
      setUploadMsg(String(e));
    }
  };

  const exportReport = async () => {
    try {
      const files = await invoke<string[]>("stats_export", { sessionDir: dir });
      setReportMsg(`已导出: ${files.join(", ")}`);
    } catch (e) {
      setReportMsg(String(e));
    }
  };

  // Draw the 高能进度条 whenever data/selection change.
  useEffect(() => {
    const canvas = canvasRef.current;
    const curve = data?.signals;
    if (!canvas || !curve) return;
    const ctx = canvas.getContext?.("2d");
    if (!ctx) return;
    const W = canvas.width;
    const H = canvas.height;
    ctx.clearRect(0, 0, W, H);

    // Highlight regions (behind curves).
    ctx.fillStyle = "rgba(255,140,0,0.25)";
    for (const h of data!.highlights) {
      const x0 = (h.start_ms / curve.total_ms) * W;
      const x1 = (h.end_ms / curve.total_ms) * W;
      ctx.fillRect(x0, 0, Math.max(x1 - x0, 2), H);
    }

    const drawCurve = (values: number[], color: string) => {
      if (!values.length) return;
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      values.forEach((v, i) => {
        const x = ((i + 0.5) / values.length) * W;
        const y = H - v * (H - 4) - 2;
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      });
      ctx.stroke();
    };
    drawCurve(norm(curve.density), "#e53935"); // 弹幕密度: 红
    drawCurve(norm(curve.audio), "#1e88e5"); // 音频能量: 蓝

    // 打点 + SC/舰长/开播 tick marks (full-height thin lines + top dots).
    const drawTick = (ms: number, color: string) => {
      const x = (ms / curve.total_ms) * W;
      ctx.strokeStyle = color;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, H);
      ctx.stroke();
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.arc(x, 5, 3, 0, Math.PI * 2);
      ctx.fill();
    };
    for (const ev of timeline?.events ?? []) {
      drawTick(
        ev.at_ms,
        ev.kind === "super_chat"
          ? "#fdd835" // SC: 黄
          : ev.kind === "guard_buy"
            ? "#8e24aa" // 舰长: 紫
            : "#9e9e9e", // 开播/下播: 灰
      );
    }
    for (const m of timeline?.markers ?? []) {
      drawTick(m.at_ms, m.kind === "manual" ? "#00c853" : "#ff8f00"); // 手动: 绿, 自动: 橙
    }

    // Selection overlay.
    if (sel) {
      ctx.fillStyle = "rgba(94,156,230,0.3)";
      const x0 = (sel.start / curve.total_ms) * W;
      const x1 = (sel.end / curve.total_ms) * W;
      ctx.fillRect(Math.min(x0, x1), 0, Math.abs(x1 - x0), H);
    }
  }, [data, sel, timeline]);

  const posFromEvent = (e: React.MouseEvent): number | null => {
    const canvas = canvasRef.current;
    const curve = data?.signals;
    if (!canvas || !curve) return null;
    const rect = canvas.getBoundingClientRect();
    const frac = (e.clientX - rect.left) / rect.width;
    return Math.round(Math.max(0, Math.min(1, frac)) * curve.total_ms);
  };

  const exportSel = async () => {
    if (!sel || !data?.video) return;
    setExportMsg(null);
    try {
      const out = await invoke<string>("clip_export", {
        input: data.video,
        outputDir: `${dir}/clips`,
        startMs: Math.min(sel.start, sel.end),
        endMs: Math.max(sel.start, sel.end),
      });
      setExportMsg(`已导出: ${out}`);
      load();
    } catch (e) {
      setExportMsg(`导出失败: ${e}`);
    }
  };

  // Built-in preview player: play a recording/clip in-app via the asset
  // protocol (no external player round-trip).
  const openPreview = (path: string) => {
    setPreview((prev) => {
      const src = convertFileSrc(path);
      return prev === src ? null : src;
    });
  };

  // Clicking the timeline while the main video is open seeks the player.
  useEffect(() => {
    if (
      cursor != null &&
      videoRef.current &&
      data?.video &&
      preview === convertFileSrc(data.video)
    ) {
      videoRef.current.currentTime = cursor / 1000;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cursor]);

  return (
    <div className="panel" data-testid="review-panel">
      <h2>录播复盘（高能进度条）</h2>
      <div className="form-row">
        <input
          data-testid="review-dir"
          placeholder="离线处理输出目录（含 signals.json）"
          value={dir}
          onChange={(e) => setDir(e.target.value)}
          style={{ flex: 1 }}
        />
        <button data-testid="review-load" disabled={!dir} onClick={load}>
          加载
        </button>
      </div>
      {error && (
        <div className="error" data-testid="review-error">
          {error}
        </div>
      )}
      {data?.signals && (
        <>
          <p className="login-note">
            红=弹幕密度 蓝=音频能量 橙色块=检出高能片段 绿线=手动打点 橙线=自动打点
            黄线=SC 紫线=舰长。点击查看时间点，拖选后可导出切片。
          </p>
          <canvas
            ref={canvasRef}
            data-testid="review-canvas"
            width={880}
            height={140}
            style={{ width: "100%", border: "1px solid #8884", borderRadius: 6, cursor: "crosshair" }}
            onMouseDown={(e) => {
              dragStart.current = posFromEvent(e);
            }}
            onMouseUp={(e) => {
              const end = posFromEvent(e);
              const start = dragStart.current;
              dragStart.current = null;
              if (start == null || end == null) return;
              if (Math.abs(end - start) < 500) {
                setCursor(end);
                setSel(null);
              } else {
                setSel({ start, end });
              }
            }}
          />
          <div className="form-row" data-testid="review-readout">
            {cursor != null && <span>时间点: {fmt(cursor)}</span>}
            {sel && (
              <>
                <span data-testid="review-selection">
                  选区: {fmt(Math.min(sel.start, sel.end))} - {fmt(Math.max(sel.start, sel.end))}
                </span>
                <button data-testid="review-export" disabled={!data.video} onClick={exportSel}>
                  导出选区切片
                </button>
              </>
            )}
          </div>
          {exportMsg && <div data-testid="review-export-msg">{exportMsg}</div>}
          {data.video && (
            <div className="form-row">
              <button
                data-testid="review-preview-video"
                onClick={() => openPreview(data.video!)}
              >
                {preview === convertFileSrc(data.video)
                  ? "关闭预览"
                  : "预览录播"}
              </button>
              <button
                data-testid="review-proxy"
                onClick={async () => {
                  setExportMsg("代理副本生成中（重编码，需要一些时间）…");
                  try {
                    const out = await invoke<string>("proxy_generate", {
                      input: data.video,
                    });
                    setExportMsg(`代理副本已生成: ${out}`);
                  } catch (e) {
                    setExportMsg(`代理生成失败: ${e}`);
                  }
                }}
              >
                生成 720p 代理副本
              </button>
            </div>
          )}
          {preview && (
            <video
              ref={videoRef}
              data-testid="review-player"
              src={preview}
              controls
              style={{ width: "100%", borderRadius: 6, background: "#000" }}
            />
          )}
          <h3>高能片段（{data.highlights.length}）</h3>
          {(data.highlights.length > 0 ||
            (timeline?.markers.length ?? 0) > 0) && (
            <div className="form-row">
              {data.highlights.length > 0 && (
                <button
                  data-testid="review-edl-export"
                  onClick={async () => {
                    try {
                      const out = await invoke<string>("edl_export", { dir });
                      setExportMsg(`EDL 已导出: ${out}`);
                    } catch (e) {
                      setExportMsg(`EDL 导出失败: ${e}`);
                    }
                  }}
                >
                  导出剪辑工程 (EDL)
                </button>
              )}
              <button
                data-testid="review-ts-export"
                onClick={async () => {
                  try {
                    const out = await invoke<string>("timestamps_export", { dir });
                    setExportMsg(`时间戳清单已导出: ${out}`);
                  } catch (e) {
                    setExportMsg(`时间戳导出失败: ${e}`);
                  }
                }}
              >
                导出时间戳清单
              </button>
              {data.highlights.length > 0 && (
                <>
                  <button
                    data-testid="review-fcpxml-export"
                    onClick={async () => {
                      try {
                        const out = await invoke<string>("fcpxml_export", { dir });
                        setExportMsg(`FCPXML 已导出: ${out}`);
                      } catch (e) {
                        setExportMsg(`FCPXML 导出失败: ${e}`);
                      }
                    }}
                  >
                    导出 FCPXML (达芬奇/FCP)
                  </button>
                  <button
                    data-testid="review-jianying-export"
                    onClick={async () => {
                      try {
                        const out = await invoke<string>("jianying_export", { dir });
                        setExportMsg(`剪映草稿已导出: ${out}（复制到剪映草稿目录）`);
                      } catch (e) {
                        setExportMsg(`剪映导出失败: ${e}`);
                      }
                    }}
                  >
                    导出剪映草稿
                  </button>
                  <button
                    data-testid="review-bundle-export"
                    onClick={async () => {
                      try {
                        const r = await invoke<{ dir: string; files: string[] }>(
                          "asset_bundle_export",
                          { dir },
                        );
                        setExportMsg(`素材包已导出 (${r.files.length} 个文件): ${r.dir}`);
                      } catch (e) {
                        setExportMsg(`素材包导出失败: ${e}`);
                      }
                    }}
                  >
                    导出素材包
                  </button>
                </>
              )}
            </div>
          )}
          <ul data-testid="review-highlights">
            {data.highlights.map((h, i) => (
              <li key={i}>
                [{fmt(h.start_ms)}-{fmt(h.end_ms)}] 分数 {h.score.toFixed(2)} — {h.title ?? h.reason}
              </li>
            ))}
          </ul>
          {timeline &&
            (timeline.markers.length > 0 || timeline.events.length > 0) && (
              <>
                <h3>
                  打点与事件（{timeline.markers.length + timeline.events.length}）
                </h3>
                <ul data-testid="review-markers">
                  {[
                    ...timeline.markers.map((m) => ({
                      at_ms: m.at_ms,
                      label: `${m.kind === "manual" ? "⏱ 打点" : "⚡ 自动"}${m.note ? ` ${m.note}` : ""}`,
                    })),
                    ...timeline.events.map((ev) => ({
                      at_ms: ev.at_ms,
                      label:
                        ev.kind === "super_chat"
                          ? `💰 ${ev.label}`
                          : ev.kind === "guard_buy"
                            ? `⚓ ${ev.label}`
                            : `● ${ev.label}`,
                    })),
                  ]
                    .sort((a, b) => a.at_ms - b.at_ms)
                    .map((item, i) => (
                      <li
                        key={i}
                        style={{ cursor: "pointer", fontSize: 13 }}
                        onClick={() => setCursor(item.at_ms)}
                      >
                        [{fmt(item.at_ms)}] {item.label}
                      </li>
                    ))}
                </ul>
              </>
            )}
          {report != null && (report.danmaku_count as number) > 0 && (
            <div data-testid="review-report">
              <h3>场次报告</h3>
              <p>
                弹幕 {String(report.danmaku_count)} 条 / 独立发言 {String(report.unique_chatters)} 人 /
                SC ¥{Number(report.sc_total ?? 0).toFixed(1)} / 礼物 ¥{Number(report.gift_total ?? 0).toFixed(1)} /
                总营收 ¥{Number(report.revenue_total ?? 0).toFixed(1)}
              </p>
              <button data-testid="review-report-export" onClick={exportReport}>
                导出报告 (Markdown + SC CSV)
              </button>
              {reportMsg && <p style={{ fontSize: 12 }}>{reportMsg}</p>}
              {Array.isArray(report.word_freq) &&
                (report.word_freq as [string, number][]).length > 0 && (
                  <>
                    <h4>弹幕热词</h4>
                    <WordCloud words={report.word_freq as [string, number][]} />
                  </>
                )}
              {Array.isArray(report.top_chatters) &&
                (report.top_chatters as [string, number][]).length > 0 && (
                  <>
                    <h4>发言榜</h4>
                    <ol data-testid="review-top-chatters" className="top-chatters">
                      {(report.top_chatters as [string, number][])
                        .slice(0, 10)
                        .map(([name, n]) => (
                          <li key={name}>
                            {name} <span className="hint">{n}</span>
                          </li>
                        ))}
                    </ol>
                  </>
                )}
            </div>
          )}
          {uploadMsg && <div data-testid="upload-msg">{uploadMsg}</div>}
          <CoverPanel dir={dir} />
          <h3>已有切片（{data.clips.length}）</h3>
          <ul>
            {data.clips.map((c) => (
              <li key={c} style={{ fontSize: 12 }}>
                {c}{" "}
                <button
                  data-testid="clip-preview"
                  style={{ fontSize: 11, padding: "1px 6px" }}
                  onClick={() => openPreview(c)}
                >
                  预览
                </button>{" "}
                <button
                  data-testid={`clip-upload`}
                  style={{ fontSize: 11, padding: "1px 6px" }}
                  onClick={() => uploadClip(c)}
                >
                  投稿
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
