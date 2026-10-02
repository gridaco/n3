# Platform boundaries and performance

This is the engineering contract for N3's native and browser hosts.

> Share product semantics. Let each host own execution and realize its
> capabilities fully.

Native remains the primary target. Browser support must preserve native
capabilities, rendering quality, and performance policy. Each supported host
should use the execution model and capabilities appropriate to its environment;
the browser's restrictions do not establish a common performance ceiling.

## Shared behavior and host ownership

The same operation must retain its meaning across input sources and hosts.
Document validation, editing transactions, history, units, selection rules, and
navigation policy stay shared. An adapter translates host input or performs a
host effect; it does not create an independent editing path.

| Concern     | Shared responsibility                                             | Host responsibility                                                          |
| ----------- | ----------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Editing     | Document semantics, commands, sessions, validation, and history   | Deliver input and execute requested external effects                         |
| Navigation  | Camera behavior, sensitivity, and input ownership                 | Translate device events, coordinates, units, and available gesture phases    |
| Rendering   | Scene evaluation, revision caches, uploads, and frame composition | Adapter/device selection, GPU limits, surfaces, recovery, and presentation   |
| Time        | Camera transitions and playback behavior                          | Monotonic or explicit replay clocks, frame pacing, and event-loop wakeups    |
| Assets      | Format decoding, unit conversion, and resource accounting         | File access, selected bytes, resource resolution, and save completion        |
| Preferences | Typed values, validation, and merge semantics                     | Storage implementation, synchronization triggers, and persistence guarantees |

For example, DOM pinch events enter the shared navigation router through the
[browser gesture adapter](../../src/web/gestures.rs). The adapter handles browser
event delivery and duplicate signals; viewport ownership and camera behavior
remain shared. Missing gesture phase information is an explicit host limitation,
not a reason to discard native phase or momentum handling.

## Focused implementation boundaries

Select target implementations at composition and module boundaries. Keep `cfg`
out of ordinary editing rules and feature UI. Prefer a capability value when the
same UI needs to express whether an operation is available.

Extract an adapter around an actual environmental difference, and share code
when its behavior and ownership agree. Keep one application crate until a real
consumer justifies a separate API. A port does not require a universal host trait,
a public modeling kernel, or identical implementations of every service.

Current boundaries illustrate this policy:

- [`asset_io/native/`](../../src/asset_io/native/mod.rs) owns filesystem operations;
  format parsers and retained-resource accounting remain portable.
- [`WorkspaceRenderer`](../../src/render/workspace.rs) shares candidate preparation,
  cache synchronization, rollback, scene rendering, and egui composition. Hosts
  supply their actual device and target and retain presentation policy.
- [Terminal implementations](../../src/terminal/mod.rs) and
  [guide inspection hooks](../../src/ui/controls.rs) are selected as modules.
- The [JavaScript wrapper](../../web/n3.js) forwards semantic commands and exposes
  application state. An embedding UX must reuse the editor's transactions and
  history. The existing Editor is still internal and depends on egui.

## Performance, clocks, and scheduling

Preserve each host's resource limits, cache behavior, concurrency options, and
presentation choices when extracting shared code. A browser limit must not become
a hard-coded limit in a shared renderer merely to make the port compile. Shared
rendering uses the device's enabled capabilities and retains revision-based work:
selection changes must not cause unnecessary geometry evaluation or uploads.

Hosts acquire monotonic time and decide when to wake or present. Shared behavior
consumes elapsed time or frame input and can request repaint deadlines; hosts
schedule their delivery. Deterministic guide replay supplies an explicit clock.
Do not spread host clock calls through editing semantics or make native scheduling
inherit browser timer constraints. The native host currently uses
`std::time::Instant`; the browser uses `web_time::Instant`.

Storage scheduling follows the same ownership rule. Browser preferences react to
local changes, storage events, and focus recovery, with timers for failed-attempt
retries. Native preferences retain filesystem polling. Both wait for safe input
boundaries before merging. Storage guarantees may differ and must be stated
accurately; sharing a controller does not make browser writes atomic across tabs.

Measure representative workloads before claiming a performance improvement.
Compilation and architectural similarity establish neither frame-time parity nor
the best achievable performance. Report the host, workload, and measured effect
alongside any remaining limits.

## One product experience

Keep one feature UI, semantic action catalog, and user guide. Each feature owns
one narrative, scenario, and generated page across hosts. Adapt availability by
hiding or disabling unsupported controls in that shared UI; preserve familiar
menu organization and supported behavior.

Presentation can use host capabilities without changing feature semantics. For
example, a future native Preferences window could be a real OS window while the
browser retains the existing egui window. Native multi-window delivery remains
future work; this example does not declare it implemented.

Keep build, embedding, and host limitations in the
[development guide](../development.md#browser-build). This contract owns the
engineering policy; user-facing guides explain the shared features without
linking back to engineering documents.

## Verification and review

Preserve the native verification path and existing guide baseline. A browser
failure must not be resolved by weakening native checks, lowering rendering
quality, skipping artifacts, or creating a separate browser edition of the guide.
Any intentional native baseline change needs its own reason and review.

Use shared behavioral regressions and native `just verify` for implementation
changes, plus target compilation and host integration checks for the port.
Separate the evidence: compilation proves build compatibility; runtime checks
exercise event delivery, rendering, storage, and lifecycle; physical-device tests
establish gesture recognition and feel; measurements support performance claims.
Browser checks supplement the shared guide's executable contract. They do not
replace its exact artifact checks or prove untested browsers and devices.

Review a boundary change with these questions:

1. Who owns the behavior, and do all entry points reuse its semantics?
2. Does a restriction belong to one host, and is it confined to that adapter?
3. Does the abstraction preserve each host's capabilities, scheduling, and caches?
4. Does the shared UI express availability without splitting the feature or guide?
5. What verifies native behavior, the affected host, and any performance claim?

The [architecture overview](architecture.md) maps the modules, and
[development](../development.md) owns the runnable verification commands.
