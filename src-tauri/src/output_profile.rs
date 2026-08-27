//! Per-application output settings (issue #123).
//!
//! The global output settings describe how AudioBud types by default. A
//! [`OutputProfile`] describes how it should type into one named application
//! instead: a terminal that wants Shift+Insert and no send key, a chat box that
//! wants Ctrl+V and Enter. Profiles are written by hand -- nothing here detects,
//! learns, or suggests one.
//!
//! This module owns the two decisions that keeps that from leaking into the
//! paste path: which profile applies to a delivery, and what the settings for
//! that one delivery therefore are. Both are pure functions over values, so the
//! rules can be tested on any platform even though reading the destination
//! window's application name is Windows-only for now (#119).
//!
//! A profile is never written back into the settings store. It changes one
//! delivery's decisions and nothing else, so switching applications cannot
//! quietly rewrite the settings the user configured.

use crate::settings::{AppSettings, AutoSubmitKey, ClipboardHandling, OutputProfile, PasteMethod};

/// Reduce an application name to the form profiles are matched on: no
/// surrounding blanks, no case, and no ".exe" suffix.
///
/// The name AudioBud reads from a window is already the program's file stem
/// ("code", "WindowsTerminal"), but people type what they see in Task Manager,
/// so "Code.exe" has to find the same profile as "code".
pub fn normalize_app_name(name: &str) -> String {
    let lowered = name.trim().to_lowercase();
    lowered
        .strip_suffix(".exe")
        .map(|stem| stem.to_string())
        .unwrap_or(lowered)
}

/// The profile for `app_name`, if the user wrote one.
///
/// `None` for an application with no profile, and for a delivery whose
/// destination application could not be read at all -- which is every delivery
/// on the platforms without a window backend yet (#119). Both mean the same
/// thing: use the global settings, exactly as before profiles existed.
///
/// The first match wins. Two profiles for the same application are a mistake the
/// settings UI already refuses to create, and picking the first keeps the answer
/// predictable if one ever reaches the store by hand-editing.
pub fn matching_profile<'a>(
    profiles: &'a [OutputProfile],
    app_name: Option<&str>,
) -> Option<&'a OutputProfile> {
    let wanted = normalize_app_name(app_name?);
    if wanted.is_empty() {
        return None;
    }
    profiles
        .iter()
        .find(|profile| normalize_app_name(&profile.app_name) == wanted)
}

/// The output settings one delivery actually runs with: the global settings,
/// with the matching profile's overrides folded over them.
///
/// Everything that decides how a transcript is delivered reads these rather than
/// the settings directly, so the focus-capability question (#162), the
/// confirmation gate (#279), and the keystrokes themselves all agree about what
/// this delivery is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveOutput {
    pub paste_method: PasteMethod,
    pub auto_submit: bool,
    pub auto_submit_key: AutoSubmitKey,
    pub clipboard_handling: ClipboardHandling,
}

impl EffectiveOutput {
    /// The global settings, with no profile applied.
    pub fn global(settings: &AppSettings) -> Self {
        Self {
            paste_method: settings.paste_method,
            auto_submit: settings.auto_submit,
            auto_submit_key: settings.auto_submit_key,
            clipboard_handling: settings.clipboard_handling,
        }
    }

    /// The settings for a delivery to `app_name`.
    pub fn resolve(settings: &AppSettings, app_name: Option<&str>) -> Self {
        let mut effective = Self::global(settings);
        if let Some(profile) = matching_profile(&settings.output_profiles, app_name) {
            effective.overlay(profile);
        }
        effective
    }

