import type { Account } from "../runtime/account";
import { createStore } from "./store";
// The account service, once the runtime has started; null in tests and
// before startup. Its own `state` store carries who is logged in.
export const accountService = createStore<Account | null>(null);
