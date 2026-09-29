---
name: ux-design-hotkeys
description: Review, ideate, and refine N3 keyboard interactions through user intent, context ownership, ergonomics, and learnability. Use when proposing shortcuts, resolving competing meanings, comparing other design tools, or reviewing tap, hold, modifier, and key-plus-pointer behavior. A design framework, not a replacement keymap.
---

# N3 hotkey UX design

Design the interaction before choosing the key. A memorable shortcut connects a
user's intention to a predictable action in a context they can recognize.
“Smart” means resolving explicit state well, not guessing intent from hidden
heuristics. Optimize a small, coherent vocabulary instead of assigning every
available key.

Follow [AGENTS.md](../../../AGENTS.md). Respect the user's current decisions and
the requested scope: a review produces recommendations; an authorized binding
change implements that change. Do not turn an ideation request into a keymap
rewrite, add speculative aliases, or create a separate design document unless
requested. The binding catalog remains the source of truth.

## Start with intent and context

Read the relevant bindings and their consumers before declaring a key free or
conflicted. A key alone is not the interaction: distinguish its modifiers,
physical identity, press/release lifecycle, input owner, and eligible state.
Trace semantic actions as well as key matches; an alias may occupy a candidate
key, and a focused widget may handle it without an application binding.

For a nontrivial proposal, sketch only the contexts that can change its meaning:

| Context                                    | Intended action                         | Trigger and ownership                                       | Feedback and finish                              |
| ------------------------------------------ | --------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------ |
| Relevant tool, mode, selection, or session | Semantic operation or intentional no-op | Press, tap, hold, repeat, or modified drag; who receives it | Visible state, release, confirm, cancel, or undo |

Consider text entry, numeric transform input, menus, held navigation, and active
gestures where relevant. “A transform tool is selected,” “an axis is armed,” and
“a transform session is changing geometry” are different contexts. Test empty
selection explicitly; an unavailable action must not silently turn into another
meaning unless that fallback is intentional and understandable.

For modified drags, include the pointer target at gesture start: object, transform
handle, empty viewport, or UI. Define any intentional ownership change on modifier
release; crossing another object or UI region must not silently reinterpret an
ongoing gesture.

Shared keys are useful when contexts are mutually exclusive and visible. For
example, N3 currently reserves Z for axis locking in transform tools and uses it
for the shading pie in Cursor. The distinction is the selected tool, not whether
a transform has already moved anything. Recheck the live implementation before
using this example in a future proposal. If users cannot tell which action will
win, reconsider the mapping or improve the mode feedback.

## Compare candidates as a system

Balance these criteria; no single rule, including first-letter matching, always
wins. State which tradeoff matters most for the particular action.

| Criterion              | Questions that change the decision                                                                                                                                                                                   |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Meaning and recall     | Does the letter, symbol, or direction suggest the action? Does that association survive the user's vocabulary and language? Is familiarity stronger than an invented mnemonic?                                       |
| Reach and frequency    | Can the keyboard hand reach it while the other hand stays on the mouse or trackpad? How far is the key from the intended modifier? Does a frequent chord require stretching, hand travel, or a second keyboard hand? |
| Families and sequences | Do related tools form a learnable cluster? Are opposite or related operations consistent? Can the user move from tool choice to axis, numeric entry, and confirmation comfortably?                                   |
| Existing habits        | What learned behavior would change? Is a convention shared across relevant apps, or specific to a particular keymap? Does an alias help enough to justify consuming another key?                                     |
| Ownership and mistakes | What else receives this key in the same state? Can an accidental press change geometry? Are cancellation, repeat, and release predictable? Can the user recover without losing work?                                 |
| Discovery and access   | Can users find the action and its shortcut in appropriate UI/help? Is there an existing pointer route? Does the proposal rely on a numpad, awkward simultaneous holds, a dead key, or an OS-reserved chord?          |

Command-Z/X/C/V illustrate a compact modifier neighborhood on a typical QWERTY
Mac keyboard; they do not prove that every Command shortcut should use that row.
Likewise, N3's Q/W/E/R tool neighborhood illustrates spatial grouping, while
the choice of R for Rotate illustrates a competing mnemonic. Preserve the live
mapping rather than assuming another application's ordering.

