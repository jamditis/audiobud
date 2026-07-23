# AudioBud visual design audit

## Design intent

AudioBud already has a distinctive point of view: a red-eyed frog mascot, a
night-pond palette, friendly rounded type, and a small-app sense of humor. This
pass keeps those assets and makes the product feel more composed, tactile, and
trustworthy. It is a refinement of the existing identity, not a rebrand.

The guiding idea is **quiet utility in a living pond**: settings should stay
calm and legible while small reactions, ripples, fireflies, and the frog give
the interface personality at the edges.

## What was already working

- The frog is recognizable at tiny sizes and expressive at larger sizes.
- Green, amber, coral, and teal create an ownable palette with useful semantic
  roles.
- The swamp background gives AudioBud more character than a typical utility
  app.
- The app's information architecture is straightforward and the main controls
  are already grouped sensibly.
- The live recording bars and vocal-sac response connect the mascot to the core
  task instead of using it as decoration alone.

## Main findings

### 1. The app needed a stronger frame

The previous sidebar, settings cards, and background had similar visual weight.
That made the interface feel like controls placed directly over an illustration
rather than one coherent application shell.

The new shell introduces a glassy navigation rail, persistent section context,
clearer content bounds, stronger active-state contrast, and a separate
waterline footer. The swamp remains visible without competing with the task.

### 2. Settings needed more tactile consistency

Rows, toggles, buttons, model cards, and section labels used related colors but
did not share a consistent interaction language. Hover, press, focus, and
selection feedback now use the same lift, inset highlight, green rim, and soft
glow vocabulary.

### 3. The website needed to stage the product

The previous hero used the app screenshot as a dim background. It communicated
the product, but it reduced screenshot legibility and made the headline compete
with the interface.

The new hero separates message and product: a direct value proposition on the
left and an app window on a pond-like stage on the right. Floating shortcut,
privacy, waveform, and frog elements explain the experience without adding a
demo video or a heavy script.

### 4. The public site needed a clearer reading rhythm

The previous page relied on repeated rectangular cards. The redesign uses a
more varied but still consistent rhythm: three-step task flow, feature bento,
large product preview, compact model comparison, and a focused final call to
action. This makes the long page easier to scan while preserving all core
information.

### 5. Motion needed hierarchy and an off switch

Ambient animation is now slow and peripheral. Task feedback is quicker and
closer to the control. Recording, transcribing, and processing have distinct
overlay treatments. All new motion is disabled or reduced through
`prefers-reduced-motion`.

### 6. Responsive behavior needed explicit art direction

The site now changes composition rather than merely stacking desktop blocks.
The app preview becomes a deliberate crop on small screens, card grids collapse
in reading order, calls to action become thumb-friendly, and the roadmap no
longer creates horizontal overflow.

## Changes implemented in this pass

- Added an application shell with a section toolbar, stronger navigation rail,
  improved active states, and an ambient sidebar pond detail.
- Refined swamp-glass cards, grouped setting rows, toggles, buttons, and model
  cards with a shared surface and motion system.
- Improved recording overlay feedback for recording, transcribing, and
  processing states.
- Rebuilt the website hero around the product, frog, local-audio promise, and
  shortcut interaction.
- Reorganized the public feature story into a task flow, bento layout, model
  comparison, and focused download call to action.
- Restyled the roadmap and public footer to match the new site system.
- Added skip links, visible focus states, semantic landmarks, reduced-motion
  behavior, and responsive checks down to 320px.
- Regenerated app screenshots and Open Graph artwork from the current UI.

## Recommended follow-up work

### High impact

- Carry the new shell treatment through onboarding and permission screens so
  first launch feels like the same product as settings.
- Add screenshot regression checks for the general, models, onboarding, and
  overlay states in CI.
- Run a full keyboard and screen-reader pass in the packaged Windows app,
  including custom selects and model-card actions.
- Keep the privacy and required pages aligned with the shared header/footer
  system as their content evolves.

### Medium impact

- Give success and error toasts small frog expressions while keeping the text
  and color semantics primary.
- Add a compact first-use shortcut rehearsal that lets the user try one short
  dictation before leaving onboarding.
- Create a small set of reusable lily-pad, ripple, lock, and waveform graphics
  for release notes and social assets.
- Consider an optional high-contrast mode if accessibility testing shows the
  translucent surfaces are too subtle on some Windows displays.

## Verification targets

- App screenshots at 680 × 570 in forced dark mode with mocked Tauri data.
- Website at 320, 390, 820, 1024, and 1440 CSS pixels.
- No horizontal overflow, duplicate IDs, missing image alternatives, empty
  links, console errors, or failed local resources.
- Frontend build, lint, formatting, and unit tests remain green.
