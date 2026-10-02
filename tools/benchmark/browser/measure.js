// Contributor harness: the feature-gated Rust host owns the actual workload.
import init, { start } from "./pkg/n3.js";
import { observeRun } from "./observations.mjs";

const canvas = document.querySelector("#editor");
const button = document.querySelector("#run");
const status = document.querySelector("#status");
const result = document.querySelector("#result");
const sleep = (milliseconds) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds));
let app;
let config;
let failure;
let loading;
const fail = (error) => {
  failure = String(error);
  console.error(`N3 measurement failed: ${failure}`);
  status.textContent = `Measurement failed: ${failure}`;
  button.disabled = true;
};
canvas.addEventListener("n3-error", (event) => fail(event.detail));

async function setup() {
  if (!navigator.gpu || !window.isSecureContext) {
    throw new Error("A WebGPU-capable browser on localhost is required");
  }
  const response = await fetch("./config.json", { cache: "no-store" });
  if (!response.ok)
    throw new Error("Run this page through python3 -m tools.benchmark.measure");
  config = await response.json();
  const ratio = window.devicePixelRatio;
  canvas.style.width = `${config.width / ratio}px`;
  canvas.style.height = `${config.height / ratio}px`;
  await init();
  await new Promise((resolve, reject) => {
    canvas.addEventListener("n3-ready", resolve, { once: true });
    canvas.addEventListener(
      "n3-error",
      (event) => reject(new Error(event.detail)),
      { once: true },
    );
    app = start(canvas);
  });
  app.resize(config.width / ratio, config.height / ratio);
  const fetchStart = performance.now();
  const source = await fetch("./input", { cache: "no-store" });
  if (!source.ok) throw new Error("Could not read the selected input");
  const bytes = new Uint8Array(await source.arrayBuffer());
  const fetchEnd = performance.now();
  app.load_file(config.input_name, bytes, false);
  loading = {
    selected_bytes_fetch_ms: fetchEnd - fetchStart,
    load_file_call_ms: performance.now() - fetchEnd,
  };
  canvas.focus();
  // Allow the asynchronous surface resize and imported scene to reach a frame.
  await sleep(250);
  if (failure) throw new Error(failure);
  if (canvas.width !== config.width || canvas.height !== config.height) {
    throw new Error(
      `Requested ${config.width}×${config.height} physical pixels, got ${canvas.width}×${canvas.height}`,
    );
  }
  button.disabled = false;
  status.textContent = `${config.input_name} · ${config.metadata.profile} · ${config.options.mode} · ${config.width}×${config.height} physical surface. Press Run and keep this tab focused.`;
}

button.addEventListener("click", async () => {
  button.disabled = true;
  const initialRatio = window.devicePixelRatio;
  const observation = observeRun({
    window,
    document,
    canvas,
    width: config.width,
    height: config.height,
  });
  try {
    if (document.visibilityState !== "visible" || !document.hasFocus()) {
      throw new Error(
        "Keep the browser page visible and focused before running",
      );
    }
    canvas.focus();
    const metadata = {
      ...config.metadata,
      loading,
      browser: {
        automation: window.n3MeasurementAutomation ?? null,
        user_agent: navigator.userAgent,
        device_pixel_ratio: initialRatio,
        physical_canvas: [canvas.width, canvas.height],
        css_canvas: [
          canvas.getBoundingClientRect().width,
          canvas.getBoundingClientRect().height,
        ],
        viewport_css: [window.innerWidth, window.innerHeight],
        visibility: document.visibilityState,
        cross_origin_isolated: window.crossOriginIsolated,
        report_poll_interval_ms: 250,
      },
    };
    status.textContent = `Measuring ${config.options.mode}… keep the page focused and avoid interacting with the canvas.`;
    app.measure_start(JSON.stringify(config.options), JSON.stringify(metadata));
    const deadline = performance.now() + config.timeout_seconds * 1000;
    let report;
    while (!report) {
      await sleep(250);
      if (failure) throw new Error(failure);
      if (performance.now() > deadline)
        throw new Error("Measurement timed out");
      observation.sample();
      report = JSON.parse(app.measure_result());
    }
    report.browser_observations = observation.finish();
    if (report.validity?.mode_contract_satisfied !== true) {
      throw new Error(
        "Measured paths did not satisfy the requested mode contract",
      );
    }
    result.textContent = JSON.stringify(report, null, 2);
    const response = await fetch("./report", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(report),
    });
    if (!response.ok)
      throw new Error(`Report was not saved (${response.status})`);
    window.n3MeasurementSaved?.();
    const comparable =
      report.browser_observations.comparable &&
      Object.values(report.validity).every((valid) => valid === true);
    status.textContent = comparable
      ? "Report saved. This sample does not establish a performance conclusion; repeat under controlled conditions."
      : "Report saved with comparison validity failures; inspect the report and exclude it from comparisons.";
  } catch (error) {
    fail(error);
  } finally {
    observation.stop();
  }
});

setup().catch(fail);
