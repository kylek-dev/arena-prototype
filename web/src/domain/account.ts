// Accounts (social-identity, ADR 0055): what the front door's `/api/auth/*`
// routes answer and how the interface words it. Login happens on WorkOS
// AuthKit's hosted page; the door keeps the session in an HttpOnly cookie,
// so the page only ever sees `GET /api/auth/me`. Pure, so it is unit tested
// without a server.

import { isHandle } from "./handles";

export type Me = {
  /** The account id (16 hex digits), or `null` for a guest. */
  account: string | null;
  /** Whether deploy configuration makes this account an admin. */
  admin: boolean;
  /** Whether this door offers login at all. */
  login: boolean;
  /** The account's `Name#number`, or `null` before one is picked. */
  handle: string | null;
  /** When a different name may be picked (Unix seconds), or `null` now. */
  renameAt: number | null;
};

/** Unix seconds up to the year 2286; anything else is not a time. */
const seconds = (value: unknown): value is number =>
  Number.isSafeInteger(value) && (value as number) > 0 && (value as number) < 1e10;

/** Parses `GET /api/auth/me`; `null` when it is not understood. */
export function parseMe(raw: unknown): Me | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const v = raw as Record<string, unknown>;
  if (typeof v.login !== "boolean" || typeof v.admin !== "boolean") return null;
  if (v.account !== null && (typeof v.account !== "string" || !/^[0-9a-f]{16}$/.test(v.account)))
    return null;
  if (v.handle !== null && v.handle !== undefined && !isHandle(v.handle)) return null;
  if (v.rename_at !== null && v.rename_at !== undefined && !seconds(v.rename_at)) return null;
  const account = v.account as string | null;
  // A guest has no handle, rename clock or admin role, whatever was sent.
  return {
    account,
    admin: account !== null && v.admin,
    login: v.login,
    handle: account === null ? null : ((v.handle as string | undefined) ?? null),
    renameAt: account === null ? null : ((v.rename_at as number | undefined) ?? null),
  };
}

/** Where the door's login routes are, on the page's own origin. */
export const LOGIN_URL = "/api/auth/login";
export const SIGN_UP_URL = "/api/auth/login?sign_up";
/** The privacy notice published beside the game page. */
export const PRIVACY_URL = "privacy.html";

/** The `logout_url` a logout answers with (WorkOS's page that ends its own
 * session and returns to the game), if it is an `https:` address (or
 * `http:` from an `http:` page, in local development); anything else, such
 * as a `javascript:` link, is never opened. */
export function logoutTarget(raw: unknown, pageProtocol = "https:"): string | null {
  if (!raw || typeof raw !== "object") return null;
  const url = (raw as Record<string, unknown>).logout_url;
  if (typeof url !== "string" || url.length > 2048) return null;
  try {
    const parsed = new URL(url);
    const allowed =
      parsed.protocol === "https:" || (pageProtocol === "http:" && parsed.protocol === "http:");
    return allowed ? parsed.href : null;
  } catch {
    return null;
  }
}

/** The `{"error"}` code of a refusal, if the body has one. */
export function errorCode(raw: unknown): string | null {
  if (!raw || typeof raw !== "object") return null;
  const code = (raw as Record<string, unknown>).error;
  return typeof code === "string" && /^[a-z_]{1,32}$/.test(code) ? code : null;
}

/** A name the door may accept: 3–16 ASCII letters and digits, starting with
 * a letter (the door also refuses blocked words and picks the number). */
export function isName(text: string) {
  return /^[A-Za-z][A-Za-z0-9]{2,15}$/.test(text);
}

/** The `rename_at` of a `too_soon` refusal, if it is a time. */
export function refusalRenameAt(raw: unknown): number | null {
  if (!raw || typeof raw !== "object") return null;
  const at = (raw as Record<string, unknown>).rename_at;
  return seconds(at) ? at : null;
}

/** How long until a different name may be picked: "in 3 days", "in 5 hours",
 * "within the hour"; `null` once it may. */
export function renameWait(renameAt: number | null, nowSeconds: number) {
  if (renameAt === null || renameAt <= nowSeconds) return null;
  const hours = Math.ceil((renameAt - nowSeconds) / 3600);
  if (hours <= 1) return "within the hour";
  if (hours < 48) return `in ${hours} hours`;
  return `in ${Math.ceil(hours / 24)} days`;
}

const ACCOUNT_ERRORS: Record<string, string> = {
  invalid_name: "Use 3–16 letters and digits, starting with a letter.",
  blocked_name: "That name is not allowed. Pick another.",
  name_full: "Too many players have that name. Pick another.",
  too_soon: "You changed your name recently. You can change only its capital letters until then.",
  unavailable:
    "The login service could not be reached, so nothing was deleted. Try again in a minute.",
  rate_limited: "Too many requests. Wait a moment and try again.",
  forbidden: "This page is not allowed to change your account. Open the game from its own address.",
  not_logged_in: "You are not logged in any more.",
  failed: "Something went wrong on the server. Try again in a minute.",
  offline: "Could not reach the server. Check your connection and try again.",
};
/** Words for an account action that did not work. */
export function accountErrorText(code: string) {
  return ACCOUNT_ERRORS[code] ?? ACCOUNT_ERRORS.failed;
}

/** The code for an answer without a JSON `{"error"}`, from its status. */
export function statusCode(status: number) {
  if (status === 429) return "rate_limited";
  if (status === 403) return "forbidden";
  if (status === 401) return "not_logged_in";
  // A gateway error without the door's own `{"error"}` means the door
  // itself could not be reached (say, the proxy's 502 while it restarts).
  if (status >= 502 && status <= 504) return "offline";
  return "failed";
}
