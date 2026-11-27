import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "../../web/src/app/App";
import { initial } from "../../web/src/domain/preferences/schema";
import { LauncherPage } from "../../web/src/features/launcher-page/LauncherPage";
import { createAccount } from "../../web/src/runtime/account";
import * as commands from "../../web/src/runtime/commands";
import { createFriends } from "../../web/src/runtime/friends";
import { accountService, friendsService } from "../../web/src/state/account";
import {
  beforeMatch,
  characterKits,
  customizerVisible,
  readPreview,
  returnToLauncher,
} from "../../web/src/state/appearance";
import { persistence, preferences } from "../../web/src/state/config";
import { matchQueue } from "../../web/src/state/network";
import { NO_NEWS, news } from "../../web/src/state/news";
import { blocksGameplay, engine, error, interaction } from "../../web/src/state/session";
import { TooltipProvider } from "../../web/src/ui/Tooltip";
import { humanKit } from "../domain/kit-fixture";
import { fakeQueue } from "./fake-queue";
import { snapshot } from "./fixtures";

type Reply = { status: number; body?: unknown } | "offline";
const GUEST = { account: null, admin: false, login: true, handle: null, rename_at: null };
const ROOK = { ...GUEST, account: "00112233445566ff", handle: "Rook#0042" };
const LIST = {
  friends: [{ handle: "Ash#1000", relation: "friend", presence: "online" }],
  blocked: [],
};

/** Real account and friends services over a scripted door. */
async function door(routes: Record<string, Reply>) {
  const calls: { url: string; method: string }[] = [];
  const fetch = async (url: string, init?: RequestInit) => {
    calls.push({ url, method: init?.method ?? "GET" });
    const reply = routes[url];
    if (!reply || reply === "offline") throw new TypeError("offline");
    return {
      ok: reply.status >= 200 && reply.status < 300,
      status: reply.status,
      json: async () => reply.body ?? null,
    };
  };
  const account = createAccount({ fetch, navigate: () => {}, pageProtocol: "https:" });
  accountService.set(account);
  friendsService.set(createFriends({ fetch, now: () => 0, later: () => () => {} }));
  await act(() => account.refresh());
  return { account, calls };
}

