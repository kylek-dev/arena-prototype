import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import {
  MAX_CHAT_CHARS,
  chatErrorText,
  chatPrefix,
  chatTabLabel,
  chatTabs,
  chatText,
  isIgnored,
  tabMessages,
  type ChatChannel,
  type ChatMessage,
} from "../../domain/chat";
import { isHandle } from "../../domain/handles";
import type { Queue } from "../../runtime/matchmaking";
import { chatFocusRequest, chatShown } from "../../state/chat";
import { updateConfig, useConfigSelector } from "../../state/config";
import { matchQueue } from "../../state/network";
import { engine, interaction } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { accountService } from "../../state/account";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { pageCommandContext, runSlash } from "../../runtime/chat-commands";
import { unescapeSlash } from "../../domain/commands";
import { AdminTag } from "../../ui/AdminTag";
import { Placed } from "../../ui/Placed";
import "./chat.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);
/** Lines shown per tab; older ones stay in the log. */
const SHOWN = 100;

/** Whisper or (un)ignore a player from their name in the log. */
function NameMenu({
  queue,
  handle,
  ignored,
  close,
}: {
  queue: Queue;
  handle: string;
  ignored: boolean;
  close: () => void;
}) {
  return (
    <span className="chat-name-menu" role="menu" aria-label={`${handle} actions`}>
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          queue.whisperTo(handle);
          queue.showChat("whisper");
          close();
        }}
      >
        Whisper
      </button>
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          queue.ignore(handle, !ignored);
          close();
        }}
      >
        {ignored ? "Unignore" : "Ignore"}
      </button>
    </span>
  );
}

function Line({
  queue,
  message,
  ignored,
  menu,
  setMenu,
  me,
}: {
  queue: Queue;
  message: ChatMessage;
  ignored: (handle: string) => boolean;
  menu: boolean;
  setMenu: (id: number | null) => void;
  /** This player's handle, if known: their own lines get no menu. */
  me: string | null;
}) {
  // The page's own lines: styled apart, and nobody to whisper or ignore.
  if (message.channel === "system") return <li className="chat-line system">{message.text}</li>;
  // One's own whisper names the recipient; other lines name the sender.
  const who = message.to ?? message.from;
  const own = !!me && message.to === null && message.from.toLowerCase() === me.toLowerCase();
  return (
    <li className={`chat-line ${message.channel}`}>
      {own ? (
        <span className="chat-name">{chatPrefix(message)}</span>
      ) : (
        <button
          type="button"
          className="chat-name"
          aria-expanded={menu}
          aria-label={`${chatPrefix(message)}: options`}
          onClick={() => setMenu(menu ? null : message.id)}
        >
          {chatPrefix(message)}
        </button>
      )}
      {message.admin && message.to === null && <AdminTag compact />}
      <span className="chat-text">: {message.text}</span>
      {menu && (
        <NameMenu queue={queue} handle={who} ignored={ignored(who)} close={() => setMenu(null)} />
      )}
    </li>
  );
}

