import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import TtsControls from "./TtsControls";

beforeEach(() => invokeMock.mockReset());

describe("TtsControls", () => {
  it("hydrates status and toggles TTS", async () => {
    let state = { enabled: false, paid_only: false };
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "tts_status") return Promise.resolve(state);
      if (cmd === "tts_set") {
        state = { enabled: Boolean(args!.enabled), paid_only: Boolean(args!.paidOnly) };
        return Promise.resolve(state);
      }
      return Promise.resolve();
    });
    render(<TtsControls />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("tts_status"));

    fireEvent.click(screen.getByTestId("tts-enabled"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("tts_set", {
        enabled: true,
        paidOnly: false,
      }),
    );

    fireEvent.click(screen.getByTestId("tts-paid-only"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("tts_set", {
        enabled: true,
        paidOnly: true,
      }),
    );
  });

  it("paid-only is disabled until TTS is enabled", async () => {
    invokeMock.mockResolvedValue({ enabled: false, paid_only: false });
    render(<TtsControls />);
    await waitFor(() =>
      expect(screen.getByTestId("tts-paid-only")).toBeDisabled(),
    );
  });
});
