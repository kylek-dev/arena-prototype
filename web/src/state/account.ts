import type { Friends } from "../runtime/friends";
import type { Account } from "../runtime/account";
import { createStore } from "./store";
// The account service, once the runtime has started; null in tests and
// before startup. Its own `state` store carries who is logged in.
export const accountService = createStore<Account | null>(null);
// The friends service, likewise; its `state` store carries the list.
export const friendsService = createStore<Friends | null>(null);
