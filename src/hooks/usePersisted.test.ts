import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { usePersisted } from "./usePersisted";

beforeEach(() => {
  invokeMock.mockReset();
  vi.useRealTimers();
});

describe("usePersisted", () => {
  it("hydrates from stored config", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "config_load"
        ? Promise.resolve({ "rec.room": "24158116" })
        : Promise.resolve(),
    );
    const { result } = renderHook(() => usePersisted("rec.room", ""));
    await waitFor(() => expect(result.current[0]).toBe("24158116"));
  });

  it("keeps the default when key absent", async () => {
    invokeMock.mockResolvedValue({});
    const { result } = renderHook(() => usePersisted("x", "fallback"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("config_load"));
    expect(result.current[0]).toBe("fallback");
  });

  it("persists changes (debounced) after hydration", async () => {
    vi.useFakeTimers();
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "config_load" ? Promise.resolve({}) : Promise.resolve(),
    );
    const { result } = renderHook(() => usePersisted("k", "a"));
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("config_load"));

    act(() => result.current[1]("b"));
    expect(result.current[0]).toBe("b");
    await vi.advanceTimersByTimeAsync(500);
    expect(invokeMock).toHaveBeenCalledWith("config_set", {
      key: "k",
      value: "b",
    });
    vi.useRealTimers();
  });

  it("survives a missing backend (browser dev)", async () => {
    invokeMock.mockRejectedValue("no tauri");
    const { result } = renderHook(() => usePersisted("k", "d"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    act(() => result.current[1]("changed"));
    expect(result.current[0]).toBe("changed");
  });
});
