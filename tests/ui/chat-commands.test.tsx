import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { App } from "../../web/src/app/App";
import { initial } from "../../web/src/domain/preferences/schema";
import { CHAT_COMMANDS, runSlash, type CommandContext } from "../../web/src/runtime/chat-commands";
import { installInput } from "../../web/src/runtime/input-router";
import { installSystemEvents } from "../../web/src/runtime/system-events";
import { createFriends } from "../../web/src/runtime/friends";
import { mapEditorVisible } from "../../web/src/state/map-editor";
import { characterKits, customizerVisible } from "../../web/src/state/appearance";
import { persistence, preferences } from "../../web/src/state/config";
import { matchQueue } from "../../web/src/state/network";
import { friendsWindowVisible, playWindowVisible } from "../../web/src/state/windows";
import {
  customLobbyVisible,
  engine,
  error,
  focus,
  interaction,
  scoreboardVisible,
} from "../../web/src/state/session";
import { createStore } from "../../web/src/state/store";
import { humanKit } from "../domain/kit-fixture";
import { fakeQueue } from "./fake-queue";
import { snapshot } from "./fixtures";

let fake: ReturnType<typeof fakeQueue>;
let said: string[];
let posts: { url: string; body: unknown }[];
let status = 204;
const ctx = (overrides: Partial<CommandContext> = {}): CommandContext => ({
  queue: fake.queue,
  admin: false,
  inMatch: false,
  spectating: false,
  say: (text) => said.push(text),
  fetch: async (url, init) => {
    posts.push({ url, body: JSON.parse(String(init?.body)) });
    return {
      ok: status < 300,
      status,
      json: async () => (status === 403 ? { error: "not_admin" } : status === 404 ? {} : {}),
    };
  },
  ...overrides,
});

beforeEach(() => {
  fake = fakeQueue();
  said = [];
  posts = [];
  status = 204;
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  customizerVisible.set(false);
  customLobbyVisible.set(false);
  mapEditorVisible.set(false);
});
afterEach(() => {
  cleanup();
  matchQueue.set(null);
});

it("opens every panel by command", () => {
  characterKits.set([humanKit()]);
  const opened = (command: string) => {
    interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
    expect(runSlash(command, ctx())).toBe(true);
  };
  opened("/spellbook");
  expect(interaction.get().panel).toBe("spellbook");
  opened("/keybinds");
  expect(interaction.get().panel).toBe("settings");
  opened("/options");
  expect(interaction.get().panel).toBe("settings");
  opened("/menu");
  expect(interaction.get().panel).toBe("menu");
  opened("/layout");
  expect(interaction.get().layout).toBe(true);
  opened("/quickbind");
  expect(interaction.get().quick).toBe(true);
  opened("/customize");
  expect(customizerVisible.get()).toBe(true);
  opened("/mapeditor");
  expect(mapEditorVisible.get()).toBe(true);
  opened("/queue");
  expect(playWindowVisible.get()).toBe(true);
  opened("/friends");
  expect(friendsWindowVisible.get()).toBe(true);
  // Panels that need something to show say so instead.
  runSlash("/lobby", ctx());
  runSlash("/scoreboard", ctx());
  expect(said).toEqual([
    "You are not in a custom lobby. Host or join one from the Play window (/host).",
    "No completed match is available.",
  ]);
  expect(scoreboardVisible.get()).toBe(false);
  runSlash("/customize", ctx({ inMatch: true }));
  expect(said.at(-1)).toBe("Customizing is unavailable during a match.");
  runSlash("/spellbook", ctx({ spectating: true }));
  expect(said.at(-1)).toBe("The spellbook is unavailable while spectating.");
});

it("prints help from the registry and refuses unknown or admin-only commands", () => {
  runSlash("/help", ctx());
  expect(said[0]).toMatch(/^Commands/);
  expect(said.filter((line) => line.startsWith("/"))).toHaveLength(
    CHAT_COMMANDS.filter((command) => !command.admin).length,
  );
  said = [];
  runSlash("/help w", ctx());
  expect(said).toEqual(["/whisper <Name#0000> <message> (also /w) — Whisper to a friend."]);
  said = [];
  runSlash("/dance", ctx());
  runSlash("/mute Rook#0042", ctx());
  expect(said).toEqual([
    "Unknown command /dance. Type /help for the list.",
    "Unknown command /mute. Type /help for the list.",
  ]);
  expect(posts).toEqual([]);
  expect(runSlash("just chatting", ctx())).toBe(false);
});

