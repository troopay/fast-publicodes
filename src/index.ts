import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import type * as addon from "../native/index.cjs";
import * as wasm from "../wasm/fast_publicodes.js";
import { type Binding, useBinding } from "./binding.js";

useBinding(loadAddon() ?? loadWasm());

/** The addon built for this platform, if one is installed. */
function loadAddon(): Binding | undefined {
  try {
    return createRequire(import.meta.url)(
      "../native/index.cjs",
    ) as typeof addon;
  } catch {
    return undefined;
  }
}

function loadWasm(): Binding {
  wasm.initSync({
    module: readFileSync(
      new URL("../wasm/fast_publicodes_bg.wasm", import.meta.url),
    ),
  });
  return wasm;
}

export * from "./api.js";
export { default } from "./api.js";
