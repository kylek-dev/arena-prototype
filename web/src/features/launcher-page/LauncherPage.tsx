import { Button, Separator, Text } from "@radix-ui/themes";
import { useEffect, useRef } from "react";
import { PRIVACY_URL } from "../../domain/account";
import { guestHandle } from "../../domain/matchmaking";
import { dispatch } from "../../runtime/commands";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { accountService } from "../../state/account";
import { beforeMatch, enterArena } from "../../state/appearance";
import { matchQueue } from "../../state/network";
import { engine, error } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { AccountPanel } from "../account/AccountPanel";
import { Friends } from "../friends/Friends";
import { arenaReady } from "../loading/Loading";
import { PartyStatus } from "./PartyStatus";
import { enterHub } from "../../runtime/hub";
import "../account/account.css";
import "./launcher-page.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);

/** The account card for every state the door can be in: checking, no
 * accounts on this server (or unreachable), a guest, or logged in. */
function Account() {
  const account = useStore(accountService, (a) => a);
  const state = useStore(account?.state ?? NO_ACCOUNT, (s) => s);
  const queue = useStore(matchQueue, (q) => q);
  const guest = queue ? guestHandle(queue.guest) : null;
  const asGuest = guest ? `You will play as the guest ${guest}.` : "You will play as a guest.";
  if (account && state.status === "ready" && state.me?.login) return <AccountPanel guest={guest} />;
  return (
    <div className="account" role="group" aria-label="Account">
      <Text size="2">
        {!account || state.status === "loading"
          ? "Checking your account."
          : state.status === "unavailable"
            ? `Accounts cannot be reached at the moment. ${asGuest}`
            : `This server does not offer accounts. ${asGuest}`}
      </Text>
    </div>
  );
}

/**
 * The launcher page (planning/issues/launcher-page.md): what a visitor sees
 * before entering the game, in place of the old home screen. Account
 * management lives here and nowhere in game: log in or sign up, play as a
 * guest, choose a name, log out, delete the account, the privacy notice;
 * friends and parties form here too. It covers the whole window while the
 * arena loads behind it, so Enter is immediate once it is ready.
 */
export function LauncherPage() {
  const visible = useStore(beforeMatch, (v) => v);
  const account = useStore(accountService, (a) => a);
  const loggedIn = useStore(account?.state ?? NO_ACCOUNT, (s) => !!s.me?.account);
  const ready = useStore(engine, (s) => arenaReady(s));
  const failure = useStore(error, (s) => s);
  const enter = useRef<HTMLButtonElement>(null);
  // The page takes the keyboard from the hidden arena: Enter is one key away.
  useEffect(() => {
    if (visible) enter.current?.focus();
  }, [visible]);
  if (!visible) return null;
  return (
    <section className="launcher-page" aria-label="Launcher" data-keyboard-owner="true">
      <div className="launcher-page-column">
        <header className="launcher-page-header">
          <span className="launcher-page-eyebrow">Arena Prototype</span>
          <h1>Welcome to the arena</h1>
          <Text as="p" size="2" color="gray">
            Log in to keep your player name, add friends and form a party, or play as a guest.
          </Text>
        </header>
        <section className="launcher-page-card" aria-labelledby="launcher-account-heading">
          <h2 id="launcher-account-heading">Account</h2>
          <Account />
        </section>
        {loggedIn && (
          <section className="launcher-page-card" aria-labelledby="launcher-friends-heading">
            <h2 id="launcher-friends-heading">Friends and party</h2>
            <PartyStatus />
            <Friends />
          </section>
        )}
        {/* A guest can be in a party through an old party link. */}
        {!loggedIn && <PartyStatus />}
        <Separator size="4" />
        <div className="launcher-page-enter">
          <Button
            ref={enter}
            size="3"
            onClick={() => {
              enterArena(() => dispatch("reset"));
              // Into the hub when this server has one (ADR 0056).
              enterHub();
            }}
          >
            Enter the game
          </Button>
          <Text size="1" color="gray" role="status">
            {failure
              ? "The arena could not be started. Enter it to see why."
              : ready
                ? "The arena is ready."
                : "The arena is loading. You can enter now or when it is ready."}
          </Text>
        </div>
        <footer className="launcher-page-footer">
          <a href={PRIVACY_URL} target="_blank" rel="noopener">
            Privacy notice
          </a>
        </footer>
      </div>
    </section>
  );
}
