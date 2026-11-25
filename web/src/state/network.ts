import type { Queue } from "../runtime/matchmaking";
import type { Link } from "../runtime/network";
import { createStore } from "./store";
// The network match link, once the runtime has started; null in tests and
// before startup. The link's own `state` store carries its status.
export const networkLink = createStore<Link | null>(null);
// The solo queues (ADR 0046), likewise; its `state` store carries the search.
export const matchQueue = createStore<Queue | null>(null);
/** Relayed per-actor `Name#number` handles in a network match (local order);
 * `null` outside one. Shown by the HUD (social-identity display follow-up). */
export const remoteHandles = createStore<(string | null)[] | null>(null);
