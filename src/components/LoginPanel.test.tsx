import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import LoginPanel from "./LoginPanel";

beforeEach(() => {
  invokeMock.mockReset();
  vi.useRealTimers();
});

describe("LoginPanel", () => {
  it("shows logged-out state and starts QR login", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "auth_status")
        return Promise.resolve({ logged_in: false, uname: null, mid: null });
      if (cmd === "auth_qr_start")
        return Promise.resolve({ svg: "<svg data-qr='1'></svg>" });
      if (cmd === "auth_qr_poll")
        return Promise.resolve({ state: "waiting_scan" });
      return Promise.resolve();
    });
    render(<LoginPanel />);
    await waitFor(() => expect(screen.getByTestId("login-form")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("qr-start"));
    await waitFor(() => expect(screen.getByTestId("qr-box")).toBeInTheDocument());
    expect(screen.getByTestId("qr-hint").textContent).toContain("扫码");
  });

  it("shows logged-in identity and can log out", async () => {
    let loggedIn = true;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "auth_status")
        return Promise.resolve(
          loggedIn
            ? { logged_in: true, uname: "测试用户", mid: 42 }
            : { logged_in: false, uname: null, mid: null },
        );
      if (cmd === "auth_logout") {
        loggedIn = false;
        return Promise.resolve();
      }
      return Promise.resolve();
    });
    render(<LoginPanel />);
    await waitFor(() =>
      expect(screen.getByTestId("login-info")).toHaveTextContent("测试用户"),
    );
    fireEvent.click(screen.getByTestId("logout-btn"));
    await waitFor(() =>
      expect(screen.getByTestId("login-form")).toBeInTheDocument(),
    );
  });

  it("transitions to logged-in when poll confirms", async () => {
    vi.useFakeTimers();
    let confirmed = false;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "auth_status")
        return Promise.resolve(
          confirmed
            ? { logged_in: true, uname: "新用户", mid: 7 }
            : { logged_in: false, uname: null, mid: null },
        );
      if (cmd === "auth_qr_start")
        return Promise.resolve({ svg: "<svg></svg>" });
      if (cmd === "auth_qr_poll") {
        confirmed = true;
        return Promise.resolve({ state: "confirmed", uname: "新用户", mid: 7 });
      }
      return Promise.resolve();
    });
    render(<LoginPanel />);
    await vi.waitFor(() => expect(screen.getByTestId("qr-start")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("qr-start"));
    await vi.waitFor(() => expect(screen.getByTestId("qr-box")).toBeInTheDocument());
    await vi.advanceTimersByTimeAsync(2100); // one poll tick
    await vi.waitFor(() =>
      expect(screen.getByTestId("login-info")).toHaveTextContent("新用户"),
    );
    vi.useRealTimers();
  });
});
