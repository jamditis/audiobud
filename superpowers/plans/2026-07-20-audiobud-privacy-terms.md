# AudioBud privacy and terms implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superjawn:subagent-driven-development (recommended) or superjawn:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish implementation-aligned privacy and terms pages at stable AudioBud URLs and link them from every public project surface.

**Architecture:** Keep the site static under `docs/`. Add two independent HTML documents that reuse the existing header, footer, favicon, assets, and stylesheet. Add a Bun contract test that prevents the legal pages, custom-domain metadata, and cross-links from drifting away from the shipped app.

**Tech stack:** Static HTML and CSS, GitHub Pages, Bun tests, Prettier, Playwright browser checks, GitHub CLI.

---

## File map

- Create `scripts/legal-pages.test.ts`: executable contract for required pages, metadata, policy claims, cross-links, and CNAME.
- Create `docs/privacy.html`: public privacy policy.
- Create `docs/terms.html`: public terms of use.
- Modify `docs/index.html`: custom-domain metadata and footer policy links.
- Modify `docs/roadmap.html`: custom-domain metadata and footer policy links.
- Modify `docs/styles.css`: legal-document layout, summary cards, focus states, and responsive rules.
- Modify `README.md`: publish the home, privacy, terms, and support URLs.
- Preserve `docs/CNAME`: existing custom-domain mapping created in commit `061ca71`.

### Task 1: Pin the public policy contract with a failing test

**Files:**

- Create: `scripts/legal-pages.test.ts`

- [ ] **Step 1: Add the contract test**

Create `scripts/legal-pages.test.ts` with:

```ts
import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const docs = join(root, "docs");
const read = (name: string) => readFileSync(join(docs, name), "utf8");

const sitePages = ["index.html", "roadmap.html", "privacy.html", "terms.html"];

describe("AudioBud public policy pages", () => {
  it("publishes the custom domain from docs", () => {
    expect(read("CNAME").trim()).toBe("audiobud.amditis.tech");
  });

  it("publishes separate privacy and terms pages", () => {
    expect(existsSync(join(docs, "privacy.html"))).toBe(true);
    expect(existsSync(join(docs, "terms.html"))).toBe(true);
  });

  it("uses the custom HTTPS origin in public metadata", () => {
    for (const page of sitePages) {
      const html = read(page);
      expect(html).toContain("https://audiobud.amditis.tech");
      expect(html).not.toContain("https://jamditis.github.io/audiobud");
    }
  });

  it("links privacy and terms from every public page", () => {
    for (const page of sitePages) {
      const html = read(page);
      expect(html).toContain('href="./privacy.html"');
      expect(html).toContain('href="./terms.html"');
    }
  });

  it("describes the local-first boundary without an encryption promise", () => {
    const privacy = read("privacy.html");
    expect(privacy).toContain(
      "AudioBud does not send your audio to an AudioBud server",
    );
    expect(privacy).toContain("transcript text and the selected prompt");
    expect(privacy).toContain("does not send your audio to that provider");
    expect(privacy).toContain("stored in AudioBud's local settings file");
    expect(privacy.toLowerCase()).not.toContain("encrypted at rest");
  });

  it("states the website tracking and sale practices", () => {
    const privacy = read("privacy.html");
    expect(privacy).toContain("no AudioBud analytics");
    expect(privacy).toContain("does not sell personal information");
    expect(privacy).toContain("Global Privacy Control");
  });

  it("keeps source-code rights under the MIT license", () => {
    const terms = read("terms.html");
    expect(terms).toContain("MIT License");
    expect(terms).toContain(
      "copying, modifying, or distributing the source code",
    );
    expect(terms).not.toContain("class-action waiver");
    expect(terms).not.toContain("binding arbitration");
  });
});
```

- [ ] **Step 2: Run the new test and verify it fails for the missing pages**

Run:

```powershell
bun test scripts/legal-pages.test.ts
```

Expected: FAIL because `docs/privacy.html` and `docs/terms.html` do not exist.

- [ ] **Step 3: Commit the failing contract**

