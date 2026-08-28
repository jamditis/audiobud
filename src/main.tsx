import React from "react";
import ReactDOM from "react-dom/client";
import { platform } from "@tauri-apps/plugin-os";
import App from "./App";

// Set platform before render so CSS can scope per-platform (e.g. scrollbar styles)
document.documentElement.dataset.platform = platform();

// Initialize i18n
import "./i18n";

// Initialize model store (loads models and sets up event listeners)
import { useModelStore } from "./stores/modelStore";
useModelStore.getState().initialize();

// Initialize settings once here instead of once per useSettings consumer.
import { useSettingsStore } from "./stores/settingsStore";
import { initializeSettingsWithRetry } from "./stores/settingsCoordination";
void initializeSettingsWithRetry(() =>
  useSettingsStore.getState().initialize(),
).catch((error) => console.error("Failed to initialize settings:", error));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
