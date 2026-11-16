import type { Link } from "../runtime/network";
import { createStore } from "./store";
// The network match link, once the runtime has started; null in tests and
// before startup. The link's own `state` store carries its status.
export const networkLink = createStore<Link | null>(null);
