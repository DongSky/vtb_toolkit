import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import LlmSettings from "./LlmSettings";

beforeEach(() => invokeMock.mockReset());

describe("LlmSettings", () => {
  it("hydrates provider/base/model from settings and persists edits", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "config_load")
        return Promise.resolve({
          llm: { provider: "openai", base_url: "https://yunwu.ai/v1", model: "gpt-4o-mini" },
        });
      if (cmd === "secret_exists") return Promise.resolve(true);
      return Promise.resolve();
    });
    render(<LlmSettings />);
    await waitFor(() =>
      expect(
        (screen.getByTestId("llm-provider") as HTMLSelectElement).value,
      ).toBe("openai"),
    );
    expect((screen.getByTestId("llm-base") as HTMLInputElement).value).toBe(
      "https://yunwu.ai/v1",
    );
    expect((screen.getByTestId("llm-model") as HTMLInputElement).value).toBe(
      "gpt-4o-mini",
    );
    // Key already in keychain → placeholder reflects it.
    expect(
      (screen.getByTestId("llm-key") as HTMLInputElement).placeholder,
    ).toContain("已存钥匙串");

    // Editing base URL persists the whole llm object (debounced).
    fireEvent.change(screen.getByTestId("llm-base"), {
      target: { value: "https://my-relay/v1" },
    });
    await waitFor(
      () =>
        expect(
          invokeMock.mock.calls.some(
            (c) =>
              c[0] === "config_set" &&
              c[1].key === "llm" &&
              c[1].value.base_url === "https://my-relay/v1",
          ),
        ).toBe(true),
      { timeout: 2000 },
    );
  });

  it("stores the API key in the keychain", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "config_load") return Promise.resolve({});
      if (cmd === "secret_exists") return Promise.resolve(false);
      return Promise.resolve();
    });
    render(<LlmSettings />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("config_load"));
    fireEvent.change(screen.getByTestId("llm-key"), {
      target: { value: "sk-test-123" },
    });
    fireEvent.click(screen.getByTestId("llm-key-save"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("secret_set", {
        name: "llm-api-key",
        value: "sk-test-123",
      }),
    );
    // Field cleared after save; placeholder flips.
    expect((screen.getByTestId("llm-key") as HTMLInputElement).value).toBe("");
  });
});
