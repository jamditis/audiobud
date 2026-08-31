import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";

const generalSettings = readFileSync(
  "src/components/settings/general/GeneralSettings.tsx",
  "utf8",
);
const advancedSettings = readFileSync(
  "src/components/settings/advanced/AdvancedSettings.tsx",
  "utf8",
);
const tray = readFileSync("src-tauri/src/tray.rs", "utf8");
const targetBackend = readFileSync(
  "src-tauri/src/output_target/backend.rs",
  "utf8",
);
const pickerBackend = readFileSync(
  "src-tauri/src/window_picker/backend.rs",
  "utf8",
);
const shortcutSettings = readFileSync("src-tauri/src/shortcut/mod.rs", "utf8");
const appSettings = readFileSync("src-tauri/src/settings.rs", "utf8");
const lockSetting = readFileSync(
  "src/components/settings/OutputTargetLock.tsx",
  "utf8",
);
const picker = readFileSync("src/window-picker/WindowPicker.tsx", "utf8");

describe("experimental Windows output targeting", () => {
  it("stays off by default", () => {
    expect(appSettings).toContain("experimental_enabled: false,");
  });

  it("hides every settings entry point until experimental features are enabled", () => {
    expect(generalSettings).not.toContain("toggle_target_lock");
    expect(generalSettings).not.toContain("pick_output_window");
    expect(advancedSettings).toContain(
      "const experimentalTargetingEnabled = isWindows && experimentalEnabled;",
    );
    expect(advancedSettings).toContain(
      "experimentalTargetingEnabled && (\n            <>\n              <OutputTargetLock",
    );
    expect(advancedSettings).toContain(
      '<ShortcutInput shortcutId="toggle_target_lock"',
    );
    expect(advancedSettings).toContain(
      '<ShortcutInput shortcutId="pick_output_window"',
    );
  });

  it("removes tray entry points while the experimental setting is off", () => {
    expect(tray).toContain(
      "let experimental_targeting_enabled = settings.experimental_enabled;",
    );
    expect(tray).toContain(
      "if experimental_targeting_enabled {\n                items.push(&toggle_target_lock_i);",
    );
    expect(tray).toContain(
      "if experimental_targeting_enabled {\n                items.push(&pick_output_window_i);",
    );
  });

  it("rejects stale tray and shortcut actions in the backend", () => {
    expect(targetBackend).toContain(
      "if !crate::settings::get_settings(app).experimental_enabled",
    );
    expect(
      pickerBackend.match(
        /if !crate::settings::get_settings\(app\)\.experimental_enabled/g,
      ),
    ).toHaveLength(3);
    expect(pickerBackend).toContain(
      "abandon_pick(app);\n        close_picker(app);\n        return PickArmed::Cancelled;",
    );
  });

  it("clears live experimental targeting state when the setting is disabled", () => {
    expect(shortcutSettings).toContain("DisableExperimentalTargeting");
    expect(shortcutSettings).toMatch(
      /"experimental_enabled" => &\[[\s\S]*?DisableExperimentalTargeting,[\s\S]*?SyncExperimentalTargetingShortcuts,[\s\S]*?RefreshTrayMenu,[\s\S]*?EmitChanged,[\s\S]*?\]/,
    );
    expect(shortcutSettings).toContain(
      "if !settings.experimental_enabled {\n                    crate::output_target::backend::unlock_output_target(app);\n                    crate::window_picker::backend::abandon_pick(app);\n                    crate::window_picker::backend::close_picker(app);",
    );
    expect(shortcutSettings).toContain(
      ".filter(|(id, _)| shortcut_enabled_for_settings(id, &user_settings))",
    );
    expect(shortcutSettings).toContain(
      "if !shortcut_enabled_for_settings(&id, &current_settings)",
    );
    expect(shortcutSettings).toContain(
      "if !shortcut_enabled_for_settings(id, &current_settings)",
    );
  });

  it("serializes every targeting mutation with disable cleanup", () => {
    expect(targetBackend).toContain("experimental_targeting_guard()");
    expect(
      pickerBackend.match(/experimental_targeting_guard\(\)/g),
    ).toHaveLength(3);
    expect(shortcutSettings).toContain("experimental_targeting_guard()");
  });

  it("labels the lock and picker as experimental on their own surfaces", () => {
    for (const surface of [lockSetting, picker]) {
      expect(surface).toContain(
        'const experimentalLabel = t("settings.advanced.groups.experimental");',
      );
      expect(surface).toContain("experimentalLabel");
    }
  });
});
