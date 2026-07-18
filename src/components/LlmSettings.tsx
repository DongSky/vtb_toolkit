import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { usePersisted } from "../hooks/usePersisted";

/**
 * Shared LLM endpoint settings: provider / API base / model persist in
 * settings.json (under "llm"); the key goes to the OS keychain.
 * Consumers read the same persisted keys to fill their invoke options.
 */
export function useLlmSettings() {
  const [llm, setLlm] = usePersisted<{
    provider?: string;
    base_url?: string;
    model?: string;
  }>("llm", {});
  return { llm, setLlm };
}

export default function LlmSettings() {
  const { llm, setLlm } = useLlmSettings();
  const [apiKey, setApiKey] = useState("");
  const [keySaved, setKeySaved] = useState(false);

  useEffect(() => {
    Promise.resolve(invoke<boolean>("secret_exists", { name: "llm-api-key" }))
      .then((v) => setKeySaved(Boolean(v)))
      .catch(() => {});
  }, []);

  const saveKey = async () => {
    if (!apiKey) return;
    try {
      await invoke("secret_set", { name: "llm-api-key", value: apiKey });
      setKeySaved(true);
      setApiKey("");
    } catch {
      /* browser dev */
    }
  };

  return (
    <details className="llm-settings" data-testid="llm-settings">
      <summary>
        翻译 / AI 接口设置{llm.base_url ? `（${llm.base_url}）` : "（默认官方端点）"}
      </summary>
      <div className="form-col" style={{ marginTop: 8 }}>
        <label>
          服务类型
          <select
            data-testid="llm-provider"
            value={llm.provider ?? "anthropic"}
            onChange={(e) => setLlm({ ...llm, provider: e.target.value })}
          >
            <option value="anthropic">Anthropic</option>
            <option value="openai">OpenAI 兼容（官方/中转/自建）</option>
          </select>
        </label>
        <input
          data-testid="llm-base"
          placeholder="API Base URL（可选，如 https://yunwu.ai/v1；留空用官方）"
          value={llm.base_url ?? ""}
          onChange={(e) => setLlm({ ...llm, base_url: e.target.value })}
        />
        <input
          data-testid="llm-model"
          placeholder="模型名（可选，如 gpt-4o-mini / claude-haiku-4-5）"
          value={llm.model ?? ""}
          onChange={(e) => setLlm({ ...llm, model: e.target.value })}
        />
        <div className="form-row">
          <input
            data-testid="llm-key"
            type="password"
            placeholder={keySaved ? "API Key 已存钥匙串（输入可更新）" : "API Key"}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            style={{ flex: 1 }}
          />
          <button data-testid="llm-key-save" disabled={!apiKey} onClick={saveKey}>
            存入钥匙串
          </button>
        </div>
        <p className="login-note">
          留空的项按优先级回退：环境变量（OPENAI_BASE_URL 等）→ 官方默认。所有翻译/同传/多模态功能共用此配置。
        </p>
      </div>
    </details>
  );
}
