# AudioBud privacy and terms design

**Status:** Approved by Joe Amditis on July 20, 2026.

## Goal

Publish accurate privacy and terms pages for AudioBud, link them from the public site and README, and provide stable URLs for the Microsoft Entra application profile.

## Scope

The change will:

- add `docs/privacy.html` and `docs/terms.html`;
- give all four public pages one semantic footer whose `.footer-links` navigation lists Roadmap, Privacy, Terms, Changelog, and GitHub in that order;
- add the exact public website, privacy, terms, and support URLs to `README.md` and remove the old GitHub Pages project URL;
- use each page's exact `https://audiobud.amditis.tech` canonical and Open Graph URL, plus the shared custom-domain Open Graph and Twitter image URL;
- give every page exactly one Open Graph image alt and one Twitter image alt in the first real head, using `AudioBud local dictation for Windows app interface`;
- use the same inline geometric frog SVG data-URI browser favicon on every page while retaining `./favicon.svg` as the visible brand image;
- extend `docs/styles.css` with a narrow legal-document layout, summary cards, a contents list, visible focus states, and mobile rules; and
- preserve the existing frog-and-swamp visual system.

The change will not alter application behavior, data storage, the updater, Azure resources, Artifact Signing settings, or the release workflow. The existing `docs/CNAME` file was created separately on `main` in commit `061ca71` and remains the custom-domain source of truth.

## Policy owner and contact

The pages will identify Joe Amditis as the AudioBud project maintainer and use `jamditis@gmail.com` for privacy and support requests. They will warn users not to put private information in public GitHub issues.

The terms will omit governing-law, arbitration, class-action waiver, and indemnity language. Those clauses would create commitments that have not received legal review. The pull request will not claim that an attorney reviewed the pages.

## Data inventory

The policy text must follow the shipped code:

- Audio capture, transcription, history, saved recordings, settings, custom words, replacements, and optional personalization are processed or stored on the user's device.
- Users can export learned personalization as JSON or reset it on their device.
- The project operator does not receive local audio, transcripts, settings, or history through a first-party AudioBud service.
- The Ctrl+V, Ctrl+Shift+V, and Shift+Insert clipboard paste modes temporarily write the transcript to the system clipboard, paste it into the focused application, and then try to restore the previous clipboard contents.
- Direct types the transcript into the focused application without using the clipboard paste path. External Script sends the transcript as one command-line argument to the configured script. None skips transcript delivery through paste or script.
- The user-facing `Copy to clipboard` setting runs after every paste method, including Direct, External Script, and None, and leaves the transcript in the system clipboard. These actions do not send text off-device by themselves. Receiving applications and scripts, along with applications that read copied text, control any later transmission and retention.
- AudioBud has no first-party account system, hosted application backend, ads, analytics, or telemetry.
- Transcription history is stored in a local SQLite database. Recordings are stored as local WAV files. The default history limit is five unsaved entries, and users can change retention or save entries for longer storage.
- Post-processing is off by default. When a user enables it, AudioBud sends transcript text and the selected prompt, but not audio, to the provider or custom endpoint selected by the user.
- Provider API keys are stored in the local settings store. The policy will not claim that local files or keys are encrypted.
- Model downloads contact their listed hosts, including `blob.handy.computer`, and expose normal request metadata such as an IP address to those hosts.
- AudioBud's automatic updater is disabled in the current release. User-opened GitHub release links and any future enabled update check can expose normal request metadata to GitHub.
- The public site is hosted by GitHub Pages. AudioBud adds no analytics, forms, advertising scripts, or first-party cookies, while GitHub may process request metadata under its own privacy statement.

## Privacy page

The privacy page will use plain language and include:

1. An effective date and local-first summary.
2. The operator, contact address, and policy scope.
3. Data processed and stored on the device.
4. The limited cases in which data leaves the device, plus how clipboard, direct, and external-script paste methods deliver transcript text locally and leave later handling to the receiving application or script.
5. Optional LLM post-processing and the user's responsibility for provider terms, privacy practices, and charges.
6. Model downloads, GitHub links, and normal network metadata.
7. GitHub Pages hosting, cookies, and the absence of AudioBud analytics.
8. Retention and deletion controls, including the choice to export or reset learned personalization locally.
9. No sale, behavioral advertising, or cross-site tracking by AudioBud, including how that affects Do Not Track and Global Privacy Control signals.
10. Security limits without an absolute security promise.
11. Privacy rights where applicable, with requests directed to the contact address and third-party requests directed to the relevant provider.
12. A statement that the project is not directed to children under 13.
13. How policy changes will be dated and published.