it("queues, whispers, replies and ignores", () => {
  fake.queue.connect();
  fake.open();
  runSlash("/queue 2v2", ctx());
  expect(fake.sent().at(-1)).toEqual({ type: "queue", mode: "2v2" });
  runSlash("/queue 4v4", ctx());
  expect(said.at(-1)).toMatch(/Specify a queue: \/queue 1v1, \/queue 2v2 or \/queue 3v3\./);
  runSlash("/w Ash#1000 hello there", ctx());
  expect(fake.sent().at(-1)).toEqual({ type: "whisper", to: "Ash#1000", text: "hello there" });
  runSlash("/r again", ctx());
  expect(fake.sent().at(-1)).toEqual({ type: "whisper", to: "Ash#1000", text: "again" });
  runSlash("/w Ash", ctx());
  expect(said.at(-1)).toMatch(/Usage: \/w Name#0000/);
  runSlash("/ignore Ash#1000", ctx());
  expect(fake.sent().at(-1)).toEqual({ type: "ignore", handle: "Ash#1000" });
  runSlash("/unignore Ash#1000", ctx());
  expect(fake.sent().at(-1)).toEqual({ type: "unignore", handle: "Ash#1000" });
});

it("sends admin commands to the door, which has the last word", async () => {
  runSlash("/mute Rook#0042 30", ctx({ admin: true }));
  await act(async () => {});
  expect(posts.at(-1)).toEqual({
    url: "/api/admin/mute",
    body: { handle: "Rook#0042", minutes: 30 },
  });
  expect(said.at(-1)).toBe("Rook#0042 is muted for 30 minutes.");
  runSlash("/unmute Rook#0042", ctx({ admin: true }));
  await act(async () => {});
  expect(posts.at(-1)?.body).toEqual({ handle: "Rook#0042", minutes: 0 });
  status = 403;
  runSlash("/clearname Rook#0042", ctx({ admin: true }));
  await act(async () => {});
  expect(posts.at(-1)).toEqual({ url: "/api/admin/handles/clear", body: { handle: "Rook#0042" } });
  expect(said.at(-1)).toBe("This command is available to administrators only.");
  runSlash("/mute Rook#0042 forever", ctx({ admin: true }));
  expect(said.at(-1)).toMatch(/1 to 10080 minutes/);
  status = 404;
  runSlash("/mute Rook#0042", ctx({ admin: true }));
  await act(async () => {});
  expect(said.at(-1)).toBe("This server does not offer administrator tools.");
});

it("applies the queue buttons' checks, and keeps the lobby out of a live match", () => {
  fake.queue.connect();
  fake.open();
  fake.say({ type: "party", code: "M9Q3RT", members: 2, leader: false, queue: null });
  runSlash("/queue 2v2", ctx());
  expect(said.at(-1)).toBe("Only the party leader can start a search.");
  expect(fake.sent().some((m) => m.type === "queue")).toBe(false);
  fake.say({ type: "party", code: "M9Q3RT", members: 2, leader: true, queue: null });
  runSlash("/queue 1v1", ctx());
  expect(said.at(-1)).toBe("The 1v1 queue does not accept parties.");
  runSlash("/queue 2v2", ctx());
  expect(fake.sent().at(-1)).toEqual({ type: "queue", mode: "2v2" });
  fake.say({ type: "queued", mode: "2v2", waiting: 2, waited_ms: 0, expected_ms: null });
  runSlash("/queue 2v2", ctx());
  expect(said.at(-1)).toBe("You are already searching for a match.");
  fake.say({
    type: "lobby",
    code: "K7P2QX",
    mode: "1v1",
    settings: { version: 1, map: "colosseum", round_seconds: 600, allow_spectators: true },
    spectator_slots: 4,
    you: 1,
    host: true,
    can_start: false,
    members: [{ id: 1, slot: "team_a", host: true, you: true, handle: "Rook#0042" }],
  });
  customizerVisible.set(true);
  runSlash("/lobby", ctx({ inMatch: true }));
  expect(customLobbyVisible.get()).toBe(false);
  expect(said.at(-1)).toBe("The custom lobby is unavailable during a match.");
  runSlash("/lobby", ctx());
  expect(customLobbyVisible.get()).toBe(true);
  expect(customizerVisible.get()).toBe(false);
});

it("treats a bare slash as nothing to send", () => {
  expect(runSlash("/", ctx())).toBe(true);
  expect(said).toEqual([]);
  expect(runSlash("/ help", ctx())).toBe(true);
  expect(said).toEqual(["Type the command directly after the slash, for example /help."]);
});

it("says nothing about the lobby when only the connection dropped", () => {
  const friends = createFriends({
    fetch: async () => ({ ok: false, status: 500, json: async () => null }),
    now: () => 0,
    later: () => () => {},
  });
  const stop = installSystemEvents(fake.queue, friends, createStore(null), null);
  fake.queue.connect();
  fake.open();
  const view = {
    type: "lobby",
    code: "K7P2QX",
    mode: "1v1",
    settings: { version: 1, map: "colosseum", round_seconds: 600, allow_spectators: true },
    spectator_slots: 4,
    you: 1,
    host: false,
    can_start: false,
    members: [{ id: 1, slot: "team_a", host: false, you: true, handle: "Rook#0042" }],
  };
  fake.say(view);
  // The door restarts: the state resets, which is not the player leaving.
  act(() => fake.latest().onclose?.());
  fake.queue.connect();
  fake.open();
  expect(fake.queue.chat.get().messages.map((m) => m.text)).toEqual([
    "You joined custom lobby K7P2QX.",
  ]);
  stop();
});

it("writes system lines for queue, lobby and friend events", async () => {
  const friends = createFriends({
    fetch: async () => ({
      ok: true,
      status: 200,
      json: async () => ({
        friends: [
          { handle: "Ash#1000", relation: "friend", presence: reads++ ? "online" : "offline" },
        ],
        blocked: [],
      }),
    }),
    now: () => (clock += 10_000),
    later: () => () => {},
  });
  let reads = 0;
  let clock = 0;
  const stop = installSystemEvents(fake.queue, friends, createStore(null), null);
  fake.queue.connect();
  fake.open();
  fake.say({
    type: "lobby",
    code: "K7P2QX",
    mode: "1v1",
    settings: { version: 1, map: "colosseum", round_seconds: 600, allow_spectators: true },
    spectator_slots: 4,
    you: 1,
    host: false,
    can_start: false,
    members: [{ id: 1, slot: "team_a", host: false, you: true, handle: "Rook#0042" }],
  });
  fake.say({ type: "lobby_left", reason: "kicked" });
  await friends.refresh();
  await friends.refresh();
  const lines = fake.queue.chat.get().messages.map((m) => m.text);
  expect(lines).toEqual([
    "You joined custom lobby K7P2QX.",
    "You were removed from the custom lobby.",
    "Ash#1000 is online.",
  ]);
  expect(fake.queue.chat.get().messages.every((m) => m.channel === "system")).toBe(true);
  stop();
});

describe("in the chat box", () => {
  let router: ReturnType<typeof installInput>;
  beforeEach(() => {
    document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
    preferences.set(initial());
    engine.set(snapshot());
    error.set("");
    focus.set(null);
    const canvas = document.getElementById("arena") as HTMLCanvasElement;
    router = installInput(canvas, () => {});
    matchQueue.set(fake.queue);
    fake.queue.connect();
    fake.open();
    canvas.focus();
  });
  afterEach(() => {
    router.dispose();
    clearTimeout(persistence.timer);
  });

  it("opens with / for a command, shows output in every tab, and sends // as text", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.keyboard("[Slash]");
    const field = screen.getByRole("textbox", { name: "Message to Public" }) as HTMLInputElement;
    expect(document.activeElement).toBe(field);
    expect(field.value).toBe("/");
    await user.keyboard("help{Enter}");
    expect(screen.getByRole("log", { name: "Public" }).textContent).toContain("Commands");
    await user.click(screen.getByRole("tab", { name: "Team" }));
    expect(screen.getByRole("log", { name: "Team" }).textContent).toContain("Commands");
    expect(fake.sent().some((m) => m.type === "chat")).toBe(false);
    await user.click(field.ownerDocument.querySelector(".chat-input") as HTMLElement);
    await user.keyboard("//shrug{Enter}");
    expect(fake.sent().at(-1)).toEqual({ type: "chat", channel: "team", text: "/shrug" });
    await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
    // `/` keeps a draft already typed, and a bare `/` sends nothing.
    await user.keyboard("{Enter}half{Escape}");
    await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
    await user.keyboard("[Slash]");
    expect(
      (screen.getByRole("textbox", { name: "Message to Team" }) as HTMLInputElement).value,
    ).toBe("half");
    await user.keyboard("{Control>}a{/Control}{Backspace}/{Enter}");
    expect(fake.sent().at(-1)).toEqual({ type: "chat", channel: "team", text: "/shrug" });
    expect(fake.sent().at(-1)).toEqual({ type: "chat", channel: "team", text: "/shrug" });
    await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  });
});
