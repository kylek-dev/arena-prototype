import { expect, it } from "vitest";
import { newsDate, newsErrorText, parseNews } from "../../web/src/domain/news";

const ITEM = { id: "0123456789abcdef", title: "T", body: "B", posted: 1_795_000_000 };

it("reads the door's news strictly, dropping malformed items only", () => {
  expect(parseNews({ news: [ITEM, { ...ITEM, author: "Jax#0417" }] })).toEqual([
    { ...ITEM, author: null },
    { ...ITEM, author: "Jax#0417" },
  ]);
  expect(
    parseNews({
      news: [
        ITEM,
        { ...ITEM, id: "../x" },
        { ...ITEM, title: 3 },
        { ...ITEM, posted: -1 },
        { ...ITEM, posted: 1.5 },
        { ...ITEM, author: 7 },
        null,
      ],
    }),
  ).toHaveLength(1);
  expect(parseNews({})).toBeNull();
  expect(parseNews(null)).toBeNull();
});

it("words dates and refusals for readers", () => {
  expect(newsDate(1_795_000_000, "en-GB")).toBe("18 November 2026");
  expect(newsErrorText("long_body")).toMatch(/2000 characters/);
  expect(newsErrorText("not_admin")).toMatch(/administrators/);
  expect(newsErrorText("anything")).toMatch(/could not change the news/);
});
