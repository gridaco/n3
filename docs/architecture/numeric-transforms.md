# Numeric transforms

A viewport transform has separate concerns: its semantic tool, its axis
constraint, its current input method, and its document transaction. Numeric entry
is another way to preview a constrained transform within that transaction. It is
not a new command that commits after every character or every pointer release.

## Input ownership

Move, Rotate, and Scale use the same axis-lock state. X/Y/Z choose the matching
axis while a transform tool is active. After a valid
selection and constraint are present, numeric characters and Backspace belong to
viewport numeric input. Text fields, menus, and other focus owners keep their
own keyboard events. Without that numeric context, existing view numbers,
period-to-toggle-navigation, and contextual Delete remain available.

Native input can deliver a key event and a text event for one keystroke. The
shortcut resolver must consume that character once and suppress only its paired
text duplicate. It must preserve event order, including axis selection followed
by digits in one frame. This avoids both doubled values such as 9900 and lost
characters from text-only input sources. Hardware numpad identity remains
separate from the semantic character after numeric entry owns the event.

The UI exposes the buffer and its unit in one prominent, centered transform
feedback control. Larger text keeps the active amount easy to read without
looking toward the viewport toolbar. The overlay remains presentation-only: it
does not claim pointer input or alter the session's keyboard ownership.
Transient syntax such as a lone minus sign is unfinished input, not a request to
commit invalid geometry. Invalid or unrepresentable values cannot be accepted.
Backspace edits the same buffer. Numeric input has no separate hidden history.

## Transform semantics

- Move: signed centimeters on the selected world axis. Typed amounts bypass
  movement-grid quantization, including fractional values. The Properties display
  unit does not reinterpret this viewport syntax.
- Rotate: signed degrees around the selected world axis and the session's shared
  baseline pivot. A group rotates together around that pivot; its topology and
  relative arrangement are preserved.
- Scale: a positive multiplicative factor. A single object's axis is local,
  matching its handles; selected vertices use a world axis. Axis scaling multiple
  objects is rejected rather than approximated, because differently rotated
  transforms can require shear beyond the current transform representation.
  Existing uniform group scaling remains separate and supported.

The first numeric character retires any active pointer gesture. A delayed
pointer-up must not overwrite the numeric preview or create another history
entry. The amount is evaluated from the original transaction baseline, including
when a drag or keyboard nudge already contributed a preview. Typing 2 after
moving 0.4 means a total move of 2, not 2.4. Input changes rebuild the candidate
from that baseline, then use the same validation and publication path as other
transform previews.

Changing to a different axis re-evaluates the same buffer from the same baseline.
Toggling the active axis off clears the numeric buffer and restores the baseline;
the session remains explicit until accepted or cancelled. Enter or a viewport
double-click accepts one history entry; Escape discards the entire session.
A confirming key cannot cascade into an idle action such as entering vertex edit
mode in the same event. No-op sessions create no history entry.

## Scope and validation

The initial syntax is a signed decimal number, with positive values required for
Scale. There are no expressions, unit suffixes, percentages, or geometry targets.
Those can later extend the input parser without changing the transform or
transaction boundaries.

The executable [numeric transforms guide](../guide/numeric-transforms.md) drives
real key and text events through the UI, including paired native key/text input,
exact fractional movement with snapping enabled, signed rotation, scale factors,
Backspace, replacement of a pointer preview, confirmation, cancellation, and
undo/redo. Core tests cover selection transforms and rejected inputs; headless
replay does not claim verification of physical keyboard layouts or native IME
behavior.