    /// Apply one profile's overrides. An override left unset keeps the global
    /// value, so a profile only has to name what it changes.
    fn overlay(&mut self, profile: &OutputProfile) {
        if let Some(paste_method) = profile.paste_method {
            self.paste_method = paste_method;
        }
        if let Some(auto_submit) = profile.auto_submit {
            self.auto_submit = auto_submit;
        }
        if let Some(auto_submit_key) = profile.auto_submit_key {
            self.auto_submit_key = auto_submit_key;
        }
        if let Some(clipboard_handling) = profile.clipboard_handling {
            self.clipboard_handling = clipboard_handling;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn profile(app_name: &str) -> OutputProfile {
        OutputProfile {
            app_name: app_name.to_string(),
            paste_method: None,
            auto_submit: None,
            auto_submit_key: None,
            clipboard_handling: None,
        }
    }

    #[test]
    fn a_typed_name_finds_the_profile_whatever_its_case_or_suffix() {
        let profiles = vec![profile("Code.exe")];
        assert!(matching_profile(&profiles, Some("code")).is_some());
        assert!(matching_profile(&profiles, Some("CODE.EXE")).is_some());
        assert!(matching_profile(&profiles, Some("  code  ")).is_some());
        assert!(matching_profile(&profiles, Some("codex")).is_none());
    }

    #[test]
    fn a_delivery_with_no_readable_application_uses_the_global_settings() {
        // Every delivery on a platform with no window backend yet (#119) lands
        // here, so this is the case that has to stay exactly as it was.
        let profiles = vec![profile("code")];
        assert!(matching_profile(&profiles, None).is_none());
        assert!(matching_profile(&profiles, Some("")).is_none());
        assert!(matching_profile(&profiles, Some("   ")).is_none());
    }

    #[test]
    fn an_empty_profile_name_never_matches() {
        // A blank name would otherwise swallow every delivery whose application
        // AudioBud could not read.
        let profiles = vec![profile("   ")];
        assert!(matching_profile(&profiles, Some("code")).is_none());
        assert!(matching_profile(&profiles, None).is_none());
    }

    #[test]
    fn no_profiles_means_the_global_settings_are_unchanged() {
        let mut settings = get_default_settings();
        settings.paste_method = PasteMethod::CtrlV;
        settings.auto_submit = true;
        settings.auto_submit_key = AutoSubmitKey::Enter;
        settings.clipboard_handling = ClipboardHandling::CopyToClipboard;

        let effective = EffectiveOutput::resolve(&settings, Some("code"));
        assert_eq!(effective, EffectiveOutput::global(&settings));
    }

    #[test]
    fn a_profile_changes_only_what_it_names() {
        let mut settings = get_default_settings();
        settings.paste_method = PasteMethod::CtrlV;
        settings.auto_submit = true;
        settings.auto_submit_key = AutoSubmitKey::Enter;
        settings.clipboard_handling = ClipboardHandling::CopyToClipboard;
        settings.output_profiles = vec![OutputProfile {
            app_name: "WindowsTerminal".to_string(),
            paste_method: Some(PasteMethod::ShiftInsert),
            auto_submit: Some(false),
            auto_submit_key: None,
            clipboard_handling: None,
        }];

        let terminal = EffectiveOutput::resolve(&settings, Some("WindowsTerminal"));
        assert_eq!(terminal.paste_method, PasteMethod::ShiftInsert);
        assert!(!terminal.auto_submit);
        // Untouched by the profile, so still whatever the user set globally.
        assert_eq!(terminal.auto_submit_key, AutoSubmitKey::Enter);
        assert_eq!(
            terminal.clipboard_handling,
            ClipboardHandling::CopyToClipboard
        );

        // And an application with no profile is unaffected by the terminal's.
        let other = EffectiveOutput::resolve(&settings, Some("chrome"));
        assert_eq!(other, EffectiveOutput::global(&settings));
    }

    #[test]
    fn two_profiles_stay_apart() {
        // The acceptance case from #123: a terminal and a chat box, each
        // delivering with its own settings and neither changed by hand.
        let mut settings = get_default_settings();
        settings.paste_method = PasteMethod::CtrlV;
        settings.auto_submit = false;
        settings.output_profiles = vec![
            OutputProfile {
                app_name: "WindowsTerminal".to_string(),
                paste_method: Some(PasteMethod::ShiftInsert),
                auto_submit: Some(false),
                auto_submit_key: None,
                clipboard_handling: None,
            },
            OutputProfile {
                app_name: "Slack".to_string(),
                paste_method: Some(PasteMethod::CtrlV),
                auto_submit: Some(true),
                auto_submit_key: Some(AutoSubmitKey::Enter),
                clipboard_handling: None,
            },
        ];

        let terminal = EffectiveOutput::resolve(&settings, Some("WindowsTerminal"));
        assert_eq!(terminal.paste_method, PasteMethod::ShiftInsert);
        assert!(!terminal.auto_submit);

        let chat = EffectiveOutput::resolve(&settings, Some("slack.exe"));
        assert_eq!(chat.paste_method, PasteMethod::CtrlV);
        assert!(chat.auto_submit);
        assert_eq!(chat.auto_submit_key, AutoSubmitKey::Enter);
    }

    #[test]
    fn the_first_profile_for_an_application_wins() {
        let mut settings = get_default_settings();
        settings.output_profiles = vec![
            OutputProfile {
                app_name: "code".to_string(),
                paste_method: Some(PasteMethod::ShiftInsert),
                auto_submit: None,
                auto_submit_key: None,
                clipboard_handling: None,
            },
            OutputProfile {
                app_name: "Code.exe".to_string(),
                paste_method: Some(PasteMethod::Direct),
                auto_submit: None,
                auto_submit_key: None,
                clipboard_handling: None,
            },
        ];
        assert_eq!(
            EffectiveOutput::resolve(&settings, Some("code")).paste_method,
            PasteMethod::ShiftInsert
        );
    }

    #[test]
    fn a_profile_can_turn_the_clipboard_copy_on_for_one_application() {
        let mut settings = get_default_settings();
        settings.clipboard_handling = ClipboardHandling::DontModify;
        settings.output_profiles = vec![OutputProfile {
            app_name: "notepad".to_string(),
            paste_method: None,
            auto_submit: None,
            auto_submit_key: None,
            clipboard_handling: Some(ClipboardHandling::CopyToClipboard),
        }];
        assert_eq!(
            EffectiveOutput::resolve(&settings, Some("notepad")).clipboard_handling,
            ClipboardHandling::CopyToClipboard
        );
        assert_eq!(
            EffectiveOutput::resolve(&settings, Some("chrome")).clipboard_handling,
            ClipboardHandling::DontModify
        );
    }
}
