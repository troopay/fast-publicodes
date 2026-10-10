import * as addon from "../native/index.js";
import { useBinding } from "./binding.js";

useBinding(addon);

export * from "./api.js";
export { default } from "./api.js";
