import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { t, tm, useLocale } from "../i18n";
import { useAiSettings, type AiConfig, type ServiceKind, type ServiceConfig } from "../hooks/useAiSettings";

const titles: Record<ServiceKind, string> = { text: "文本模型", vision: "视觉模型", image: "图片生成" };
const purposes: Record<ServiceKind, string> = {
  text: "用于弹幕和字幕翻译、语义粗剪、热词整理及封面创意。",
  vision: "用于画面扫描与高光复核，需要支持图片输入的模型。",
  image: "用于封面文生图和参考图编辑，使用独立的地址、模型和密钥。",
};
const sources: Record<string, string> = { settings: "已保存设置", environment: "环境变量", default: "默认值", keychain: "系统凭据库", none: "无需认证", missing: "未配置" };

export default function AiSettingsPanel({ initialKind = "text" }: { initialKind?: ServiceKind }) {
  useLocale();
  const { snapshot, error: loadError, busy: loading, load, save, clear } = useAiSettings();
  const [kind, setKind] = useState<ServiceKind>(initialKind);
  const [draft, setDraft] = useState<AiConfig | null>(null);
  const [keys, setKeys] = useState<Record<ServiceKind, string>>({ text: "", vision: "", image: "" });
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (snapshot && !draft) setDraft(snapshot.config); }, [snapshot, draft]);
  useEffect(() => setKind(initialKind), [initialKind]);
  if (!draft || !snapshot) return <section><h2>{t("AI 服务")}</h2>{loadError ? <><p role="alert">{tm(loadError)}</p><button onClick={() => void load()}>{t("重试")}</button></> : <p>{t("加载中…")}</p>}</section>;
  const service = draft[kind];
  const effective = snapshot.effective[kind];
  const inherited = kind === "vision" && draft.vision_uses_text;
  const dirty = JSON.stringify(service) !== JSON.stringify(snapshot.config[kind]) || (kind === "vision" && draft.vision_uses_text !== snapshot.config.vision_uses_text) || !!keys[kind];
  const update = (patch: Partial<ServiceConfig>) => { setDraft({ ...draft, [kind]: { ...service, ...patch } }); setMessage(null); setError(null); };
  const act = async (operation: "save" | "clear" | "test") => {
    setBusy(true); setError(null); setMessage(null);
    try {
      if (operation === "save") {
        const next = await save(kind, service, draft.vision_uses_text, inherited ? "" : keys[kind]);
        setDraft({ ...draft, [kind]: next.config[kind], vision_uses_text: kind === "vision" ? next.config.vision_uses_text : draft.vision_uses_text });
        setKeys({ ...keys, [kind]: "" }); setMessage("配置已保存，新任务将使用此配置；运行中的任务需停止后重新开始。");
      } else if (operation === "clear") {
        await clear(kind); setMessage("已清除当前服务的已保存密钥；如允许环境变量回退，将使用环境变量。");
      } else { setMessage(await invoke<string>("ai_connection_test", { kind })); }
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  return <section className="panel ai-settings" data-testid="ai-settings-panel">
    <h2>{t("设置")} · {t("AI 服务")}</h2>
    <p className="hint">{t("语音识别使用本地 Whisper，不需要 API Key。以下配置用于可选的在线 AI 功能。")}</p>
    <div className="form-row" role="tablist" aria-label={t("AI 服务")}>
      {(Object.keys(titles) as ServiceKind[]).map((role) => <button role="tab" aria-selected={kind === role} key={role} disabled={busy} onClick={() => { setKind(role); setMessage(null); setError(null); }}>{t(titles[role])}</button>)}
    </div>
    <p>{t(purposes[kind])}</p>
    <fieldset disabled={busy || loading}>
      {kind === "vision" && <label className="ai-check"><input type="checkbox" checked={draft.vision_uses_text} onChange={(e) => { setDraft({ ...draft, vision_uses_text: e.target.checked }); setKeys({ ...keys, vision: "" }); setMessage(null); setError(null); }} />{t("复用文本模型配置")}</label>}
      {!inherited && <div className="form-col">
        <label>{t("服务类型")}<select data-testid="ai-provider" value={service.provider} onChange={(e) => { update({ provider: e.target.value, base_url: "", model: "" }); setKeys({ ...keys, [kind]: "" }); }}>
          <option value="openai">{t("OpenAI 兼容（官方/中转/自建）")}</option>
          {kind !== "image" && <option value="anthropic">Anthropic</option>}
        </select></label>
        <label>Base URL<input data-testid="ai-base" value={service.base_url} placeholder={service.provider === "openai" ? "https://api.openai.com/v1" : "https://api.anthropic.com"} onChange={(e) => { update({ base_url: e.target.value }); setKeys({ ...keys, [kind]: "" }); }} /></label>
        <label>{t("模型名称")}<input data-testid="ai-model" value={service.model} placeholder={effective.model} onChange={(e) => update({ model: e.target.value })} /></label>
        <label className="ai-check"><input type="checkbox" checked={service.use_env} onChange={(e) => { update({ use_env: e.target.checked }); setKeys({ ...keys, [kind]: "" }); }} />{t("允许环境变量回退")}</label>
        <label className="ai-check"><input type="checkbox" checked={service.no_auth} onChange={(e) => { update({ no_auth: e.target.checked }); setKeys({ ...keys, [kind]: "" }); }} />{t("无需认证（用于不要求 Key 的本地服务）")}</label>
        {!service.no_auth && <label>API Key<input data-testid="ai-key" type="password" autoComplete="off" value={keys[kind]} placeholder={t("留空保留该服务的已保存密钥，输入可更新")} onChange={(e) => { setKeys({ ...keys, [kind]: e.target.value }); setMessage(null); setError(null); }} /></label>}
      </div>}
      <p className="hint">{t("密钥按用途、服务类型和地址隔离，保存在系统凭据库，不写入普通配置文件。")}</p>
      {dirty && <p role="status">{t("有未保存的修改，请先保存再测试。")}</p>}
      <div className="form-row">
        <button data-testid="ai-save" onClick={() => void act("save")}>{t("保存配置")}</button>
        <button data-testid="ai-test" disabled={dirty || !effective.ready} onClick={() => void act("test")}>{busy ? t("处理中…") : t("测试连接")}</button>
        <button data-testid="ai-clear" disabled={dirty || inherited || !effective.has_saved_key} onClick={() => void act("clear")}>{t("清除已保存密钥")}</button>
      </div>
    </fieldset>
    <p className="hint">{t(kind === "image" ? "图片测试只查询模型列表，不生成图片；通过不代表图片生成或编辑权限已验证。" : "连接测试发送固定的短测试内容；视觉测试附带测试图片，可能产生少量 API 费用。")}</p>
    {error && <p className="error" role="alert">{tm(error)}</p>}
    {message && <p role="status">{tm(message)}</p>}
    <div className="ai-effective">
      <h3>{t("当前生效配置")}{effective.using_text ? ` · ${t("复用文本模型配置")}` : ""}</h3>
      <dl>
        <dt>{t("服务地址")}</dt><dd>{effective.base_url} <small>({t(sources[effective.base_source])})</small></dd>
        <dt>{t("模型名称")}</dt><dd>{effective.model} <small>({t(sources[effective.model_source])})</small></dd>
        <dt>{t("密钥来源")}</dt><dd>{t(sources[effective.key_source])}</dd>
      </dl>
      <p className="hint">{t("已保存密钥优先于环境变量。环境变量中的密钥只用于该环境指定的地址或对应官方地址。")}</p>
    </div>
  </section>;
}
