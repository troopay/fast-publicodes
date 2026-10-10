import init, * as module from "../wasm/fast_publicodes.js";
import { useBinding } from "./binding.js";

await init();
useBinding(module);

export * from "./api.js";
export { default } from "./api.js";
