// One-shot window picker surface (#124, wired in #259).
//
// A thin renderer over two pure cores: the backend decides what a gesture means
// (src-tauri/src/window_picker.rs) and src/lib/window-picker-overlay.ts owns the
// interaction (which row is highlighted, which keystroke ends the pick). This
// component only draws the rows, forwards input into that reducer, and hands the
// resulting gesture back to the backend, which arms the pick and closes this
// window.

import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands } from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import {
  chooseAt,
  createPickerState,
  foregroundGesture,
  handleKey,
  handleKeyWhileLoading,
  pickWasRefused,
  targetOwnsKey,
  type PickerGesture,
  type PickerState,
} from "@/lib/window-picker-overlay";
import { getLanguageDirection } from "@/lib/utils/rtl";
import "./WindowPicker.css";

// The DOM id of the row at `index`, for aria-activedescendant to point at.
const rowId = (index: number) => `picker-row-${index}`;

const WindowPicker: React.FC = () => {
  const { t } = useTranslation();
  const [state, setState] = useState<PickerState>(() => createPickerState([]));
  const [loading, setLoading] = useState(true);
  // Set when a row the user clicked turned out to be gone. The picker stays
  // open so they can pick again, rather than closing as if it had worked.
  const [refused, setRefused] = useState(false);
  const direction = getLanguageDirection(i18n.language);
  // One pick per opening: a click that lands while a gesture is in flight must
  // not send a second one.
  const sending = useRef(false);
  const listRef = useRef<HTMLUListElement>(null);

  const load = useCallback(async () => {
    const windows = await commands.listPickerWindows();
    setState(createPickerState(windows));
    setLoading(false);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const start = async () => {
      await syncLanguageFromSettings();
      if (cancelled) return;
      // The window is built before the translations are loaded, so its native
      // title -- what a screen reader and the window list announce -- is set
      // here, in the language the rest of the app is using.
      await getCurrentWindow()
        .setTitle(t("windowPicker.title"))
        .catch(() => {});
      await load();
    };
    void start();
    return () => {
      cancelled = true;
    };
    // Runs once: the picker lives for a single pick, and `t` and `load` are
    // stable for its whole life.
  }, [load, t]);

  const send = useCallback(
    async (gesture: PickerGesture) => {
      if (sending.current) return;
      sending.current = true;
      const armed = await commands.resolveWindowPick(gesture);

      // A row that could not be honored: its window closed, or its handle was
      // recycled, since the list was drawn. The backend leaves the picker open
      // for exactly this, so say what happened and offer a fresh list.
      if (pickWasRefused(gesture, armed)) {
        setRefused(true);
        setLoading(true);
        await load();
        sending.current = false;
        return;
      }
      // Otherwise the backend closes this window, so there is nothing to undo.
    },
    [load],
  );

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // A keydown on the footer buttons reaches this window-level listener too.
      // Enter there means "press this button", not "take the highlighted row",
      // so the control keeps that key (Escape always backs out, from anywhere).
      if (targetOwnsKey(event.target as HTMLElement | null, event.key)) return;

      // Before the rows arrive there is nothing to choose, and an empty list
      // reads as a dismissal -- so an eager Enter would end the pick and clear
      // whatever route was already armed. Only Escape acts while loading.
      if (loading) {
        const early = handleKeyWhileLoading(event.key);
        if (early?.gesture) {
          event.preventDefault();
          void send(early.gesture);
        }
        return;
      }

      const step = handleKey(state, event.key);
      if (step.gesture) {
        event.preventDefault();
        void send(step.gesture);
        return;
      }
      if (step.next !== state) {
        event.preventDefault();
        setState(step.next);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [state, send, loading]);

  // Keep the highlighted row in view when the list is longer than the surface.
  useEffect(() => {
    const row = listRef.current?.children[state.highlighted];
    if (row instanceof HTMLElement) {
      row.scrollIntoView({ block: "nearest" });
    }
  }, [state.highlighted]);

  // Focus the list itself once it has rows. A listbox that never holds focus is
  // never announced, so a screen-reader user hears nothing as the highlight
  // moves; with focus here, aria-activedescendant below names the active row on
  // every move. Keys are handled on the window either way, so this changes what
  // is announced, not what works.
  useEffect(() => {
    if (!loading && state.windows.length > 0) {
      listRef.current?.focus();
    }
  }, [loading, state.windows.length]);

  const onRowClick = (index: number) => {
    const step = chooseAt(state, index);
    if (step.gesture) {
      void send(step.gesture);
    }
  };

  return (
    <div className="window-picker" dir={direction}>
      <header className="picker-header">
        <h1 className="picker-title">{t("windowPicker.title")}</h1>
        <p className="picker-hint">
          {refused ? t("windowPicker.refused") : t("windowPicker.hint")}
        </p>
      </header>

      {loading ? (
        <p className="picker-empty">{t("windowPicker.loading")}</p>
      ) : state.windows.length === 0 ? (
        <p className="picker-empty">{t("windowPicker.empty")}</p>
      ) : (
        <ul
          className="picker-list"
          role="listbox"
          tabIndex={0}
          aria-label={t("windowPicker.title")}
          aria-activedescendant={
            state.highlighted >= 0 ? rowId(state.highlighted) : undefined
          }
          ref={listRef}
        >
          {state.windows.map((win, index) => (
            <li
              key={win.handle}
              id={rowId(index)}
              role="option"
              aria-selected={index === state.highlighted}
              className={`picker-row ${index === state.highlighted ? "highlighted" : ""}`}
              onClick={() => onRowClick(index)}
            >
              <span className="picker-row-index">{index + 1}</span>
              <span className="picker-row-label">{win.label}</span>
            </li>
          ))}
        </ul>
      )}

      <footer className="picker-actions">
        <button
          type="button"
          className="picker-button"
          onClick={() => void send(foregroundGesture())}
        >
          {t("windowPicker.useForeground")}
        </button>
        <button
          type="button"
          className="picker-button"
          onClick={() => void send({ kind: "dismiss" })}
        >
          {t("windowPicker.cancel")}
        </button>
      </footer>
    </div>
  );
};

export default WindowPicker;
