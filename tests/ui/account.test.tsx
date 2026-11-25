import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { AccountPanel } from "../../web/src/features/account/AccountPanel";
import { createAccount } from "../../web/src/runtime/account";
import { accountService } from "../../web/src/state/account";

type Reply = { status: number; body?: unknown } | "offline";
const GUEST = { account: null, admin: false, login: true, handle: null, rename_at: null };
const ROOK = {
  account: "00112233445566ff",
  admin: false,
  login: true,
  handle: "Rook#0042",
  rename_at: null,
};

/** A real account service over a scripted door. */
function fakeAccount(routes: Record<string, Reply | Reply[]>) {
  const calls: { url: string; method: string; body?: unknown }[] = [];
  const visited: string[] = [];
  const account = createAccount({
    fetch: async (url, init) => {
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
    },
    navigate: (url) => visited.push(url),
    pageProtocol: "https:",
  });
  accountService.set(account);
  return { account, calls, visited };
}

afterEach(() => {
  cleanup();
  accountService.set(null);
});

it("offers login and sign-up to a guest, with the privacy notice", async () => {
  const { account } = fakeAccount({ "/api/auth/me": { status: 200, body: GUEST } });
  await account.refresh();
  render(<AccountPanel />);
  expect(screen.getByText(/playing as a guest/i)).toBeTruthy();
  expect(screen.getByRole("link", { name: "Log in" }).getAttribute("href")).toBe("/api/auth/login");
  expect(screen.getByRole("link", { name: "Sign up" }).getAttribute("href")).toBe(
    "/api/auth/login?sign_up",
  );
  expect(screen.getByRole("link", { name: "Privacy notice" }).getAttribute("href")).toBe(
    "privacy.html",
  );
});

it("shows nothing where the door has no login or cannot be reached", async () => {
  const off = fakeAccount({ "/api/auth/me": { status: 200, body: { ...GUEST, login: false } } });
  await off.account.refresh();
  const { container, rerender } = render(<AccountPanel />);
  expect(container.textContent).toBe("");
  const offline = fakeAccount({ "/api/auth/me": "offline" });
  await offline.account.refresh();
  rerender(<AccountPanel />);
  expect(offline.account.state.get().status).toBe("unavailable");
  expect(container.textContent).toBe("");
});

it("logs out through WorkOS's page, and everywhere through its own route", async () => {
  const { account, calls, visited } = fakeAccount({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/auth/logout": { status: 200, body: { logout_url: "https://auth.test/logout?s=1" } },
    "/api/auth/logout_everywhere": { status: 200, body: { logout_url: "https://auth.test/out" } },
  });
  await account.refresh();
  render(<AccountPanel />);
  expect(screen.getByText("Rook#0042")).toBeTruthy();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Log out" })));
  expect(calls.at(-1)).toEqual({ url: "/api/auth/logout", method: "POST" });
  expect(visited).toEqual(["https://auth.test/logout?s=1"]);
  // The page is leaving; a second logout waits.
  expect(account.state.get().busy).toBe("logout");
  act(() => account.state.set({ ...account.state.get(), busy: null }));
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "Log out everywhere" })),
  );
  expect(calls.at(-1)).toEqual({ url: "/api/auth/logout_everywhere", method: "POST" });
  expect(visited.at(-1)).toBe("https://auth.test/out");
});

it("refreshes in place when there is no WorkOS session to end", async () => {
  const { account } = fakeAccount({
    "/api/auth/me": [
      { status: 200, body: ROOK },
      { status: 200, body: GUEST },
    ],
    "/api/auth/logout": { status: 204 },
  });
  await account.refresh();
  render(<AccountPanel />);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Log out" })));
  expect(screen.getByText(/playing as a guest/i)).toBeTruthy();
});

