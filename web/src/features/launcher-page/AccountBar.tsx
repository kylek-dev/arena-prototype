import { Button, Text } from "@radix-ui/themes";
import { useEffect, useRef, useState } from "react";
import {
  LOGIN_URL,
  SIGN_UP_URL,
  accountErrorText,
  isName,
  renameWait,
  type Me,
} from "../../domain/account";
import { handleName } from "../../domain/handles";
import { guestHandle } from "../../domain/matchmaking";
import { LOADING_ACCOUNT, type Account } from "../../runtime/account";
import { accountService } from "../../state/account";
import { matchQueue } from "../../state/network";
import { createStore, useStore } from "../../state/store";
import { AdminTag } from "../../ui/AdminTag";
import { Modal } from "../../ui/Dialog";
import { Hint } from "../../ui/Tooltip";
import "../account/account.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);

/** "Choose your name": the player picks the word, the door the number. */
function ChooseName({ account, me, close }: { account: Account; me: Me; close: () => void }) {
  const busy = useStore(account.state, (s) => !!s.busy);
  const error = useStore(account.state, (s) => s.error);
  const [name, setName] = useState("");
  const wait = renameWait(me.renameAt, Date.now() / 1000);
  const valid = isName(name);
  const save = async () => {
    if (!valid || busy) return;
    if (await account.chooseName(name)) close();
  };
  return (
    <form
      className="account-name"
      aria-label="Choose your name"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <label className="account-name-field">
        <Text size="2">Name</Text>
        <Hint text="Use 3 to 16 letters and digits, starting with a letter. A number is assigned automatically.">
          <input
            value={name}
            maxLength={16}
            placeholder={me.handle ? handleName(me.handle) : "Name"}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setName(event.target.value.trim())}
          />
        </Hint>
      </label>
      {wait && (
        <Text size="1" color="gray">
          You can choose a different name {wait}. Changes to capitalization are allowed at any time.
        </Text>
      )}
      {error && (
        <Text size="1" className="account-error" role="alert">
          {accountErrorText(error)}
        </Text>
      )}
      <div className="account-actions">
        {me.handle && (
          <Button
            type="button"
            variant="soft"
            color="gray"
            onClick={() => {
              account.dismiss();
              close();
            }}
          >
            Cancel
          </Button>
        )}
        <Button type="submit" disabled={!valid || busy}>
          Save name
        </Button>
      </div>
    </form>
  );
}

/** Deleting the account: asked once more; a refusal keeps the question open
 * so retrying is one click. */
function DeleteAccount({ account, close }: { account: Account; close: () => void }) {
  const busy = useStore(account.state, (s) => s.busy);
  const error = useStore(account.state, (s) => s.error);
  return (
    <div className="account-confirm" role="alertdialog" aria-label="Delete account">
      <Text size="2">
        Delete your account? Your name, friends and blocks are removed here, and your login is
        removed at WorkOS. This cannot be undone.
      </Text>
      {error && (
        <Text size="1" className="account-error" role="alert">
          {accountErrorText(error)}
        </Text>
      )}
      <div className="account-actions">
        <Button
          variant="soft"
          color="gray"
          disabled={!!busy}
          onClick={() => {
            account.dismiss();
            close();
          }}
        >
          Keep account
        </Button>
        <Button
          color="red"
          disabled={!!busy}
          onClick={async () => {
            await account.deleteAccount();
            if (!account.state.get().error) close();
          }}
        >
          {busy === "delete" ? "Deleting…" : "Delete account"}
        </Button>
      </div>
    </div>
  );
}

