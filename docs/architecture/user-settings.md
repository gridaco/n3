# Global user settings

Settings are independent of documents. A single user-level `settings.json`
supplies preferences for every document; there is no workspace, project lookup,
inheritance chain, or settings embedded in `.n3.json`.

## Responsibilities

- `src/settings/` owns the typed values, defaults, JSON validation, and merge
  controller. It has no filesystem, dialog, window, or rendering dependency.
- `SettingsStore` is a small byte-storage interface: read an optional snapshot,
  then conditionally replace the exact snapshot that was read. Missing data is
  distinct from malformed or inaccessible data.
- `src/native/settings_store.rs` resolves the macOS user path and implements
  bounded reads, guarded writes, permissions, and temporary-file ownership.
- The native host schedules synchronization and handles opening the text file.
  `src/ui/user_settings.rs` maps validated settings into existing runtime behavior;
  Preferences emits host requests and displays errors without performing I/O.
- The native host supplies the current OS appearance. The settings model keeps
  the user's `System`, `Light`, or `Dark` choice distinct from the resolved
  appearance used by egui. Style colors and measurements live in `src/theme.rs`;
  they are not serialized as a general theme configuration.
- Core tests and documentation replay use memory stores. Native storage tests use
  temporary directories. None of them reads or writes the user's preferences.

The interface follows the needs of this native feature. A different host can
provide another store and an appropriate way to edit its text without changing
the settings model or widgets. This is not a browser implementation, and it does
not claim that other native application services are already portable.

`src/theme.rs` follows [shadcn's semantic theme tokens](https://ui.shadcn.com/docs/theming#theme-tokens) for the interface:
background, card, popover, primary, secondary, muted, accent, borders, focus rings,
and sidebar roles. Each surface has a foreground partner where needed. The saved
`appearance.accentColor` resolves to `primary` and focus `ring`; `accent` is the
neutral hover surface. egui's widget fills, strokes, and pressed states derive
from these tokens. Its text-edit fill comes from `secondary`, since `input` names
the field border. The `workbench_*` tokens cover the viewport and HUD; axis and
geometry feedback colors remain N3 specific rather than masquerading as
interface components.

## Synchronization and interaction

Startup reads the user's file or uses defaults if it is absent. Reading defaults
does not create a file. A UI change or the explicit Open settings.json action
creates it. The first write includes the complete known defaults; an existing
partial file keeps omitted defaults and unknown keys.

UI changes apply immediately to the running editor. Persist them after an active
gesture or text entry finishes, rather than writing on every intermediate drag
value. The native host checks for external changes about every 500 ms and on
focus return. It requests a redraw only when visible state changes; file polling
must not turn an idle editor into a continuously rendered viewport.

The controller compares three values per known key: the last successfully applied
snapshot, current local preferences, and freshly read external preferences. It
merges changes to different keys, accepts identical concurrent changes, and
rejects conflicting changes to the same key. Failure does not advance the
baseline or overwrite the file. Explicit Reload adopts valid file values and
discards pending local preference edits. Invalid input retains active preferences
and exposes an actionable error.

Preferences are not document edits and never enter document history. Display
units persist across New/Open without changing canonical centimeters. Camera pose,
projection, selection, tool gestures, UI layout, and the source-specific Z-up
presentation flag remain transient.

`appearance.theme` defaults to `system`: N3 resolves it against the host's
current appearance, including changes while the app is open. An unavailable
host appearance resolves to Light. Explicit `light` and `dark` override the
host without changing the saved preference. `appearance.accentColor` is one
opaque `#RRGGBB` RGB color for interface emphasis, defaulting to `#2563EB`.
The standard egui color picker edits it; Reset restores the default. Error
feedback and the X/Y/Z axis colors keep their semantic roles. The rendered
accent may adjust lightness for contrast in each appearance while preserving
the saved RGB value. Applying either
appearance preference changes only presentation, not the document or history.

## File contract

The macOS adapter uses `~/Library/Application Support/N3/settings.json`. The
application does not search the current directory for settings. A plain UTF-8
JSON object uses dotted keys. Unknown values are retained during UI writes;
known keys are validated before application. Duplicate keys, invalid known types
or values, and input over 1 MiB are rejected. Comments and trailing commas are
not supported. The [generated guide](../guide/settings.md) contains defaults and
allowed values sourced from the settings model.

Documentation replay uses an explicit Dark appearance for stable existing
images. The settings feature deliberately selects Light and Dark before its
respective captures; tests use isolated stores, never the host's actual file.

Writes use a sibling temporary file, preserve existing permissions, and replace
the destination only after checking its expected bytes. Cooperating N3 writers
serialize through a sidecar lock. Creating a missing file must not overwrite a
file that appeared concurrently. Symlinks and nonregular destinations fail
explicitly. Temporary/lock cleanup removes only files owned by that operation.

An ordinary filesystem does not offer compare-and-swap against arbitrary text
editors. Rechecking immediately before rename narrows but cannot eliminate the
race with a noncooperating writer in that final interval. Do not describe this as
an unconditional cross-process transaction guarantee. Merge tests, file conflict
tests, and the executable guide verify the implemented contract.
