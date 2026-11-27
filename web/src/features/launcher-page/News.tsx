import { Button, Text } from "@radix-ui/themes";
import { useEffect, useState } from "react";
import {
  MAX_NEWS_BODY,
  MAX_NEWS_TITLE,
  newsDate,
  newsErrorText,
  type NewsItem,
} from "../../domain/news";
import { deleteNews, dismissNewsError, loadNews, news, postNews } from "../../state/news";
import { useStore } from "../../state/store";

/** The admin's form for a new item. */
function PostNews({ close }: { close: () => void }) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const busy = useStore(news, (n) => n.busy);
  const ready = title.trim() !== "" && body.trim() !== "";
  return (
    <form
      className="news-post"
      aria-label="Post news"
      onSubmit={async (event) => {
        event.preventDefault();
        if (ready && (await postNews(title, body))) close();
      }}
    >
      <label className="news-field">
        <span>Title</span>
        <input
          value={title}
          maxLength={MAX_NEWS_TITLE}
          autoComplete="off"
          onChange={(event) => setTitle(event.target.value)}
        />
      </label>
      <label className="news-field">
        <span>
          News{" "}
          <Text size="1" color="gray">
            ({body.length}/{MAX_NEWS_BODY})
          </Text>
        </span>
        <textarea
          value={body}
          rows={5}
          maxLength={MAX_NEWS_BODY}
          onChange={(event) => setBody(event.target.value)}
        />
      </label>
      <div className="news-actions">
        <Button
          type="button"
          variant="soft"
          color="gray"
          onClick={() => {
            dismissNewsError();
            close();
          }}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={!ready || busy}>
          Post
        </Button>
      </div>
    </form>
  );
}

/** One item; admins may delete it, after confirming. */
function Item({ item, admin }: { item: NewsItem; admin: boolean }) {
  const [confirming, setConfirming] = useState(false);
  const busy = useStore(news, (n) => n.busy);
  return (
    <article className="news-item" aria-labelledby={`news-${item.id}`}>
      <header>
        <h3 id={`news-${item.id}`}>{item.title}</h3>
        <Text size="1" color="gray">
          {newsDate(item.posted)}
          {item.author ? ` · ${item.author}` : ""}
        </Text>
      </header>
      <p className="news-body">{item.body}</p>
      {admin &&
        (confirming ? (
          <div className="news-actions">
            <Text size="1">Delete this news for everyone?</Text>
            <Button size="1" color="red" disabled={busy} onClick={() => void deleteNews(item.id)}>
              Delete
            </Button>
            <Button size="1" variant="soft" color="gray" onClick={() => setConfirming(false)}>
              Keep
            </Button>
          </div>
        ) : (
          <div className="news-actions">
            <Button
              size="1"
              variant="surface"
              color="gray"
              className="action-button"
              onClick={() => setConfirming(true)}
            >
              Delete…
            </Button>
          </div>
        ))}
    </article>
  );
}

/** News on the launcher page, newest first; admins post and delete here
 * (the door checks the role on every change). */
export function News({ admin }: { admin: boolean }) {
  const state = useStore(news, (n) => n);
  const [posting, setPosting] = useState(false);
  useEffect(() => {
    void loadNews();
  }, []);
  return (
    <section className="launcher-news" aria-labelledby="launcher-news-heading">
      <div className="launcher-news-head">
        <h2 id="launcher-news-heading">News</h2>
        {admin && !posting && (
          <Button size="1" variant="soft" onClick={() => setPosting(true)}>
            Post news
          </Button>
        )}
      </div>
      {state.error && (
        <div className="news-error" role="alert">
          <Text size="1">{newsErrorText(state.error)}</Text>
          <Button size="1" variant="soft" color="gray" onClick={dismissNewsError}>
            OK
          </Button>
        </div>
      )}
      {admin && posting && <PostNews close={() => setPosting(false)} />}
      {state.status === "loading" ? (
        <Text size="2" color="gray">
          Loading news.
        </Text>
      ) : state.status === "unavailable" ? (
        <Text size="2" color="gray">
          News cannot be loaded at the moment.
        </Text>
      ) : state.items.length === 0 ? (
        <Text size="2" color="gray">
          There is no news yet.
        </Text>
      ) : (
        <div className="news-list">
          {state.items.map((item) => (
            <Item key={item.id} item={item} admin={admin} />
          ))}
        </div>
      )}
    </section>
  );
}
