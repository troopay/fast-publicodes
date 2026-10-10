import { readFileSync } from "node:fs";
import { useBinding } from "../src/binding.js";
import * as module from "../wasm/fast_publicodes.js";

module.initSync({
  module: readFileSync(
    new URL("../wasm/fast_publicodes_bg.wasm", import.meta.url),
  ),
});
useBinding(module);

export * from "../src/api.js";
export { default } from "../src/api.js";
