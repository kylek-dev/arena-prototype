import { expect, it } from "vitest";
import {
  EMPTY_CHAT,
  MAX_CHAT_LOG,
  chatErrorText,
  chatPrefix,
  chatTabs,
  chatText,
  isIgnored,
  parseChatLine,
  receiveChat,
  showChatTab,
  tabMessages,
  withIgnored,
} from "../../web/src/domain/chat";
import { parseQueueMessage, requests } from "../../web/src/domain/matchmaking";

const LINE = { channel: "team", from: "Rook#0042", admin: false, text: "hi" };

it("parses chat lines strictly", () => {
  expect(parseQueueMessage(JSON.stringify({ type: "chat", ...LINE }))).toEqual({
    type: "chat",
    line: { channel: "team", from: "Rook#0042", to: null, admin: false, text: "hi" },
  });
  expect(
    parseChatLine({
      channel: "whisper",
      from: "Rook#0042",
      to: "Ash#1000",
      admin: true,
      text: "x",
    }),
  )?.toMatchObject({ to: "Ash#1000", admin: true });
  for (const bad of [
    { ...LINE, channel: "system" },
    { ...LINE, from: "nobody" },
    { ...LINE, admin: "yes" },
    { ...LINE, text: "" },
    { ...LINE, text: "x".repeat(1001) },
    { ...LINE, to: "Ash#1000" },
    { ...LINE, channel: "whisper", to: "<b>" },
  ])
    expect(parseChatLine(bad as Record<string, unknown>), JSON.stringify(bad)).toBeNull();
  expect(JSON.parse(requests.chat("team", "gg"))).toEqual({
    type: "chat",
    channel: "team",
    text: "gg",
  });
  expect(JSON.parse(requests.whisper("Ash#1000", "hey"))).toEqual({
    type: "whisper",
    to: "Ash#1000",
    text: "hey",
  });
  expect(JSON.parse(requests.ignore("Ash#1000", false))).toEqual({
    type: "unignore",
    handle: "Ash#1000",
  });
});

it("counts characters as the door does and words lines and refusals", () => {
  expect(chatText("  hi  ")).toEqual({ text: "hi", tooLong: false });
  expect(chatText("😀".repeat(200)).tooLong).toBe(false);
  expect(chatText("😀".repeat(201)).tooLong).toBe(true);
  expect(chatTabs(false)).toEqual(["public", "team", "match", "whisper"]);
  expect(chatTabs(true)).toEqual(["spectator", "public", "whisper"]);
  expect(chatPrefix({ ...LINE, channel: "team", to: null } as never)).toBe("[Team] Rook#0042");
  expect(chatPrefix({ ...LINE, channel: "whisper", to: "Ash#1000" } as never)).toBe("To Ash#1000");
  expect(chatPrefix({ ...LINE, channel: "whisper", to: null } as never)).toBe("Rook#0042 whispers");
  expect(chatErrorText("spectators_only")).toMatch(/other spectators/);
  expect(chatErrorText("new_code")).toBe("The message was not sent.");
});

it("keeps unread counts per tab, answers the last whisperer and filters ignored players", () => {
  let chat = receiveChat(EMPTY_CHAT, { ...LINE, to: null } as never);
  chat = receiveChat(chat, { ...LINE, channel: "public", to: null } as never);
  expect(chat.unread).toEqual({ team: 1 });
  chat = receiveChat(chat, { ...LINE, channel: "whisper", to: null, from: "Ash#1000" } as never);
  expect(chat.whisperTo).toBe("Ash#1000");
  // One's own whisper copy is not unread.
  chat = receiveChat(chat, { ...LINE, channel: "whisper", to: "Bea#0002" } as never);
  expect(chat.unread).toEqual({ team: 1, whisper: 1 });
  chat = showChatTab(chat, "team");
  expect(chat.unread).toEqual({ whisper: 1 });
  chat = withIgnored(chat, "rook#0042", true);
  expect(isIgnored(chat, "ROOK#0042")).toBe(true);
  expect(tabMessages(chat, "team")).toEqual([]);
  chat = withIgnored(chat, "Rook#0042", false);
  expect(tabMessages(chat, "team")).toHaveLength(1);
  let full = EMPTY_CHAT;
  for (let i = 0; i < MAX_CHAT_LOG + 5; i += 1)
    full = receiveChat(full, { ...LINE, channel: "public", to: null, text: `m${i}` } as never);
  expect(full.messages).toHaveLength(MAX_CHAT_LOG);
  expect(full.messages[0].text).toBe("m5");
});