it("asks before deleting, and explains a retry when WorkOS is unreachable", async () => {
  const { account, calls } = fakeAccount({
    "/api/auth/me": [
      { status: 200, body: ROOK },
      { status: 200, body: GUEST },
    ],
    "/api/auth/delete": [{ status: 502, body: { error: "unavailable" } }, { status: 204 }],
  });
  await account.refresh();
  render(<AccountPanel />);
  fireEvent.click(screen.getByRole("button", { name: "Delete account…" }));
  expect(calls.some((call) => call.url === "/api/auth/delete")).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Keep account" }));
  expect(screen.queryByRole("alertdialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Delete account…" }));
  expect(screen.getByRole("alertdialog", { name: "Delete account" })).toBeTruthy();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Delete account" })));
  expect(screen.getByRole("alert").textContent).toMatch(/nothing was deleted/i);
  expect(screen.getByText("Rook#0042")).toBeTruthy();
  // The question stays open, so retrying is one click.
  expect(screen.getByRole("alertdialog", { name: "Delete account" })).toBeTruthy();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Delete account" })));
  expect(calls.filter((call) => call.url === "/api/auth/delete")).toHaveLength(2);
  expect(screen.getByText(/playing as a guest/i)).toBeTruthy();
});

it("tells the player when the door refuses without a reason", async () => {
  const { account } = fakeAccount({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/auth/logout": { status: 429 },
  });
  await account.refresh();
  render(<AccountPanel />);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Log out" })));
  expect(screen.getByRole("alert").textContent).toMatch(/too many requests/i);
  fireEvent.click(screen.getByRole("button", { name: "OK" }));
  expect(screen.queryByRole("alert")).toBeNull();
});

it("shows a logged-in player without a name as such", async () => {
  const { account } = fakeAccount({
    "/api/auth/me": { status: 200, body: { ...ROOK, handle: null } },
  });
  await account.refresh();
  render(<AccountPanel />);
  expect(screen.getByText(/no name yet/i)).toBeTruthy();
});

it("hides the panel when the door answers me with an error", async () => {
  const { account } = fakeAccount({ "/api/auth/me": { status: 500 } });
  await account.refresh();
  const { container } = render(<AccountPanel />);
  expect(account.state.get().status).toBe("unavailable");
  expect(container.textContent).toBe("");
});

it("explains offline actions and plain-text refusals", async () => {
  const { account } = fakeAccount({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/auth/logout": "offline",
    "/api/auth/logout_everywhere": { status: 403 },
  });
  await account.refresh();
  render(<AccountPanel />);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Log out" })));
  expect(screen.getByRole("alert").textContent).toMatch(/could not reach the server/i);
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "Log out everywhere" })),
  );
  expect(screen.getByRole("alert").textContent).toMatch(/not allowed/i);
});

it("never opens a non-web logout link, and refreshes instead", async () => {
  const { account, visited } = fakeAccount({
    "/api/auth/me": [
      { status: 200, body: ROOK },
      { status: 200, body: GUEST },
    ],
    "/api/auth/logout": { status: 200, body: { logout_url: "javascript:alert(1)" } },
  });
  await account.refresh();
  render(<AccountPanel />);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Log out" })));
  expect(visited).toEqual([]);
  expect(screen.getByText(/playing as a guest/i)).toBeTruthy();
});

it("sends one request for a quick double click", async () => {
  const { account, calls } = fakeAccount({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/auth/logout": { status: 200, body: { logout_url: "https://auth.test/out" } },
  });
  await account.refresh();
  await Promise.all([account.logout(), account.logout()]);
  expect(calls.filter((call) => call.url === "/api/auth/logout")).toHaveLength(1);
});

