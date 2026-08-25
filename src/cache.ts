// Persistent search-result cache (localStorage, capped) so previously
// loaded lists render instantly; freshness comes from the background
// revalidate that always follows.
import type { ModelSummary } from "./types";

const STORE_KEY = "lalalm.search-cache.v1";
const MAX_ENTRIES = 24;

export interface CacheEntry {
  ts: number;
  results: ModelSummary[];
}

let mem: Map<string, CacheEntry> | null = null;

function load(): Map<string, CacheEntry> {
  if (mem) return mem;
  mem = new Map();
  try {
    const raw = localStorage.getItem(STORE_KEY);
    if (raw) {
      const obj = JSON.parse(raw) as Record<string, CacheEntry>;
      for (const [k, v] of Object.entries(obj)) {
        if (v && Array.isArray(v.results)) mem.set(k, v);
      }
    }
  } catch {
    /* corrupted cache — start fresh */
  }
  return mem;
}

function persist(m: Map<string, CacheEntry>) {
  try {
    const newest = [...m.entries()]
      .sort((a, b) => b[1].ts - a[1].ts)
      .slice(0, MAX_ENTRIES);
    localStorage.setItem(STORE_KEY, JSON.stringify(Object.fromEntries(newest)));
  } catch {
    /* quota exceeded — drop silently */
  }
}

export const searchCache = {
  get(key: string): CacheEntry | undefined {
    return load().get(key);
  },
  put(key: string, results: ModelSummary[]) {
    if (!key || results.length === 0) return;
    const m = load();
    m.set(key, { ts: Date.now(), results });
    persist(m);
  },
};
