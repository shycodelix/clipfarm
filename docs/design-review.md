# Apple HIG design review — clipfarm

Reviewed against `.design-rules/references/hig/*` (the 123-page Apple Design Skill
clone) and `.design-rules/references/cross-platform.md` for Electron translation.
Citations name the guideline file and heading so each claim can be re-checked.

Scope: the primary editor window at 1480×940, its empty/library state, active
timeline, AI job progress, export sheet, settings and shortcuts sheets, error
toasts, plus system Light and Dark appearance.

## What was checked

| Surface | State reviewed | Result |
| --- | --- | --- |
| Editor | Media bin, preview, inspector, timeline, status bar | Pass |
| Editor | No project, project with no clips | Pass (distinct empty states, one primary action) |
| Timeline | Active, selection, drag/trim, silence zones, scene markers | Pass |
| AI | Idle, running (determinate bar + Cancel), complete toast, error toast | Pass |
| Export | Sheet with presets, caption toggle, estimate, progress handoff | Pass |
| Library | Populated grid and zero projects | Pass |
| Agent Bridge | Config, copy action, tool list, no-project state | Pass |
| Settings / Shortcuts | Appearance, engine diagnostics, MCP link | Pass |
| Appearance | System Light and Dark | Pass |
| Appearance | Reduce Transparency, Reduce Motion | CSS fallbacks implemented, not screenshot-tested |

## Layout and windows

- **Resizable, one primary window.** `windows.md › Best practices` asks for fluid
  resizing and no custom window chrome. The app uses native frames outside macOS
  and `hiddenInset` traffic lights on macOS; minimum size is 1200×760 so the four
  columns (sidebar, bin, stage, inspector) never collapse into overlap.
- **Spacious desktop density.** `designing-for-macos.md › Best practices` calls for
  more content and less nested modality. Everything lives in one window: media,
  transcript, preview, inspector, timeline. The only modal surfaces are Export,
  Settings, Shortcuts, New/Delete Project and the command palette — all transient
  tasks, which is the sanctioned use of modality in `modality.md`.
- **Split view semantics.** `cross-platform.md › Conventions to check` maps
  NavigationSplitView to sidebar + content. The sidebar is the single navigation
  column, the inspector is a secondary panel, and both can be hidden from the
  title bar, the View menu and keyboard shortcuts, per `sidebars.md › Best practices`.

## Materials and hierarchy

- **Two layers only.** Translucency is confined to the functional layer: title bar,
  sidebar and the preview transport (`liquid-glass.md › The two layers`). Media,
  timeline and inspector content are opaque surfaces, because
  `materials.md › Liquid Glass` says not to put glass in the content layer.
- **Regular variant behaviour.** The chrome uses a 28 px blur with a 74% tinted
  background — the "regular" behaviour that keeps text legible, appropriate for
  sidebars with labels (`materials.md › Which variant`).
- **Reduced transparency.** A `prefers-reduced-transparency` rule swaps every
  material for a solid surface, and `prefers-reduced-motion` collapses animation,
  matching Apple's guidance that materials must not fight accessibility settings.
- **No Liquid Glass on the timeline.** Clips and caption blocks are solid content;
  only their hover/selection rings respond.

## Color, typography, contrast

- Colors are **semantic CSS custom properties** (label, secondary, separator,
  surface, accent, danger, success) that are redefined for Dark appearance, per
  `dark-mode.md › Dark Mode colors`. No hard-coded foreground/background pairs in
  component code.
- Typography uses the system font stack (SF Pro on macOS, Segoe UI on Windows) with
  a dense 10.5–13 pt scale for chrome and 24 pt titles, matching desktop information
  density in `typography.md`.
- Primary label on surface exceeds 7:1 in both appearances; secondary labels stay
  above 4.5:1 (`dark-mode.md › Aim for sufficient color contrast`). Accent is only
  used for interactive states, never as low-contrast body text.
- **No app-specific appearance switch.** `dark-mode.md › Best practices` explicitly
  discourages one; Settings states "Automatic" and the app follows the OS live.

## Sidebar and toolbar

- Sidebar shows at most two levels: section headings and single rows, with no
  disclosure triangles (`sidebars.md › Best practices` — keep hierarchy shallow).
- Selection uses the system accent fill with white label, matching source-list
  selection expectations; icons are monochrome and inherit the label color.
- The sidebar can be hidden (`View › Toggle Sidebar`, ⌥⌘S), as recommended for
  macOS, and is visible by default so navigation stays discoverable.
- **Menu bar parity.** `designing-for-macos.md › Best practices` and
  `cross-platform.md › Conventions to check` require every command in the menu bar
  with standard shortcuts. `src/main/menu.ts` covers File, Edit, View, AI, Window,
  Help; the same ids drive the command palette and the shortcut sheet.
- Toolbar controls are minimal and untinted: a command search field, two panel
  toggles and one primary Export button (`toolbars.md` guidance to reduce tinted
  controls and toolbar backgrounds).

## Keyboard and pointer

- Standard shortcuts are preserved: ⌘Z/⇧⌘Z undo/redo, ⌘S save, ⌘I import, ⌘, settings,
  ⌘K palette, ⌘/ shortcuts, ⌘1–4 navigation. Nothing repurposes an Apple-reserved
  combination (`keyboards.md › Standard keyboard shortcuts`).
- Custom shortcuts cover frequently used editing commands (⌘B split, ⇧⌘D remove
  silence, ⌘T transcribe) and are shown in menus, tooltips, the palette and the
  shortcuts sheet — never hidden.
- Focus is visible everywhere via `:focus-visible` rings; the palette supports
  ↑/↓/Enter/Esc, and every clip and row is reachable with Tab and activated with
  Enter/Space.
- Pointer affordances: pointer cursor changes for scrub, trim handles and clip drag;
  drop targets highlight during drag-over; hover reveals secondary actions.

## Feedback, progress and errors

- **Determinate progress** is used for analysis and export, with a stage string,
  percentage and Cancel, following `progress-indicators.md › Best practices`. The
  indicator never jumps backwards; cancellation is immediate and announced.
- Undo is offered for destructive AI cuts, and the toast names it ("Press ⌘Z to
  restore the original timing"), matching the guidance to let people halt or reverse
  lengthy operations.
- Errors are plain-language toasts with a specific recovery line (missing FFmpeg,
  missing model, unreadable media) rather than raw stack traces.
- Success feedback is quiet: a toast, a job bar completion state, and a visible
  change on the timeline.
- Empty states always explain the next action and provide exactly one primary
  button (`feedback.md`, `loading.md`).

## Privacy and responsibility

- All processing is local; the only network activity is a model download on first
  transcription. Settings and the Agent Bridge both state this, and the MCP server
  is stdio-only with no listener.
- Destructive project deletion is confirmed with a sheet that explains media is
  untouched (`alerts.md` guidance for confirming destructive actions).

## Known gaps / not verified

- No VoiceOver or Full Keyboard Access pass was run; ARIA roles, labels, `aria-current`,
  `aria-pressed` and live regions are present but need assistive-technology testing.
- Light and Dark were screenshotted at 1480×940; Increase Contrast, Reduce
  Transparency and 200% text scaling were implemented in CSS but not inspected.
- Native macOS vibrancy/traffic-light placement and Windows/Linux window chrome were
  not visually verified in this Linux sandbox (the UI was reviewed through the
  renderer preview). That remains a manual check on each target OS.
- The app ships a single-window workflow; auxiliary windows (e.g. a floating
  transcript) are not implemented.
