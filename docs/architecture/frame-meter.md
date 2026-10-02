# Application frame meter

The shared **View → Developer → Show FPS Meter** action enables a transient,
off-by-default observation of host frame cadence. Native and browser use the
same arithmetic and overlay. The native host retains its presentation policy;
the browser retains its event-loop scheduling. This is a small live diagnostic,
separate from the repeatable [viewport measurement harness](viewport-measurement.md).

## Measurement contract

Each host records one monotonic timestamp after a successful application frame
has been submitted and its surface presentation requested. Failed acquisition,
skipped rendering, and egui layout retries do not count as completed frames. The
first timestamp establishes an anchor; subsequent timestamps define intervals.
For N intervals lasting T seconds, the sample is N/T FPS and 1000T/N ms per frame.
It is not an average of instantaneous reciprocal frame times.

A sample publishes on a successful frame once its observation span reaches at
least 500 ms. Actual elapsed time is the denominator, including scheduling gaps,
surface-acquisition waits, idle time, and stalls. There is no assumed refresh
rate, fabricated sample at startup, clamping of long intervals, or catch-up timer.
Toggle-off discards observations so re-enabling starts a fresh measurement.

These timestamps establish application submission cadence. They do not establish
GPU completion, display refresh, compositor presentation, dropped display frames,
or input-to-display latency. Host/browser throttling can reduce the observed
cadence; a low idle rate is not evidence of slow scene rendering. The visible
label is **App FPS · last sample**, with a waiting state before the first sample.

## Execution and UI boundaries

The meter never requests a redraw or installs a timer. An idle window retains
its last painted sample until ordinary work causes another frame. The host
records after painting, so a newly published sample becomes visible on a later
ordinary frame. Shared measurement state accepts elapsed monotonic timestamps;
it does not own native or browser clock APIs.

Collection uses bounded state and constant work per observed frame. It introduces
no GPU query, readback, fence, wait, per-frame allocation, or sorting.
Enabling any overlay has some CPU and draw cost; this contract promises a small,
bounded observation path, not mathematically zero overhead. Disabling the meter
avoids collection, per-frame clock reads, and its overlay. Display text is
formatted only when its sample changes.

The overlay deliberately uses a plain black rectangle and monospace white/gray
text in either theme. It is anchored to the bottom-right corner of the application
window, independent of the viewport rectangle, panels, and other overlays.
Its paint-only geometry has no hit target or focus owner.
Hiding the UI hides the overlay while leaving collection enabled.
The toggle is not a document edit or persistent preference and has no shortcut.
Use the ordinary semantic action and menu presenter, not a second UI-specific
toggle path.

## Verification boundaries

Pure recorder tests use explicit timestamps to verify the denominator, initial
anchor, publication interval, variable cadence, long gaps, and toggle resets.
Host wiring must count only successful surface presentation submissions and must
not introduce extra wakeups. UI tests cover paint-only input behavior and normal
menu dispatch.

For an overhead comparison, use repeated runs of the same build and scene with
the real toggle off/on, record that state, and keep frame dimensions and workload
identical. Include a stationary forced-rendering case as well as orbit: expensive
projection can mask a small overlay cost. Compare run-level cadence and CPU
distributions, retaining tails and background-load limitations. A browser clock
read, a rendered overlay, and an extra scheduling loop are different costs;
the meter adds only the first two. The measurement harness records actual meter
state so unlike workloads cannot silently become repeats of one case.

The [executable user guide](../guide/fps-meter.md) exercises the actual menu,
waiting overlay, Hide UI, and unchanged document, selection, settings, and undo
history. Its replay clock is deliberately not passed off as a host presentation
clock: the screenshot shows a waiting meter, never invented numeric performance.
Runtime native/browser observation is required to establish actual host cadence;
the guide is UI evidence, not a performance result.
