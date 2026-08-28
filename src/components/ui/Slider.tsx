import React, { useEffect, useRef, useState } from "react";
import { SettingContainer } from "./SettingContainer";
import { createSliderInteraction } from "./sliderDraft";

interface SliderProps {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  disabled?: boolean;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  showValue?: boolean;
  formatValue?: (value: number) => string;
}

export const Slider: React.FC<SliderProps> = ({
  value,
  onChange,
  min,
  max,
  step = 0.01,
  disabled = false,
  label,
  description,
  descriptionMode = "tooltip",
  grouped = false,
  showValue = true,
  formatValue = (v) => v.toFixed(2),
}) => {
  const onChangeRef = useRef(onChange);
  const [draftValue, setDraftValue] = useState(value);
  const [interaction] = useState(() =>
    createSliderInteraction(value, setDraftValue, (nextValue) =>
      onChangeRef.current(nextValue),
    ),
  );

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    interaction.sync(value);
  }, [interaction, value]);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const nextValue = parseFloat(e.target.value);
    interaction.update(nextValue);
  };

  const handleCommit = () => {
    interaction.finish();
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLInputElement>) => {
    interaction.begin();
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const handlePointerCancel = () => {
    interaction.cancel();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    interaction.keyDown(e.key);
  };

  const handleKeyUp = (e: React.KeyboardEvent<HTMLInputElement>) => {
    interaction.keyUp(e.key);
  };

  return (
    <SettingContainer
      title={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="horizontal"
      disabled={disabled}
    >
      <div className="w-full">
        <div className="flex items-center space-x-1 h-6">
          <input
            type="range"
            min={min}
            max={max}
            step={step}
            value={draftValue}
            onChange={handleChange}
            onPointerDown={handlePointerDown}
            onPointerUp={handleCommit}
            onPointerCancel={handlePointerCancel}
            onKeyDown={handleKeyDown}
            onKeyUp={handleKeyUp}
            onBlur={handleCommit}
            disabled={disabled}
            className="flex-grow h-2 rounded-lg appearance-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-logo-primary disabled:opacity-50 disabled:cursor-not-allowed"
            style={{
              background: `linear-gradient(to right, var(--color-background-ui) ${
                ((draftValue - min) / (max - min)) * 100
              }%, rgba(128, 128, 128, 0.2) ${
                ((draftValue - min) / (max - min)) * 100
              }%)`,
            }}
          />
          {showValue && (
            <span className="text-sm font-medium text-text/90 w-12 text-end">
              {formatValue(draftValue)}
            </span>
          )}
        </div>
      </div>
    </SettingContainer>
  );
};
