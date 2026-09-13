import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { AiSnapshot } from "../hooks/useAiSettings";
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
import AiSettingsPanel from "./AiSettingsPanel";
import LlmSettings from "./LlmSettings";
const config = { provider: "openai", base_url: "https://service.example/v1", model: "demo", use_env: false, no_auth: false };
const effective = { ...config, base_source: "settings", model_source: "settings", key_source: "keychain", ready: true, using_text: false, has_saved_key: true };
let data: AiSnapshot;
beforeEach(() => {
  data = { config: { text: { ...config }, vision: { ...config }, image: { ...config }, vision_uses_text: true }, effective: { text: { ...effective }, vision: { ...effective, using_text: true }, image: { ...effective } } };
  invokeMock.mockReset().mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "ai_settings_save") {
      data = { ...data, config: { ...data.config, [args.kind]: args.service, vision_uses_text: args.visionUsesText }, effective: { ...data.effective, [args.kind]: { ...effective, ...args.service } } };
    }
    if (cmd === "ai_connection_test") return "文本连接测试通过";
    return structuredClone(data);
  });
});
it("saves explicitly and tests only saved settings without returning the key to the form", async () => {
  render(<AiSettingsPanel />);
  await screen.findByTestId("ai-base");
  fireEvent.change(screen.getByTestId("ai-model"), { target: { value: "new-model" } });
  fireEvent.change(screen.getByTestId("ai-key"), { target: { value: "test-only-value" } });
  expect(screen.getByTestId("ai-test")).toBeDisabled();
  expect(invokeMock).not.toHaveBeenCalledWith("config_set", expect.anything());
  fireEvent.click(screen.getByTestId("ai-save"));
  await waitFor(() => expect(screen.getByTestId("ai-key")).toHaveValue(""));
  expect(invokeMock).toHaveBeenCalledWith("ai_settings_save", expect.objectContaining({ kind: "text", apiKey: "test-only-value", service: { ...config, model: "new-model" } }));
  expect(screen.getByTestId("ai-test")).toBeEnabled();
  fireEvent.click(screen.getByTestId("ai-test"));
  await screen.findByText("文本连接测试通过");
  expect(invokeMock).toHaveBeenCalledWith("ai_connection_test", { kind: "text" });
});
it("shows keychain failures and leaves the unsaved key available for retry", async () => {
  invokeMock.mockImplementation(async (cmd: string) => { if (cmd === "ai_settings_save") throw new Error("无法访问系统凭据库，请检查系统凭据服务"); return structuredClone(data); });
  render(<AiSettingsPanel />); await screen.findByTestId("ai-key");
  fireEvent.change(screen.getByTestId("ai-key"), { target: { value: "retry-test-key" } });
  fireEvent.click(screen.getByTestId("ai-save"));
  expect(await screen.findByRole("alert")).toHaveTextContent("无法访问系统凭据库");
  expect(screen.getByTestId("ai-key")).toHaveValue("retry-test-key");
});
it("discards an unsaved key when changing its endpoint and blocks clearing another endpoint", async () => {
  render(<AiSettingsPanel />); await screen.findByTestId("ai-key");
  fireEvent.change(screen.getByTestId("ai-key"), { target: { value: "old-endpoint-key" } });
  fireEvent.change(screen.getByTestId("ai-base"), { target: { value: "https://new.example/v1" } });
  expect(screen.getByTestId("ai-key")).toHaveValue("");
  expect(screen.getByTestId("ai-clear")).toBeDisabled();
});
it("vision inheritance uses the text configuration and cannot clear the shared key", async () => {
  render(<AiSettingsPanel initialKind="vision" />); await screen.findByTestId("ai-clear");
  expect(screen.queryByTestId("ai-key")).not.toBeInTheDocument();
  expect(screen.getByTestId("ai-clear")).toBeDisabled();
  fireEvent.click(screen.getByLabelText("复用文本模型配置"));
  expect(screen.getByTestId("ai-key")).toBeInTheDocument();
  expect(screen.getByTestId("ai-test")).toBeDisabled();
});
it("clears the saved key through a dedicated operation", async () => {
  render(<AiSettingsPanel />); await screen.findByTestId("ai-clear");
  fireEvent.click(screen.getByTestId("ai-clear"));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_key_clear", { kind: "text" }));
});
it("image connection tests are labelled as model queries", async () => {
  render(<AiSettingsPanel initialKind="image" />); await screen.findByTestId("ai-test");
  expect(screen.getByText(/图片测试只查询模型列表/)).toBeInTheDocument();
});
it("links feature pages to the correct AI service", () => {
  const listener = vi.fn(); window.addEventListener("open-ai-settings", listener);
  render(<LlmSettings kind="image" />); fireEvent.click(screen.getByText("打开 AI 服务设置"));
  expect((listener.mock.calls[0][0] as CustomEvent).detail).toBe("image");
  window.removeEventListener("open-ai-settings", listener);
});
