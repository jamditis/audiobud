import { useEffect, useState, useRef } from "react";
import { toast, Toaster } from "sonner";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { platform } from "@tauri-apps/plugin-os";
import {
  ModelStateEvent,
  RecordingErrorEvent,
  TranscriptionErrorEvent,
  TranscriptionTimeoutEvent,
} from "./lib/types/events";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import Footer from "./components/footer";
import Onboarding, { AccessibilityOnboarding } from "./components/onboarding";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import SwampBackground from "./components/SwampBackground";
import { useSettings } from "./hooks/useSettings";
import { usePermissionController } from "./hooks/usePermissionController";
import { useSettingsStore } from "./stores/settingsStore";
import { commands, events } from "@/bindings";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";
import {
  classifyTranscriptionError,
  recordingDurationLabel,
} from "@/lib/transcription-error";
import {
  formatDeliveredWindowName,
  truncateName,
} from "@/lib/output-target-indicator";
import { claimPermissionCompletion } from "@/lib/permission-controller";

type OnboardingStep = "accessibility" | "model" | "done";
const PRODUCT_NAME = "AudioBud";

const renderSettingsContent = (
  section: SidebarSection,
  onNavigate: (section: SidebarSection) => void,
) => {
  const ActiveComponent =
    SECTIONS_CONFIG[section]?.component || SECTIONS_CONFIG.general.component;
  // Sections that ignore onNavigate are unaffected; only About's guide uses it
  // to send the reader to the section that owns a given control.
  return <ActiveComponent onNavigate={onNavigate} />;
};