it("re-checks login when the door no longer offers it, and after the page returns", async () => {
  const { account, calls } = fakeAccount({
    "/api/auth/me": [
      { status: 200, body: ROOK },
      { status: 200, body: { ...GUEST, login: false } },
      { status: 200, body: ROOK },
    ],
    "/api/auth/logout": { status: 404 },
  });
  await account.refresh();
  const { container } = render(<AccountPanel />);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Log out" })));
  expect(container.textContent).toBe("");
  // Back from WorkOS's page through the back/forward cache: nothing waits.
  act(() => account.state.set({ ...account.state.get(), busy: "logout" }));
  await act(async () => account.resume());
  expect(account.state.get().busy).toBeNull();
  expect(calls.filter((call) => call.url === "/api/auth/me")).toHaveLength(3);
  expect(screen.getByText("Rook#0042")).toBeTruthy();
});

it("keeps the panel when a refresh after an action fails", async () => {
  const { account } = fakeAccount({
    "/api/auth/me": [{ status: 200, body: ROOK }, "offline"],
    "/api/auth/logout": { status: 204 },
  });
  await account.refresh();
  render(<AccountPanel />);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Log out" })));
  expect(screen.getByRole("alert").textContent).toMatch(/could not reach the server/i);
});

it("lets a player without a name choose one, and shows the door's number", async () => {
  const { account, calls } = fakeAccount({
    "/api/auth/me": [
      { status: 200, body: { ...ROOK, handle: null } },
      { status: 200, body: { ...ROOK, handle: "Ash#1000" } },
    ],
    "/api/auth/handle": [
      { status: 422, body: { error: "blocked_name" } },
      { status: 200, body: { handle: "Ash#1000" } },
    ],
  });
  await account.refresh();
  render(<AccountPanel />);
  const field = screen.getByRole("textbox");
  const save = screen.getByRole("button", { name: "Save name" });
  fireEvent.change(field, { target: { value: "7up" } });
  expect((save as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(field, { target: { value: "Badword" } });
  await act(async () => fireEvent.click(save));
  expect(calls.at(-1)).toEqual({
    url: "/api/auth/handle",
    method: "POST",
    body: { name: "Badword" },
  });
  expect(screen.getByRole("alert").textContent).toMatch(/not allowed/i);
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "Ash" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save name" })));
  expect(screen.getByText("Ash#1000")).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.getByRole("button", { name: "Change name" })).toBeTruthy();
});

it("explains full names and the rename wait", async () => {
  const later = Math.floor(Date.now() / 1000) + 3 * 86_400;
  const { account } = fakeAccount({
    "/api/auth/me": { status: 200, body: ROOK },
    "/api/auth/handle": [
      { status: 409, body: { error: "name_full" } },
      { status: 429, body: { error: "too_soon", rename_at: later } },
    ],
  });
  await account.refresh();
  render(<AccountPanel />);
  fireEvent.click(screen.getByRole("button", { name: "Change name" }));
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "Popular" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save name" })));
  expect(screen.getByRole("alert").textContent).toMatch(/too many players have that name/i);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save name" })));
  expect(screen.getByRole("alert").textContent).toMatch(/changed your name recently/i);
  expect(screen.getByText(/a different name in 3 days/i)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("textbox")).toBeNull();
});

it("handles name answers without a reason, offline, and after logging out elsewhere", async () => {
  const { account, calls } = fakeAccount({
    "/api/auth/me": [
      { status: 200, body: ROOK },
      { status: 200, body: GUEST },
    ],
    "/api/auth/handle": ["offline", { status: 429 }, { status: 401 }],
  });
  await account.refresh();
  render(<AccountPanel />);
  fireEvent.click(screen.getByRole("button", { name: "Change name" }));
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "Ash" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save name" })));
  expect(screen.getByRole("alert").textContent).toMatch(/could not reach the server/i);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save name" })));
  expect(screen.getByRole("alert").textContent).toMatch(/too many requests/i);
  // Cancel clears the old refusal.
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("alert")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Change name" }));
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "Ash" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save name" })));
  expect(calls.filter((call) => call.url === "/api/auth/me")).toHaveLength(2);
  expect(screen.getByText(/playing as a guest/i)).toBeTruthy();
});
