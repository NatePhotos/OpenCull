import path from "node:path";
import { defineConfig } from "vitest/config";

// The frontend's unit tests: the culling rules and the store, run in Node
// against the in-browser stand-in for the app shell (src/lib/mock.ts).
export default defineConfig({
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "./src") },
  },
  test: {
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test-setup.ts"],
  },
});
