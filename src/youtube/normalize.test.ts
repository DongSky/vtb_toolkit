import { describe, expect, it } from "vitest";
import { YTNodes } from "youtubei.js/web";
import { liveVideoId, normalizeItem, parseMoney, youtubeInput } from "./normalize";

const author = { contextMenuAccessibility: { accessibilityData: { label: "Menu" } }, id: "message-id", authorExternalChannelId: "UC-real-channel", authorName: { simpleText: "同名观众" }, authorPhoto: { thumbnails: [{ url: "https://example.org/avatar.png" }] }, timestampUsec: "1789214400000000" };

describe("YouTube upstream event normalization", () => {
  it("retains custom emoji images and readable text without internal IDs", () => {
    const node = new YTNodes.LiveChatTextMessage({ ...author, message: { runs: [
      { text: "hello " }, { emoji: { emojiId: "UC-secret/emoji-internal-id", isCustomEmoji: true, shortcuts: [":smile:"], image: { thumbnails: [{ url: "https://example.org/emoji.png" }] } } }, { text: " 世界" },
    ] } });
    expect(normalizeItem(node, "abcdefghijk")).toMatchObject({ text: "hello :smile: 世界", source: { message_runs: [{ text: "hello " }, { text: ":smile:", emoji_url: "https://example.org/emoji.png" }, { text: " 世界" }] } });
  });
  it("preserves source identity, unicode and owner permissions from real parser nodes", () => {
    const node = new YTNodes.LiveChatTextMessage({ ...author, message: { runs: [{ text: "打点 🎉" }] }, authorBadges: [{ liveChatAuthorBadgeRenderer: { icon: { iconType: "OWNER" }, tooltip: "Owner" } }] });
    expect(normalizeItem(node, "abcdefghijk")).toMatchObject({ kind: "danmaku", uid: 0, text: "打点 🎉", is_admin: true, timestamp: "2026-09-12T12:00:00.000Z", source: { platform: "youtube", room_id: "abcdefghijk", user_id: "UC-real-channel", message_id: "message-id" } });
  });
  it("keeps paid sticker media and the actual monetary label", () => {
    const node = new YTNodes.LiveChatPaidSticker({ ...author, purchaseAmountText: { simpleText: "HK$25.00" }, sticker: { accessibility: { accessibilityData: { label: "Sticker" } }, thumbnails: [{ url: "https://example.org/sticker.png" }] } });
    expect(normalizeItem(node, "abcdefghijk")).toMatchObject({ kind: "super_chat", source: { money: { currency: "HKD", amount: 25, display: "HK$25.00" }, sticker_url: "https://example.org/sticker.png" } });
  });
  it("does not map channel membership to a Bilibili guard rank", () => {
    const node = new YTNodes.LiveChatMembershipItem({ ...author, headerSubtext: { simpleText: "Member for 3 months" } });
    expect(normalizeItem(node, "abcdefghijk")).toMatchObject({ kind: "guard_buy", guard_level: 0, source: { membership: "Member for 3 months" } });
  });
  it("does not guess currencies from ambiguous symbols", () => {
    expect(parseMoney("$12.00")).toEqual({ currency: null, amount: 12, display: "$12.00" });
    expect(parseMoney("¥1,000").currency).toBeNull();
    expect(parseMoney("EUR 1.234,56")).toMatchObject({ currency: "EUR", amount: 1234.56 });
    expect(parseMoney("HK$1,234.56")).toMatchObject({ currency: "HKD", amount: 1234.56 });
  });
  it("recognizes current channel live badges and excludes recorded videos", () => {
    expect(liveVideoId({ type: "LockupView", content_id: "abcdefghijk", content_image: { overlays: [{ badges: [{ icon_name: "LIVE", badge_style: "THUMBNAIL_OVERLAY_BADGE_STYLE_LIVE" }] }] } })).toBe("abcdefghijk");
    expect(liveVideoId({ content_id: "abcdefghijk", content_image: { overlays: [{ badges: [{ text: "12:30" }] }] } })).toBeNull();
  });
  it("canonicalizes video/channel inputs and rejects lookalike hosts", () => {
    expect(youtubeInput("https://youtu.be/abcdefghijk?t=30")).toBe(youtubeInput("https://youtube.com/live/abcdefghijk"));
    expect(youtubeInput("@LofiGirl")).toBe("https://www.youtube.com/@LofiGirl/live");
    for (const bad of ["https://youtube.com.evil.test/watch?v=abcdefghijk", "http://youtube.com/watch?v=abcdefghijk", "https://user@youtube.com/watch?v=abcdefghijk", "https://youtube.com/watch?v=bad"]) expect(() => youtubeInput(bad)).toThrow();
  });
});
