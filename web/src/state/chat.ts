import { createStore } from "./store";
// Enter (or `/`) opens the chat box when one is shown: the input router
// counts the requests, with any text to start the field with, and the chat
// box focuses its field on each.
export const chatFocusRequest = createStore<{ count: number; prefill: string }>({
  count: 0,
  prefill: "",
});
/** Whether a chat box is shown that Enter can open. */
export const chatShown = createStore(false);
/** Asks the chat box to take the keys, starting the field with `prefill`. */
export function requestChat(prefill = "") {
  chatFocusRequest.set({ count: chatFocusRequest.get().count + 1, prefill });
}
