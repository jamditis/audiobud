import React from "react";
import { useTranslation } from "react-i18next";
import { Cog, FlaskConical, History, Info, Sparkles, Cpu } from "lucide-react";
import HandyTextLogo from "./icons/HandyTextLogo";
import HandyHand from "./icons/HandyHand";
import { useSettings } from "../hooks/useSettings";
import { GeneralSettings } from "./settings/general/GeneralSettings";

const ModelsSettings = React.lazy(async () => {
  const module = await import("./settings/models/ModelsSettings");
  return { default: module.ModelsSettings };
});

const AdvancedSettings = React.lazy(async () => {
  const module = await import("./settings/advanced/AdvancedSettings");
  return { default: module.AdvancedSettings };
});

const HistorySettings = React.lazy(async () => {
  const module = await import("./settings/history/HistorySettings");
  return { default: module.HistorySettings };
});

const PostProcessingSettings = React.lazy(async () => {
  const module = await import(
    "./settings/post-processing/PostProcessingSettings"
  );
  return { default: module.PostProcessingSettings };
});

const DebugSettings = React.lazy(async () => {
  const module = await import("./settings/debug/DebugSettings");
  return { default: module.DebugSettings };
});

const AboutSettings = React.lazy(async () => {
  const module = await import("./settings/about/AboutSettings");
  return { default: module.AboutSettings };
});

export type SidebarSection =
  | "general"
  | "models"
  | "advanced"
  | "history"
  | "postprocessing"
  | "debug"
  | "about";

export interface SettingsSectionProps {
  onNavigate?: (section: SidebarSection) => void;
}

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  component:
    | React.ComponentType<SettingsSectionProps>
    | React.LazyExoticComponent<React.ComponentType<SettingsSectionProps>>;
  enabled: (settings: any) => boolean;
}

export const SECTIONS_CONFIG = {
  general: {
    labelKey: "sidebar.general",
    icon: HandyHand,
    component: GeneralSettings,
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    enabled: () => true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: (settings) => settings?.post_process_enabled ?? false,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
  },
} as const satisfies Record<SidebarSection, SectionConfig>;

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  return (
    <aside className="sidebar-shell flex flex-col w-48 h-full items-center px-2">
      <div className="sidebar-brand w-full flex items-center py-4 pl-1 pr-5">
        <HandyTextLogo width={138} />
      </div>
      <nav
        aria-label={t("sidebar.navLabel")}
        className="sidebar-nav flex flex-col w-full items-center gap-1 pt-2"
      >
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;

          return (
            <button
              key={section.id}
              type="button"
              aria-current={isActive ? "page" : undefined}
              className={`sidebar-link flex gap-2 items-center p-2 w-full text-start rounded-xl cursor-pointer ${
                isActive
                  ? "is-active bg-logo-primary/90 text-[#0e1c0c] font-semibold nav-pad"
                  : "hover:bg-logo-primary/10 hover:opacity-100 opacity-85"
              }`}
              onClick={() => onSectionChange(section.id)}
            >
              <Icon
                width={24}
                height={24}
                className="shrink-0"
                aria-hidden="true"
              />
              <p
                className="text-sm font-medium truncate"
                title={t(section.labelKey)}
              >
                {t(section.labelKey)}
              </p>
            </button>
          );
        })}
      </nav>
      <div className="sidebar-pond" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
    </aside>
  );
};