```powershell
git add -- scripts/legal-pages.test.ts
git commit -m "test(docs): pin public policy contract"
```

### Task 2: Publish the privacy policy

**Files:**

- Create: `docs/privacy.html`
- Test: `scripts/legal-pages.test.ts`

- [ ] **Step 1: Create the static page shell**

Use the same `doctype`, viewport, favicon, stylesheet, fixed header, `.swamp` layer, brand link, and footer structure as `docs/roadmap.html`. Set these metadata values:

```html
<title>AudioBud privacy policy</title>
<meta
  name="description"
  content="How AudioBud processes audio, transcripts, settings, optional AI requests, downloads, and website visits."
/>
<link rel="canonical" href="https://audiobud.amditis.tech/privacy.html" />
<meta property="og:title" content="AudioBud privacy policy" />
<meta
  property="og:description"
  content="AudioBud is local-first. Learn what stays on your device and when a feature contacts another service."
/>
<meta property="og:type" content="website" />
<meta property="og:url" content="https://audiobud.amditis.tech/privacy.html" />
<meta
  property="og:image"
  content="https://audiobud.amditis.tech/assets/og-image.png"
/>
<meta name="twitter:card" content="summary_large_image" />
<meta name="twitter:title" content="AudioBud privacy policy" />
<meta
  name="twitter:description"
  content="What stays on your device and when AudioBud contacts another service."
/>
<meta
  name="twitter:image"
  content="https://audiobud.amditis.tech/assets/og-image.png"
/>
```

The header navigation must link to Home, Roadmap, Privacy, and Terms, with `aria-current="page"` on Privacy. The footer must link to Privacy, Terms, Changelog, and GitHub.

- [ ] **Step 2: Add the approved privacy content**

Use an `.legal-hero` followed by `.legal-layout`. The hero must contain this summary:

```html
<p class="eyebrow">Privacy</p>
<h1 id="privacy-title">Your voice stays local unless you choose otherwise.</h1>
<p class="lede">
  AudioBud records and transcribes on your device. The project does not run an
  AudioBud account service or receive your audio, transcripts, history, or
  settings. A few optional actions contact services you select.
</p>
<p class="legal-meta">Effective July 20, 2026</p>
```

Add three `.data-card` summaries with these headings and claims:

```html
<article class="data-card">
  <span class="data-label">Local by default</span>
  <h2>Audio, transcripts, and history stay on your device.</h2>
  <p>You control retention, saved entries, learned words, and deletion.</p>
</article>
<article class="data-card">
  <span class="data-label">Only when enabled</span>
  <h2>Cloud post-processing sends text, not audio.</h2>
  <p>
    Your selected provider receives the transcript and prompt you ask it to
    process.
  </p>
</article>
<article class="data-card">
  <span class="data-label">Public website</span>
  <h2>GitHub Pages hosts this site.</h2>
  <p>
    There are no AudioBud analytics, forms, ads, or first-party tracking
    cookies.
  </p>
</article>
```

Add a contents navigation and the following sections with these exact facts:

1. `Who operates AudioBud`: Joe Amditis maintains the open-source AudioBud project; privacy contact is `jamditis@gmail.com`; public issues must not contain private information.
2. `What this policy covers`: the desktop app, official GitHub repository/releases, and `audiobud.amditis.tech`; third-party sites and providers follow their own policies.
3. `What stays on your device`: microphone audio, WAV recordings, raw/formatted/post-processed transcripts, history titles/timestamps, settings, device selections, shortcuts, custom words, word replacements, optional learned personalization, model files, logs, and provider keys; no claim of encryption.
4. `When information leaves your device`: the following exact lead paragraph must appear:

```html
<p>
  AudioBud does not send your audio to an AudioBud server. Network activity is
  limited to actions such as downloading a model, opening a project or release
  link, contacting a provider you configured, or using an update check if a
  future release enables that feature.
</p>
```

5. `Optional AI post-processing`: include the exact boundary sentences:

