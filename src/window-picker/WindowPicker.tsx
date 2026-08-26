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
import { commands } from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import {
  chooseAt,
  createPickerState,
  foregroundGesture,
  handleKey,
  targetOwnsKey,
  type PickerGesture,
  type PickerState,
} from "@/lib/window-picker-overlay";
import { getLanguageDirection } from "@/lib/utils/rtl";
import "./WindowPicker.css";

const WindowPicker: React.FC = () => {
  const { t } = useTranslation();
  const [state, setState] = useState<PickerState>(() => createPickerState([]));
  const [loading, setLoading] = useState(true);
  const direction = getLanguageDirection(i18n.language);
  // One pick per opening: a click that lands while a gesture is in flight must
  // not send a second one.
  const sending = useRef(false);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      await syncLanguageFromSettings();
      const windows = await commands.listPickerWindows();
      if (cancelled) return;
      setState(createPickerState(windows));
      setLoading(false);
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const send = useCallback(async (gesture: PickerGesture) => {
    if (sending.current) return;
    sending.current = true;
    // The backend closes this window once the pick is resolved, so there is
    // nothing to restore here on success.
    await commands.resolveWindowPick(gesture);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // A keydown on the footer buttons reaches this window-level listener too.
      // Enter there means "press this button", not "take the highlighted row",
      // so the control keeps the key (Escape always backs out, from anywhere).
      if (targetOwnsKey(event.target as HTMLElement | null, event.key)) return;
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
  }, [state, send]);

  // Keep the highlighted row in view when the list is longer than the surface.
  useEffect(() => {
    const row = listRef.current?.children[state.highlighted];
    if (row instanceof HTMLElement) {
      row.scrollIntoView({ block: "nearest" });
    }
  }, [state.highlighted]);

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
        <p className="picker-hint">{t("windowPicker.hint")}</p>
      </header>

      {loading ? (
        <p className="picker-empty">{t("windowPicker.loading")}</p>
      ) : state.windows.length === 0 ? (
        <p className="picker-empty">{t("windowPicker.empty")}</p>
      ) : (
        <ul
          className="picker-list"
          role="listbox"
          aria-label={t("windowPicker.title")}
          ref={listRef}
        >
          {state.windows.map((win, index) => (
            <li
              key={win.handle}
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
