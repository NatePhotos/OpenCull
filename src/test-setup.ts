// Minimal browser globals the store touches when it loads.

const g = globalThis as unknown as Record<string, unknown>;

if (g.window === undefined) {
  g.window = globalThis;
  g.addEventListener = () => {};
  g.removeEventListener = () => {};
}

if (g.navigator === undefined) {
  g.navigator = { userAgent: "" };
}

if (g.localStorage === undefined) {
  const items = new Map<string, string>();
  g.localStorage = {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, String(value)),
    removeItem: (key: string) => void items.delete(key),
    clear: () => items.clear(),
  };
}
