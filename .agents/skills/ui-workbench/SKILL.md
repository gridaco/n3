---
name: ui-workbench
description: Develop and review N3 reusable UI components through its internal workbench, including fixture cases, input ownership, isolation, and deterministic visual evidence. Use when adding or strengthening component boundaries and their harness.
---

# N3 UI component workbench

Use the workbench as a real consumer of production components. Keep fixtures and
their effects in the workbench host; preserve editor integration and public-guide
behavior. Follow [AGENTS.md](../../../AGENTS.md) and the existing
[egui skill](../egui/SKILL.md) for repository and visual conventions.

## Start with the owning boundary

Inspect the [case catalog and fixture host](../../../src/workbench/mod.rs),
[replay](../../../src/workbench/replay.rs), and
[behavioral tests](../../../src/workbench/tests.rs). The Rust catalog owns page
names, case names, and stable identities; do not maintain a second case inventory.

Distinguish native egui widgets, reusable custom presentation/interaction, and
application composition or effects. Ordinary buttons, fields, and checkboxes
usually need no extra abstraction. Extract only what real component consumers
justify; keep the single crate and explicit case catalog.

Current boundaries:

- Menus: [menu presentation](../../../src/ui/menu.rs) owns rows, shortcuts,
  submenus, separators, and keyboard policy. [Workspace menu groups](../../../src/ui/workspace_menus.rs)
  own shared application composition. Consume production action metadata and typed
  bindings; fixture availability is supplied through ActionState. Actual
  capabilities and document effects remain in WorkspaceUi::action_state and
  WorkspaceUi::dispatch.
- Held-key pies: reuse the typed View/Shading content, shared geometry/painting,
  and [PieInput](../../../src/input/pie_input.rs) event ordering. Supply stable
  instance IDs and explicit bounds. The editor's pointer policy includes its
  gizmo; isolated fixtures must not reserve a nonexistent gizmo region.
- 2D ruler: supply projected axis origin, scale, and ranges to the existing
  [ruler painter](../../../src/ui/ruler_2d.rs). The minimal
  [scalar tick planner](../../../src/ui/scalar_ticks.rs) also serves the timeline.
  Camera projection, selected geometry, units, and viewport painting remain
  viewport responsibilities.
- Timeline: use the read-only [kit](../../../src/ui/timeline/mod.rs), revisioned
  tracks/keys and semantic requests. Accepted playback, clocks and evaluation
  belong to hosts. Follow the [timeline contract and integration example](references/timeline.md)
  for lifecycle, per-pass delivery, dense markers and ownership. Its synthetic
  fixture is independent of the editor and exchange formats.

Do not create a fake Editor or scene to display a component, copy its renderer,
or add demonstration shortcuts. Toasts, value editing, gizmos, and toolbars need
meaningful fixture consumers before further extraction.

## Add or revise a case

1. Give the case a stable identity and a useful name in the catalog. Keep
   deterministic fixture state, reset, and effect recording local to the host.
   Reset must close popups, clear focus, gestures, and events, and restore fixture
   defaults while preserving component identities, appearance, and available
   dimensions.
2. Call the production component with presentation data and configuration.
   Resolve labels, actions, shortcuts, and real availability from their existing
   authorities. Synthetic long labels and availability states are fixture stress
   inputs, not new production definitions.
3. Extend replay using actual pointer/key events, canonical bindings, and live
   control bounds. Assert meaningful intermediate states and outcomes before
   capture. Keep time explicit and virtual input/annotations paint-only.
4. Add focused regressions for the changed contract. Exercise relevant enabled,
   selected, disabled, hidden, nested, long-label, edge/corner, resize, and clipping
   states. Probe text entry, popups, focus loss, held-key release/cancellation,
   repeated egui layout passes, and independent instances when applicable.
   Set Replay::retry on the actual activation frame; replay clears it after each
   frame. Queue effects and deliver once per logical frame. Consume output during every
   UI pass; continuous timeline requests are ordered events and must not use
   equal-command deduplication. Resetting held state
   must not release input already claimed by the current cancellation frame.
5. Retain application regressions: isolated behavior does not establish editor
   focus handling, command availability, document effects, or history correctness.

Canonical guide tracing rejects duplicate Control identities. Single-instance
cases can reuse it; multiple-instance cases use Replay::target(index, name) and
the workbench's instance-qualified observations. Switch tracing with the case
instead of weakening guide validation. Native menus share egui's popup ownership;
independent instances need independent IDs/state, not simultaneous ownership of
one keyboard gesture.

## Run and validate

From the repository root:

    just workbench
    just cargo test workbench -- --test-threads=1
    just workbench evidence
    just verify

Review live fixtures using Light/Dark appearance, available dimensions, reset, and
the event/state inspector. Inspect generated stills at actual size and animation
timing/playback. Report interactive review separately from automated replay and
physical keyboard, trackpad, or screen-reader testing.

Evidence lives in ignored .cache/workbench/: themed case stills, a held-key
release animation, timeline scrub/navigation animation, and manifest.json
renderer/event receipts and scoped CPU navigation measurements. Each still is
replayed twice and must match exactly before writing. A missing native capture
adapter fails visibly.

Reuse documentation::capture::UiCapture, VirtualInput, annotations, framing, and
the bounded lossless animation encoder. UiCapture shares the guide compositor
without creating a SceneRenderer or scene texture. Keep internal artifacts
separate from public user-guide claims and baselines; do not add another capture
pipeline. If production output intentionally changes, use the existing
[docs-driven development workflow](../docs-driven-development/SKILL.md).
