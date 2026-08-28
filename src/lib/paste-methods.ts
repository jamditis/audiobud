import type { PasteMethod } from "@/bindings";
import type { OSType } from "./utils/keyboard";

/** Paste methods that the current platform can safely offer in settings. */
export function pasteMethodsForOs(osType: OSType): PasteMethod[] {
  const methods: PasteMethod[] = ["ctrl_v"];

  if (osType === "windows" || osType === "linux") {
    methods.push("direct");
  }

  methods.push("none");

  if (osType === "windows" || osType === "linux") {
    methods.push("ctrl_shift_v", "shift_insert");
  }

  if (osType === "linux") {
    methods.push("external_script");
  }

  return methods;
}
