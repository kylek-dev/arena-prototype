import { expect, it } from "vitest";
import { anchorAt, layout, position } from "../../web/src/domain/layout/model";
import {
  CHAT_SIZE,
  clampChatSize,
  defaultConfig,
  validateConfig,
} from "../../web/src/domain/preferences/schema";

it("keeps the chat box between its minimum and maximum size", () => {
  expect(clampChatSize({ width: 10, height: 10 })).toEqual(CHAT_SIZE.min);
  expect(clampChatSize({ width: 5000, height: 5000 })).toEqual(CHAT_SIZE.max);
  expect(clampChatSize({ width: 400.4, height: 300.6 })).toEqual({ width: 400, height: 301 });
});

it("gives older or broken preferences the default size", () => {
  const { chatSize: _, ...old } = defaultConfig();
  expect(validateConfig(old).chatSize).toEqual(CHAT_SIZE.initial);
  expect(validateConfig({ ...old, chatSize: { width: "wide", height: null } }).chatSize).toEqual(
    CHAT_SIZE.initial,
  );
  expect(validateConfig({ ...old, chatSize: { width: 100, height: 9000 } }).chatSize).toEqual({
    width: CHAT_SIZE.min.width,
    height: CHAT_SIZE.max.height,
  });
});

it("keeps a resized frame's top-left corner where it was", () => {
  const viewport = { width: 1256, height: 696 };
  const before = layout(0.01, 0.7);
  const small = { width: 360, height: 210 };
  const at = position(before, viewport, small);
  const big = { width: 600, height: 300 };
  const after = anchorAt(before, at.left, at.top, viewport, big);
  const moved = position(after, viewport, big);
  expect(moved.left).toBeCloseTo(at.left, 6);
  expect(moved.top).toBeCloseTo(at.top, 6);
  // Grown past the bottom edge, it stays on screen instead.
  const tall = anchorAt(before, at.left, at.top, viewport, { width: 360, height: 500 });
  expect(tall.y).toBe(1);
  // A frame as big as the space has nowhere to travel.
  expect(anchorAt(before, 5, 5, viewport, { width: 1256, height: 696 })).toMatchObject({
    x: 0,
    y: 0,
  });
});
