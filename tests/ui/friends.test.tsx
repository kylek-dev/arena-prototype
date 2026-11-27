import userEvent from "@testing-library/user-event";
import { WindowDock } from "../../web/src/features/launcher/WindowDock";
import { installInput } from "../../web/src/runtime/input-router";
import { friendsWindowVisible } from "../../web/src/state/windows";
import { TooltipProvider } from "../../web/src/ui/Tooltip";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { Friends } from "../../web/src/features/friends/Friends";
import { FriendsWindow } from "../../web/src/features/launcher/FriendsWindow";
import { beforeMatch } from "../../web/src/state/appearance";
import { InvitePrompt } from "../../web/src/features/friends/InvitePrompt";
import { createAccount } from "../../web/src/runtime/account";
import { createFriends } from "../../web/src/runtime/friends";
import { followAccount } from "../../web/src/runtime/social";
import { accountService, friendsService } from "../../web/src/state/account";
import { matchQueue } from "../../web/src/state/network";
import { fakeQueue } from "./fake-queue";
import { RECONNECT_MS, type SocialDeps } from "../../web/src/runtime/social";
import { createFriends as makeFriends, READ_GAP_MS } from "../../web/src/runtime/friends";

/** Social deps under test control: leadership, timers and visibility. */
function social(leading = true) {
  const timers: { run: () => void; ms: number; cancelled: boolean }[] = [];
  let lead: (() => void) | null = null;
  let visible: (() => void) | null = null;
  const deps: SocialDeps & {
    timers: typeof timers;
    elect(): void;
    show(): void;
  } = {
    leader(run) {
      if (leading) run();
      else lead = run;
      return () => {};
    },
    later(run, ms) {
      const timer = { run, ms, cancelled: false };
      timers.push(timer);
      return () => {
        timer.cancelled = true;
      };
    },
    onVisible(run) {
      visible = run;
      return () => {};
    },
    timers,
    elect: () => lead?.(),
    show: () => visible?.(),
  };
  return deps;
}

type Reply = { status: number; body?: unknown } | "offline";
const ROOK = {
  account: "00112233445566ff",
  admin: false,
  login: true,
  handle: "Rook#0042",
  rename_at: null,
};
const LIST = {
  friends: [
    { handle: "Ash#1000", relation: "friend", presence: "online" },
    { handle: "Zed#0001", relation: "friend", presence: "offline" },
    { handle: "Cyd#0003", relation: "incoming" },
    { handle: "Dot#0004", relation: "outgoing" },
  ],
  blocked: ["Eve#0005"],
};

/** A scripted door answering both account and friends routes. */
function fakeDoor(routes: Record<string, Reply | Reply[]>) {
  const calls: { url: string; method: string; body?: unknown }[] = [];
  const fetch = async (url: string, init?: RequestInit) => {
    calls.push({
      url,
      method: init?.method ?? "GET",
      ...(init?.body ? { body: JSON.parse(String(init.body)) } : {}),
    });
    const route = routes[url];
    const reply = Array.isArray(route) ? (route.length > 1 ? route.shift() : route[0]) : route;
    if (!reply || reply === "offline") throw new TypeError("offline");
    return {
      ok: reply.status >= 200 && reply.status < 300,
      status: reply.status,
      json: async () => {
        if (reply.body === undefined) throw new SyntaxError("no body");
        return reply.body;
      },
    };
  };
  const account = createAccount({ fetch, navigate: () => {}, pageProtocol: "https:" });
  // A clock that moves 10 s per read, so list reads are never spaced out.
  let clock = 0;
  const friends = createFriends({
    fetch,
    now: () => (clock += 10_000),
    later: () => () => {},
  });
  accountService.set(account);
  friendsService.set(friends);
  return { account, friends, calls };
}

afterEach(() => {
  cleanup();
  beforeMatch.set(false);
  accountService.set(null);
  friendsService.set(null);
  matchQueue.set(null);
});

/** Opens a row's ⋮ menu and returns it. */
function menuOf(handle: string) {
  fireEvent.click(screen.getByRole("button", { name: `Actions for ${handle}` }));
  return within(screen.getByRole("menu", { name: `Actions for ${handle}` }));
}

