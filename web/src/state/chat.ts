import { createStore } from "./store";
// Enter opens the chat box (when one is shown): the input router counts the
// requests and the chat box focuses its field on each.
export const chatFocusRequest = createStore(0);
/** Whether a chat box is shown that Enter can open. */
export const chatShown = createStore(false);
