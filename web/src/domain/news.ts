// News for the launcher page: the door's `GET /api/news` items, checked
// strictly, and how the page words them. Items are plain text: the page
// never renders them as HTML. Pure, so it is unit tested without a door.

export type NewsItem = {
  id: string;
  title: string;
  body: string;
  /** Unix seconds. */
  posted: number;
  /** The posting admin's `Name#number`, when they had one. */
  author: string | null;
};

/** The door's limits, in characters. */
export const MAX_NEWS_TITLE = 80;
export const MAX_NEWS_BODY = 2000;

/** The items of a `{news: [...]}` answer, newest first; `null` when the
 * answer is not one. Malformed items are dropped, not the whole list. */
export function parseNews(body: unknown): NewsItem[] | null {
  const items = (body as { news?: unknown } | null)?.news;
  if (!Array.isArray(items)) return null;
  const news: NewsItem[] = [];
  for (const raw of items) {
    const v = raw as Record<string, unknown> | null;
    if (
      !v ||
      typeof v.id !== "string" ||
      !/^[0-9a-f]{16}$/.test(v.id) ||
      typeof v.title !== "string" ||
      typeof v.body !== "string" ||
      !Number.isSafeInteger(v.posted) ||
      (v.posted as number) < 0 ||
      (v.author !== undefined && typeof v.author !== "string")
    )
      continue;
    news.push({
      id: v.id,
      title: v.title,
      body: v.body,
      posted: v.posted as number,
      author: (v.author as string | undefined) ?? null,
    });
  }
  return news;
}

/** The posting date, as a reader expects it: `26 November 2026`. */
export function newsDate(posted: number, locale?: string) {
  return new Date(posted * 1000).toLocaleDateString(locale, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

const PROBLEMS: Record<string, string> = {
  empty_title: "Give the news a title.",
  long_title: `Titles hold up to ${MAX_NEWS_TITLE} characters.`,
  empty_body: "Write the news before posting it.",
  long_body: `News holds up to ${MAX_NEWS_BODY} characters.`,
  not_admin: "Only administrators can post news.",
  not_logged_in: "Log in as an administrator to post news.",
};
/** Words for a refused post or delete. */
export function newsErrorText(code: string) {
  return PROBLEMS[code] ?? "The server could not change the news. Try again in a moment.";
}