function App() {
  const { t, i18n } = useTranslation();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  // Track if this is a returning user who just needs to grant permissions
  // (vs a new user who needs full onboarding including model selection)
  const [isReturningUser, setIsReturningUser] = useState(false);
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>("general");
  const { settings, updateSetting } = useSettings();
  const direction = getLanguageDirection(i18n.language);
  const refreshAudioDevices = useSettingsStore(
    (state) => state.refreshAudioDevices,
  );
  const refreshOutputDevices = useSettingsStore(
    (state) => state.refreshOutputDevices,
  );
  const refreshSettings = useSettingsStore((state) => state.refreshSettings);
  const permissions = usePermissionController();
  const hasCompletedPostOnboardingInit = useRef(false);
  const permissionCompletionGuard = useRef({ completed: false });

  useEffect(() => {
    checkOnboardingStatus();
  }, []);

  useEffect(() => {
    if (onboardingStep !== "accessibility") return;
    if (
      !claimPermissionCompletion(
        permissionCompletionGuard.current,
        permissions.allGranted,
      )
    ) {
      return;
    }

    let cancelled = false;
    let didComplete = false;
    const completionTimer = setTimeout(() => {
      if (!cancelled) {
        didComplete = true;
        setOnboardingStep(isReturningUser ? "done" : "model");
      }
    }, 300);

    return () => {
      cancelled = true;
      clearTimeout(completionTimer);
      if (!didComplete) permissionCompletionGuard.current.completed = false;
    };
  }, [isReturningUser, onboardingStep, permissions.allGranted]);

  // Initialize RTL direction when language changes
  useEffect(() => {
    initializeRTL(i18n.language);
  }, [i18n.language]);

  // Initialize Enigo, shortcuts, and refresh audio devices when main app loads
  useEffect(() => {
    if (onboardingStep === "done" && !hasCompletedPostOnboardingInit.current) {
      hasCompletedPostOnboardingInit.current = true;
      // These commands return a tauri-specta Result: a Rust-side Err resolves as
      // { status: "error" } rather than rejecting, so the status must be checked explicitly. The
      // .catch only fires for transport-level failures (a thrown Error).
      commands
        .initializeEnigo()
        .then((res) => {
          if (res.status === "error") {
            console.warn("Failed to initialize Enigo:", res.error);
          }
        })
        .catch((e) => console.warn("Failed to initialize Enigo:", e));
      // initializeShortcuts may back-fill default bindings added in a newer version into the
      // persisted settings; refresh the store so they appear in the UI this session instead of only
      // after a later launch. This is independent of Enigo so a failed Enigo init cannot hide the
      // newly added binding until restart. Refresh runs regardless of the result so any bindings that
      // were persisted still surface; a hard error is logged rather than silently swallowed.
      commands
        .initializeShortcuts()
        .then((res) => {
          if (res.status === "error") {
            console.warn("Failed to initialize shortcuts:", res.error);
          }
          refreshSettings();
        })
        .catch((e) => console.warn("Failed to initialize shortcuts:", e));
      refreshAudioDevices();
      refreshOutputDevices();
    }
  }, [
    onboardingStep,
    refreshAudioDevices,
    refreshOutputDevices,
    refreshSettings,
  ]);

  // Handle keyboard shortcuts for debug mode toggle
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check for Ctrl+Shift+D (Windows/Linux) or Cmd+Shift+D (macOS)
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);

      if (isDebugShortcut) {
        event.preventDefault();
        const currentDebugMode = settings?.debug_mode ?? false;
        updateSetting("debug_mode", !currentDebugMode);
      }
    };

    // Add event listener when component mounts
    document.addEventListener("keydown", handleKeyDown);

    // Cleanup event listener when component unmounts
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settings?.debug_mode, updateSetting]);

  // Listen for recording errors from the backend and show a toast
  useEffect(() => {
    const unlisten = listen<RecordingErrorEvent>("recording-error", (event) => {
      const { error_type, detail } = event.payload;

      if (error_type === "microphone_permission_denied") {
        const currentPlatform = platform();
        const platformKey = `errors.micPermissionDenied.${currentPlatform}`;
        const description = t(platformKey, {
          defaultValue: t("errors.micPermissionDenied.generic"),
        });
        toast.error(t("errors.micPermissionDeniedTitle"), { description });
      } else if (error_type === "no_input_device") {
        toast.error(t("errors.noInputDeviceTitle"), {
          description: t("errors.noInputDevice"),
        });
      } else {
        toast.error(
          t("errors.recordingFailed", { error: detail ?? "Unknown error" }),
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for paste failures and show a toast.
  // The technical error detail is logged to handy.log on the Rust side
  // (see actions.rs `error!("Failed to paste transcription: ...")`),
  // so we show a localized, user-friendly message here instead of the raw error.
  useEffect(() => {
    const unlisten = listen("paste-error", () => {
      toast.error(t("errors.pasteFailedTitle"), {
        description: t("errors.pasteFailed"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for a dropped target lock (issue #120). The locked window closed, so
  // the backend suppressed the paste rather than send the transcript to whatever
  // now holds focus. The lock is already released when this fires.
  useEffect(() => {
    const unlisten = listen("target-lock-lost", () => {
      toast.warning(t("errors.targetLockLostTitle"), {
        description: t("errors.targetLockLost"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for a successful delivery to a pinned window -- a target lock
  // (#120) or a one-shot pick (#124) -- and confirm which window it reached
  // (issue #165). A plain foreground paste lands wherever the user is already
  // looking, so it gets no toast here: the point is confirming a delivery the
  // user was deliberately not watching, not narrating every paste.
  useEffect(() => {
    const unlisten = events.transcriptDeliveredEvent.listen((event) => {
      const name = truncateName(
        formatDeliveredWindowName(
          event.payload.app ?? undefined,
          event.payload.title ?? undefined,
        ),
      );
      // The fallback (both label lookups failed) has to describe the right
      // kind of destination: a one-shot pick (#124) is not a lock, and may
      // coexist with an unrelated lock, so it must not be described as one.
      const unknownKey =
        event.payload.source === "pick"
          ? "errors.transcriptDeliveredUnknownPick"
          : "errors.transcriptDeliveredUnknown";
      toast.success(t("errors.transcriptDeliveredTitle"), {
        description:
          name.length > 0
            ? t("errors.transcriptDelivered", { window: name })
            : t(unknownKey),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for a one-shot pick whose window closed before the transcript fired
  // (issue #124). The paste was suppressed rather than sent to whatever now
  // holds focus; nothing stays armed.
  useEffect(() => {
    const unlisten = listen("window-pick-lost", () => {
      toast.warning(t("errors.windowPickLostTitle"), {
        description: t("errors.windowPickLost"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for a transcript that reached no window because the window it was
  // dictated to closed first, after the user had already moved on from it --
  // either by unlocking or by locking onto something else (issue #160).
  // Distinct from target-lock-lost: whatever they set since still stands, so the
  // notice must neither tell them to lock again nor claim their lock changed.
  useEffect(() => {
    const unlisten = listen("target-window-gone", () => {
      toast.warning(t("errors.targetWindowGoneTitle"), {
        description: t("errors.targetWindowGone"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for a transcript that finished while the window picker was open
  // (issue #124). The picker holds the foreground, so pasting would have typed
  // into AudioBud itself; the text was withheld instead.
  useEffect(() => {
    const unlisten = listen("window-pick-in-progress", () => {
      toast.warning(t("errors.pickerOpenTitle"), {
        description: t("errors.pickerOpen"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for transcription watchdog timeouts (wedged engine, issue #58)
  // and show a toast. The Rust side has already recovered the overlay/tray.
  useEffect(() => {
    const unlisten = listen<TranscriptionTimeoutEvent>(
      "transcription-timeout",
      (event) => {
        toast.error(t("errors.transcriptionTimeoutTitle"), {
          description: t("errors.transcriptionTimeout", {
            seconds: event.payload.timeout_secs,
          }),
        });
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for transcription failures (e.g. Parakeet refusing a recording
  // past its length limit, issue #169) and show a toast. The payload carries
  // the backend's specific explanation, shown like the model-load error.
  useEffect(() => {
    const unlisten = listen<TranscriptionErrorEvent>(
      "transcription-error",
      (event) => {
        const presentation = classifyTranscriptionError(event.payload.message);
        if (presentation.kind === "generic") {
          console.error("Transcription failed:", event.payload.message);
        }
        toast.error(t("errors.transcriptionErrorTitle"), {
          description:
            presentation.kind === "generic"
              ? t("errors.transcriptionErrorGeneric")
              : t("errors.parakeetInputTooLong", {
                  duration: recordingDurationLabel(presentation.seconds),
                }),
        });
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for model loading failures and show a toast
  useEffect(() => {
    const unlisten = listen<ModelStateEvent>("model-state-changed", (event) => {
      if (event.payload.event_type === "loading_failed") {
        toast.error(
          t("errors.modelLoadFailed", {
            model:
              event.payload.model_name || t("errors.modelLoadFailedUnknown"),
          }),
          {
            description: event.payload.error,
          },
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  const revealMainWindowForPermissions = async () => {
    try {
      await commands.showMainWindowCommand();
    } catch (e) {
      console.warn("Failed to show main window for permission onboarding:", e);
    }
  };

  const checkOnboardingStatus = async () => {
    try {
      const [modelResult, permissionResult] = await Promise.allSettled([
        commands.hasAnyModelsAvailable(),
        permissions.check(),
      ]);
      const hasModels =
        modelResult.status === "fulfilled" &&
        modelResult.value.status === "ok" &&
        modelResult.value.data;
      setIsReturningUser(hasModels);

      if (permissionResult.status === "rejected") {
        console.error(
          "Failed to check system permissions:",
          permissionResult.reason,
        );
        await revealMainWindowForPermissions();
        setOnboardingStep("accessibility");
        return;
      }

      if (hasModels) {
        if (!permissionResult.value.allGranted) {
          await revealMainWindowForPermissions();
          setOnboardingStep("accessibility");
          return;
        }

        setOnboardingStep("done");
      } else {
        // New user - start full onboarding
        setOnboardingStep("accessibility");
      }
    } catch (error) {
      console.error("Failed to check onboarding status:", error);
      setOnboardingStep("accessibility");
    }
  };

  const handleModelSelected = () => {
    // Transition to main app - user has started a download
    setOnboardingStep("done");
  };

  // Still checking onboarding status
  if (onboardingStep === null) {
    return null;
  }

  if (onboardingStep === "accessibility") {
    return (
      <AccessibilityOnboarding
        permissions={permissions}
        onRequestAccessibility={permissions.requestAccessibility}
        onRequestMicrophone={permissions.requestMicrophone}
      />
    );
  }

  if (onboardingStep === "model") {
    return <Onboarding onModelSelected={handleModelSelected} />;
  }

  const activeSection = SECTIONS_CONFIG[currentSection];
  const ActiveSectionIcon = activeSection.icon;

  return (
    <div
      dir={direction}
      className="app-shell h-screen flex flex-col select-none cursor-default relative isolate"
    >
      <SwampBackground />
      <Toaster
        theme="system"
        toastOptions={{
          unstyled: true,
          classNames: {
            toast:
              "bg-background border border-mid-gray/20 rounded-lg shadow-lg px-4 py-3 flex items-center gap-3 text-sm",
            title: "font-medium",
            description: "text-mid-gray",
          },
        }}
      />
      {/* Main content area that takes remaining space */}
      <div className="flex-1 flex overflow-hidden min-h-0">
        <Sidebar
          activeSection={currentSection}
          onSectionChange={setCurrentSection}
        />
        {/* Scrollable content area */}
        <main className="app-main flex-1 flex flex-col overflow-hidden min-w-0">
          <header className="content-toolbar">
            <div className="content-toolbar-icon" aria-hidden="true">
              <ActiveSectionIcon width={19} height={19} />
            </div>
            <div className="min-w-0">
              <span className="content-toolbar-kicker">{PRODUCT_NAME}</span>
              <h1 className="content-toolbar-title">
                {t(activeSection.labelKey)}
              </h1>
            </div>
            <div className="content-toolbar-pond" aria-hidden="true">
              <span className="content-toolbar-ripple" />
              <span className="content-toolbar-ripple content-toolbar-ripple-delay" />
            </div>
          </header>
          <div className="app-scroll flex-1 overflow-y-auto">
            <div
              key={currentSection}
              className="app-content flex flex-col items-center gap-4"
            >
              <AccessibilityPermissions
                permissions={permissions}
                onRequestAccessibility={permissions.requestAccessibility}
                onRequestMicrophone={permissions.requestMicrophone}
              />
              {renderSettingsContent(currentSection, setCurrentSection)}
            </div>
          </div>
        </main>
      </div>
      {/* Fixed footer at bottom */}
      <Footer />
    </div>
  );
}

export default App;
