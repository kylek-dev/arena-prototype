import { LOADING_ACCOUNT } from "../../runtime/account";
import { accountService } from "../../state/account";
import { mapEditorAllowed, mapWritable } from "../../state/map-editor";
import { createStore, useStore } from "../../state/store";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);

/** Whether the door says this player is an admin (it checks every request). */
export function useAdmin() {
  const account = useStore(accountService, (a) => a);
  return useStore(account?.state ?? NO_ACCOUNT, (s) => !!s.me?.admin);
}

/** Whether the map editor is offered: admins only, or the local authoring server. */
export function useMapEditorAllowed() {
  const admin = useAdmin();
  const writable = useStore(mapWritable, (w) => w);
  return mapEditorAllowed(admin, writable);
}
