import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import type { Snapshot } from "../../web/src/bridge/snapshot";
import { InvitePrompt } from "../../web/src/features/friends/InvitePrompt";
import { UnitMenu } from "../../web/src/features/unit-frames/UnitMenu";
import { matchQueue } from "../../web/src/state/network";
import { engine, focus } from "../../web/src/state/session";
import { closeUnitMenu, openUnitMenu } from "../../web/src/state/unit-menu";
import { fakeQueue } from "./fake-queue";

// 1v1 challenges between players in a hub world (ADR 0060): the unit
// menu's item, the cards, and what the door's answers show.

const TICKET = "ab".repeat(40);
let fake: ReturnType<typeof fakeQueue>;
beforeEach(() => {
  fake = fakeQueue();
  matchQueue.set(fake.queue);
  focus.set(null);
});
afterEach(() => {
  cleanup();
  closeUnitMenu();
  matchQueue.set(null);
  engine.set(null);
});

const unit = (actor: number, tag: string, handle?: string) => ({
  actor,
  tag,
  id: `world-actor-${actor}`,
  name: handle ?? tag,
  health: 100,
  maxHealth: 100,
  ...(handle ? { handle } : {}),
});

/** In a hub world beside Zed and a training enemy. */
function inHub() {
  act(() => {
    fake.queue.joinHub();
  });
  fake.open();
  fake.say({
    type: "world",
    instance: "00000000000000ab",
    server: "wss://world.test/play",
    ticket: TICKET,
    expires: 2_000_000_000,
  });
  act(() =>
    engine.set({
      hub: true,
      networked: true,
      player: unit(0, "Player", "Rook#0042"),
      roster: [unit(0, "Player", "Rook#0042"), unit(1, "Opponent1"), unit(3, "Team1", "Zed#0001")],
    } as unknown as Snapshot),
  );
}

const menuOn = (tag: string, actor: number) =>
  act(() =>
    openUnitMenu({ tag: tag as never, unitId: `world-actor-${actor}`, x: 10, y: 10, frame: null }),
  );
const challengeItem = () => screen.getByRole("menuitem", { name: /^Challenge to a 1v1/ });

it("challenges a player in the hub, waits, and can withdraw", () => {
  inHub();
  render(
    <>
      <UnitMenu />
      <InvitePrompt />
    </>,
  );
  // A training enemy is no player.
  menuOn("Opponent1", 1);
  expect(challengeItem().textContent).toContain("Only players can be challenged.");
  menuOn("Team1", 3);
  expect(challengeItem().getAttribute("aria-disabled")).not.toBe("true");
  fireEvent.click(challengeItem());
  expect(fake.sent().at(-1)).toEqual({ type: "challenge", handle: "Zed#0001" });
  fake.say({ type: "challenge_sent", to: "Zed#0001" });
  expect(screen.getByRole("group", { name: "Challenge to Zed#0001" }).textContent).toContain(
    "Your challenge to Zed#0001 is waiting for an answer.",
  );
  menuOn("Team1", 3);
  expect(challengeItem().textContent).toContain("waiting for an answer");
  fireEvent.click(screen.getByRole("button", { name: "Withdraw" }));
  expect(fake.sent().at(-1)).toEqual({ type: "withdraw_challenge" });
  expect(screen.queryByRole("group", { name: "Challenge to Zed#0001" })).toBeNull();
});

it("tells the challenger how it ended, or why it was refused", () => {
  inHub();
  render(<InvitePrompt />);
  act(() => fake.queue.challenge("Zed#0001"));
  fake.say({ type: "challenge_sent", to: "Zed#0001" });
  fake.say({ type: "challenge_ended", with: "Zed#0001", reason: "declined" });
  expect(screen.getByRole("group", { name: "Challenge notice" }).textContent).toContain(
    "Zed#0001 declined your challenge.",
  );
  fireEvent.click(screen.getByRole("button", { name: "OK" }));
  expect(screen.queryByRole("group", { name: "Challenge notice" })).toBeNull();
  // Expired while waiting.
  act(() => fake.queue.challenge("Zed#0001"));
  fake.say({ type: "challenge_sent", to: "Zed#0001" });
  fake.say({ type: "challenge_ended", with: "Zed#0001", reason: "expired" });
  expect(screen.getByRole("group", { name: "Challenge notice" }).textContent).toContain(
    "did not answer your challenge in time",
  );
  // Refused: told, and nothing is waiting.
  act(() => fake.queue.challenge("Nobody#0009"));
  fake.say({ type: "refused", code: "no_such_player" });
  expect(screen.getByRole("group", { name: "Challenge notice" }).textContent).toContain(
    "Nobody#0009 cannot be challenged here.",
  );
  expect(fake.queue.state.get().challenging).toBeNull();
  // A refused new challenge leaves the earlier one standing.
  act(() => fake.queue.challenge("Zed#0001"));
  fake.say({ type: "challenge_sent", to: "Zed#0001" });
  act(() => fake.queue.challenge("Ash#1000"));
  fake.say({ type: "refused", code: "rate_limited" });
  expect(fake.queue.state.get().challenging).toMatchObject({ to: "Zed#0001", sent: true });
});

