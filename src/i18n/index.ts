import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { locale } from "@tauri-apps/plugin-os";
import { LANGUAGE_METADATA } from "./languages";
import englishTranslation from "./locales/en/translation.json";
import { commands } from "@/bindings";
import {
  getLanguageDirection,
  updateDocumentDirection,
  updateDocumentLanguage,
} from "@/lib/utils/rtl";

type TranslationModule = {
  default: Record<string, unknown>;
};

// Keep English in the initial bundle. Load each other language when it is used.
const localeModules = import.meta.glob<TranslationModule>([
  "./locales/*/translation.json",
  "!./locales/en/translation.json",
]);

const discoveredLanguageCodes = new Set<string>(["en"]);
for (const path of Object.keys(localeModules)) {
  const langCode = path.match(/\.\/locales\/(.+)\/translation\.json/)?.[1];
  if (langCode) discoveredLanguageCodes.add(langCode);
}

// Build the language list from the file names and the small metadata table.
export const SUPPORTED_LANGUAGES = [...discoveredLanguageCodes]
  .map((code) => {
    const meta = LANGUAGE_METADATA[code];
    if (!meta) {
      console.warn(`Missing metadata for locale "${code}" in languages.ts`);
      return { code, name: code, nativeName: code, priority: undefined };
    }
    return {
      code,
      name: meta.name,
      nativeName: meta.nativeName,
      priority: meta.priority,
    };
  })
  .sort((a, b) => {
    if (a.priority !== undefined && b.priority !== undefined) {
      return a.priority - b.priority;
    }
    if (a.priority !== undefined) return -1;
    if (b.priority !== undefined) return 1;
    return a.name.localeCompare(b.name);
  });

export type SupportedLanguageCode = string;

const getSupportedLanguage = (
  langCode: string | null | undefined,
): SupportedLanguageCode | null => {
  if (!langCode) return null;
  const normalized = langCode.toLowerCase();

  let supported = SUPPORTED_LANGUAGES.find(
    (lang) => lang.code.toLowerCase() === normalized,
  );
  if (!supported) {
    const prefix = normalized.split("-")[0];
    supported = SUPPORTED_LANGUAGES.find(
      (lang) => lang.code.toLowerCase() === prefix,
    );
  }
  return supported ? supported.code : null;
};

const i18nReady = i18n.use(initReactI18next).init({
  resources: {
    en: { translation: englishTranslation },
  },
  lng: "en",
  fallbackLng: "en",
  interpolation: {
    escapeValue: false,
  },
  react: {
    useSuspense: false,
  },
});

const languageLoads = new Map<string, Promise<void>>();

const loadLanguage = async (langCode: SupportedLanguageCode): Promise<void> => {
  await i18nReady;

  if (i18n.hasResourceBundle(langCode, "translation")) return;

  const activeLoad = languageLoads.get(langCode);
  if (activeLoad) return activeLoad;

  const path = `./locales/${langCode}/translation.json`;
  const loader = localeModules[path];
  if (!loader) {
    throw new Error(`Translation file is not available for ${langCode}`);
  }

  const load = loader()
    .then((module) => {
      i18n.addResourceBundle(
        langCode,
        "translation",
        module.default,
        true,
        true,
      );
    })
    .finally(() => {
      languageLoads.delete(langCode);
    });

  languageLoads.set(langCode, load);
  return load;
};

export const changeAppLanguage = async (
  langCode: string | null | undefined,
): Promise<boolean> => {
  const supported = getSupportedLanguage(langCode);
  if (!supported) return false;

  await loadLanguage(supported);
  if (i18n.language !== supported) {
    await i18n.changeLanguage(supported);
  }
  return true;
};

// Read only the language setting. Small webviews do not need the full settings
// object or the post-processing keys that it can contain.
export const syncLanguageFromSettings = async (): Promise<void> => {
  try {
    const savedLanguage = await commands.getAppLanguage();
    const selectedLanguage = savedLanguage || (await locale());
    await changeAppLanguage(selectedLanguage);
  } catch (error) {
    console.warn("Failed to sync language from settings:", error);
  }
};

void syncLanguageFromSettings();

i18n.on("languageChanged", (lng) => {
  const dir = getLanguageDirection(lng);
  updateDocumentDirection(dir);
  updateDocumentLanguage(lng);
});

export { getLanguageDirection, isRTLLanguage } from "@/lib/utils/rtl";

export default i18n;
