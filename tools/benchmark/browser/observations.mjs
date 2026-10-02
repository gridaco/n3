// Observe the run's environment without driving camera input or rendering.
export function observeRun({ window, document, canvas, width, height }) {
  const observations = {
    visibility_changes: 0,
    focus_losses: 0,
    size_changed: false,
  };
  const initialRatio = window.devicePixelRatio;
  const abort = new AbortController();
  document.addEventListener(
    "visibilitychange",
    () => {
      observations.visibility_changes += 1;
    },
    { signal: abort.signal },
  );
  window.addEventListener(
    "blur",
    () => {
      observations.focus_losses += 1;
    },
    { signal: abort.signal },
  );
  const sample = () => {
    observations.size_changed ||=
      canvas.width !== width ||
      canvas.height !== height ||
      window.devicePixelRatio !== initialRatio;
  };
  return {
    sample,
    finish() {
      sample();
      abort.abort();
      return {
        ...observations,
        comparable:
          observations.visibility_changes === 0 &&
          observations.focus_losses === 0 &&
          !observations.size_changed,
      };
    },
    stop() {
      abort.abort();
    },
  };
}