function Box({ queue, editing }: { queue: Queue; editing: boolean }) {
  const chat = useStore(queue.chat, (c) => c);
  const spectating = useStore(engine, (s) => Boolean(s?.spectating));
  const seated = useStore(engine, (s) => Boolean(s?.networked && !s.spectating));
  const account = useStore(accountService, (a) => a);
  const me = useStore(account?.state ?? NO_ACCOUNT, (s) => s.me?.handle ?? null);
  const tabs = chatTabs(spectating);
  const active: ChatChannel = tabs.includes(chat.active) ? chat.active : tabs[0];
  const [draft, setDraft] = useState("");
  const [menu, setMenu] = useState<number | null>(null);
  const [focused, setFocused] = useState(false);
  const [needsTarget, setNeedsTarget] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const log = useRef<HTMLOListElement>(null);
  const focusRequest = useStore(chatFocusRequest, (r) => r);
  const lines = tabMessages(chat, active).slice(-SHOWN);
  // The tab follows where the player is: a seated player on Team (players
  // in a live match never hear Public), a spectator on Spectators (the only
  // one they post to), and anyone else on Public. Only on a change, so a
  // tab picked by hand stays.
  const place: ChatChannel = spectating ? "spectator" : seated ? "team" : "public";
  const wasPlace = useRef<ChatChannel | null>(null);
  useEffect(() => {
    const before = wasPlace.current;
    wasPlace.current = place;
    if (before === null ? place !== "public" : before !== place) queue.showChat(place);
  }, [queue, place]);
  // Enter (from the input router) opens the field: requests after this box
  // appeared, never the ones before it (a remount must not take the keys).
  const seenRequest = useRef(focusRequest.count);
  useEffect(() => {
    if (focusRequest.count > seenRequest.current) {
      // `/` starts a command (keeping a draft already begun with one).
      // `/` starts a command in an empty field; a draft already typed stays.
      if (focusRequest.prefill) setDraft((d) => d || focusRequest.prefill);
      field.current?.focus();
    }
    seenRequest.current = focusRequest.count;
  }, [focusRequest]);
  // Keep the newest line in view, unless the reader scrolled up.
  const newest = lines.at(-1)?.id ?? 0;
  const pinned = useRef(true);
  useEffect(() => {
    const element = log.current;
    if (element && pinned.current) element.scrollTop = element.scrollHeight;
  }, [newest, active]);
  const { text, tooLong } = chatText(draft);
  const whisperTo = chat.whisperTo;
  const leave = () => field.current?.blur();
  const send = () => {
    if (!text) {
      leave();
      return;
    }
    if (tooLong) return;
    // Commands run here (`/help` and the rest); `//` sends a line with one `/`.
    if (runSlash(text, pageCommandContext(queue))) {
      setDraft("");
      leave();
      return;
    }
    const message = unescapeSlash(text);
    if (active === "whisper") {
      if (!whisperTo || !isHandle(whisperTo)) {
        setNeedsTarget(true);
        return;
      }
      queue.whisper(whisperTo, message);
    } else queue.say(active, message);
    setDraft("");
    leave();
  };
  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    // Enter while an input method composes a character is not a send.
    if (event.key === "Enter" && !event.nativeEvent.isComposing) {
      event.preventDefault();
      send();
    } else if (event.key === "Escape") {
      // Leave the field, keeping the draft; the game has the keys again.
      event.preventDefault();
      event.stopPropagation();
      leave();
    }
  };
  return (
    <section
      className={`chat-box${focused ? " typing" : ""}`}
      aria-label="Chat"
      data-keyboard-owner={focused ? "true" : undefined}
    >
      <div className="chat-tabs" role="tablist" aria-label="Chat channels">
        {tabs.map((tab) => {
          const unread = chat.unread[tab] ?? 0;
          return (
            <button
              key={tab}
              type="button"
              role="tab"
              aria-selected={tab === active}
              className="chat-tab"
              onClick={() => queue.showChat(tab)}
            >
              {chatTabLabel(tab)}
              {unread > 0 && tab !== active && (
                <span className="chat-unread" aria-label={`${unread} unread`}>
                  {unread > 99 ? "99+" : unread}
                </span>
              )}
            </button>
          );
        })}
      </div>
      <ol
        className="chat-log"
        ref={log}
        onScroll={(event) => {
          const element = event.currentTarget;
          pinned.current = element.scrollTop + element.clientHeight >= element.scrollHeight - 4;
        }}
        role="log"
        aria-live="polite"
        aria-label={chatTabLabel(active)}
      >
        {editing && lines.length === 0 ? (
          <li className="chat-line public">
            <span className="chat-name">[Public] A player</span>
            <span className="chat-text">: chat appears here</span>
          </li>
        ) : (
          lines.map((message) => (
            <Line
              key={message.id}
              queue={queue}
              message={message}
              ignored={(handle) => isIgnored(chat, handle)}
              menu={menu === message.id}
              setMenu={setMenu}
              me={me}
            />
          ))
        )}
      </ol>
      {chat.error && (
        <p className="chat-error" role="alert">
          {chatErrorText(chat.error)}{" "}
          <button type="button" onClick={() => queue.dismissChatError()}>
            OK
          </button>
        </p>
      )}
      {active === "whisper" && (
        <label className="chat-to">
          To
          <input
            value={whisperTo ?? ""}
            maxLength={21}
            placeholder="Name#number"
            aria-label="Whisper to"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setNeedsTarget(false);
              queue.whisperTo(event.target.value.trim() || null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                field.current?.focus();
              } else if (event.key === "Escape") {
                event.preventDefault();
                event.currentTarget.blur();
              }
            }}
          />
        </label>
      )}
      {needsTarget && active === "whisper" && (
        <p className="chat-error" role="status">
          Pick who to whisper: their name and number, or Whisper from a friend's name.
        </p>
      )}
      <input
        ref={field}
        className="chat-input"
        value={draft}
        placeholder={focused ? `Say in ${chatTabLabel(active)}` : "Press Enter to chat"}
        aria-label={`Message to ${chatTabLabel(active)}`}
        aria-invalid={tooLong || undefined}
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={keys}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
      />
      {tooLong && (
        <p className="chat-error" role="status">
          Too long: {MAX_CHAT_CHARS} characters at most.
        </p>
      )}
    </section>
  );
}

/** The movable chat box (planning/issues/chat.md): channel tabs with unread
 * markers, the door's messages and refusals, whispers, and ignoring from a
 * name. Enter opens it; while typing, keys go to it, not to combat. Placed
 * with the layout editor like the unit frames. */
export function ChatBox() {
  const queue = useStore(matchQueue, (q) => q);
  const editing = useStore(interaction, (s) => s.layout);
  const value = useConfigSelector((c) => c.frames.chat);
  const shown = !!queue && (value.visible || editing);
  useEffect(() => {
    chatShown.set(shown && value.visible);
    return () => chatShown.set(false);
  }, [shown, value.visible]);
  // An unconnected queue still shows the box: sending opens the connection.
  if (!queue) return null;
  return (
    <Placed
      id="chat"
      value={value}
      onChange={(next) => updateConfig((c) => ({ ...c, frames: { ...c.frames, chat: next } }))}
    >
      <Box queue={queue} editing={editing} />
    </Placed>
  );
}