## Terms page

The standard, Open Graph, and Twitter description will be:
`Terms for AudioBud's official project website, release pages, support channels, and other maintainer-operated surfaces.` This metadata describes the website and
maintainer-operated surfaces, not permissions to download, use, modify, or
distribute AudioBud software.

The Open Graph and Twitter image alt metadata will use
`AudioBud local dictation for Windows app interface`. Each tag must appear once
in the first real document head. This text describes the shared image itself,
not the terms page's legal scope.

The terms page will include:

1. An effective date, operator, and scope statement. Acceptance applies to the official project website, release and support pages, and other maintainer-operated surfaces, not to downloading or using the software.
2. A clear distinction between these website terms and the MIT License. The MIT License governs downloading, using, copying, modifying, and distributing AudioBud software, and nothing in the terms limits MIT permissions.
3. Factual legal-compliance and user-responsibility statements that do not create software-license restrictions. No-interference and submitted-content rules apply only to project infrastructure, the issue tracker, release pages, and support channels, and do not narrow MIT permissions.
4. A statement that users retain their content and AudioBud receives no ownership grant for local content.
5. Terms for optional third-party providers, user-supplied credentials, provider charges, model hosts, and GitHub-hosted downloads.
6. A warning that transcription and optional AI output can be wrong and must be reviewed before medical, legal, financial, or safety-related use.
7. Availability, updates, support, and policy-change language that makes no uptime or support promise.
8. Warranty and liability language that defers AudioBud software terms to the MIT License. Separate permitted-law disclaimers and limits apply only to the official project website and other maintainer-operated surfaces, while preserving non-waivable rights and non-excludable liability.
9. A contact section and a link to the privacy page.

## Page design

Both pages will reuse the existing colors, fixed header, frog mark, background layers, cards, buttons, footer, and reduced-motion behavior. Legal content will use a readable column of about 760 pixels rather than the marketing-page width.

Each page will open with a short summary and a compact contents navigation with a semantic heading and list of links. A non-section wrapper will hold the contents navigation and legal document when that wrapper has no heading. The privacy page will include three fact cards that distinguish data that stays on the device, data sent only after a user action, and GitHub's role as site host. These cards summarize the policy but do not replace the detailed sections.

The legal hero will clear the fixed header and separate itself from the document with the existing line color. Privacy's three fact cards will form a compact grid on wide screens. The main legal layout will center a 210-pixel sticky contents column beside a document column capped at 760 pixels. The contents heading will be an `h2`, its links will be one unmarked list, and hover and keyboard focus will remain readable. Section separators, muted body copy, green underlined links, and amber callouts will use the existing swamp palette.

Global keyboard focus will use a three-pixel amber outline with visible offset. Skip links will stay fixed off-canvas until focused, then appear above the header with an amber background and dark text. The home, roadmap, privacy, and terms heading targets will each clear the 64-pixel fixed header when reached. Current-page header and footer links will use the existing amber color and stronger weight without extra decoration.

At 860 pixels and below, the privacy fact grid and legal layout will collapse to one column, the contents navigation will stop being sticky, and the legal hero will use reduced padding. Footer links will wrap so all five destinations remain usable without horizontal overflow. The roadmap `h1` and existing section `h2` will share the same approved heading dimensions.

Every public page will use its skip link as the first real element child of `body`, before the decorative swamp. Home uses `Skip to main content` with `#hero-title`, roadmap uses `Skip to roadmap` with `#roadmap-title`, privacy uses `Skip to privacy policy` with `#privacy-title`, and terms uses `Skip to terms of use` with `#terms-title`. Each target ID must occur exactly once as a real `id` attribute; comments and quoted lookalikes do not count.

The roadmap's page-level heading will be its only `h1` and will use `id="roadmap-title"`. The privacy and terms footer links will retain `aria-current="page"` on the current policy destination, and the roadmap footer will retain it on Roadmap.

