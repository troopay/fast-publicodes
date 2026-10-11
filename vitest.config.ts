import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

/** `wasm` runs everything against the WebAssembly engine instead of the Node addon. */
const target = process.env.FAST_PUBLICODES_TARGET;

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    // Comparisons build and evaluate whole models with publicodes, slow on some CI runners.
    testTimeout: 60_000,
  },
  resolve: {
    alias:
      target === "wasm"
        ? [
            {
              find: /^(\.\.\/)+src\/index\.js$/,
              replacement: fileURLToPath(
                new URL("test/wasm.ts", import.meta.url),
              ),
            },
          ]
        : [],
  },
});
