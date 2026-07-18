import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Entry {
  term: string;
  aliases?: string[];
  reading?: string | null;
  translation?: string | null;
  category?: string | null;
  note?: string | null;
}

interface Table {
  name: string;
  description: string;
  entries: Entry[];
  updated_at: string;
}

interface Summary {
  name: string;
  description: string;
  entries: number;
  updated_at: string;
}

export default function HotwordsPanel() {
  const [tables, setTables] = useState<Summary[]>([]);
  const [current, setCurrent] = useState<Table | null>(null);
  const [name, setName] = useState("");
  const [raw, setRaw] = useState("");
  const [useLlm, setUseLlm] = useState(true);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mergeSel, setMergeSel] = useState<string[]>([]);
  const [mergeName, setMergeName] = useState("");

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<Summary[]>("hotwords_list");
      setTables(Array.isArray(list) ? list : []);
    } catch {
      /* browser dev */
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const saveRaw = async () => {
    setError(null);
    setMsg(null);
    setBusy(true);
    try {
      const r = await invoke<{ table: Table; used_llm: boolean }>(
        "hotwords_save_raw",
        { name, raw, description: null, useLlm },
      );
      setMsg(
        `已保存「${r.table.name}」，${r.table.entries.length} 个词条` +
          (useLlm ? (r.used_llm ? "（AI 规整）" : "（AI 不可用，已用规则解析）") : "（规则解析）"),
      );
      setCurrent(r.table);
      setRaw("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const openTable = async (n: string) => {
    setError(null);
    try {
      setCurrent(await invoke<Table>("hotwords_get", { name: n }));
    } catch (e) {
      setError(String(e));
    }
  };

  const deleteTable = async (n: string) => {
    try {
      await invoke("hotwords_delete", { name: n });
      if (current?.name === n) setCurrent(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const removeEntry = async (term: string) => {
    if (!current) return;
    const updated = {
      ...current,
      entries: current.entries.filter((e) => e.term !== term),
    };
    try {
      setCurrent(await invoke<Table>("hotwords_update", { table: updated }));
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const doMerge = async () => {
    setError(null);
    try {
      const merged = await invoke<Table>("hotwords_merge", {
        names: mergeSel,
        newName: mergeName,
      });
      setMsg(`已合并为「${merged.name}」（${merged.entries.length} 词条）`);
      setMergeSel([]);
      setMergeName("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="panel" data-testid="hotwords-panel">
      <h2>热词表</h2>
      <p className="login-note">
        用任意格式描述你的专有名词（选手ID、英雄名、梗、固定译法…），AI
        会自动规整成统一词条。热词表可在离线处理与实时字幕中启用，提高识别与翻译准确率。
      </p>

      <div className="form-col">
        <div className="form-row">
          <input
            data-testid="hw-name"
            placeholder="表名（如：DOTA选手 / 英雄译名）"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <label>
            <input
              type="checkbox"
              data-testid="hw-use-llm"
              checked={useLlm}
              onChange={(e) => setUseLlm(e.target.checked)}
            />
            AI 规整
          </label>
        </div>
        <textarea
          data-testid="hw-raw"
          rows={5}
          placeholder={
            "任意格式，例如：\n我们队打野是GPK，经常被打成gpk或者鸡皮开\n帕克=Puck，敌法师(AM)翻译成Anti-Mage\n- XinQ  # 四号位"
          }
          value={raw}
          onChange={(e) => setRaw(e.target.value)}
        />
        <button
          data-testid="hw-save"
          disabled={busy || !name || !raw.trim()}
          onClick={saveRaw}
        >
          {busy ? "规整中…" : "保存为热词表"}
        </button>
      </div>

      {msg && <div className="done" data-testid="hw-msg">{msg}</div>}
      {error && <div className="error" data-testid="hw-error">{error}</div>}

      <h3>已有热词表（{tables.length}）</h3>
      <ul data-testid="hw-list">
        {tables.map((t) => (
          <li key={t.name} data-testid={`hw-table-${t.name}`}>
            <label>
              <input
                type="checkbox"
                checked={mergeSel.includes(t.name)}
                onChange={(e) =>
                  setMergeSel(
                    e.target.checked
                      ? [...mergeSel, t.name]
                      : mergeSel.filter((x) => x !== t.name),
                  )
                }
              />
            </label>{" "}
            <b>{t.name}</b>（{t.entries} 词条）{t.description && ` — ${t.description}`}
            <button style={{ marginLeft: 8 }} onClick={() => openTable(t.name)}>
              查看
            </button>
            <button className="danger" data-testid={`hw-del-${t.name}`} onClick={() => deleteTable(t.name)}>
              删除
            </button>
          </li>
        ))}
      </ul>
      {mergeSel.length >= 2 && (
        <div className="form-row" data-testid="hw-merge-bar">
          <input
            data-testid="hw-merge-name"
            placeholder="合并后的新表名"
            value={mergeName}
            onChange={(e) => setMergeName(e.target.value)}
          />
          <button data-testid="hw-merge" disabled={!mergeName} onClick={doMerge}>
            合并 {mergeSel.length} 个表
          </button>
        </div>
      )}

      {current && (
        <div data-testid="hw-detail">
          <h3>
            「{current.name}」词条（{current.entries.length}）
          </h3>
          <table className="hw-table">
            <thead>
              <tr>
                <th>词条</th>
                <th>别名</th>
                <th>读音</th>
                <th>译法</th>
                <th>类别</th>
                <th>备注</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {current.entries.map((e) => (
                <tr key={e.term}>
                  <td><b>{e.term}</b></td>
                  <td>{(e.aliases ?? []).join(", ")}</td>
                  <td>{e.reading ?? ""}</td>
                  <td>{e.translation ?? ""}</td>
                  <td>{e.category ?? ""}</td>
                  <td>{e.note ?? ""}</td>
                  <td>
                    <button className="danger" onClick={() => removeEntry(e.term)}>
                      ✕
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/** Multi-select of hotword tables for the processing panels. */
export function HotwordSelect({
  value,
  onChange,
}: {
  value: string[];
  onChange: (names: string[]) => void;
}) {
  const [tables, setTables] = useState<Summary[]>([]);
  useEffect(() => {
    Promise.resolve(invoke<Summary[]>("hotwords_list"))
      .then((l) => setTables(Array.isArray(l) ? l : []))
      .catch(() => {});
  }, []);
  if (tables.length === 0) return null;
  return (
    <div className="form-row" data-testid="hw-select">
      <span style={{ fontSize: 13 }}>热词表:</span>
      {tables.map((t) => (
        <label key={t.name} style={{ fontSize: 13 }}>
          <input
            type="checkbox"
            checked={value.includes(t.name)}
            onChange={(e) =>
              onChange(
                e.target.checked
                  ? [...value, t.name]
                  : value.filter((x) => x !== t.name),
              )
            }
          />
          {t.name}
        </label>
      ))}
    </div>
  );
}
