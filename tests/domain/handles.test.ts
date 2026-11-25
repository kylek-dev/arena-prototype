import { expect, it } from "vitest";
import { handleName, isHandle, parseHandles, parseTags } from "../../web/src/domain/handles";
import { isHandles, isTags } from "../../web/src/domain/network";

it("accepts only server-shaped handles", () => {
  for (const good of ["Rook#0042", "abc#9999", "Gladiator7#0001", "abcdefghijklmnop#1234"])
    expect(isHandle(good), good).toBe(true);
  for (const bad of [
    "Rook",
    "Rook#42",
    "Rook#0000",
    "ab#0001",
    "abcdefghijklmnopq#0001",
    "7Rook#0001",
    "Ro ok#0001",
    "<b>x</b>#0001",
    42,
    null,
  ])
    expect(isHandle(bad), String(bad)).toBe(false);
  expect(handleName("Rook#0042")).toBe("Rook");
});

it("parses the relay strictly: even seat counts, handles or null", () => {
  expect(parseHandles(["Rook#0042", null])).toEqual(["Rook#0042", null]);
  expect(parseHandles([null, null, "Guest#0007", "Ash#1000"])).toHaveLength(4);
  expect(parseHandles(null)).toBeNull();
  expect(parseHandles(["Rook#0042"])).toBeNull();
  expect(parseHandles(Array(8).fill(null))).toBeNull();
  expect(parseHandles(["Rook#0042", "nope"])).toBeNull();
});

it("recognizes the Handles frame (variant 5)", () => {
  expect(isHandles(new Uint8Array([5, 2, 0, 0]))).toBe(true);
  expect(isHandles(new Uint8Array([4, 2, 0, 0]))).toBe(false);
  expect(isHandles(new Uint8Array([]))).toBe(false);
});

it("parses the tags relay strictly", () => {
  expect(parseTags([{ admin: true }, { admin: false }])).toEqual([
    { admin: true },
    { admin: false },
  ]);
  expect(parseTags(null)).toBeNull();
  expect(parseTags([{ admin: true }])).toBeNull();
  expect(parseTags([{ admin: "yes" }, { admin: false }])).toBeNull();
  expect(parseTags([{ admin: true, owner: true }, { admin: false }])).toBeNull();
  expect(parseTags([null, { admin: false }])).toBeNull();
  expect(isTags(new Uint8Array([6, 2, 1, 0]))).toBe(true);
  expect(isTags(new Uint8Array([5, 2]))).toBe(false);
});