```html
<p>
  Post-processing is off by default. If you enable it, AudioBud sends transcript
  text and the selected prompt to the provider or compatible endpoint you chose.
  AudioBud does not send your audio to that provider. The provider can receive
  normal connection data such as your IP address and the request headers needed
  to identify AudioBud.
</p>
<p>
  Provider API keys are stored in AudioBud's local settings file. Provider
  privacy terms, retention, security, account rules, and charges apply to those
  requests. A custom local endpoint can keep this step on your network,
  depending on how you configured it.
</p>
```

6. `Downloads and external links`: models currently come from listed download hosts including `blob.handy.computer`; GitHub handles releases and project links; hosts receive normal network metadata; current automatic update checks are disabled.
7. `Website hosting and cookies`: GitHub Pages hosts the site; no AudioBud analytics, forms, advertising scripts, or first-party cookies; link GitHub's privacy statement.
8. `Sharing, sale, and tracking`: include the exact statement:

```html
<p>
  AudioBud does not sell personal information, share it for cross-context
  behavioral advertising, or run advertising profiles. Because AudioBud does not
  perform first-party cross-site tracking, Do Not Track and Global Privacy
  Control signals do not change AudioBud's behavior. GitHub and linked providers
  handle their own signals under their policies.
</p>
```

9. `Retention and deletion`: local unsaved history defaults to five entries; saved entries can remain until deletion; users can delete entries and recordings, change retention, reset personalization, remove keys/settings, and remove app data; provider/GitHub retention is separate; support email is kept only as needed to respond and maintain project records.
10. `Security`: local-first reduces transfers but no system is perfectly secure; OS/account protection matters; secrets in local settings should not be used on an untrusted shared device.
11. `Your choices and rights`: users control optional features and local deletion; applicable-law access/correction/deletion requests can be emailed; third-party data requests go to the third party; users may complain to their local authority where applicable.
12. `Children`: general-purpose tool, not directed to children under 13; contact the maintainer if the project received a child's information.
13. `Changes`: material changes get a revised effective date and publication at the same URL.
14. `Contact`: Joe Amditis, AudioBud project maintainer, `mailto:jamditis@gmail.com`.

- [ ] **Step 3: Run the contract test**

```powershell
bun test scripts/legal-pages.test.ts
```

Expected: FAIL only because `docs/terms.html` is still missing or site-wide links are not complete.

- [ ] **Step 4: Commit the privacy page**

```powershell
git add -- docs/privacy.html
git commit -m "docs: publish AudioBud privacy policy"
```

### Task 3: Publish the terms of use

**Files:**

- Create: `docs/terms.html`
- Test: `scripts/legal-pages.test.ts`

- [ ] **Step 1: Create the static page shell**

Reuse the privacy-page shell. Set the canonical URL to `https://audiobud.amditis.tech/terms.html`, set the page title to `AudioBud terms of use`, and use this description in standard, Open Graph, and Twitter metadata:

```text
The terms that apply when you download, use, modify, or access AudioBud and its project website.
```

Set `aria-current="page"` on the Terms navigation and footer link.

- [ ] **Step 2: Add the approved terms content**

Use this hero:

```html
<p class="eyebrow">Terms</p>
<h1 id="terms-title">Use AudioBud carefully, lawfully, and on your terms.</h1>
<p class="lede">
  AudioBud is free, open-source software for local dictation. These terms cover
  use of the app, official downloads, and project website. The MIT License still
  governs the source code.
</p>
<p class="legal-meta">Effective July 20, 2026</p>
```

Add a contents navigation and these sections:

1. `Agreement and scope`: using the app/site means agreeing to the terms; do not use if unable to agree; official surfaces are the repository, releases, and custom domain.
2. `Open-source license`: include this exact statement:

```html
<p>
  AudioBud's source code is released under the MIT License. The MIT License, not
  these website terms, governs copying, modifying, or distributing the source
  code. If these terms and the MIT License address the same source-code right,
  the MIT License controls.
</p>
```

