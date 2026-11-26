import { expect, it } from "vitest";
import {
  EMPTY_CHAT,
  MAX_CONVERSATIONS,
  chatTabLabel,
  closeConversation,
  defaultChannel,
  openConversation,
  whisperDraft,
  whisperQuery,
  whisperSuggestions,
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
  expect(chatTabs(false)).toEqual(["all", "public", "team", "match"]);
  expect(chatTabs(true, ["Ash#1000"])).toEqual(["all", "spectator", "public", "w:Ash#1000"]);
  expect(chatTabLabel("all")).toBe("All");
  expect(chatTabLabel("w:Ash#1000")).toBe("Ash#1000");
  expect(defaultChannel({ seated: true, spectating: false })).toBe("team");
  expect(defaultChannel({ seated: false, spectating: true })).toBe("spectator");
  expect(defaultChannel({ seated: false, spectating: false })).toBe("public");
  expect(chatPrefix({ ...LINE, channel: "team", to: null } as never)).toBe("[Team] Rook#0042");
  expect(chatPrefix({ ...LINE, channel: "whisper", to: "Ash#1000" } as never)).toBe("To Ash#1000");
  expect(chatPrefix({ ...LINE, channel: "whisper", to: null } as never)).toBe("Rook#0042 whispers");
  expect(chatErrorText("spectators_only")).toMatch(/other spectators/);
  expect(chatErrorText("new_code")).toBe("The message was not sent.");
});

it("reads a whisper draft once the handle is complete and followed by a space", () => {
  expect(whisperDraft("/w Ash#1000")).toBeNull();
  expect(whisperDraft("/w Ash#10")).toBeNull();
  expect(whisperDraft("/w Ash#1000 ")).toEqual({ to: "Ash#1000", text: "" });
  expect(whisperDraft("/whisper Ash#1000 hi there")).toEqual({ to: "Ash#1000", text: "hi there" });
  expect(whisperDraft("/W  Ash#1000 hi")).toEqual({ to: "Ash#1000", text: "hi" });
  expect(whisperDraft("/w nobody hi")).toBeNull();
  expect(whisperDraft("hello /w Ash#1000 hi")).toBeNull();
});

it("shows everything in All, opens a tab per whisper partner and keeps unread counts", () => {
  let chat = receiveChat(EMPTY_CHAT, { ...LINE, to: null } as never);
  // All is shown: nothing is unread.
  expect(chat.active).toBe("all");
  expect(chat.unread).toEqual({});
  chat = showChatTab(chat, "public");
  chat = receiveChat(chat, { ...LINE, channel: "team", to: null } as never);
  expect(chat.unread).toEqual({ all: 1, team: 1 });
  chat = receiveChat(chat, { ...LINE, channel: "whisper", to: null, from: "Ash#1000" } as never);
  expect(chat.conversations).toEqual(["Ash#1000"]);
  expect(chat.whisperTo).toBe("Ash#1000");
  expect(chat.unread).toEqual({ all: 2, team: 1, "w:Ash#1000": 1 });
  // One's own whisper to someone new opens their tab too, never unread.
  chat = receiveChat(chat, { ...LINE, channel: "whisper", to: "bea#0002" } as never);
  expect(chat.conversations).toEqual(["Ash#1000", "bea#0002"]);
  expect(chat.unread.all).toBe(2);
  // The same partner, in other letter case, keeps one tab.
  chat = receiveChat(chat, { ...LINE, channel: "whisper", to: null, from: "BEA#0002" } as never);
  expect(chat.conversations).toHaveLength(2);
  expect(tabMessages(chat, "w:bea#0002").map((m) => m.channel)).toEqual(["whisper", "whisper"]);
  expect(tabMessages(chat, "all")).toHaveLength(5);
  expect(tabMessages(chat, "team")).toHaveLength(2);
  chat = showChatTab(chat, "all");
  expect(chat.unread).toEqual({});
  chat = openConversation(chat, "Cyd#0003");
  expect(chat.active).toBe("w:Cyd#0003");
  chat = closeConversation(chat, "Cyd#0003");
  expect(chat.active).toBe("all");
  expect(chat.conversations).toEqual(["Ash#1000", "bea#0002"]);
  chat = withIgnored(chat, "rook#0042", true);
  expect(isIgnored(chat, "ROOK#0042")).toBe(true);
  expect(tabMessages(chat, "team")).toEqual([]);
  let full = EMPTY_CHAT;
  for (let i = 0; i < MAX_CHAT_LOG + 5; i += 1)
    full = receiveChat(full, { ...LINE, channel: "public", to: null, text: `m${i}` } as never);
  expect(full.messages).toHaveLength(MAX_CHAT_LOG);
  expect(full.messages[0].text).toBe("m5");
  let many = EMPTY_CHAT;
  for (const name of ["Aaa", "Bbb", "Ccc", "Ddd", "Eee", "Fff", "Ggg"])
    many = receiveChat(many, {
      ...LINE,
      channel: "whisper",
      to: null,
      from: `${name}#0001`,
    } as never);
  expect(many.conversations).toHaveLength(MAX_CONVERSATIONS);
  expect(many.conversations[0]).toBe("Bbb#0001");
});

it("never closes the conversation being shown, and reading a tab clears its share of All", () => {
  let chat = EMPTY_CHAT;
  chat = receiveChat(chat, { ...LINE, channel: "whisper", to: null, from: "Aaa#0001" } as never);
  chat = showChatTab(chat, "w:Aaa#0001");
  for (const name of ["Bbb", "Ccc", "Ddd", "Eee", "Fff", "Ggg"])
    chat = receiveChat(chat, {
      ...LINE,
      channel: "whisper",
      to: null,
      from: `${name}#0001`,
    } as never);
  expect(chat.conversations).toHaveLength(MAX_CONVERSATIONS);
  expect(chat.conversations).toContain("Aaa#0001");
  expect(chat.conversations).not.toContain("Bbb#0001");
  // Five unread whispers in their tabs and All; reading one takes one off All.
  expect(chat.unread.all).toBe(5);
  chat = showChatTab(chat, "w:Ggg#0001");
  expect(chat.unread.all).toBe(4);
  chat = closeConversation(chat, "ccc#0001");
  expect(chat.unread.all).toBe(3);
  expect(chat.unread["w:Ccc#0001"]).toBeUndefined();
});

it("suggests whisper targets while the handle is typed, friends who are on first", () => {
  expect(whisperQuery("/w As")).toEqual({ command: "w", prefix: "As" });
  expect(whisperQuery("/whisper ")).toEqual({ command: "whisper", prefix: "" });
  expect(whisperQuery("/w Ash#1000 hi")).toBeNull();
  expect(whisperQuery("hello")).toBeNull();
  const candidates = [
    { handle: "Ashe#0002", online: false },
    { handle: "Ash#1000", online: true },
    { handle: "Bea#0003", online: true },
    { handle: "ash#1000", online: false },
  ];
  expect(whisperSuggestions(candidates, "as")).toEqual(["Ash#1000", "Ashe#0002"]);
  expect(whisperSuggestions(candidates, "ash#1")).toEqual(["Ash#1000"]);
  expect(whisperSuggestions(candidates, "")).toEqual(["Ash#1000", "Bea#0003", "Ashe#0002"]);
  expect(whisperSuggestions(candidates, "z")).toEqual([]);
  expect(whisperSuggestions(candidates, "", 1)).toEqual(["Ash#1000"]);
});
