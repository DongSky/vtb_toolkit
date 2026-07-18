import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { usePersisted } from "../hooks/usePersisted";
import WordCloud from "./WordCloud";

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

    // Selection overlay.
    if (sel) {
      ctx.fillStyle = "rgba(94,156,230,0.3)";
      const x0 = (sel.start / curve.total_ms) * W;
      const x1 = (sel.end / curve.total_ms) * W;
      ctx.fillRect(Math.min(x0, x1), 0, Math.abs(x1 - x0), H);
    }
  }, [data, sel]);

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
            红=弹幕密度 蓝=音频能量 橙色块=检出高能片段。点击查看时间点，拖选后可导出切片。
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
          <ul data-testid="review-highlights">
            {data.highlights.map((h, i) => (
              <li key={i}>
                [{fmt(h.start_ms)}-{fmt(h.end_ms)}] 分数 {h.score.toFixed(2)} — {h.title ?? h.reason}
              </li>
            ))}
          </ul>
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
