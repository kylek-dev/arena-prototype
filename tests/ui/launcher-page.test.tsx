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
});

const page = () => screen.getByRole("region", { name: "Launcher" });
const account = () => within(screen.getByRole("group", { name: "Account" }));

it("covers the arena until Enter, which starts a practice round", () => {
  const reset = vi.spyOn(commands, "dispatch").mockImplementation(() => {});
  engine.set(snapshot());
  render(<App />);
  expect(page()).toBeTruthy();
  // Enter is one key away.
  expect(document.activeElement?.textContent).toBe("Enter the arena");
  // Nothing of the game shows meanwhile, and play waits.
  expect(screen.queryByRole("navigation", { name: "Windows" })).toBeNull();
  expect(blocksGameplay()).toBe(true);
  expect(screen.getByRole("link", { name: "Privacy notice" }).getAttribute("href")).toBe(
    "privacy.html",
  );
  fireEvent.click(screen.getByRole("button", { name: "Enter the arena" }));
  expect(beforeMatch.get()).toBe(false);
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

it("shows the account in every state the door can be in", async () => {
  // Still asking the door.
  accountService.set(
    createAccount({
      fetch: () => new Promise(() => {}),
      navigate: () => {},
      pageProtocol: "https:",
    }),
  );
  const { unmount } = render(<LauncherPage />, { wrapper: TooltipProvider });
  expect(account().getByText("Checking your account.")).toBeTruthy();
  unmount();
  // Unreachable: the browser's own guest name.
  await door({ "/api/auth/me": "offline" });
  const second = render(<LauncherPage />, { wrapper: TooltipProvider });
  expect(screen.getByRole("group", { name: "Account" }).textContent).toBe(
    "Accounts cannot be reached at the moment. You will play as the guest Guest#7070.",
  );
  second.unmount();
  // A server without accounts.
  await door({ "/api/auth/me": { status: 200, body: { ...GUEST, login: false } } });
  const third = render(<LauncherPage />, { wrapper: TooltipProvider });
  expect(screen.getByRole("group", { name: "Account" }).textContent).toBe(
    "This server does not offer accounts. You will play as the guest Guest#7070.",
  );
  third.unmount();
  // A guest: their guest name, Log in and Sign up; no friends yet.
  await door({ "/api/auth/me": { status: 200, body: GUEST } });
  const fourth = render(<LauncherPage />, { wrapper: TooltipProvider });
  expect(screen.getByRole("group", { name: "Account" }).textContent).toMatch(
    /^You are playing as the guest Guest#7070\. Log in to keep a player name/,
  );
  expect(account().getByRole("link", { name: "Log in" }).getAttribute("href")).toBe(
    "/api/auth/login",
  );
  expect(account().getByRole("link", { name: "Sign up" })).toBeTruthy();
  expect(screen.queryByRole("heading", { name: "Friends and party" })).toBeNull();
  fourth.unmount();
  // Logged in without a name yet: choosing one comes first.
  await door({ "/api/auth/me": { status: 200, body: { ...ROOK, handle: null } } });
  const fifth = render(<LauncherPage />, { wrapper: TooltipProvider });
  expect(account().getByText("You are logged in without a player name.")).toBeTruthy();
  expect(screen.getByRole("group", { name: "Choose your name" })).toBeTruthy();
  fifth.unmount();
  // Logged in with a name.
  await door({ "/api/auth/me": { status: 200, body: ROOK }, "/api/friends": "offline" });
  render(<LauncherPage />, { wrapper: TooltipProvider });
  expect(account().getByText("Rook#0042")).toBeTruthy();
  for (const name of ["Change name", "Log out", "Log out everywhere", "Delete account…"])
    expect(account().getByRole("button", { name })).toBeTruthy();
});

it("deletes the account only after confirming", async () => {
  const { calls } = await door({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/auth/delete": { status: 204 },
    "/api/friends": "offline",
  });
  render(<LauncherPage />, { wrapper: TooltipProvider });
  fireEvent.click(account().getByRole("button", { name: "Delete account…" }));
  const confirm = screen.getByRole("alertdialog", { name: "Delete account" });
  fireEvent.click(within(confirm).getByRole("button", { name: "Keep account" }));
  expect(calls.some((c) => c.url === "/api/auth/delete")).toBe(false);
  fireEvent.click(account().getByRole("button", { name: "Delete account…" }));
  await act(async () =>
    fireEvent.click(
      within(screen.getByRole("alertdialog", { name: "Delete account" })).getByRole("button", {
        name: "Delete account",
      }),
    ),
  );
  expect(calls.filter((c) => c.url === "/api/auth/delete")).toEqual([
    { url: "/api/auth/delete", method: "POST" },
  ]);
});

it("forms parties here: friends with Invite to group, and the party with Leave party", async () => {
  await door({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  fake.queue.connect();
  fake.open();
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: true, queue: null });
  await act(async () => {
    render(<LauncherPage />, { wrapper: TooltipProvider });
  });
  const section = within(screen.getByRole("region", { name: "Friends and party" }));
  const party = section.getByRole("group", { name: "Party" });
  expect(party.textContent).toContain("You are in a party of 2. You lead the party.");
  // The party code stays internal.
  expect(party.textContent).not.toContain("K7P2QX");
  fireEvent.click(section.getByRole("button", { name: "Actions for Ash#1000" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Invite to group" }));
  expect(fake.sent().at(-1)).toEqual({ type: "invite", handle: "Ash#1000" });
  fireEvent.click(within(party).getByRole("button", { name: "Leave party" }));
  expect(fake.sent().at(-1)).toEqual({ type: "leave_party" });
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
