import { useSyncExternalStore } from "react";
export function createStore<T>(initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => value,
    set(next: T) {
      if (Object.is(next, value)) return;
      value = next;
      for (const listener of listeners) listener();
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
export function useStore<T, S>(store: ReturnType<typeof createStore<T>>, select: (v: T) => S) {
  return useSyncExternalStore(store.subscribe, () => select(store.get()));
}
