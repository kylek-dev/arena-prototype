import { expect, it } from "vitest";
import {
  isName,
  refusalRenameAt,
  renameWait,
  accountErrorText,
  errorCode,
  logoutTarget,
  parseMe,
  statusCode,
} from "../../web/src/domain/account";

const ME = {
  account: "00112233445566ff",
  admin: false,
  login: true,
  handle: "Rook#0042",
  rename_at: 1_800_000_000,
};

it("parses the door's me answer", () => {
  expect(parseMe(ME)).toEqual({
    account: "00112233445566ff",
    admin: false,
    login: true,
    handle: "Rook#0042",
    renameAt: 1_800_000_000,
  });
  expect(
    parseMe({ account: null, admin: false, login: true, handle: null, rename_at: null }),
  ).toEqual({ account: null, admin: false, login: true, handle: null, renameAt: null });
  // A door without login.
  expect(
    parseMe({ account: null, admin: false, login: false, handle: null, rename_at: null })?.login,
  ).toBe(false);
});

it("refuses malformed me answers", () => {
  for (const bad of [
    null,
    [],
    "me",
    { ...ME, login: "yes" },
    { ...ME, admin: 1 },
    { ...ME, account: "XYZ" },
    { ...ME, account: 42 },
    { ...ME, handle: "<b>#0001" },
    { ...ME, rename_at: -1 },
    { ...ME, rename_at: 1.5 },
    { ...ME, rename_at: "soon" },
  ])
    expect(parseMe(bad), JSON.stringify(bad)).toBeNull();
});

it("never treats a guest as an admin or named", () => {
  expect(parseMe({ ...ME, account: null, admin: true })).toEqual({
    account: null,
    admin: false,
    login: true,
    handle: null,
    renameAt: null,
  });
});

it("opens only web addresses as the logout page", () => {
  expect(logoutTarget({ logout_url: "https://api.workos.com/logout?x=1" })).toBe(
    "https://api.workos.com/logout?x=1",
  );
  expect(logoutTarget({ logout_url: "javascript:alert(1)" })).toBeNull();
  // Plain http only from a plain-http page (local development).
  expect(logoutTarget({ logout_url: "http://localhost:8000/" })).toBeNull();
  expect(logoutTarget({ logout_url: "http://localhost:8000/" }, "http:")).toBe(
    "http://localhost:8000/",
  );
  expect(logoutTarget({ logout_url: "javascript:alert(1)" }, "http:")).toBeNull();
  expect(logoutTarget({ logout_url: "data:text/html,hi" })).toBeNull();
  expect(logoutTarget({ logout_url: "/relative" })).toBeNull();
  expect(logoutTarget({ logout_url: `https://a.test/${"x".repeat(3000)}` })).toBeNull();
  expect(logoutTarget({})).toBeNull();
  expect(logoutTarget(null)).toBeNull();
});

it("words refusals, including the deletion retry", () => {
  expect(errorCode({ error: "unavailable" })).toBe("unavailable");
  expect(errorCode({ error: "<script>" })).toBeNull();
  expect(errorCode("unavailable")).toBeNull();
  // A bare gateway error is the proxy, not WorkOS: never "nothing was deleted".
  expect(statusCode(502)).toBe("offline");
  expect(statusCode(429)).toBe("rate_limited");
  expect(statusCode(403)).toBe("forbidden");
  expect(statusCode(500)).toBe("failed");
  expect(accountErrorText("unavailable")).toMatch(/nothing was deleted.*try again/i);
  expect(accountErrorText("something_new")).toBe(accountErrorText("failed"));
});

it("checks names as the door does, and words the rename wait", () => {
  for (const good of ["Rook", "abc", "Gladiator7", "abcdefghijklmnop"])
    expect(isName(good)).toBe(true);
  for (const bad of ["ab", "7Rook", "Ro ok", "abcdefghijklmnopq", "Rook#0042", "Röök", ""])
    expect(isName(bad), bad).toBe(false);
  expect(refusalRenameAt({ error: "too_soon", rename_at: 1_800_000_000 })).toBe(1_800_000_000);
  expect(refusalRenameAt({ error: "too_soon", rename_at: "later" })).toBeNull();
  const now = 1_000_000;
  expect(renameWait(null, now)).toBeNull();
  expect(renameWait(now, now)).toBeNull();
  expect(renameWait(now + 600, now)).toBe("within the hour");
  expect(renameWait(now + 5 * 3600, now)).toBe("in 5 hours");
  expect(renameWait(now + 3 * 86_400, now)).toBe("in 3 days");
  expect(accountErrorText("name_full")).toMatch(/pick another/i);
});
