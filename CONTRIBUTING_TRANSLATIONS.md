# Contributing translations to AudioBud

Use this guide to improve an existing translation or add a language. English
is the source in `src/i18n/locales/en/translation.json`.

## Find a translation to improve

Check `src/i18n/locales/` for the languages that already have translation files.
Check [languages.ts](src/i18n/languages.ts) for their names and display order.
Korean (`ko`) and Portuguese (`pt`) already have translations.

Run the current check from the repository root:

```bash
bun run check:translations
```

The check reports missing keys, extra keys, and values that still match English
after case, whitespace, and Unicode normalization. It checks all non-English
locales and exits with a nonzero status if any locale has issues. Use its output
to find work; a language in the app is not a claim that its translation is
complete. Passing this check does not prove that the wording is correct.

The output shows the first ten paths in each category for each language, plus
the number of remaining paths. The [checker](scripts/check-translations.ts)
defines the checks.

## Improve an existing translation

1. Fork and clone the repository. Follow [BUILD.md](BUILD.md) for the development
   tools and setup.
2. Edit `src/i18n/locales/<language-code>/translation.json`. Change the values
   and preserve the keys and JSON structure.
3. Add missing keys from the English source and translate their values. Do not
   replace the whole locale file with English; that would remove existing work.
4. Run `bun run check:translations` again. Fix issues in your changed locale.
   If other locales still fail, report those existing failures in your PR.
5. Check the language in the app as described below, then submit a PR.

## Add a language

First check that its folder does not already exist. Use the locale code format
in the repository, such as `de` or `zh-TW`.

For a new language only, create its folder and copy the English source:

```bash
mkdir src/i18n/locales/<language-code>
cp src/i18n/locales/en/translation.json src/i18n/locales/<language-code>/translation.json
```

Replace `<language-code>` before you run these commands. Translate the values
in the new file, then add one entry to `LANGUAGE_METADATA` in
[languages.ts](src/i18n/languages.ts). Keep the existing entries and type
definition. For example, Arabic's entry is:

```typescript
ar: { name: "Arabic", nativeName: "العربية", priority: 17, direction: "rtl" },
```

Use the English name and native name for your language. `priority` is optional;
lower numbers appear first, and languages without a priority follow in
alphabetical order by English name. Set `direction: "rtl"` for a language that
reads from right to left.

[index.ts](src/i18n/index.ts) discovers translation files automatically. You do
not need to add a manual import there. Run `bun run check:translations` before
you test the language in the app.

## Translation rules

- Use natural wording and keep it short enough for the interface.
- Follow the meaning of the English source.
- Preserve brand names: AudioBud, whisper.cpp, Parakeet, and OpenAI.
- Preserve technical terms such as API and GPU when suitable for the language.
- Keep keys and interpolation variables exactly as written.
- Keep valid JSON. Do not add comments to translation files.

### Variables

Translate the surrounding text and keep variables such as `{{error}}`,
`{{model}}`, and `{{count}}` unchanged. For example:

```json
{
  "downloadModel": "Échec du téléchargement du modèle : {{error}}"
}
```

Changing `{{error}}` to `{{erreur}}` would prevent the value from being inserted.

### Plurals

Preserve the plural keys in the English source, such as keys ending in `_one`
and `_other`. Add the categories your language needs, such as `_few` and
`_many`, to the same key group. Keep `{{count}}` where the message uses it.
The checker accepts these extra plural categories only when the group exists
in English. Check messages with several counts in the app; one general form
does not cover every language.

### Values that match English

A brand name or technical term can be correct in both languages. Existing
exceptions are in
[translation-identical-allowlist.ts](scripts/translation-identical-allowlist.ts).
If the checker flags an intentional match, explain it in your PR. A new
exception needs review of the exact key path, English value, and locale scope.
Do not add an exception to hide untranslated text.

## Check the language in the app

Follow [BUILD.md](BUILD.md) to run the native app on a maintained platform.
Linux can run frontend checks; Linux native application support is retired.
See [platform support](docs/platform-support.md).

1. Run `bun run tauri dev` on Windows or Apple Silicon macOS with the required
   development tools and model assets.
2. Open Settings, then About, then Application language.
3. Select the language and check the changed text, variables, plural counts,
   and layout. Check right-to-left layout when it applies.

## Submit a PR

Use the [PR template](.github/PULL_REQUEST_TEMPLATE.md). State the language,
what you changed, the translation check result, and any existing failures in
other locales. Include the platform and app checks you completed, or explain
which checks you could not do. See [CONTRIBUTING.md](CONTRIBUTING.md) for the
contributor workflow.

For questions, open an issue or comment on an existing translation PR.