3. `Using AudioBud`: lawful use; responsibility for microphone permission, device security, credentials, storage, backups, and content; no interference with project infrastructure or unlawful/harmful content.
4. `Your content`: users keep ownership; the project receives no license to local audio/transcripts because it does not receive them; third-party provider requests follow provider terms.
5. `Optional providers and costs`: user-selected post-processing provider or custom endpoint; user provides credentials and accepts provider terms/privacy/charges; no AudioBud promise about provider availability or output handling.
6. `Models, downloads, and third-party components`: models/downloads can be hosted by third parties; third-party licenses/notices apply; verify official release source; no promise every model remains available.
7. `Transcription and AI output`: output can be incomplete or wrong; review before use; do not rely without qualified human review for medical, legal, financial, emergency, accessibility, or other safety-related decisions.
8. `Privacy`: link `./privacy.html`; explain local-first boundary and third-party policy responsibility.
9. `Availability, updates, and support`: free project with no uptime, update schedule, compatibility, or individual support promise; features can change; current release facts control over roadmap statements.
10. `No warranty`: use MIT-aligned language: the app/site are provided “as is” and “as available,” without warranties to the extent permitted by law; nothing excludes non-waivable rights.
11. `Limits on liability`: to extent permitted by law, maintainer/contributors are not liable for indirect, incidental, special, consequential, or lost-data/profit damages; do not exclude liability that cannot be excluded.
12. `Stopping use and changes`: users can stop at any time; new terms apply prospectively from revised effective date; continued use after publication means acceptance.
13. `Contact`: Joe Amditis, AudioBud project maintainer, `mailto:jamditis@gmail.com`.

Do not add arbitration, class-action waiver, indemnity, or governing-law clauses.

- [ ] **Step 3: Run the contract test**

```powershell
bun test scripts/legal-pages.test.ts
```

Expected: FAIL only for missing custom-domain metadata or cross-links in the existing pages.

- [ ] **Step 4: Commit the terms page**

```powershell
git add -- docs/terms.html
git commit -m "docs: publish AudioBud terms of use"
```

### Task 4: Connect the policies to the existing public site

**Files:**

- Modify: `docs/index.html`
- Modify: `docs/roadmap.html`
- Modify: `README.md`
- Test: `scripts/legal-pages.test.ts`

- [ ] **Step 1: Update custom-domain metadata**

In `docs/index.html`, replace every `https://jamditis.github.io/audiobud/` metadata origin with `https://audiobud.amditis.tech/`.

In `docs/roadmap.html`, use:

```html
<link rel="canonical" href="https://audiobud.amditis.tech/roadmap.html" />
<meta property="og:url" content="https://audiobud.amditis.tech/roadmap.html" />
<meta
  property="og:image"
  content="https://audiobud.amditis.tech/assets/og-image.png"
/>
<meta
  name="twitter:image"
  content="https://audiobud.amditis.tech/assets/og-image.png"
/>
```

- [ ] **Step 2: Add footer links to both existing pages**

Insert before Changelog in both footers:

```html
<a href="./privacy.html">Privacy</a> <a href="./terms.html">Terms</a>
```

- [ ] **Step 3: Add public links to README**

Replace the current website line near the top with:

```markdown
- **Website:** <https://audiobud.amditis.tech/>
- **Privacy:** <https://audiobud.amditis.tech/privacy.html>
- **Terms:** <https://audiobud.amditis.tech/terms.html>
- **Support:** <https://github.com/jamditis/audiobud/issues>
```

- [ ] **Step 4: Run the contract test**

```powershell
bun test scripts/legal-pages.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit the links and metadata**

```powershell
git add -- README.md docs/index.html docs/roadmap.html
git commit -m "docs: connect policies to public site"
```

### Task 5: Add the legal-document layout

**Files:**

- Modify: `docs/styles.css`

- [ ] **Step 1: Add visible focus behavior**

Add near the base link rules:

```css
:focus-visible {
  outline: 3px solid var(--amber);
  outline-offset: 4px;
}
```

- [ ] **Step 2: Add the legal layout styles**

Add before the existing responsive block:

```css
.legal-hero {
  padding: 132px 0 48px;
  border-bottom: 1px solid var(--line);
}

.legal-hero h1 {
  max-width: 850px;
}

.legal-meta {
  margin-bottom: 0;
  color: var(--muted);
  font-size: 14px;
}