/** The logged-in player's menu, opened from their name. */
function AccountMenu({ account, me }: { account: Account; me: Me }) {
  const busy = useStore(account.state, (s) => !!s.busy);
  const error = useStore(account.state, (s) => s.error);
  const [open, setOpen] = useState(false);
  const [dialog, setDialog] = useState<"name" | "delete" | null>(me.handle ? null : "name");
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const items = () => [
    ...(list.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? []),
  ];
  // Opening moves the keyboard into the menu, as menus do.
  useEffect(() => {
    if (open) items()[0]?.focus();
  }, [open]);
  // A click elsewhere closes the menu.
  useEffect(() => {
    if (!open) return;
    const away = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [open]);
  const choose = (next: "name" | "delete" | (() => void)) => {
    setOpen(false);
    account.dismiss();
    if (typeof next === "function") next();
    else setDialog(next);
  };
  return (
    <div
      className="account-menu"
      ref={root}
      onKeyDown={(event) => {
        if (!open) return;
        if (event.key === "Escape") {
          event.stopPropagation();
          setOpen(false);
          trigger.current?.focus();
        } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          const all = items();
          const at = all.indexOf(document.activeElement as HTMLButtonElement);
          const step = event.key === "ArrowDown" ? 1 : -1;
          all[(at + step + all.length) % all.length]?.focus();
        }
      }}
      // Tabbing away closes it.
      onBlur={(event) => {
        if (open && !root.current?.contains(event.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      {me.handle ? (
        <button
          ref={trigger}
          type="button"
          className="account-menu-trigger"
          aria-haspopup="menu"
          aria-expanded={open}
          aria-label={`Account menu for ${me.handle}`}
          onClick={() => setOpen(!open)}
        >
          <span className="account-handle">{me.handle}</span>
          {me.admin && <AdminTag compact />}
          <span aria-hidden="true">▾</span>
        </button>
      ) : (
        <>
          <Text size="1" color="gray">
            You are logged in without a player name.
          </Text>
          <Button size="2" onClick={() => setDialog("name")}>
            Choose your name
          </Button>
        </>
      )}
      {open && (
        <div className="account-menu-list" role="menu" aria-label="Account" ref={list}>
          <button type="button" role="menuitem" onClick={() => choose("name")}>
            Change name…
          </button>
          <button
            type="button"
            role="menuitem"
            disabled={busy}
            onClick={() => choose(() => void account.logout())}
          >
            Log out
          </button>
          <button
            type="button"
            role="menuitem"
            disabled={busy}
            title="Ends your sessions on every device and browser"
            onClick={() => choose(() => void account.logoutEverywhere())}
          >
            Log out everywhere
          </button>
          <button
            type="button"
            role="menuitem"
            className="danger"
            disabled={busy}
            onClick={() => choose("delete")}
          >
            Delete account…
          </button>
        </div>
      )}
      {/* Refusals of the menu's own actions; dialogs show theirs inside. */}
      {error && !dialog && !busy && (
        <div className="account-bar-notice" role="alert">
          <Text size="1">{accountErrorText(error)}</Text>
          <Button size="1" variant="soft" color="gray" onClick={() => account.dismiss()}>
            OK
          </Button>
        </div>
      )}
      {dialog === "name" && (
        <Modal
          title={me.handle ? "Change name" : "Choose your name"}
          close={() => {
            account.dismiss();
            setDialog(null);
          }}
        >
          <ChooseName account={account} me={me} close={() => setDialog(null)} />
        </Modal>
      )}
      {dialog === "delete" && (
        <Modal
          title="Delete account"
          close={() => {
            account.dismiss();
            setDialog(null);
          }}
        >
          <DeleteAccount account={account} close={() => setDialog(null)} />
        </Modal>
      )}
    </div>
  );
}

/** The launcher page's account, at the right of its top bar: who is
 * playing, Log in and Sign up for guests, and a menu on a logged-in
 * player's name. Account management lives here and nowhere in game. */
export function AccountBar() {
  const account = useStore(accountService, (a) => a);
  const state = useStore(account?.state ?? NO_ACCOUNT, (s) => s);
  const queue = useStore(matchQueue, (q) => q);
  const guest = queue ? guestHandle(queue.guest) : null;
  const notice =
    account && state.error && !state.busy ? (
      <div className="account-bar-notice" role="alert">
        <Text size="1">{accountErrorText(state.error)}</Text>
        <Button size="1" variant="soft" color="gray" onClick={() => account.dismiss()}>
          OK
        </Button>
      </div>
    ) : null;
  const guestName = (note: string) => (
    <div className="account-bar" role="group" aria-label="Account">
      <div className="account-bar-who">
        <span className="account-handle">{guest ?? "Guest"}</span>
        <Text size="1" color="gray">
          {note}
        </Text>
      </div>
    </div>
  );
  if (!account || state.status === "loading")
    return (
      <div className="account-bar" role="group" aria-label="Account">
        <Text size="2" color="gray">
          Checking your account.
        </Text>
      </div>
    );
  if (state.status === "unavailable")
    return guestName("Playing as a guest. Accounts cannot be reached at the moment.");
  if (!state.me?.login)
    return guestName("Playing as a guest. This server does not offer accounts.");
  if (!state.me.account)
    return (
      <div className="account-bar" role="group" aria-label="Account">
        <div className="account-bar-who">
          <span className="account-handle">{guest ?? "Guest"}</span>
          <Text size="1" color="gray">
            Playing as a guest.
          </Text>
        </div>
        <Hint text="Log in to keep a player name, add friends and use public chat.">
          <Button variant="soft" asChild>
            <a href={LOGIN_URL}>Log in</a>
          </Button>
        </Hint>
        <Button asChild>
          <a href={SIGN_UP_URL}>Sign up</a>
        </Button>
        {notice}
      </div>
    );
  return (
    <div className="account-bar" role="group" aria-label="Account">
      <AccountMenu key={state.me.account} account={account} me={state.me} />
    </div>
  );
}
