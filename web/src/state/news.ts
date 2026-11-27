// The launcher page's news: read from the door when the page shows, posted
// and deleted by admins (the door checks the role on every change).

import { parseNews, type NewsItem } from "../domain/news";
import { createStore } from "./store";

export type NewsState = {
  status: "loading" | "ready" | "unavailable";
  items: NewsItem[];
  /** A post or delete on its way. */
  busy: boolean;
  /** The last refusal (`newsErrorText`). */
  error: string | null;
};
export const NO_NEWS: NewsState = { status: "loading", items: [], busy: false, error: null };
export const news = createStore<NewsState>(NO_NEWS);
const update = (patch: Partial<NewsState>) => news.set({ ...news.get(), ...patch });

const url = (path: string) => new URL(path, window.location.origin);

/** Reads the news; a door without it (or none reachable) shows none. A
 * failed read keeps news already shown. */
export async function loadNews() {
  const keep = () =>
    news.get().status === "ready" ? {} : { status: "unavailable" as const, items: [] };
  try {
    const answer = await fetch(url("/api/news"), { cache: "no-store" });
    const items = answer.ok ? parseNews(await answer.json()) : null;
    update(items ? { status: "ready", items } : keep());
  } catch {
    update(keep());
  }
}

async function change(path: string, init: RequestInit): Promise<boolean> {
  if (news.get().busy) return false;
  update({ busy: true, error: null });
  try {
    const answer = await fetch(url(path), {
      credentials: "same-origin",
      headers: { "content-type": "application/json" },
      ...init,
    });
    if (!answer.ok) {
      const body = (await answer.json().catch(() => null)) as {
        error?: unknown;
        problem?: unknown;
      } | null;
      const code = typeof body?.problem === "string" ? body.problem : body?.error;
      update({ busy: false, error: typeof code === "string" ? code : "failed" });
      return false;
    }
    update({ busy: false });
    await loadNews();
    return true;
  } catch {
    update({ busy: false, error: "failed" });
    return false;
  }
}

/** Posts an item (admins); `true` once the door took it. */
export function postNews(title: string, body: string) {
  return change("/api/admin/news", { method: "POST", body: JSON.stringify({ title, body }) });
}

/** Deletes an item (admins). */
export function deleteNews(id: string) {
  return change(`/api/admin/news/${id}`, { method: "DELETE" });
}

export function dismissNewsError() {
  update({ error: null });
}
