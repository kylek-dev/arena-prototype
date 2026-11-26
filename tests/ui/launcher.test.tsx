import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { App } from "../../web/src/app/App";
import { initial } from "../../web/src/domain/preferences/schema";
import { PlayOnline } from "../../web/src/features/play-online/PlayOnline";
import { createAccount } from "../../web/src/runtime/account";
import { installInput } from "../../web/src/runtime/input-router";
import { beforeMatch, characterKits, customizerVisible } from "../../web/src/state/appearance";
import { accountService, friendsService } from "../../web/src/state/account";
import { createFriends } from "../../web/src/runtime/friends";
import { persistence, preferences } from "../../web/src/state/config";
import { matchQueue } from "../../web/src/state/network";
import { engine, error, focus, interaction, selectedLayout } from "../../web/src/state/session";
import { friendsWindowVisible, playWindowVisible } from "../../web/src/state/windows";
import { humanKit } from "../domain/kit-fixture";
import { fakeQueue } from "./fake-queue";
import { snapshot } from "./fixtures";

let router: ReturnType<typeof installInput>;

async function logIn(account: string | null) {
  const service = createAccount({
    fetch: async () =>
      Response.json({
        account,
        admin: false,
        login: true,
        handle: account ? "Rook#0042" : null,
        rename_at: null,
      }),
    navigate: () => {},
    pageProtocol: "https:",
  });
  await service.refresh();
  accountService.set(service);
}

beforeEach(() => {
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  preferences.set(initial());
  engine.set(snapshot());
  error.set("");
  focus.set(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  characterKits.set([humanKit()]);
  customizerVisible.set(false);
  playWindowVisible.set(false);
  friendsWindowVisible.set(false);
  matchQueue.set(fakeQueue().queue);
  const canvas = document.getElementById("arena") as HTMLCanvasElement;
  router = installInput(canvas, () => {});
  canvas.focus();
});
afterEach(() => {
  router.dispose();
  cleanup();
  matchQueue.set(null);
  accountService.set(null);
  friendsService.set(null);
  characterKits.set([]);
  clearTimeout(persistence.timer);
});

const launcher = () => within(screen.getByRole("navigation", { name: "Windows" }));

it("opens each window from a one-letter button with a full tooltip", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.hover(launcher().getByRole("button", { name: "Play" }));
  await waitFor(() =>
    expect(screen.getByRole("tooltip").textContent).toMatch(/queue for 1v1, 2v2 or 3v3/),
  );
  await user.click(launcher().getByRole("button", { name: "Play" }));
  const play = screen.getByRole("complementary", { name: "Play" });
  // Queues, custom games and practice; no account or party section in game.
  expect(within(play).getByRole("button", { name: "2v2" })).toBeTruthy();
  expect(within(play).getByRole("button", { name: "Host custom match" })).toBeTruthy();
  expect(within(play).getByRole("combobox", { name: "Practice mode" })).toBeTruthy();
  expect(within(play).getByRole("button", { name: "Reset practice" })).toBeTruthy();
  expect(within(play).queryByRole("group", { name: "Party" })).toBeNull();
  expect(within(play).queryByRole("group", { name: "Account" })).toBeNull();
  expect(within(play).queryByRole("button", { name: "Invite friends" })).toBeNull();
  expect(launcher().getByRole("button", { name: "Play" }).getAttribute("aria-pressed")).toBe(
    "true",
  );
  await user.click(launcher().getByRole("button", { name: "Play" }));
  expect(screen.queryByRole("complementary", { name: "Play" })).toBeNull();

  await user.click(launcher().getByRole("button", { name: "Friends" }));
  expect(screen.getByRole("complementary", { name: "Friends" }).textContent).toMatch(/account/);
  await user.click(launcher().getByRole("button", { name: "Spells" }));
  expect(screen.getByRole("dialog", { name: "Spells" })).toBeTruthy();
  await user.click(launcher().getByRole("button", { name: "Spells" }));
  expect(interaction.get().panel).toBeNull();
  await user.click(launcher().getByRole("button", { name: "Customize character" }));
  expect(customizerVisible.get()).toBe(true);
});