it("answers another player's challenge: accepting leads to the match", () => {
  inHub();
  render(<InvitePrompt />);
  fake.say({ type: "challenged", from: "Zed#0001", remaining_ms: 30_000 });
  const card = () => screen.queryByRole("group", { name: "Challenge from Zed#0001" });
  expect(card()?.textContent).toContain("Zed#0001 challenges you to a 1v1.");
  // Busy: refused, and the card stays to accept later.
  fireEvent.click(screen.getByRole("button", { name: "Accept" }));
  expect(fake.sent().at(-1)).toEqual({
    type: "answer_challenge",
    from: "Zed#0001",
    accept: true,
  });
  fake.say({ type: "refused", code: "party_busy" });
  expect(card()).not.toBeNull();
  expect(screen.getByRole("group", { name: "Challenge notice" }).textContent).toContain(
    "while searching for a match",
  );
  fireEvent.click(screen.getByRole("button", { name: "Accept" }));
  expect(screen.getByRole("button", { name: "Accepted" }).hasAttribute("disabled")).toBe(true);
  const tickets = fake.tickets.length;
  fake.say({
    type: "match",
    mode: "1v1",
    match_id: "00000000000000cd",
    seat: 1,
    server: "wss://world.test/play",
    ticket: TICKET,
    expires: 2_000_000_000,
  });
  expect(fake.tickets.length).toBe(tickets + 1);
  expect(card()).toBeNull();
});

it("declines, and drops cards the challenger takes back", () => {
  inHub();
  render(<InvitePrompt />);
  fake.say({ type: "challenged", from: "Zed#0001", remaining_ms: 30_000 });
  fireEvent.click(screen.getByRole("button", { name: "Decline" }));
  expect(fake.sent().at(-1)).toEqual({
    type: "answer_challenge",
    from: "Zed#0001",
    accept: false,
  });
  expect(screen.queryByRole("group", { name: /Challenge/ })).toBeNull();
  // Withdrawn or expired before an answer: the card goes quietly.
  fake.say({ type: "challenged", from: "Zed#0001", remaining_ms: 30_000 });
  fake.say({ type: "challenged", from: "Ash#1000", remaining_ms: 30_000 });
  fake.say({ type: "challenge_ended", with: "Zed#0001", reason: "withdrawn" });
  fake.say({ type: "challenge_ended", with: "Ash#1000", reason: "expired" });
  expect(screen.queryByRole("group", { name: /Challenge/ })).toBeNull();
  // An answer to a challenge already gone is told and the card removed.
  fake.say({ type: "challenged", from: "Zed#0001", remaining_ms: 30_000 });
  fireEvent.click(screen.getByRole("button", { name: "Accept" }));
  fake.say({ type: "refused", code: "challenge_gone" });
  expect(screen.queryByRole("group", { name: "Challenge from Zed#0001" })).toBeNull();
  expect(screen.getByRole("group", { name: "Challenge notice" }).textContent).toContain(
    "no longer open",
  );
});

it("offers challenges only in the hub", () => {
  act(() =>
    engine.set({
      networked: true,
      player: unit(0, "Player", "Rook#0042"),
      roster: [unit(0, "Player", "Rook#0042"), unit(3, "Team1", "Zed#0001")],
    } as unknown as Snapshot),
  );
  render(<UnitMenu />);
  menuOn("Team1", 3);
  expect(challengeItem().textContent).toContain("Challenges are available in the hub.");
});

it("tells challenges both ways apart, and forgets them out of the hub or after a match", () => {
  let clock = 1000;
  fake = fakeQueue(() => clock);
  matchQueue.set(fake.queue);
  inHub();
  render(<InvitePrompt />);
  // Zed challenged first; then this player challenged Zed.
  fake.say({ type: "challenged", from: "Zed#0001", remaining_ms: 30_000 });
  clock = 5000;
  act(() => fake.queue.challenge("Zed#0001"));
  fake.say({ type: "challenge_sent", to: "Zed#0001" });
  const held = () => screen.queryByRole("group", { name: "Challenge from Zed#0001" });
  const own = () => screen.queryByRole("group", { name: "Challenge to Zed#0001" });
  // Zed's expires first: the card goes, this player's own still waits.
  fake.say({ type: "challenge_ended", with: "Zed#0001", reason: "expired" });
  expect(held()).toBeNull();
  expect(own()).not.toBeNull();
  // Declined reaches only one's own; withdrawn only a held card.
  fake.say({ type: "challenged", from: "Zed#0001", remaining_ms: 30_000 });
  fake.say({ type: "challenge_ended", with: "Zed#0001", reason: "declined" });
  expect(own()).toBeNull();
  expect(held()).not.toBeNull();
  // Leaving the hub: the door closed them all.
  act(() => fake.queue.challenge("Zed#0001"));
  fake.say({ type: "challenge_sent", to: "Zed#0001" });
  fake.say({ type: "hub_left", reason: "left" });
  expect(held()).toBeNull();
  expect(own()).toBeNull();
  // A match (from the queue too) ends every challenge.
  fake.say({ type: "challenged", from: "Ash#1000", remaining_ms: 30_000 });
  fake.say({
    type: "match",
    mode: "2v2",
    match_id: "00000000000000cd",
    seat: 1,
    server: "wss://world.test/play",
    ticket: TICKET,
    expires: 2_000_000_000,
  });
  expect(screen.queryByRole("group", { name: /Challenge/ })).toBeNull();
});
