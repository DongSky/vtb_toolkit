import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
const mocks=vi.hoisted(() => ({ invoke:vi.fn(),open:vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke:mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen:vi.fn().mockResolvedValue(() => {}) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl:mocks.open }));
import YoutubeRooms from "./YoutubeRooms";
import ObsPanel from "./ObsPanel";
import YoutubeUpload from "./YoutubeUpload";

beforeEach(() => {
  localStorage.clear(); mocks.invoke.mockReset(); mocks.open.mockResolvedValue(undefined);
  mocks.invoke.mockImplementation(async (cmd) => cmd === "config_load" ? {} : []);
});
describe("YouTube controls", () => {
  it("connects a canonical URL without asking for credentials", async () => {
    render(<YoutubeRooms compact />);
    fireEvent.change(screen.getByLabelText("YouTube 直播或频道"), { target:{ value:"https://youtu.be/abcdefghijk" } });
    fireEvent.click(screen.getByRole("button",{ name:"连接 YouTube" }));
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("youtube_call", { method:"connect",args:{ input:"https://www.youtube.com/watch?v=abcdefghijk" } }));
    expect(screen.queryByLabelText(/密钥/)).toBeNull();
  });
  it("starts OBS on a stable port and generates a filtered independent page", async () => {
    const running={running:true,port:18990,danmaku_url:"http://127.0.0.1:18990/overlay/danmaku",subtitle_url:"http://127.0.0.1:18990/overlay/subtitle",clients:0};
    mocks.invoke.mockImplementation(async (cmd) => cmd === "config_load" ? {} : cmd === "overlay_start" ? running : {running:false});
    render(<ObsPanel />);
    fireEvent.click(screen.getByTestId("obs-start"));
    await waitFor(() => expect(screen.getByTestId("obs-danmaku-url")).toBeInTheDocument());
    expect(mocks.invoke).toHaveBeenCalledWith("overlay_start",{port:18990});
    fireEvent.change(screen.getByLabelText("叠加层平台"),{target:{value:"youtube"}});
    fireEvent.change(screen.getByLabelText("叠加层直播"),{target:{value:"youtube:abcdefghijk"}});
    const url=(screen.getByTestId("obs-danmaku-url") as HTMLInputElement).value;
    expect(new URL(url).searchParams.get("room")).toBe("youtube:abcdefghijk");
    expect(new URL(url).searchParams.get("platform")).toBe("youtube");
    expect(new URL(url).searchParams.has("preview")).toBe(false);
    fireEvent.click(screen.getByText("单独打开评论网页"));
    expect(mocks.open).toHaveBeenCalledWith(url);
    expect(screen.getByTitle("评论网页预览")).toHaveAttribute("src",`${url}&preview=1`);
  });
  it("keeps uploads private by default and transmits only after the upload button", async () => {
    mocks.invoke.mockImplementation(async (cmd) => cmd === "youtube_upload_status" ? {configured:true,authorized:true,uploading:false} : undefined);
    render(<YoutubeUpload file={"D:\\clips\\sample.mp4"} />);
    const button=screen.getByRole("button",{name:"上传到 YouTube（私享）"});
    await waitFor(() => expect(button).toBeEnabled());
    expect(mocks.invoke.mock.calls.some(([cmd]) => cmd === "youtube_upload_start")).toBe(false);
    fireEvent.click(button);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("youtube_upload_start",expect.objectContaining({options:expect.objectContaining({privacy:"private",file:"D:\\clips\\sample.mp4",made_for_kids:false})})));
  });
});
