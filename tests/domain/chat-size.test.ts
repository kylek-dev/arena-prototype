import { expect, it } from "vitest";
import { anchorAt, besideLauncher, layout, position } from "../../web/src/domain/layout/model";
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

it("places an attached chat beside the launcher, as wide as the space to the action bar", () => {
  const launcher = { right: 46, bottom: 960 };
  expect(besideLauncher(launcher, 512, 210, 1, 1835, 260)).toEqual({
    left: 42,
    top: 738,
    width: 450,
  });
  // Scaled frames take less layout width per screen pixel.
  expect(besideLauncher(launcher, 600, 210, 2, 1835, 260)?.width).toBe(269);
  // No bar level with it: the saved width applies.
  expect(besideLauncher(launcher, null, 210, 1, 1835, 260)?.width).toBeNull();
  // No room for the minimum width (a small screen, or a launcher at the
  // right edge): the chat keeps its own place.
  expect(besideLauncher(launcher, 300, 210, 1, 1835, 260)).toBeNull();
  expect(besideLauncher({ right: 1820, bottom: 960 }, null, 210, 1, 1835, 260)).toBeNull();
});

it("moves frames still at old defaults, and attaches only a chat nobody moved", () => {
  const { chatAttached: _a, chatSizeCustom: _c, ...base } = defaultConfig();
  const profile = (launcher: ReturnType<typeof layout>, chat: ReturnType<typeof layout>) => ({
    ...base,
    frames: { ...base.frames, launcher, chat },
  });
  for (const [launcher, chat] of [
    [layout(0.99, 0.74), layout(0.01, 0.7)],
    [layout(1, 0.66), layout(0.01, 0.97)],
  ]) {
    const migrated = validateConfig(profile(launcher, chat));
    expect(migrated.frames.launcher).toEqual(layout(0, 1));
    expect(migrated.chatAttached).toBe(true);
    expect(migrated.frames.chat).toEqual(defaultConfig().frames.chat);
    expect(migrated.chatSizeCustom).toBe(false);
  }
  // A launcher the player moved: the chat stays where it is, detached.
  const moved = validateConfig(profile(layout(0.5, 0.5), layout(0.01, 0.7)));
  expect(moved.frames.launcher).toEqual(layout(0.5, 0.5));
  expect(moved.chatAttached).toBe(false);
  expect(moved.frames.chat).toEqual(layout(0.01, 0.7));
  // A chat the player moved stays detached too.
  expect(validateConfig(profile(layout(0.99, 0.74), layout(0.3, 0.4))).chatAttached).toBe(false);
  expect(defaultConfig().chatAttached).toBe(true);
});

it("gives profiles still at the first default width the wider default, and keeps chosen sizes", () => {
  const saved = (chatSize: object, chatSizeCustom: boolean) =>
    validateConfig({ ...defaultConfig(), chatSize, chatSizeCustom });
  expect(saved({ width: 360, height: 210 }, false).chatSize).toEqual({ width: 432, height: 210 });
  // A player who chose 360 keeps it.
  expect(saved({ width: 360, height: 210 }, true).chatSize).toEqual({ width: 360, height: 210 });
  expect(saved({ width: 500, height: 300 }, false).chatSize).toEqual({ width: 500, height: 300 });
  // The height follows the launcher until the player sets one.
  expect(defaultConfig().chatHeightCustom).toBe(false);
  // An older profile (no flag) with a height of its own keeps it.
  const { chatHeightCustom: _, ...old } = defaultConfig();
  expect(validateConfig(old).chatHeightCustom).toBe(false);
  expect(validateConfig({ ...old, chatSize: { width: 360, height: 400 } }).chatHeightCustom).toBe(
    true,
  );
  expect(validateConfig({ ...defaultConfig(), chatHeightCustom: true }).chatHeightCustom).toBe(
    true,
  );
  expect(CHAT_SIZE.initial.width).toBe(432);
});