Describe the keyboard/layout and hand assumptions behind an ergonomic claim.
Distinguish logical characters from physical positions, top-row digits from
numpad digits, and macOS Command/Option from other platforms' modifiers. Consider
non-US layouts and text composition for affected keys without inventing a full
cross-platform remapping system. Code can verify routing; actual comfort and
native key delivery need human testing.

## Ask humans about the real tradeoff

Investigate the current behavior first. Ask when a material ambiguity remains:
two useful meanings compete, a learned binding would be displaced, the relevant
context is unclear, or ergonomics depend on the user's hardware or habits.
Avoid asking them to select from an unexplained list of free keys.

Describe a concrete sequence and its competing outcomes. Offer a recommendation
and two or three distinct choices with their costs. For example: “With Move
selected but no drag started, should this key lock an axis or open a menu?” is
more useful than “Should this shortcut be context-aware?” A short context table
can make a complicated choice reviewable.

Ask only what changes the design, preferably one focused question at a time.
Use existing answers and explicit instructions; do not reopen a settled choice
or request approval for routine wiring. Continue independent investigation while
an answer is pending. An unanswered required design choice is not approval.
For a minor reversible detail, state a reasonable assumption when appropriate.

Push back with a specific consequence, not taste: extra modifier reach, an
invisible mode dependency, delayed response, accidental edits, or broken muscle
memory. In particular, tap-versus-hold multiplexing is not a free way to solve a
collision. Decide when the tap fires, whether a hold requires a delay, what
release does, and how cancellation prevents both actions from firing. Do not
introduce a timing threshold merely to preserve two desirable assignments.

## Research comparable workflows

When research is part of the task, compare the same user intention and sequence
in relevant 3D and 2D tools, including Blender and Grida. Add Figma, Unity, or
another tool when its workflow answers the actual question. Do not copy a
shortcut just because the app is familiar.

Use current official manuals, shortcut references, first-party source, or direct
app observation. Record the version/keymap preset, OS, relevant mode, trigger
phase, and source URL or observation method. Separate verified behavior from
inference and untested ergonomic judgment; mark missing evidence rather than
presenting remembered defaults as fact. A static shortcut list may omit modal
behavior or alternate keymaps.

Compare applicability to N3: a modal transform command may justify a different
axis key policy than a persistent tool; a 2D canvas gesture may conflict with 3D
orbit. Favor a small comparison of plausible choices and explain the selected
tradeoff. Keep proposed changes distinct from accepted decisions. Research notes
can stay in the task response unless a durable document is requested.

## Ground changes in N3's implementation and guides

Use these entry points, then follow the relevant code rather than auditing every
module for a small change:

- [bindings.rs](../../../src/input/bindings.rs): semantic binding IDs, primary keys,
  aliases, modifiers, and trigger kinds; this owns displayed shortcut labels.
- [shortcuts.rs](../../../src/input/shortcuts.rs): keyboard ownership, context,
  ordered command resolution, numeric entry, and tap handling.
- [pie_input.rs](../../../src/input/pie_input.rs) and
  [temporary_navigation.rs](../../../src/input/temporary_navigation.rs): shared
  hold/release ownership and temporary pointer behavior.
- [keyboard_input.rs](../../../src/input/keyboard_input.rs): physical number-key
  identity; inspect native event routing when platform delivery matters.
- [workspace_ui.rs](../../../src/ui/workspace_ui.rs) and
  [editor](../../../src/editor/mod.rs): semantic dispatch, tool/session state,
  and commit/cancel/history effects.

Keep the action separate from its binding. Reuse existing routing and ownership;
do not add independent native, UI, and documentation interpretations. An alias
invokes the same semantic action, while guides retain the intended primary key.

For implementation work, use the
[docs-driven development skill](../docs-driven-development/SKILL.md). Describe
the user sequence through semantic shortcut bindings, not copied key strings.
Cover the relevant successful action and competing context: text/popups, empty
selection, tool changes in one event batch, modifiers, repeats, cancellation,
focus loss, and releases after ownership changes as applicable. Assert that
unrelated geometry, camera, selection, and history remain unchanged. A duplicate
key scan alone cannot validate contextual behavior.

Report the chosen mapping and rationale, what it displaces or preserves, the
evidence checked, and any remaining human trial. A skill-only edit needs content,
link, and skill validation; unchanged application output does not need guide
regeneration. Refine this framework from observed usability problems without
turning every one-off decision into a universal rule.
