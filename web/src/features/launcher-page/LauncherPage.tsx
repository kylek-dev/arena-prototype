import { Button, Text } from "@radix-ui/themes";
import { useEffect, useRef } from "react";
import { PRIVACY_URL } from "../../domain/account";
import { dispatch } from "../../runtime/commands";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { enterHub } from "../../runtime/hub";
import { accountService } from "../../state/account";
import { beforeMatch, enterArena } from "../../state/appearance";
import { engine, error } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { arenaReady } from "../loading/Loading";
import { AccountBar } from "./AccountBar";
import { News } from "./News";
import { PartyStatus } from "./PartyStatus";
import "./launcher-page.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);

/**
 * The launcher page (planning/issues/launcher-page.md): what a visitor sees
 * before entering the game. A top bar with the game's name and the account
 * (log in or sign up, a guest's name, a logged-in player's menu: name,
 * log out, delete), the way in, and the news, which admins post here. It
 * covers the whole window while the arena loads behind it, so Enter is
 * immediate once it is ready.
 */
export function LauncherPage() {
  const visible = useStore(beforeMatch, (v) => v);
  const account = useStore(accountService, (a) => a);
  const admin = useStore(account?.state ?? NO_ACCOUNT, (s) => !!s.me?.admin);
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
      <header className="launcher-top">
        <span className="launcher-brand">Arena Prototype</span>
        <AccountBar />
      </header>
      <main className="launcher-main">
        <div className="launcher-hero">
          <h1>Welcome to the arena</h1>
          <Text as="p" size="3" color="gray">
            Meet other players in the hub, queue for matches, or practice on your own.
          </Text>
          <div className="launcher-page-enter">
            <Button
              ref={enter}
              size="4"
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
          <PartyStatus />
        </div>
        <News admin={admin} />
      </main>
      <footer className="launcher-page-footer">
        <a href={PRIVACY_URL} target="_blank" rel="noopener">
          Privacy notice
        </a>
      </footer>
    </section>
  );
}