.data-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
  margin-top: 34px;
}

.data-card {
  padding: 20px;
  border: 1px solid rgba(243, 247, 238, 0.12);
  border-radius: 8px;
  background: rgba(16, 27, 19, 0.78);
}

.data-card h2 {
  margin-bottom: 10px;
  font-size: 20px;
  line-height: 1.2;
}

.data-card p {
  margin-bottom: 0;
  color: var(--muted);
  line-height: 1.55;
}

.data-label {
  display: block;
  margin-bottom: 12px;
  color: var(--green);
  font-size: 12px;
  font-weight: 900;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.legal-layout {
  display: grid;
  grid-template-columns: 210px minmax(0, 760px);
  gap: 52px;
  align-items: start;
  justify-content: center;
}

.legal-toc {
  position: sticky;
  top: 92px;
  padding: 18px;
  border: 1px solid rgba(243, 247, 238, 0.1);
  border-radius: 8px;
  background: rgba(16, 27, 19, 0.72);
}

.legal-toc strong {
  display: block;
  margin-bottom: 10px;
  color: var(--text);
  font-size: 13px;
}

.legal-toc a {
  display: block;
  padding: 5px 0;
  color: var(--muted);
  font-size: 13px;
  line-height: 1.35;
  text-decoration: none;
}

.legal-toc a:hover {
  color: var(--green);
}

.legal-document {
  min-width: 0;
}

.legal-section {
  scroll-margin-top: 90px;
  padding: 0 0 34px;
}

.legal-section + .legal-section {
  padding-top: 34px;
  border-top: 1px solid rgba(243, 247, 238, 0.1);
}

.legal-section h2 {
  margin-bottom: 14px;
  font-size: clamp(24px, 3vw, 34px);
  line-height: 1.12;
}

.legal-section h3 {
  margin: 24px 0 10px;
  font-size: 18px;
}

.legal-section p,
.legal-section li {
  color: var(--muted);
  font-size: 16px;
  line-height: 1.72;
}

.legal-section li + li {
  margin-top: 8px;
}

.legal-section a {
  color: var(--green);
  text-underline-offset: 3px;
}

.legal-callout {
  margin: 22px 0;
  padding: 18px 20px;
  border-left: 3px solid var(--amber);
  background: rgba(255, 178, 62, 0.08);
}

.legal-callout p:last-child {
  margin-bottom: 0;
}
```

- [ ] **Step 3: Extend the mobile rules**

Inside `@media (max-width: 860px)`, include `.data-grid` in the existing one-column selector and add:

```css
.legal-hero {
  padding: 108px 0 34px;
}

.legal-layout {
  grid-template-columns: 1fr;
  gap: 30px;
}

.legal-toc {
  position: static;
}
```

- [ ] **Step 4: Format the touched files**

Run:

```powershell
bunx prettier --write README.md docs/index.html docs/roadmap.html docs/privacy.html docs/terms.html docs/styles.css scripts/legal-pages.test.ts superpowers/specs/2026-07-20-audiobud-privacy-terms-design.md superpowers/plans/2026-07-20-audiobud-privacy-terms.md
```

Expected: command exits 0 and only the listed files are formatted.

- [ ] **Step 5: Commit the legal-page presentation**

```powershell
git add -- docs/styles.css docs/privacy.html docs/terms.html
git commit -m "docs: style policy pages for clear reading"
```

### Task 6: Verify the pages and publish the review PR

**Files:**

- Verify all changed files.
- Use `.github/PULL_REQUEST_TEMPLATE.md` for the PR body.

- [ ] **Step 1: Run focused and repository checks**

```powershell
bun test scripts/legal-pages.test.ts
bun test scripts src
bun run build
bunx prettier --check README.md docs/index.html docs/roadmap.html docs/privacy.html docs/terms.html docs/styles.css scripts/legal-pages.test.ts superpowers/specs/2026-07-20-audiobud-privacy-terms-design.md superpowers/plans/2026-07-20-audiobud-privacy-terms.md
git diff --check origin/main...HEAD
```

Expected: all commands exit 0; Bun reports 0 failed tests; Vite produces `dist/`; Prettier reports all matched files use its style; Git reports no whitespace errors.

- [ ] **Step 2: Run a local Pages server**

```powershell
$server = Start-Process python -ArgumentList '-m','http.server','4173','--directory','docs' -WindowStyle Hidden -PassThru
```

Open and inspect:

- `http://127.0.0.1:4173/`
- `http://127.0.0.1:4173/roadmap.html`
- `http://127.0.0.1:4173/privacy.html`
- `http://127.0.0.1:4173/terms.html`

Check each page at 1440×1000 and 390×844. Verify readable line length, no horizontal overflow, visible focus, working contents links, correct `aria-current`, working footer links, and reduced-motion behavior. Stop the server after inspection:

```powershell
Stop-Process -Id $server.Id
```

- [ ] **Step 3: Verify custom-domain readiness**

```powershell
Resolve-DnsName audiobud.amditis.tech -Type CNAME
gh api repos/jamditis/audiobud/pages
curl.exe -I https://audiobud.amditis.tech/
curl.exe -I https://jamditis.github.io/audiobud/
```

Expected before merge: DNS points to `jamditis.github.io`; GitHub reports the custom domain; the custom domain serves or is awaiting only GitHub certificate issuance; the old URL redirects to the matching custom path. Enable `Enforce HTTPS` only after GitHub reports the certificate is ready.

- [ ] **Step 4: Inspect the exact publication scope**

```powershell
git status --short
git diff --stat origin/main...HEAD
git diff --name-status origin/main...HEAD
```

Expected tracked scope: the approved spec and plan, `scripts/legal-pages.test.ts`, `docs/privacy.html`, `docs/terms.html`, `docs/index.html`, `docs/roadmap.html`, `docs/styles.css`, and `README.md`. No screenshots, `.firecrawl`, credentials, or Azure values are staged.

- [ ] **Step 5: Push the branch**

```powershell
git push -u origin agent/privacy-terms-pages
```

- [ ] **Step 6: Open a ready-for-review PR**

Write a PR body that completes every section of `.github/PULL_REQUEST_TEMPLATE.md`:

```markdown
## Before submitting

- [x] I searched existing issues and pull requests (including closed ones) so this isn't a duplicate
- [x] I tested this change locally

Skipping any of the above? Explain why here:

Nothing skipped.

## Description

Publishes implementation-aligned privacy and terms pages for AudioBud, links them from the GitHub Pages site and README, and moves public metadata to `audiobud.amditis.tech`.

The privacy policy distinguishes local audio and history from optional text-only provider requests, model downloads, GitHub hosting, and support email. The terms keep source-code rights under the MIT License and avoid unreviewed dispute clauses.

## Related issues

No existing issue. This work supplies stable privacy and terms URLs for the AudioBud Microsoft Entra application profile and release-signing setup.

## Testing

- `bun test scripts/legal-pages.test.ts`
- `bun test scripts src`
- `bun run build`
- focused Prettier check
- local desktop and mobile browser review of all four Pages documents
- DNS, GitHub Pages, redirect, and HTTPS readiness checks

## Screenshots / videos (optional)

Desktop and mobile policy-page screenshots are attached if they materially help review the layout.
```

Use `apply_patch` to create
`C:\Users\amdit\AppData\Local\Temp\audiobud-privacy-terms-pr.md` with the
approved body above. Then run:

```powershell
$prBodyFile = 'C:\Users\amdit\AppData\Local\Temp\audiobud-privacy-terms-pr.md'
gh pr create --base main --head agent/privacy-terms-pages --title "docs: publish AudioBud privacy and terms pages" --body-file $prBodyFile
```

Do not pass `--draft`; the user requested a ready-for-review PR.

- [ ] **Step 7: Confirm the PR state and checks**

```powershell
gh pr view --json url,title,isDraft,headRefName,baseRefName,state,statusCheckRollup
```

Expected: `isDraft` is false, base is `main`, head is `agent/privacy-terms-pages`, state is `OPEN`, and required checks have started or passed.
