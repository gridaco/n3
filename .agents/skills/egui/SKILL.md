---
name: egui
description: Apply N3-specific egui layout, theme, icon, overlay, and guide-capture practices when editing its Rust UI. Not a general egui reference.
---

# N3 egui UI

Use this skill for changes to N3's egui panels, Preferences, theme, icons, or viewport overlays. Inspect the owning UI code, `src/theme.rs`, and the relevant executable guide scenario before editing.

For reusable component development, fixture cases, input isolation, or internal
visual evidence, use the [UI workbench skill](../ui-workbench/SKILL.md).

## Interpreting HTML/CSS references

Treat HTML/CSS examples and vocabulary as descriptions of visual intent: density,
hierarchy, alignment, spacing, contrast, and interaction feedback. Translate that
intent into egui's native layout and styling APIs (`Ui`, `Layout`, `Frame`,
`Spacing`, and `WidgetVisuals`) and N3's existing theme values. For example, `sm`
or `xs` means compact, readable typography; it does not require copying a CSS
utility's exact pixel value. Do not introduce CSS abstractions or recreate browser
component behavior to match an analogy.

Check the pinned egui APIs and existing code before deciding an effect is costly.
If it needs substantial custom painting, shaders, repeated layout workarounds, or
widget replacement, flag that specific mismatch early and explain the cost.
Prefer a simple native equivalent, or defer the effect while completing the
feasible styling. For example, a blurred CSS text shadow should not trigger a
custom text renderer; improve contrast or use a supported frame treatment where
appropriate. Native frame shadows are a separate supported feature. Unless the
user explicitly requests the custom implementation, keep the native approach;
an analogy alone is not a reason to stop the whole task or ask for approval.

Explain material caveats when translating the intent: frames and strokes can
affect layout size, clipping differs from browser overflow, and overlay layers
can affect pointer ownership. Preserve focus, hover, selected, disabled, and
accessible interaction states when simplifying the appearance. Consult the
[observed pitfalls](references/pitfalls.md) for the concrete egui behavior rather
than assuming CSS box-model rules apply.

## Implementation practices

- Start with egui's built-in widgets and light/dark styles. Keep the small shared overrides in `src/theme.rs`; avoid a parallel component or CSS-like system. Apply style changes when the resolved theme or accent changes, rather than rebuilding them every frame.
- Keep layout geometry distinct: a scrollable window's outer frame and scrollbar should reach the window content edge, while spacing around controls belongs inside the scroll area. Do not globally alter window margins to repair one dialog.
- Derive neutral overlay text and surfaces from the active theme. Preserve semantic error, selection, and X/Y/Z axis colors where their identity matters.
- Check egui layer order, clipping, and pointer ownership when changing viewport overlays. A readable overlay must not hide a menu or intercept an unrelated viewport gesture.
- Keep the bundled Lucide font in its dedicated named egui family and its codepoints in `src/ui/lucide.rs`. Icon-only controls still need semantic labels and shortcut tooltips; inspect the resulting guide captures for legibility after changing icons.
- Author action menus through `src/ui/menu.rs`: `content`, `submenu`, `separator`, and `WorkspaceUi::menu_action` with an `ActionId`. Labels and shortcut hints come from the action catalog and binding table; availability is queried from current workspace state. Keep adjacent rows gapless. The shared family supplies the Lucide ChevronRight, minimum width, and full-width dividers. See [actions and menus](../../../docs/architecture/actions-and-menus.md) for the authoring contract.
- For generated guide images, choose Light or Dark explicitly. Exercise real scroll/input routing and verify that illustrated controls are visible and reachable; a recorded control rectangle alone does not prove that.

Read [observed pitfalls](references/pitfalls.md) when adjusting scrollable dialogs, overlays, or executable UI documentation. Follow the repository's `AGENTS.md` for documentation generation and verification.
