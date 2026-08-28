export interface SliderDraft {
  value: () => number;
  update: (value: number) => void;
  commit: () => boolean;
  sync: (value: number) => void;
  cancel: () => number;
}

export function createSliderDraft(
  initialValue: number,
  onCommit: (value: number) => void,
): SliderDraft {
  let draftValue = initialValue;
  let committedValue = initialValue;

  return {
    value: () => draftValue,
    update(value) {
      draftValue = value;
    },
    commit() {
      if (Object.is(draftValue, committedValue)) {
        return false;
      }
      committedValue = draftValue;
      onCommit(draftValue);
      return true;
    },
    sync(value) {
      draftValue = value;
      committedValue = value;
    },
    cancel() {
      draftValue = committedValue;
      return draftValue;
    },
  };
}

const SLIDER_KEYS = new Set([
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "End",
  "Home",
  "PageDown",
  "PageUp",
]);

export interface SliderInteraction {
  value: () => number;
  update: (value: number) => void;
  sync: (value: number) => void;
  begin: () => void;
  finish: () => boolean;
  cancel: () => void;
  keyDown: (key: string) => void;
  keyUp: (key: string) => boolean;
}

export function createSliderInteraction(
  initialValue: number,
  onDraftChange: (value: number) => void,
  onCommit: (value: number) => void,
): SliderInteraction {
  const draft = createSliderDraft(initialValue, onCommit);
  let interactionActive = false;
  let latestValue = initialValue;

  const finish = () => {
    const wasActive = interactionActive;
    interactionActive = false;
    const didCommit = draft.commit();
    if (wasActive && !didCommit) {
      draft.sync(latestValue);
      onDraftChange(latestValue);
    }
    return didCommit;
  };

  return {
    value: draft.value,
    update(value) {
      draft.update(value);
      onDraftChange(value);
    },
    sync(value) {
      latestValue = value;
      if (!interactionActive) {
        draft.sync(value);
        onDraftChange(value);
      }
    },
    begin() {
      interactionActive = true;
    },
    finish,
    cancel() {
      interactionActive = false;
      draft.cancel();
      draft.sync(latestValue);
      onDraftChange(latestValue);
    },
    keyDown(key) {
      if (SLIDER_KEYS.has(key)) {
        interactionActive = true;
      }
    },
    keyUp(key) {
      return SLIDER_KEYS.has(key) ? finish() : false;
    },
  };
}
