import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { App } from "../../web/src/app/App";
import { initial } from "../../web/src/domain/preferences/schema";
import { installInput } from "../../web/src/runtime/input-router";
import { persistence, preferences } from "../../web/src/state/config";
import { matchQueue } from "../../web/src/state/network";
import { engine, error, focus, interaction } from "../../web/src/state/session";
import { fakeQueue } from "./fake-queue";
import { snapshot } from "./fixtures";

let router: ReturnType<typeof installInput>;
let fake: ReturnType<typeof fakeQueue>;

beforeEach(() => {
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  preferences.set(initial());
  engine.set(snapshot());
  error.set("");
  focus.set(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  const canvas = document.getElementById("arena") as HTMLCanvasElement;
  router = installInput(canvas, () => {});
  fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  canvas.focus();
});
afterEach(() => {
  router.dispose();
  cleanup();
  matchQueue.set(null);
  clearTimeout(persistence.timer);
});

const chatMessage = (fields: object) =>
  fake.say({
    type: "chat",
    from: "Rook#0042",
    admin: false,
    text: "hi",
    channel: "public",
    ...fields,
  });

it("opens with Enter, keeps keys from combat while typing, and sends with Enter", async () => {
  const user = userEvent.setup();
  render(<App />);
  const field = screen.getByRole("textbox", { name: "Message to Public" });
  await user.keyboard("{Enter}");
  expect(document.activeElement).toBe(field);
  await user.keyboard("{w>}");
  expect(router.sample()[1]).toBe(0);
  await user.keyboard("{/w}ello{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "chat", channel: "public", text: "wello" });
  expect((field as HTMLInputElement).value).toBe("");
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  // Escape leaves the field with the draft kept, and opens no menu.
  await user.keyboard("{Enter}draft{Escape}");
  expect(interaction.get().panel).toBeNull();
  expect((field as HTMLInputElement).value).toBe("draft");
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
});

it("marks unread tabs, switches channels and shows refusals in words", async () => {
  const user = userEvent.setup();
  render(<App />);
  chatMessage({ channel: "team", text: "push left", admin: true });
  const team = screen.getByRole("tab", { name: /Team/ });
  expect(within(team).getByLabelText("1 unread")).toBeTruthy();
  await user.click(team);
  expect(within(team).queryByLabelText("1 unread")).toBeNull();
  const log = screen.getByRole("log", { name: "Team" });
  expect(log.textContent).toContain("[Team] Rook#0042");
  expect(log.textContent).toContain("push left");
  expect(within(log).getByText("Admin")).toBeTruthy();
  await user.click(screen.getByRole("textbox", { name: "Message to Team" }));
  await user.keyboard("gg{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "chat", channel: "team", text: "gg" });
  fake.say({ type: "refused", code: "not_in_match" });
  expect(screen.getByRole("alert").textContent).toMatch(/not in a match/);
  // A shared code right after a chat post is the chat box's too.
  await user.click(screen.getByRole("button", { name: "OK" }));
  await user.click(screen.getByRole("textbox", { name: "Message to Team" }));
  await user.keyboard("again{Enter}");
  fake.say({ type: "refused", code: "rate_limited" });
  expect(screen.getByRole("alert").textContent).toMatch(/too fast/);
  expect(fake.queue.state.get().notice).toBeNull();
});

it("refuses over-long messages before sending", async () => {
  const user = userEvent.setup();
  render(<App />);
  const field = screen.getByRole("textbox", { name: "Message to Public" });
  await user.click(field);
  fireEvent.change(field, { target: { value: "x".repeat(201) } });
  const before = fake.sent().length;
  await user.keyboard("{Enter}");
  expect(fake.sent()).toHaveLength(before);
  expect(screen.getByText(/Too long: 200 characters at most/)).toBeTruthy();
});

it("whispers from a name, answers the last whisperer and ignores players", async () => {
  const user = userEvent.setup();
  render(<App />);
  chatMessage({ text: "hello all" });
  await user.click(screen.getByRole("button", { name: "[Public] Rook#0042: options" }));
  await user.click(screen.getByRole("menuitem", { name: "Whisper" }));
  expect(screen.getByRole("tab", { name: "Whispers", selected: true })).toBeTruthy();
  expect((screen.getByRole("textbox", { name: "Whisper to" }) as HTMLInputElement).value).toBe(
    "Rook#0042",
  );
  await user.click(screen.getByRole("textbox", { name: "Message to Whispers" }));
  await user.keyboard("psst{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "whisper", to: "Rook#0042", text: "psst" });
  chatMessage({ channel: "whisper", to: "Rook#0042", from: "Mee#0001", text: "psst" });
  expect(screen.getByRole("log", { name: "Whispers" }).textContent).toContain("To Rook#0042");

  await user.click(screen.getByRole("tab", { name: "Public" }));
  await user.click(screen.getByRole("button", { name: "[Public] Rook#0042: options" }));
  await user.click(screen.getByRole("menuitem", { name: "Ignore" }));
  expect(fake.sent().at(-1)).toEqual({ type: "ignore", handle: "Rook#0042" });
  expect(screen.getByRole("log", { name: "Public" }).textContent).not.toContain("hello all");
  // A new connection hears the ignore again.
  act(() => fake.latest().onclose?.());
  fake.queue.connect();
  fake.open();
  expect(fake.sent()).toEqual([
    { type: "hello", guest: "07".repeat(16) },
    { type: "ignore", handle: "Rook#0042" },
  ]);
});

it("undoes an ignore the door refused, but not after a later request", () => {
  fake.queue.ignore("Ghost#0009", true);
  fake.say({ type: "refused", code: "no_such_player" });
  expect(fake.queue.chat.get().ignored).toEqual([]);
  expect(fake.queue.chat.get().error).toBe("no_such_player");
  fake.queue.ignore("Rook#0042", true);
  fake.queue.say("public", "hi");
  fake.say({ type: "refused", code: "rate_limited" });
  expect(fake.queue.chat.get().ignored).toEqual(["Rook#0042"]);
});

it("gives spectators their own tab first", () => {
  engine.set({ ...snapshot(), spectating: true });
  render(<App />);
  expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
    "Spectators",
    "Public",
    "Whispers",
  ]);
  expect(screen.getByRole("textbox", { name: "Message to Spectators" })).toBeTruthy();
});

it("leaves Enter alone where gameplay does not own the keys, and never refocuses on remount", async () => {
  const user = userEvent.setup();
  const { unmount } = render(<App />);
  const field = () => screen.getByRole("textbox", { name: /Message to/ });
  act(() => interaction.set({ ...interaction.get(), layout: true }));
  await user.keyboard("{Enter}");
  expect(document.activeElement).not.toBe(field());
  act(() => interaction.set({ ...interaction.get(), layout: false }));
  await user.keyboard("{Enter}");
  expect(document.activeElement).toBe(field());
  await user.keyboard("{Escape}");
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  unmount();
  render(<App />);
  expect(document.activeElement).not.toBe(field());
  // A hidden box does not open.
  act(() =>
    preferences.set({
      ...preferences.get(),
      profiles: preferences.get().profiles.map((p) => ({
        ...p,
        config: {
          ...p.config,
          frames: { ...p.config.frames, chat: { ...p.config.frames.chat, visible: false } },
        },
      })),
    }),
  );
  await user.keyboard("{Enter}");
  expect(screen.queryByRole("textbox", { name: /Message to/ })).toBeNull();
});

it("leaves other requests' refusals to the queue, even right after a chat post", () => {
  fake.queue.say("public", "hi");
  fake.say({ type: "refused", code: "party_full" });
  expect(fake.queue.state.get().notice).toBe("party_full");
  expect(fake.queue.chat.get().error).toBeNull();
});

it("replays the newest ignores on a new connection, quietly", () => {
  for (const name of ["Aaa", "Bbb", "Ccc", "Ddd", "Eee", "Fff", "Ggg"])
    fake.queue.ignore(`${name}#0001`, true);
  act(() => fake.latest().onclose?.());
  fake.queue.connect();
  fake.open();
  const replayed = fake.sent().filter((m) => m.type === "ignore");
  expect(replayed.map((m) => m.handle)).toEqual([
    "Ccc#0001",
    "Ddd#0001",
    "Eee#0001",
    "Fff#0001",
    "Ggg#0001",
  ]);
  // An ignored player who is offline now: expected, so no error and no undo.
  fake.say({ type: "refused", code: "no_such_player" });
  expect(fake.queue.chat.get().error).toBeNull();
  expect(fake.queue.chat.get().ignored).toHaveLength(7);
});

it("starts a seated player on Team and hints a missing whisper target", async () => {
  const user = userEvent.setup();
  engine.set({ ...snapshot(), networked: true });
  render(<App />);
  expect(screen.getByRole("tab", { name: "Team", selected: true })).toBeTruthy();
  await user.click(screen.getByRole("tab", { name: "Whispers" }));
  await user.click(screen.getByRole("textbox", { name: "Message to Whispers" }));
  await user.keyboard("hello{Enter}");
  expect(fake.sent().some((m) => m.type === "whisper")).toBe(false);
  expect(screen.getByText(/Pick who to whisper/)).toBeTruthy();
});
