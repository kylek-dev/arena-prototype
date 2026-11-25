import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { Friends } from "../../web/src/features/friends/Friends";
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
  accountService.set(null);
  friendsService.set(null);
  matchQueue.set(null);
});

it("lists requests, friends with presence, sent requests and blocks", async () => {
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
  expect(screen.getByText("Friends · 1 online")).toBeTruthy();
  const friendsList = screen.getByRole("list", { name: "Friends list" });
  expect(within(friendsList).getByText("Ash#1000")).toBeTruthy();
  expect(within(friendsList).getByText("Online")).toBeTruthy();
  expect(within(friendsList).getByText("Offline")).toBeTruthy();
  // Only friends who are on can be invited.
  expect(screen.getByRole("button", { name: "Invite Ash#1000" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Invite Zed#0001" })).toBeNull();
  expect(screen.getByRole("button", { name: "Accept Cyd#0003" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Withdraw request to Dot#0004" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Unblock Eve#0005" })).toBeTruthy();
});

it("adds, accepts, removes and blocks through the door, and words refusals", async () => {
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
  const field = screen.getByRole("textbox", { name: /add a friend/i });
  const add = screen.getByRole("button", { name: "Add friend" });
  fireEvent.change(field, { target: { value: "Nobody" } });
  expect((add as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(field, { target: { value: "Nobody#0009" } });
  await act(async () => fireEvent.click(add));
  expect(screen.getByRole("alert").textContent).toMatch(/no player has that name/i);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Add friend" })));
  expect(calls.filter((c) => c.url === "/api/friends/request").at(-1)?.body).toEqual({
    handle: "Nobody#0009",
  });
  expect(screen.getByRole("status").textContent).toMatch(/request sent to Nobody#0009/i);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Accept Cyd#0003" })));
  expect(calls.at(-2)).toEqual({
    url: "/api/friends/accept",
    method: "POST",
    body: { handle: "Cyd#0003" },
  });
  // Removing and blocking ask first.
  const ash = within(screen.getByRole("list", { name: "Friends list" }))
    .getByText("Ash#1000")
    .closest("li") as HTMLElement;
  fireEvent.click(within(ash).getByRole("button", { name: "Remove" }));
  fireEvent.click(within(ash).getByRole("button", { name: "Keep" }));
  expect(calls.some((c) => c.url === "/api/friends/remove")).toBe(false);
  fireEvent.click(within(ash).getByRole("button", { name: "Block" }));
  await act(async () =>
    fireEvent.click(within(ash).getByRole("button", { name: "Block Ash#1000" })),
  );
  expect(calls.some((c) => c.url === "/api/friends/block")).toBe(true);
  expect(screen.getByRole("status").textContent).toMatch(/blocked Ash#1000/i);
});

it("asks a player without a name to choose one, and shows nothing to guests", async () => {
  const { account } = fakeDoor({
    "/api/auth/me": { status: 200, body: { ...ROOK, handle: null } },
  });
  await account.refresh();
  const { container, rerender } = render(<Friends />);
  expect(screen.getByText(/choose your name to add friends/i)).toBeTruthy();
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
  fireEvent.click(screen.getByRole("button", { name: "Invite Ash#1000" }));
  fake.open();
  expect(fake.sent().at(-1)).toEqual({ type: "invite", handle: "Ash#1000" });
  fake.say({ type: "invite_sent", to: "Ash#1000" });
  expect(screen.getByRole("status").textContent).toMatch(/^Invite sent to Ash#1000\./);
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
  expect(failed.textContent).toMatch(/could not join Ash#1000: that custom lobby is full/i);
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
  const invite = screen.getByRole("button", { name: "Invite Ash#1000" }) as HTMLButtonElement;
  fireEvent.click(invite);
  expect(invite.disabled).toBe(true);
  fake.queue.invite("Ash#1000");
  expect(fake.sent().filter((m) => m.type === "invite")).toHaveLength(1);
  fake.say({ type: "invite_sent", to: "Ash#1000" });
  expect(invite.disabled).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "OK" }));
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
