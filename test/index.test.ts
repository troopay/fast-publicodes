import { expect, test, vi } from "vitest";

test("falls back to WebAssembly when no addon is installed", async () => {
  vi.doMock("node:module", () => ({
    createRequire: () => () => {
      throw new Error("Cannot find native binding");
    },
  }));
  const { default: Engine } = await import("../src/index.js");
  const { binding } = await import("../src/binding.js");
  expect(binding()).toBe(await import("../wasm/fast_publicodes.js"));
  expect(new Engine({ a: 2, b: "a * 3" }).evaluate("b").nodeValue).toBe(6);
});
