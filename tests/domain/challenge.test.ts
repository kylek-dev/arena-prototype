import { expect, it } from "vitest";
import { challengeText, parseQueueMessage, requests } from "../../web/src/domain/matchmaking";

// The door's challenge messages (ADR 0060) and what the page says of them.

it("parses challenge messages strictly", () => {
  expect(
    parseQueueMessage('{"type":"challenged","from":"Rook#0042","remaining_ms":30000}'),
  ).toEqual({ type: "challenged", from: "Rook#0042", remainingMs: 30_000 });
  expect(parseQueueMessage('{"type":"challenge_sent","to":"Rook#0042"}')).toEqual({
    type: "challenge_sent",
    to: "Rook#0042",
  });
  expect(
    parseQueueMessage('{"type":"challenge_ended","with":"Rook#0042","reason":"declined"}'),
  ).toEqual({ type: "challenge_ended", with: "Rook#0042", reason: "declined" });
  for (const bad of [
    '{"type":"challenged","from":"Rook#0042"}',
    '{"type":"challenged","from":"Rook#0042","remaining_ms":-1}',
    '{"type":"challenged","from":"not a handle","remaining_ms":1}',
    '{"type":"challenge_sent","to":7}',
    '{"type":"challenge_ended","with":"Rook#0042","reason":"exploded"}',
    '{"type":"challenge_ended","reason":"declined"}',
  ]) {
    expect(parseQueueMessage(bad), bad).toBeNull();
  }
});

it("asks the door as it expects", () => {
  expect(JSON.parse(requests.challenge("Rook#0042"))).toEqual({
    type: "challenge",
    handle: "Rook#0042",
  });
  expect(JSON.parse(requests.answerChallenge("Rook#0042", false))).toEqual({
    type: "answer_challenge",
    from: "Rook#0042",
    accept: false,
  });
  expect(JSON.parse(requests.withdrawChallenge())).toEqual({ type: "withdraw_challenge" });
});

it("words each outcome formally, from either side", () => {
  const text = (code: string, mine: boolean) => challengeText({ with: "Rook#0042", code, mine });
  expect(text("declined", true)).toBe("Rook#0042 declined your challenge.");
  expect(text("expired", true)).toBe("Rook#0042 did not answer your challenge in time.");
  expect(text("expired", false)).toBe("The challenge from Rook#0042 has expired.");
  expect(text("busy", false)).toContain("is searching or in a lobby");
  expect(text("not_in_hub", true)).toBe("Challenges are available in the hub.");
  for (const code of ["withdrawn", "left", "challenge_busy", "challenge_gone", "rate_limited"]) {
    expect(text(code, true)).toMatch(/\.$/);
  }
  expect(text("something_new", true)).toBe("The challenge could not be made.");
});
