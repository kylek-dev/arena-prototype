import { Button, Text } from "@radix-ui/themes";
import { useState } from "react";
import {
  LOGIN_URL,
  SIGN_UP_URL,
  accountErrorText,
  isName,
  renameWait,
  type Me,
} from "../../domain/account";
import { handleName } from "../../domain/handles";
import { LOADING_ACCOUNT, type Account } from "../../runtime/account";
import { accountService } from "../../state/account";
import { createStore, useStore } from "../../state/store";
import { Hint } from "../../ui/Tooltip";
import "./account.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);

/** "Choose your name": the player picks the word, the door the number. */
function ChooseName({ account, me, busy }: { account: Account; me: Me; busy: boolean }) {
  const [open, setOpen] = useState(!me.handle);
  const [name, setName] = useState("");
  const wait = renameWait(me.renameAt, Date.now() / 1000);
  if (!open)
    return (
      <div className="account-actions">
        <Button size="1" variant="surface" color="gray" onClick={() => setOpen(true)}>
          Change name
        </Button>
        {wait && (
          <Text size="1" color="gray">
            You can choose a different name {wait}.
          </Text>
        )}
      </div>
    );
  const valid = isName(name);
  const save = async () => {
    if (!valid || busy) return;
    if (await account.chooseName(name)) {
      setName("");
      setOpen(false);
    }
  };
  return (
    <div className="account-name" role="group" aria-label="Choose your name">
      <label className="account-name-field">
        <Text size="1">Choose your name</Text>
        <Hint text="Use 3 to 16 letters and digits, starting with a letter. A number is assigned automatically.">
          <input
            value={name}
            maxLength={16}
            placeholder={me.handle ? handleName(me.handle) : "Name"}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setName(event.target.value.trim())}
            onKeyDown={(event) => event.key === "Enter" && void save()}
          />
        </Hint>
      </label>
      {wait && (
        <Text size="1" color="gray">
          You can choose a different name {wait}. Changes to capitalization are allowed at any time.
        </Text>
      )}
      <div className="account-actions">
        <Button size="1" disabled={!valid || busy} onClick={() => void save()}>
          Save name
        </Button>
        {me.handle && (
          <Button
            size="1"
            variant="soft"
            color="gray"
            onClick={() => {
              setName("");
              setOpen(false);
              account.dismiss();
            }}
          >
            Cancel
          </Button>
        )}
      </div>
    </div>
  );
}

/** Login and account status on the launcher page (social-identity, ADR
 * 0055), which links the privacy notice. Shows nothing where the door offers
 * no login, or cannot be reached. `guest` is this browser's guest name. */
export function AccountPanel({ guest = null }: { guest?: string | null }) {
  const account = useStore(accountService, (a) => a);
  const state = useStore(account?.state ?? NO_ACCOUNT, (s) => s);
  const [confirming, setConfirming] = useState(false);
  if (!account || state.status !== "ready" || !state.me?.login) return null;
  const { me, busy, error } = state;
  const notice = error && (
    <div className="account-error" role="alert">
      <Text size="1">{accountErrorText(error)}</Text>
      <Button size="1" variant="soft" color="gray" onClick={() => account.dismiss()}>
        OK
      </Button>
    </div>
  );
  if (!me.account)
    return (
      <div className="account" role="group" aria-label="Account">
        <Text size="2">
          {guest ? (
            <>
              You are playing as the guest <strong className="account-handle">{guest}</strong>.
            </>
          ) : (
            "You are playing as a guest."
          )}{" "}
          Log in to keep a player name, add friends and use public chat.
        </Text>
        <div className="account-actions">
          <Button size="1" asChild>
            <a href={LOGIN_URL}>Log in</a>
          </Button>
          <Button size="1" variant="surface" asChild>
            <a href={SIGN_UP_URL}>Sign up</a>
          </Button>
        </div>
        {notice}
      </div>
    );
  return (
    <div className="account" role="group" aria-label="Account">
      <Text size="1">
        {me.handle ? (
          <>
            Logged in as <strong className="account-handle">{me.handle}</strong>
          </>
        ) : (
          "You are logged in without a player name."
        )}
      </Text>
      <ChooseName key={me.handle ?? ""} account={account} me={me} busy={!!busy} />
      {confirming ? (
        <div className="account-confirm" role="alertdialog" aria-label="Delete account">
          <Text size="1">
            Delete your account? Your name, friends and blocks are removed here, and your login is
            removed at WorkOS. This cannot be undone.
          </Text>
          <div className="account-actions">
            <Button
              size="1"
              color="red"
              disabled={!!busy}
              onClick={async () => {
                await account.deleteAccount();
                // On a refusal the question stays open, so retrying is one click.
                if (!account.state.get().error) setConfirming(false);
              }}
            >
              {busy === "delete" ? "Deleting…" : "Delete account"}
            </Button>
            <Button
              size="1"
              variant="soft"
              color="gray"
              disabled={!!busy}
              onClick={() => setConfirming(false)}
            >
              Keep account
            </Button>
          </div>
        </div>
      ) : (
        <div className="account-actions">
          <Button
            size="1"
            variant="soft"
            color="gray"
            disabled={!!busy}
            onClick={() => account.logout()}
          >
            Log out
          </Button>
          <Button
            size="1"
            variant="soft"
            color="gray"
            disabled={!!busy}
            title="Ends your sessions on every device and browser"
            onClick={() => account.logoutEverywhere()}
          >
            Log out everywhere
          </Button>
          <Button
            size="1"
            variant="soft"
            color="red"
            disabled={!!busy}
            onClick={() => {
              account.dismiss();
              setConfirming(true);
            }}
          >
            Delete account…
          </Button>
        </div>
      )}
      {notice}
    </div>
  );
}
