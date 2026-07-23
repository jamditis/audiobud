import React from "react";

interface SettingsGroupProps {
  title?: string;
  description?: string;
  children: React.ReactNode;
}

export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  description,
  children,
}) => {
  return (
    <section className="settings-group space-y-2">
      {title && (
        <div className="settings-group-heading px-4">
          <h2 className="text-xs font-semibold text-mid-gray uppercase tracking-[0.12em]">
            {title}
          </h2>
          {description && (
            <p className="text-xs text-mid-gray mt-1">{description}</p>
          )}
        </div>
      )}
      <div className="swamp-card rounded-2xl overflow-visible">
        <div className="divide-y divide-mid-gray/20">{children}</div>
      </div>
    </section>
  );
};