it("lists requests, friends with presence, sent requests and blocks as rows", async () => {
  const { account } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  await account.refresh();
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  await act(async () => {
    render(<Friends />);
  });
  const rows = within(screen.getByRole("list", { name: "Friends list" }))
    .getAllByRole("listitem")
    .map((row) => row.querySelector(".friend-name")?.textContent);
  expect(rows).toEqual([
    "Cyd#0003Friend request",
    "Ash#1000Online",
    "Zed#0001Offline",
    "Dot#0004Request sent",
    "Eve#0005Blocked",
  ]);
  expect(screen.getByRole("button", { name: "Accept Cyd#0003" })).toBeTruthy();
  // Only friends who are on can be invited or whispered.
  expect(menuOf("Ash#1000").getByRole("menuitem", { name: "Invite to group" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Actions for Ash#1000" }));
  expect(menuOf("Zed#0001").queryByRole("menuitem", { name: "Invite to group" })).toBeNull();
  expect(menuOf("Dot#0004").getByRole("menuitem", { name: "Withdraw request" })).toBeTruthy();
  expect(menuOf("Eve#0005").getByRole("menuitem", { name: "Unblock" })).toBeTruthy();
});

it("offers no party invitations in the in-game Friends window", async () => {
  // Decided 2026-11-26: parties form outside the game; the ⋮ menu in game
  // keeps Whisper, Remove friend and Block.
  const { account } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  await account.refresh();
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  await act(async () => {
    render(<FriendsWindow />, { wrapper: TooltipProvider });
  });
  const menu = menuOf("Ash#1000");
  expect(menu.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
    "Whisper",
    "Remove friend",
    "Block",
  ]);
  expect(screen.queryByRole("menuitem", { name: "Invite to group" })).toBeNull();
  expect(fake.sent().some((m) => m.type === "invite")).toBe(false);
});

it("adds through a prompt, accepts, removes and blocks after confirming, and words refusals", async () => {
  const { account, calls } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
    "/api/friends/request": [
      { status: 404, body: { error: "no_such_player" } },
      { status: 200, body: { relation: "outgoing" } },
    ],
    "/api/friends/accept": { status: 204 },
    "/api/friends/remove": { status: 204 },
    "/api/friends/block": { status: 204 },
  });
  await account.refresh();
  await act(async () => {
    render(<Friends />);
  });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  const form = within(screen.getByRole("form", { name: "Add or block a player" }));
  const field = form.getByRole("textbox", { name: "Player name and number" });
  expect(field.getAttribute("placeholder")).toBe("Name#0000");
  fireEvent.change(field, { target: { value: "Nobody" } });
  await act(async () => fireEvent.click(form.getByRole("button", { name: "Send request" })));
  expect(screen.getByRole("alert").textContent).toBe(
    "Enter the player's name and number, for example Name#0000.",
  );
  expect(calls.some((c) => c.url === "/api/friends/request")).toBe(false);
  fireEvent.change(field, { target: { value: "Nobody#0009" } });
  await act(async () => fireEvent.click(form.getByRole("button", { name: "Send request" })));
  expect(screen.getByRole("alert").textContent).toMatch(/^No player has that name and number\./);
  await act(async () => fireEvent.click(form.getByRole("button", { name: "Send request" })));
  expect(calls.filter((c) => c.url === "/api/friends/request").at(-1)?.body).toEqual({
    handle: "Nobody#0009",
  });
  expect(screen.queryByRole("form", { name: "Add or block a player" })).toBeNull();
  expect(screen.getByRole("status").textContent).toMatch(/^Friend request sent to Nobody#0009\./);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Accept Cyd#0003" })));
  expect(calls.at(-2)).toEqual({
    url: "/api/friends/accept",
    method: "POST",
    body: { handle: "Cyd#0003" },
  });
  // Removing and blocking ask once more.
  fireEvent.click(menuOf("Ash#1000").getByRole("menuitem", { name: "Remove friend" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Cancel" }));
  expect(calls.some((c) => c.url === "/api/friends/remove")).toBe(false);
  fireEvent.click(screen.getByRole("menuitem", { name: "Block" }));
  await act(async () => fireEvent.click(screen.getByRole("menuitem", { name: "Confirm block" })));
  expect(calls.some((c) => c.url === "/api/friends/block")).toBe(true);
  expect(screen.getByRole("status").textContent).toMatch(
    /^Ash#1000 is blocked and can no longer send you requests or invitations\./,
  );
});

it("asks a player without a name to choose one, and shows nothing to guests", async () => {
  const { account } = fakeDoor({
    "/api/auth/me": { status: 200, body: { ...ROOK, handle: null } },
  });
  await account.refresh();
  const { container, rerender } = render(<Friends />);
  expect(screen.getByText("Choose a player name before adding friends.")).toBeTruthy();
  const guest = fakeDoor({ "/api/auth/me": { status: 200, body: { ...ROOK, account: null } } });
  await guest.account.refresh();
  rerender(<Friends />);
  expect(container.textContent).toBe("");
});

it("invites a friend over the queue and says when it arrived", async () => {
  const { account } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  await account.refresh();
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  await act(async () => {
    render(<Friends />);
  });
  fireEvent.click(menuOf("Ash#1000").getByRole("menuitem", { name: "Invite to group" }));
  fake.open();
  expect(fake.sent().at(-1)).toEqual({ type: "invite", handle: "Ash#1000" });
  fake.say({ type: "invite_sent", to: "Ash#1000" });
  expect(screen.getByRole("status").textContent).toMatch(/^Invitation sent to Ash#1000\./);
});

it("prompts for a friend's invite and joins by its code, or just dismisses", () => {
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  expect(fake.sent()).toEqual([{ type: "hello", guest: "07".repeat(16) }]);
  render(<InvitePrompt />);
  fake.say({ type: "invite", from: "Ash#1000", kind: "lobby", code: "K7P2QX" });
  fake.say({ type: "invite", from: "Bea#0002", kind: "party", code: "M9Q3RT" });
  // A newer invite from the same friend replaces theirs.
  fake.say({ type: "invite", from: "Ash#1000", kind: "lobby", code: "W4X5YZ" });
  const prompts = screen.getAllByRole("group");
  expect(prompts.map((p) => p.getAttribute("aria-label"))).toEqual([
    "Invite from Bea#0002",
    "Invite from Ash#1000",
  ]);
  fireEvent.click(
    within(screen.getByRole("group", { name: "Invite from Ash#1000" })).getByRole("button", {
      name: "Accept",
    }),
  );
  expect(fake.sent().at(-1)).toEqual({ type: "join_lobby", code: "W4X5YZ" });
  const sentBefore = fake.sent().length;
  fireEvent.click(
    within(screen.getByRole("group", { name: "Invite from Bea#0002" })).getByRole("button", {
      name: "Decline",
    }),
  );
  expect(fake.sent()).toHaveLength(sentBefore);
  expect(screen.queryByRole("group")).toBeNull();
  // At most a few prompts at once.
  for (const from of ["Aa#0001", "Bb#0002", "Cc#0003", "Dd#0004"])
    fake.say({
      type: "invite",
      from: from.replace(/(\w)(\w)/, "$1$2x"),
      kind: "party",
      code: "K7P2QX",
    });
  expect(screen.getAllByRole("group")).toHaveLength(3);
});

it("connects logged-in players, refreshes friends on pushes and forgets them on logout", async () => {
  const { account, friends, calls } = fakeDoor({
    "/api/auth/me": [
      { status: 200, body: ROOK },
      { status: 200, body: { ...ROOK, account: null, handle: null } },
    ],
    "/api/friends": { status: 200, body: LIST },
  });
  const fake = fakeQueue();
  const stop = followAccount(account, fake.queue, friends, social());
  expect(fake.sockets).toHaveLength(0);
  await account.refresh();
  expect(fake.sockets).toHaveLength(1);
  fake.open();
  await act(async () => {});
  const reads = () => calls.filter((c) => c.url === "/api/friends").length;
  expect(reads()).toBe(1);
  fake.say({ type: "friends_changed" });
  await act(async () => {});
  expect(reads()).toBe(2);
  await account.refresh();
  expect(friends.state.get().list).toBeNull();
  fake.say({ type: "friends_changed" });
  await act(async () => {});
  expect(reads()).toBe(2);
  stop();
});

it("keeps a listening connection quiet, reopens it with a backoff, but never after a newer tab", async () => {
  const { account, friends } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  const fake = fakeQueue();
  const deps = social();
  const stop = followAccount(account, fake.queue, friends, deps);
  await account.refresh();
  fake.open();
  // The door restarts: no "lost" bar for a player who never queued.
  act(() => fake.latest().onclose?.());
  expect(fake.queue.state.get().connection).toBe("offline");
  expect(fake.queue.state.get().ended).toBe("lost");
  expect(deps.timers.map((t) => t.ms)).toEqual([RECONNECT_MS[0]]);
  act(() => deps.timers[0].run());
  expect(fake.sockets).toHaveLength(2);
  // Fails to connect: waits longer.
  act(() => fake.latest().onclose?.());
  expect(deps.timers.at(-1)?.ms).toBe(RECONNECT_MS[1]);
  act(() => deps.timers.at(-1)?.run());
  fake.open();
  // Another tab took over: stay away, even when shown again.
  fake.say({ type: "error", code: "replaced", message: "replaced" });
  expect(fake.queue.state.get().connection).toBe("offline");
  const before = deps.timers.length;
  act(() => deps.show());
  expect(deps.timers).toHaveLength(before);
  expect(fake.sockets).toHaveLength(3);
  stop();
});

it("shows a drop to a player who queued, and listens only in the elected tab", async () => {
  const { account, friends } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  const fake = fakeQueue();
  const deps = social(false);
  const stop = followAccount(account, fake.queue, friends, deps);
  await account.refresh();
  expect(fake.sockets).toHaveLength(0);
  act(() => deps.elect());
  expect(fake.sockets).toHaveLength(1);
  fake.open();
  fake.queue.queue("1v1");
  act(() => fake.latest().onclose?.());
  expect(fake.queue.state.get().connection).toBe("ended");
  stop();
});

it("tells the player why an accepted invite could not be joined", () => {
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  render(<InvitePrompt />);
  fake.say({ type: "invite", from: "Ash#1000", kind: "lobby", code: "K7P2QX" });
  fireEvent.click(screen.getByRole("button", { name: "Accept" }));
  fake.say({ type: "refused", code: "lobby_full" });
  const failed = screen.getByRole("group", { name: "Invite could not be joined" });
  expect(failed.textContent).toMatch(
    /The invitation from Ash#1000 could not be accepted\. This custom lobby is full\./,
  );
  fireEvent.click(within(failed).getByRole("button", { name: "OK" }));
  expect(screen.queryByRole("group")).toBeNull();
  // A refusal that is not about joining stays a plain notice.
  fake.queue.invite("Ash#1000");
  fake.say({ type: "refused", code: "rate_limited" });
  expect(fake.queue.state.get().notice).toBe("rate_limited");
  expect(fake.queue.state.get().inviting).toBeNull();
});

it("waits for one invite at a time and words invite refusals", async () => {
  const { account } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  await account.refresh();
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  await act(async () => {
    render(<Friends />);
  });
  fireEvent.click(menuOf("Ash#1000").getByRole("menuitem", { name: "Invite to group" }));
  // While one invitation is on its way, no other is offered.
  expect(menuOf("Ash#1000").queryByRole("menuitem", { name: "Invite to group" })).toBeNull();
  fake.queue.invite("Ash#1000");
  expect(fake.sent().filter((m) => m.type === "invite")).toHaveLength(1);
  fake.say({ type: "invite_sent", to: "Ash#1000" });
  fireEvent.click(screen.getByRole("button", { name: "Actions for Ash#1000" }));
  expect(menuOf("Ash#1000").getByRole("menuitem", { name: "Invite to group" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("status")).toBeNull();
});

it("spaces list reads and retries after the door's 429", async () => {
  let now = 0;
  const timers: { run: () => void; ms: number }[] = [];
  const answers = [429, 200, 200];
  let reads = 0;
  const friends = makeFriends({
    fetch: async () => {
      reads += 1;
      const status = answers.shift() ?? 200;
      return { ok: status === 200, status, json: async () => LIST };
    },
    now: () => now,
    later: (run, ms) => {
      timers.push({ run, ms });
      return () => {};
    },
  });
  void friends.refresh();
  await act(async () => {});
  // Refused: still loading, and a retry waits for the gap.
  expect(friends.state.get().status).toBe("loading");
  expect(timers.map((t) => t.ms)).toEqual([READ_GAP_MS]);
  now += READ_GAP_MS;
  await act(async () => timers[0].run());
  expect(friends.state.get().status).toBe("ready");
  // Three quick requests fold into one read after the gap.
  void friends.refresh();
  void friends.refresh();
  void friends.refresh();
  expect(reads).toBe(2);
  expect(timers).toHaveLength(2);
  now += READ_GAP_MS;
  await act(async () => timers[1].run());
  expect(reads).toBe(3);
});

it("closes only an open ⋮ menu on Escape, and answers a refused invitation", async () => {
  const { account } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  await account.refresh();
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  const canvas = document.getElementById("arena") as HTMLCanvasElement;
  const router = installInput(canvas, () => {});
  try {
    friendsWindowVisible.set(true);
    await act(async () => {
      render(
        <TooltipProvider>
          <WindowDock />
        </TooltipProvider>,
      );
    });
    menuOf("Ash#1000");
    // The first item has focus; Escape closes the menu, not the window.
    // (No Invite to group in game: decided 2026-11-26.)
    expect(document.activeElement?.textContent).toBe("Whisper");
    fireEvent.keyDown(document.activeElement!, { key: "Escape", code: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(friendsWindowVisible.get()).toBe(true);
    expect(document.activeElement?.getAttribute("aria-label")).toBe("Actions for Ash#1000");
    // An invitation sent before entering the game is still answered here.
    fake.queue.invite("Ash#1000");
    fake.say({ type: "refused", code: "offline" });
    expect(screen.getByRole("alert").textContent).toMatch(/^This friend is offline\./);
  } finally {
    router.dispose();
    friendsWindowVisible.set(false);
  }
});

it("blocks any player by name from the Add prompt, and a requester from their row", async () => {
  const { account, calls } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
    "/api/friends/block": { status: 204 },
  });
  await account.refresh();
  await act(async () => {
    render(<Friends />);
  });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  const form = within(screen.getByRole("form", { name: "Add or block a player" }));
  fireEvent.change(form.getByRole("textbox", { name: "Player name and number" }), {
    target: { value: "Spammer#0666" },
  });
  await act(async () => fireEvent.click(form.getByRole("button", { name: "Block player" })));
  expect(calls.filter((c) => c.url === "/api/friends/block").at(-1)?.body).toEqual({
    handle: "Spammer#0666",
  });
  expect(screen.queryByRole("form")).toBeNull();
  // An incoming request can be blocked from its ⋮ menu, after confirming.
  fireEvent.click(menuOf("Cyd#0003").getByRole("menuitem", { name: "Block" }));
  await act(async () => fireEvent.click(screen.getByRole("menuitem", { name: "Confirm block" })));
  expect(calls.filter((c) => c.url === "/api/friends/block").at(-1)?.body).toEqual({
    handle: "Cyd#0003",
  });
});

it("opens the ⋮ menu under its button, or above it near the bottom of the screen", async () => {
  const { account } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
  });
  await account.refresh();
  await act(async () => {
    render(<Friends />);
  });
  const button = screen.getByRole("button", { name: "Actions for Ash#1000" });
  const at = (top: number) =>
    (button.getBoundingClientRect = () =>
      ({
        top,
        bottom: top + 26,
        left: 300,
        right: 326,
        width: 26,
        height: 26,
        x: 300,
        y: top,
      }) as DOMRect);
  at(100);
  fireEvent.click(button);
  let list = screen.getByRole("menu", { name: "Actions for Ash#1000" });
  expect(list.style.top).toBe("130px");
  expect(list.style.right).toBe(`${innerWidth - 326}px`);
  fireEvent.click(button);
  at(innerHeight - 40);
  fireEvent.click(button);
  list = screen.getByRole("menu", { name: "Actions for Ash#1000" });
  expect(list.style.top).toBe("");
  expect(list.style.bottom).toBe("44px");
});

it("keeps the ⋮ menu open under real clicks while the game owns the keyboard", async () => {
  const user = userEvent.setup();
  const { account, calls } = fakeDoor({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/friends": { status: 200, body: LIST },
    "/api/friends/remove": { status: 204 },
  });
  await account.refresh();
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  const canvas = document.getElementById("arena") as HTMLCanvasElement;
  const router = installInput(canvas, () => {});
  canvas.focus();
  try {
    friendsWindowVisible.set(true);
    await act(async () => {
      render(
        <TooltipProvider>
          <WindowDock />
        </TooltipProvider>,
      );
    });
    await user.click(screen.getByRole("button", { name: "Actions for Ash#1000" }));
    // Let the focus policy run (it restores the game's focus in a microtask).
    await act(async () => {});
    const menu = screen.getByRole("menu", { name: "Actions for Ash#1000" });
    await user.click(within(menu).getByRole("menuitem", { name: "Remove friend" }));
    await act(async () => {});
    await user.click(screen.getByRole("menuitem", { name: "Confirm removal" }));
    expect(calls.some((c) => c.url === "/api/friends/remove")).toBe(true);
  } finally {
    router.dispose();
    friendsWindowVisible.set(false);
  }
});