it("closes the launcher's windows with Escape before opening the menu", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(launcher().getByRole("button", { name: "Play" }));
  await user.click(launcher().getByRole("button", { name: "Friends" }));
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  await user.keyboard("{Escape}");
  expect(playWindowVisible.get()).toBe(false);
  expect(friendsWindowVisible.get()).toBe(false);
  expect(interaction.get().panel).toBeNull();
  await user.keyboard("{Escape}");
  expect(interaction.get().panel).toBe("menu");
});

it("shows friends to a logged-in player, and asks guests to log in on the home screen", async () => {
  await logIn(null);
  friendsWindowVisible.set(true);
  const { unmount } = render(<App />);
  expect(screen.getByRole("complementary", { name: "Friends" }).textContent).toMatch(
    /Log in on the home screen/,
  );
  unmount();
  await logIn("00112233445566ff");
  friendsService.set(
    createFriends({
      fetch: async () => ({
        ok: true,
        status: 200,
        json: async () => ({ friends: [], blocked: [] }),
      }),
      now: () => 0,
      later: () => () => {},
    }),
  );
  render(<App />);
  expect(
    within(screen.getByRole("complementary", { name: "Friends" })).getByRole("group", {
      name: "Friends",
    }),
  ).toBeTruthy();
});

it("keeps account and party controls on the home screen", async () => {
  await logIn(null);
  render(<PlayOnline />);
  expect(screen.getByRole("group", { name: "Account" })).toBeTruthy();
  expect(screen.getByRole("group", { name: "Party" })).toBeTruthy();
});

it("disables Spells while spectating, and moves with the layout editor", async () => {
  engine.set({ ...snapshot(), spectating: true });
  render(<App />);
  const spells = launcher().getByRole("button", { name: "Spells" });
  expect(spells.getAttribute("aria-disabled")).toBe("true");
  spells.click();
  expect(interaction.get().panel).toBeNull();
  act(() => {
    engine.set(snapshot());
    interaction.set({ ...interaction.get(), layout: true });
  });
  act(() => screen.getByRole("button", { name: "Edit launcher" }).focus());
  expect(selectedLayout.get()).toBe("launcher");
});

it("goes back to the home screen from the menu, outside a match", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Home screen" }));
  expect(beforeMatch.get()).toBe(true);
  beforeMatch.set(false);
});

it("shows a party already formed, with Leave party, but no invite or join", () => {
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: false, queue: null });
  playWindowVisible.set(true);
  render(<App />);
  const party = within(screen.getByRole("complementary", { name: "Play" })).getByRole("group", {
    name: "Party",
  });
  expect(party.textContent).toContain("K7P2QX");
  expect(within(party).queryByRole("button", { name: /Copy invite link/ })).toBeNull();
  within(party).getByRole("button", { name: "Leave party" }).click();
  expect(fake.sent().at(-1)).toEqual({ type: "leave_party" });
});

it("closes the windows when a full-screen overlay opens, so Escape reaches it", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(launcher().getByRole("button", { name: "Play" }));
  await user.click(launcher().getByRole("button", { name: "Customize character" }));
  expect(playWindowVisible.get()).toBe(false);
  // With a window reopened behind the customizer, Escape still closes the customizer.
  act(() => friendsWindowVisible.set(true));
  await user.keyboard("{Escape}");
  expect(customizerVisible.get()).toBe(false);
});

it("opens the Play window from /queue", async () => {
  const { runSlash, pageCommandContext } = await import("../../web/src/runtime/chat-commands");
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  render(<App />);
  act(() => {
    runSlash("/queue", pageCommandContext(fake.queue));
  });
  expect(screen.getByRole("complementary", { name: "Play" })).toBeTruthy();
});