Headings will use sentence case. Links and keyboard focus will remain visible. Mobile layouts will collapse to one column without hiding access to the policy pages.

## Public URLs and deployment

The stable URLs will be:

- Home: `https://audiobud.amditis.tech/`
- Roadmap: `https://audiobud.amditis.tech/roadmap.html`
- Privacy: `https://audiobud.amditis.tech/privacy.html`
- Terms: `https://audiobud.amditis.tech/terms.html`
- Support: `https://github.com/jamditis/audiobud/issues`

GitHub Pages is configured to serve `docs/` from `main`. The custom hostname is stored in `docs/CNAME`, and public DNS uses a DNS-only CNAME from `audiobud.amditis.tech` to `jamditis.github.io`. GitHub's HTTPS setting must only be enabled after its certificate is ready.

## Verification

Before opening the pull request:

- format the changed HTML, CSS, Markdown, and CNAME files with the repository's formatter;
- run the frontend build and tests;
- serve `docs/` locally and verify the home, roadmap, privacy, and terms pages at desktop and mobile widths;
- check headings, landmarks, `aria-current`, visible focus, reduced motion, and keyboard navigation;
- verify every local link and asset target;
- verify all four pages use their exact custom-domain canonical and Open Graph URLs, shared custom-domain social image URL and alt text, and inline browser favicon;
- verify each page has exactly one real semantic footer with one `.footer-links` navigation containing Roadmap, Privacy, Terms, Changelog, and GitHub in order;
- verify each skip link is the first real element child of `body`, each target ID occurs once as a real attribute, and roadmap has one page-level `h1` with `id="roadmap-title"`;
- verify global focus, skip-link stacking and reveal behavior, all four fixed-header-clearing heading offsets, and visible current-page navigation treatment;
- verify both policy contents navigations use one semantic `h2` and one `ul`, and privacy's unlabelled layout wrapper is a `div` rather than a `section`;
- verify the legal hero, summary cards, sticky contents, document sections, links, callouts, shared roadmap heading treatment, wrapping footer links, and 860-pixel one-column rules match the approved layout;
- verify README contains the exact Website, Privacy, Terms, and Support URLs and no old GitHub Pages project URL;
- confirm the old GitHub Pages URL redirects to the matching custom-domain path;
- confirm the custom-domain CNAME, GitHub Pages build status, certificate, and HTTPS redirect; and
- preserve unrelated untracked files when staging and committing.

## Research notes

- The codebase audit found no first-party backend, analytics, ads, or telemetry. Local history, recordings, settings, personalization, and provider keys are stored on the device. Optional post-processing sends transcript text and a prompt to the provider selected by the user.
- The FTC advises app developers to make clear disclosures, limit collection, protect retained data, and keep promises aligned with actual practice: <https://www.ftc.gov/business-guidance/resources/marketing-your-mobile-app-get-it-right-start>.
- California guidance favors readable policies that identify collected categories, sharing, retention, choices, change dates, and tracking behavior: <https://oag.ca.gov/privacy/privacy-resources>.
- The UK ICO lists operator identity, purpose, lawful basis, recipients, retention, rights, and complaint information as core notice fields when UK GDPR applies: <https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/individual-rights/the-right-to-be-informed/what-privacy-information-should-we-provide/>.
- Microsoft accepts HTTPS privacy and terms URLs up to 2,048 characters for app metadata. Its consent-screen requirement targets multi-tenant apps, while AudioBud's signing identity is single-tenant: <https://learn.microsoft.com/en-us/entra/identity-platform/howto-add-terms-of-service-privacy-statement>.
- GitHub documents the custom-subdomain CNAME, account-level domain verification, and HTTPS sequence: <https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site>.
- GitHub Pages is the site host and applies GitHub's own privacy statement to its service: <https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement>.
- Contract review found that document-wide link checks can pass when a required footer link exists elsewhere, and a partial focusability list can miss elements that precede a skip link. The legal-page test therefore scopes navigation to real semantic footer elements and checks the first real body element directly.

## Approval record

Joe approved two separate pages, the public contact address, the implementation-aligned content, the restrained legal-page layout, consistent semantic footer navigation, first-element-child skip links, a single roadmap `h1`, custom-domain canonical and social metadata, inline browser favicons, exact README URLs, and omission of unreviewed governing-law and dispute clauses.
