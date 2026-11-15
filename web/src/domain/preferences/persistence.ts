import { initial, parsePreferences, type Preferences } from "./schema";
export const STORAGE_KEY = "arena-ui-preferences";
export class Persistence {
  blocked = false;
  timer: ReturnType<typeof setTimeout> | undefined;
  pending: Preferences | null = null;
  constructor(
    private storage: Pick<Storage, "getItem" | "setItem">,
    private report: (message: string) => void,
  ) {}
  load() {
    try {
      const raw = this.storage.getItem(STORAGE_KEY);
      return raw ? parsePreferences(raw) : initial();
    } catch (e) {
      this.blocked = true;
      this.report(
        `${String(e)} Using temporary defaults; export your changes before leaving. Stored data will not be overwritten.`,
      );
      return initial();
    }
  }
  schedule(p: Preferences) {
    this.pending = p;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.flush(), 250);
  }
  flush() {
    clearTimeout(this.timer);
    if (!this.pending || this.blocked) return;
    try {
      this.storage.setItem(STORAGE_KEY, JSON.stringify(this.pending));
      this.pending = null;
      this.report("");
    } catch {
      this.report(
        "Saving failed. Changes work for this session; export your profiles to keep them.",
      );
    }
  }
  // Explicit recovery only; unknown future versions never get overwritten silently.
  enableWrites() {
    this.blocked = false;
    this.flush();
  }
}