let fake: ReturnType<typeof fakeQueue>;
beforeEach(() => {
  // News is read on every page show; most tests do not care.
  vi.stubGlobal("fetch", async () => Response.json({ news: [] }));
  news.set(NO_NEWS);
  preferences.set(initial());
  engine.set(null);
  error.set("");
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  characterKits.set([humanKit()]);
  customizerVisible.set(false);
  beforeMatch.set(true);
  fake = fakeQueue();
  matchQueue.set(fake.queue);
});
afterEach(() => {
  cleanup();
  beforeMatch.set(false);
  matchQueue.set(null);
  accountService.set(null);
  friendsService.set(null);
  characterKits.set([]);
  clearTimeout(persistence.timer);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const page = () => screen.getByRole("region", { name: "Launcher" });

it("covers the arena until Enter, which starts a practice round and asks for the hub", () => {
  const reset = vi.spyOn(commands, "dispatch").mockImplementation(() => {});
  engine.set(snapshot());
  render(<App />);
  expect(page()).toBeTruthy();
  // Enter is one key away.
  expect(document.activeElement?.textContent).toBe("Enter the game");
  // Nothing of the game shows meanwhile, and play waits.
  expect(screen.queryByRole("navigation", { name: "Windows" })).toBeNull();
  expect(blocksGameplay()).toBe(true);
  expect(screen.getByRole("link", { name: "Privacy notice" }).getAttribute("href")).toBe(
    "privacy.html",
  );
  fireEvent.click(screen.getByRole("button", { name: "Enter the game" }));
  expect(beforeMatch.get()).toBe(false);
  // Into the hub (ADR 0056), with the saved look.
  expect(fake.queue.hub.get().status).toBe("joining");
  fake.open();
  expect(fake.sent().at(-1)?.type).toBe("hub_join");
  expect(blocksGameplay()).toBe(false);
  expect(reset).toHaveBeenCalledWith("reset");
  expect(screen.queryByRole("region", { name: "Launcher" })).toBeNull();
  expect(screen.getByRole("navigation", { name: "Windows" })).toBeTruthy();
});

it("says whether the arena is still loading, ready, or failed", () => {
  render(<LauncherPage />, { wrapper: TooltipProvider });
  expect(screen.getByRole("status").textContent).toBe(
    "The arena is loading. You can enter now or when it is ready.",
  );
  act(() => engine.set({ ...snapshot(), frames: 60 }));
  expect(screen.getByRole("status").textContent).toBe("The arena is ready.");
  act(() => error.set("WebGL2 is unavailable."));
  expect(screen.getByRole("status").textContent).toBe(
    "The arena could not be started. Enter it to see why.",
  );
});

it("puts the account in the top bar and has no friends list", async () => {
  await door({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  await act(async () => {
    render(<LauncherPage />, { wrapper: TooltipProvider });
  });
  const top = within(page().querySelector("header.launcher-top") as HTMLElement);
  expect(top.getByText("Arena Prototype")).toBeTruthy();
  expect(top.getByRole("button", { name: "Account menu for Rook#0042" })).toBeTruthy();
  // Friends live in game (F, and the hub's players); not on this page.
  expect(screen.queryByRole("list", { name: "Friends list" })).toBeNull();
  expect(screen.queryByText("Ash#1000")).toBeNull();
});

it("shows the party with Leave party", async () => {
  await door({ "/api/auth/me": { status: 200, body: ROOK } });
  fake.queue.connect();
  fake.open();
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: true, queue: null });
  await act(async () => {
    render(<LauncherPage />, { wrapper: TooltipProvider });
  });
  const party = screen.getByRole("group", { name: "Party" });
  expect(party.textContent).toContain("You are in a party of 2. You lead the party.");
  // The party code stays internal.
  expect(party.textContent).not.toContain("K7P2QX");
  fireEvent.click(within(party).getByRole("button", { name: "Leave party" }));
  expect(fake.sent().at(-1)).toEqual({ type: "leave_party" });
});

/** Stubs the door's news routes: a list that posts and deletes change. */
function newsDoor(items: object[] = []) {
  const calls: { url: string; method: string; body?: unknown }[] = [];
  let list = [...items];
  vi.stubGlobal("fetch", async (input: URL | string, init?: RequestInit) => {
    const url = new URL(String(input)).pathname;
    const method = init?.method ?? "GET";
    calls.push({ url, method, ...(init?.body ? { body: JSON.parse(String(init.body)) } : {}) });
    if (url === "/api/news") return Response.json({ news: list });
    if (url === "/api/admin/news" && method === "POST") {
      const { title } = JSON.parse(String(init?.body));
      if (title === "nope")
        return Response.json({ error: "invalid_news", problem: "long_title" }, { status: 422 });
      const item = { id: "00000000000000aa", title, body: "posted", posted: 1_795_000_000 };
      list = [item, ...list];
      return Response.json(item, { status: 201 });
    }
    const deleted = url.match(/^\/api\/admin\/news\/([0-9a-f]{16})$/);
    if (deleted && method === "DELETE") {
      list = list.filter((item) => (item as { id: string }).id !== deleted[1]);
      return new Response(null, { status: 204 });
    }
    return new Response(null, { status: 404 });
  });
  return calls;
}

const NEWS = {
  id: "0123456789abcdef",
  title: "The hub is open",
  body: "Meet other players.\nJump around.",
  posted: 1_795_000_000,
  author: "Jax#0417",
};

it("shows the news, newest first, as plain text", async () => {
  newsDoor([NEWS, { ...NEWS, id: "0123456789abcdee", title: "Older", body: "<b>not bold</b>" }]);
  await act(async () => {
    render(<LauncherPage />, { wrapper: TooltipProvider });
  });
  const news = within(screen.getByRole("region", { name: "News" }));
  const items = news.getAllByRole("article");
  expect(items.map((item) => item.querySelector("h3")?.textContent)).toEqual([
    "The hub is open",
    "Older",
  ]);
  expect(items[0].textContent).toContain("Jax#0417");
  expect(items[0].textContent).toContain("2026");
  expect(items[0].querySelector(".news-body")?.textContent).toBe(
    "Meet other players.\nJump around.",
  );
  // Never HTML.
  expect(items[1].querySelector("b")).toBeNull();
  expect(items[1].textContent).toContain("<b>not bold</b>");
  // Players get no news controls.
  expect(news.queryByRole("button", { name: "Post news" })).toBeNull();
  expect(news.queryByRole("button", { name: "Delete…" })).toBeNull();
});

it("words a post the door refuses for the role", async () => {
  await door({ "/api/auth/me": { status: 200, body: { ...ROOK, admin: true } } });
  newsDoor([]);
  await act(async () => {
    render(<LauncherPage />, { wrapper: TooltipProvider });
  });
  // The role was taken away since the page loaded: the door has the last word.
  vi.stubGlobal("fetch", async (input: URL | string) =>
    new URL(String(input)).pathname === "/api/news"
      ? Response.json({ news: [] })
      : Response.json({ error: "not_admin" }, { status: 403 }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Post news" }));
  const form = within(screen.getByRole("form", { name: "Post news" }));
  fireEvent.change(form.getByLabelText("Title"), { target: { value: "Hello" } });
  fireEvent.change(form.getByLabelText(/^News/), { target: { value: "Text" } });
  await act(async () => fireEvent.click(form.getByRole("button", { name: "Post" })));
  expect(screen.getByRole("alert").textContent).toMatch(/Only administrators can post news\./);
  // Cancel clears it.
  fireEvent.click(form.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("alert")).toBeNull();
});

it("says when there is no news, or it cannot be loaded", async () => {
  newsDoor([]);
  const { unmount } = await act(async () => render(<LauncherPage />, { wrapper: TooltipProvider }));
  expect(screen.getByText("There is no news yet.")).toBeTruthy();
  unmount();
  // A fresh page (news already shown stays shown when a later read fails).
  news.set(NO_NEWS);
  vi.stubGlobal("fetch", async () => new Response(null, { status: 404 }));
  await act(async () => {
    render(<LauncherPage />, { wrapper: TooltipProvider });
  });
  expect(screen.getByText("News cannot be loaded at the moment.")).toBeTruthy();
});

it("lets an admin post and delete news", async () => {
  await door({ "/api/auth/me": { status: 200, body: { ...ROOK, admin: true } } });
  const calls = newsDoor([NEWS]);
  await act(async () => {
    render(<LauncherPage />, { wrapper: TooltipProvider });
  });
  const news = within(screen.getByRole("region", { name: "News" }));
  fireEvent.click(news.getByRole("button", { name: "Post news" }));
  const form = within(screen.getByRole("form", { name: "Post news" }));
  const post = form.getByRole("button", { name: "Post" }) as HTMLButtonElement;
  expect(post.disabled).toBe(true);
  // A refusal is worded and the form stays.
  fireEvent.change(form.getByLabelText("Title"), { target: { value: "nope" } });
  fireEvent.change(form.getByLabelText(/^News/), { target: { value: "Text" } });
  await act(async () => fireEvent.click(post));
  expect(screen.getByRole("alert").textContent).toMatch(/up to 80 characters/);
  fireEvent.click(screen.getByRole("button", { name: "OK" }));
  fireEvent.change(form.getByLabelText("Title"), { target: { value: "Patch 0.2" } });
  await act(async () => fireEvent.click(post));
  expect(calls.find((c) => c.method === "POST")?.body).toEqual({ title: "nope", body: "Text" });
  expect(calls.filter((c) => c.method === "POST").at(-1)?.body).toEqual({
    title: "Patch 0.2",
    body: "Text",
  });
  expect(screen.queryByRole("form", { name: "Post news" })).toBeNull();
  expect(news.getAllByRole("article")[0].querySelector("h3")?.textContent).toBe("Patch 0.2");
  // Deleting asks once more.
  const first = within(news.getAllByRole("article")[0]);
  fireEvent.click(first.getByRole("button", { name: "Delete…" }));
  fireEvent.click(first.getByRole("button", { name: "Keep" }));
  expect(calls.some((c) => c.method === "DELETE")).toBe(false);
  fireEvent.click(first.getByRole("button", { name: "Delete…" }));
  await act(async () => fireEvent.click(first.getByRole("button", { name: "Delete" })));
  expect(calls.at(-2)).toEqual({ url: "/api/admin/news/00000000000000aa", method: "DELETE" });
  expect(news.getAllByRole("article").map((item) => item.querySelector("h3")?.textContent)).toEqual(
    ["The hub is open"],
  );
});

it("explains party refusals, such as an old party link that no longer works", async () => {
  render(<LauncherPage />, { wrapper: TooltipProvider });
  act(() => fake.queue.joinParty("K7P2QX"));
  fake.open();
  fake.say({ type: "refused", code: "no_such_party" });
  const alert = screen.getByRole("alert");
  expect(alert.textContent).toMatch(/^This party no longer exists\./);
  fireEvent.click(within(alert).getByRole("button", { name: "OK" }));
  expect(screen.queryByRole("alert")).toBeNull();
});

it("returns here from the game, with or without character kits", () => {
  characterKits.set([]);
  beforeMatch.set(false);
  returnToLauncher();
  expect(beforeMatch.get()).toBe(true);
  // No kit: no character close-up behind the page.
  expect(readPreview()[0]).toBe(0);
  characterKits.set([humanKit()]);
  expect(readPreview()[0]).toBe(1);
});
