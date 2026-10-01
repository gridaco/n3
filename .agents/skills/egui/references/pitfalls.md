# N3 egui pitfalls

These are N3-specific checks learned while refining its Preferences and viewport appearance. Extend this file only when a real UI failure reveals a reusable lesson.

## Scrollable Preferences

`egui::Window` frame spacing and `egui::ScrollArea` content spacing affect different boundaries. If the scrollbar appears inside the control padding, inspect the window's inner margin before changing the scrollbar itself. Let the scroll area use the available content width and place control padding _inside_ its scrolling content. Keep any correction local to this window so other dialogs retain their intended margins. Inspect the top, a scrolled middle, and the bottom; the close and settings-file controls must remain reachable.

## Full-width panel dividers

A separator uses its parent UI's available width. A padded panel frame therefore
insets every separator too. For sections separated by edge-to-edge lines, use
zero panel inner margin and give each header or content section its own native
`Frame::NONE` padding; place the separator at the panel root, outside those
frames. N3's workspace panel helpers share this layout across Layers, Properties,
and the bottom tab panel. Keep root item spacing at zero and define spacing
inside sections so nested margins do not inflate the layout. Keep scrollbars
outside content padding. Avoid CSS-style negative margins or growing a line
beyond its clip rectangle to repair the underlying layout.

Check actual painted and clipped separator endpoints at default and resized
widths, including empty states. egui may nest painted lines in `Shape::Vec`;
inspecting only top-level `LineSegment` shapes misses them.
Native `Panel` reserves its resize-edge separator stroke in the outer margin;
section lines should meet that border, not paint over it. Distinguish this
border width from content padding when checking panel bounds.

## Theme changes

N3's `System` preference resolves against the native window appearance; explicit Light and Dark override that resolution. Keep the persisted preference distinct from the resolved theme, and ensure an OS appearance event refreshes both egui style and viewport background. Start from `egui::Visuals::light()` and `dark()`, then change only N3's small shared palette, spacing, typography, borders, and radii. Use the active palette for neutral HUD and popup colors. Error feedback and X/Y/Z axes are semantic, so a neutral theme or user accent should not recolor them indiscriminately.

## Viewport overlays

N3 mixes a wgpu scene with egui panels, `Area`s, and painter layers. A shape painted on the background layer may be covered by a panel or later layer; a foreground area can affect hit testing even if it looks transparent. Inspect layer order, clip rectangles, and pointer routing before changing an overlay's order. Check the same overlay against both light and dark viewport backgrounds and while a menu or Preferences window is open. Keep documentation cursors and annotations visual-only.

## Button hover geometry

In egui 0.36, a `Button` is measured with its frame: `button_style` derives inner margin from button padding, expansion, and stroke width. Changing the stroke between idle and hover can therefore change the widget's measured size and shift neighboring controls. There is no CSS `box-sizing` switch. For fixed toolbars, keep stroke width constant across widget states or use fill-only hover feedback, then check both the button rectangles and their containing toolbar rectangle while hovering each item. `Button::min_size` sets a minimum, not an exact size, and its text may remain left-aligned within the extra space. For an icon-only square button, check the painted glyph as well as the response rectangle; center the glyph explicitly when needed. `Button::fill` and `Button::stroke` override their respective hover effects; set state-specific `WidgetVisuals` when the control needs a different hover fill.

`Button::selectable(false, ...)` and `frame_when_inactive(false)` omit the idle
frame while retaining its derived padding. A constant nonzero stroke can still
produce a size jump when the hover frame appears. For a fill-only tab or ghost
button, set `bg_stroke = Stroke::NONE` and `expansion = 0.0` in every widget state,
including `noninteractive`, inside the owning UI scope. Keep the native frame
enabled so hover and keyboard-focus feedback remain available.

Avoid `.small()` on an icon button that shares a row with normal buttons: it
removes vertical padding and the normal minimum interaction height. N3's panel
header uses a centered, fixed-size icon atom (`AtomExt::atom_size`) with matching
padding to align its native close button with the tab. Test idle, hover, press,
and disabled geometry in both themes; an idle screenshot misses the hover shift.

## Executable guide captures

The app defaults to `System`, but guide pixels must not depend on the capture host's current appearance. Set an explicit theme in each theme-specific scenario and review both outputs. Control tracing can record a rectangle for a widget below the visible scroll region; scroll the real UI before interacting with or illustrating it, then check the resulting image. A successful replay or byte match does not establish legibility, scrollbar placement, or reachable controls.

Menu controls can enter the trace before their fade-in is visible. `Session::settle` resolves layout without advancing time; it cannot finish that animation. Advance the explicit scenario clock with `Session::wait` before capturing an opened menu, and inspect the resulting image for full opacity. Do not use wall-clock sleeps or disable application animations just for the guide.

## Live values in sliders

A slider showing an externally advancing value, such as animation time, must not
turn passive layout into a user action. In egui 0.36, the default
`SliderClamping::Always` calls its value setter during layout; together with
`fixed_decimals`, this rounds the live value and can report `changed()` without
input. N3's playback slider once dispatched Seek from that synthetic change and
paused itself. Use `SliderClamping::Edits` for an already-validated live value,
keep range validation in the model, and test several advancing frames plus
layout retries. Display precision must not change the authoritative clock.

## Circle and arc stroke alignment

In egui 0.36, circle tessellation applies an outside stroke, while an open path
uses a centered stroke by default. Giving a circle and an arc the same radius
and width therefore does not align their visible bands. For a ring with an arc
highlight, draw both as paths with the same centerline radius and stroke kind.

## Menu keyboard focus

In egui 0.36, mouse-opened Popup/MenuButton and ComboBox contents do not
automatically focus a row. Generic spatial arrows need existing focus and can
leave the menu; native SubMenu has no Left/Right navigation and its hover
heuristics can override a keyboard choice under a stationary pointer. N3's
shared `ui::menu` layer supplies row-order focus, submenu entry/exit, and focus
restoration around native Button/SelectableLabel/Popup widgets. Use its action
items or typed value dropdown helpers instead of adding per-menu workarounds.

Do not focus rows during an invisible sizing pass (their disabled widgets
surrender focus). Escape still needs handling during that pass so only the
intended menu level closes. Consuming an arrow event is insufficient: cancel
egui's already queued spatial move with `Memory::move_focus(None)`. Deduplicate
navigation across layout retries, including an Escape that removes a child menu.
Keep keyboard tests for mouse-opened menus, disabled rows, stationary-pointer
submenu switching, and outside clicks into text fields.

## Continuous feedback and layout retries

`Context::request_discard` reruns the whole UI immediately. Reserve it for rare
layout measurement; using it after every scrub/slider update pays for two UI
passes per frame and triggers egui's red sustained-multipass warning. N3's
animation integration exposed this mistake. The timeline now prepares layout
and input, lets the host accept or reject requests, then paints the accepted
playhead in the same pass. Native transport widgets keep their ordinary edit
buffers and next-frame refresh. `request_repaint` schedules normal redraws.

Test continuous raw gesture frames without idle settling between them. Verify
accepted feedback, once-only effects, and one settled pass per motion frame;
a settled screenshot can hide this defect. Keep diagnostics enabled. The shared
capture compositor rejects this warning rather than publishing it into guides.
