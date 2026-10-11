// The few parts of a browser window the store reaches for as it loads. The
// logic under test needs none of them to work; they only have to be there.

const g = globalThis as unknown as Record<string, unknown>;

if (g.window === undefined) {
  g.window = globalThis;
  g.addEventListener = () => {};
  g.removeEventListener = () => {};
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
