// Optional contributor runner. It never connects to an existing browser/profile.
import { readFile } from "node:fs/promises";

import {
  clearExternalControl,
  parseOptions,
  recordedLaunchArguments,
} from "./policy.mjs";

// Ambient automation settings must not redirect this owned local browser.
clearExternalControl(process.env);
const { url, mode, width, height, scale, timeout } = parseOptions(
  process.argv.slice(2),
);

const installation = new URL(
  "../../../.cache/viewport-tools/node_modules/playwright/",
  import.meta.url,
);
const { version } = JSON.parse(
  await readFile(new URL("package.json", installation), "utf8"),
);
if (version !== "1.63.0") {
  throw new Error(
    "Run `just measure-browser-setup` to install the pinned Playwright version",
  );
}
const { chromium } = await import(new URL("index.mjs", installation).href);
let browser;
let browserServer;
let interrupted = false;
let rejectFailure;
const failure = new Promise((_, reject) => {
  rejectFailure = reject;
});
// The same failure promise is raced against setup and completion operations.
failure.catch(() => {});
const onSignal = (signal) => {
  interrupted = true;
  rejectFailure(new Error(`Browser measurement interrupted (${signal})`));
};
process.on("SIGTERM", onSignal);
process.on("SIGINT", onSignal);
const watchdog = setTimeout(() => {
  rejectFailure(new Error("Browser measurement timed out"));
}, timeout);

try {
  // Context DPR emulation alone does not change Chrome's device-pixel resize
  // observer on this host. winit consumes that observer; match the real scale.
  const launchArguments = [`--force-device-scale-factor=${scale}`];
  // Full Chromium/new headless; leave the sandbox and default GPU policy enabled.
  browserServer = await chromium.launchServer({
    channel: "chromium",
    headless: mode === "headless",
    chromiumSandbox: true,
    args: launchArguments,
    host: "127.0.0.1",
    timeout: Math.min(timeout, 30_000),
  });
  if (interrupted)
    throw new Error("Browser measurement interrupted during launch");
  // This endpoint belongs only to the fresh process launched above. Server
  // ownership also provides a public force-kill API if graceful shutdown hangs.
  // Race the complete owned session: setup calls can stall before navigation.
  // Any interruption must reach finally even when a Playwright await is pending.
  const collect = async () => {
    browser = await chromium.connect(browserServer.wsEndpoint(), {
      timeout: Math.min(timeout, 30_000),
    });
    const viewport = {
      width: Math.max(1280, Math.ceil(width / scale) + 24),
      height: Math.max(800, Math.ceil(height / scale) + 100),
    };
    const context = await browser.newContext({
      viewport,
      deviceScaleFactor: scale,
    });
    const page = await context.newPage();
    page.setDefaultTimeout(timeout);
    page.on("pageerror", (error) => rejectFailure(error));
    page.on("console", (message) => {
      if (
        message.type() === "error" &&
        message.text().startsWith("N3 measurement failed:")
      ) {
        rejectFailure(new Error(message.text()));
      }
    });
    const automation = {
      tool: "playwright",
      version,
      browser_version: browser.version(),
      mode,
      channel: "chromium",
      fresh_profile: true,
      chromium_sandbox: true,
      custom_launch_arguments: launchArguments,
      launch_arguments: recordedLaunchArguments(
        browserServer.process().spawnargs.slice(1),
      ),
      viewport_css: [viewport.width, viewport.height],
      device_scale_factor: scale,
      gpu_process: null,
      gpu_diagnostics_error: null,
    };
    // Collected once before loading/measuring; no tracing or CDP polling in the run.
    try {
      const cdp = await browser.newBrowserCDPSession();
      const { gpu } = await cdp.send("SystemInfo.getInfo");
      automation.gpu_process = {
        devices: gpu.devices,
        feature_status: gpu.featureStatus,
        gl_renderer: gpu.auxAttributes?.glRenderer ?? null,
        gl_vendor: gpu.auxAttributes?.glVendor ?? null,
        gl_version: gpu.auxAttributes?.glVersion ?? null,
        // This describes Chrome's GPU process, not necessarily wgpu's chosen adapter.
        software_renderer_detected:
          /swiftshader|llvmpipe|lavapipe|software/i.test(
            JSON.stringify([gpu.devices, gpu.auxAttributes?.glRenderer]),
          ),
      };
      await cdp.detach();
    } catch (error) {
      automation.gpu_diagnostics_error = String(error);
    }
    await page.addInitScript((metadata) => {
      window.n3MeasurementAutomation = metadata;
    }, automation);
    let resolveSaved;
    const saved = new Promise((resolve) => {
      resolveSaved = resolve;
    });
    // One completion notification after the server accepts the report. There is
    // no per-frame automation, trace, video, or screenshot while measuring.
    await page.exposeFunction("n3MeasurementSaved", () => {
      resolveSaved();
    });
    await page.goto(url.href, { timeout });
    await page.waitForFunction(
      () => document.querySelector("#run")?.disabled === false,
    );
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await saved;
    console.log(
      `Isolated ${mode} Chromium ${browser.version()} measurement finished.`,
    );
  };
  await Promise.race([collect(), failure]);
} catch (error) {
  console.error(String(error));
  process.exitCode = 1;
} finally {
  clearTimeout(watchdog);
  // Playwright owns the fresh process and temporary profile; close both on every
  // exit path, including the Python parent's timeout or keyboard interruption.
  if (browserServer) {
    let deadline;
    try {
      await Promise.race([
        browserServer.close(),
        new Promise((_, reject) => {
          deadline = setTimeout(
            () => reject(new Error("Browser shutdown timed out")),
            5_000,
          );
        }),
      ]);
    } catch (error) {
      console.error(String(error));
      process.exitCode = 1;
      await browserServer.kill();
    } finally {
      clearTimeout(deadline);
    }
  }
  process.removeListener("SIGTERM", onSignal);
  process.removeListener("SIGINT", onSignal);
}
