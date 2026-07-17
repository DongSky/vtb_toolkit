import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface NotifyCfg {
  bark_key?: string;
  serverchan_key?: string;
  telegram?: { bot_token: string; chat_id: string };
  webhook_urls?: string[];
}

/** Notification channels for recording events (Bark/ServerChan/TG/webhook). */
export default function NotifySettings() {
  const [bark, setBark] = useState("");
  const [serverchan, setServerchan] = useState("");
  const [tgToken, setTgToken] = useState("");
  const [tgChat, setTgChat] = useState("");
  const [webhooks, setWebhooks] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    Promise.resolve(invoke<Record<string, unknown>>("config_load"))
      .then((cfg) => {
        const n = (cfg?.notify ?? {}) as NotifyCfg;
        setBark(n.bark_key ?? "");
        setServerchan(n.serverchan_key ?? "");
        setTgToken(n.telegram?.bot_token ?? "");
        setTgChat(n.telegram?.chat_id ?? "");
        setWebhooks((n.webhook_urls ?? []).join("\n"));
      })
      .catch(() => {});
  }, []);

  const save = async () => {
    const value: NotifyCfg = {};
    if (bark) value.bark_key = bark;
    if (serverchan) value.serverchan_key = serverchan;
    if (tgToken && tgChat) value.telegram = { bot_token: tgToken, chat_id: tgChat };
    const urls = webhooks
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);
    if (urls.length) value.webhook_urls = urls;
    try {
      await invoke("config_set", { key: "notify", value });
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } catch {
      /* browser dev */
    }
  };

  return (
    <details className="notify-settings" data-testid="notify-settings">
      <summary>通知设置（开播/录制完成/异常推送）</summary>
      <div className="form-col" style={{ marginTop: 8 }}>
        <input
          data-testid="notify-bark"
          placeholder="Bark Key（https://api.day.app/后面的部分）"
          value={bark}
          onChange={(e) => setBark(e.target.value)}
        />
        <input
          data-testid="notify-serverchan"
          placeholder="ServerChan SendKey"
          value={serverchan}
          onChange={(e) => setServerchan(e.target.value)}
        />
        <div className="form-row">
          <input
            data-testid="notify-tg-token"
            placeholder="Telegram Bot Token"
            value={tgToken}
            onChange={(e) => setTgToken(e.target.value)}
          />
          <input
            data-testid="notify-tg-chat"
            placeholder="Chat ID"
            value={tgChat}
            onChange={(e) => setTgChat(e.target.value)}
          />
        </div>
        <textarea
          data-testid="notify-webhooks"
          placeholder="Webhook URL（每行一个，POST JSON）"
          value={webhooks}
          onChange={(e) => setWebhooks(e.target.value)}
        />
        <button data-testid="notify-save" onClick={save}>
          {saved ? "已保存" : "保存通知设置"}
        </button>
      </div>
    </details>
  );
}
