import { useEffect, useState } from "react";
import { platform } from "@tauri-apps/plugin-os";
import { commands } from "../bindings";
import { updaterFeedReady } from "../lib/updater";

export function useUpdateChannelAvailable(): boolean {
  const [available, setAvailable] = useState(false);

  useEffect(() => {
    if (!updaterFeedReady(platform())) return;

    let active = true;
    commands
      .isUpdateChannelAvailable()
      .then((value) => {
        if (active) setAvailable(value);
      })
      .catch((error) => {
        console.error("Failed to resolve the installed update channel:", error);
      });

    return () => {
      active = false;
    };
  }, []);

  return available;
}
