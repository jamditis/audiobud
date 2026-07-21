# AudioBud privacy and terms design

**Status:** Approved by Joe Amditis on July 20, 2026.

## Goal

Publish accurate privacy and terms pages for AudioBud, link them from the public site and README, and provide stable URLs for the Microsoft Entra application profile.

## Scope

The change will:

- add `docs/privacy.html` and `docs/terms.html`;
- add privacy and terms links to the footers in `docs/index.html` and `docs/roadmap.html`;
- cross-link the two policy pages;
- add the public website, privacy, and terms URLs to `README.md`;
- update canonical, Open Graph, and Twitter URLs from the old GitHub Pages project path to `https://audiobud.amditis.tech`;
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
- AudioBud delivers completed transcript text to the focused application using the paste method selected by the user. Clipboard modes temporarily write the transcript to the system clipboard and normally restore its previous contents unless the user selects `CopyToClipboard`. Direct mode types the transcript into the focused application.
- External-script mode passes the transcript as one command-line argument to the configured script. These local delivery paths do not send text off-device by themselves, but the receiving application or script controls any later transmission and retention.
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

The terms page will include:

1. An effective date, operator, scope, and acceptance statement.
2. A clear distinction between these usage terms and the MIT license, which continues to govern copying, modification, and distribution of the source code.
3. Lawful-use and user-responsibility terms.
4. A statement that users retain their content and AudioBud receives no ownership grant for local content.
5. Terms for optional third-party providers, user-supplied credentials, provider charges, model hosts, and GitHub-hosted downloads.
6. A warning that transcription and optional AI output can be wrong and must be reviewed before medical, legal, financial, or safety-related use.
7. Availability, updates, support, and policy-change language that makes no uptime or support promise.
8. MIT-aligned warranty and liability language qualified by applicable law.
9. A contact section and a link to the privacy page.

## Page design

Both pages will reuse the existing colors, fixed header, frog mark, background layers, cards, buttons, footer, and reduced-motion behavior. Legal content will use a readable column of about 760 pixels rather than the marketing-page width.

Each page will open with a short summary and a compact contents list. The privacy page will include three fact cards that distinguish data that stays on the device, data sent only after a user action, and GitHub's role as site host. These cards summarize the policy but do not replace the detailed sections.

The privacy page will place a `Skip to privacy policy` link to `#privacy-title` before every other focusable body element.

Headings will use sentence case. Links and keyboard focus will remain visible. Mobile layouts will collapse to one column without hiding access to the policy pages.

## Public URLs and deployment

The stable URLs will be:

- Home: `https://audiobud.amditis.tech/`
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
- verify canonical, Open Graph, and Twitter URLs use the custom HTTPS hostname;
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

## Approval record

Joe approved two separate pages, the public contact address, the implementation-aligned content, the restrained legal-page layout, site-wide links, custom-domain canonical URLs, and omission of unreviewed governing-law and dispute clauses.
