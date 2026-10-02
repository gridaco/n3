import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { copyFile, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

// Keep the real runner CLI and ownership path. Only Playwright is replaced;
// an owned keepalive simulates Chromium remaining alive during a stalled API.
const playwright = `
let keepalive;
let saved;
async function stage(name) {
  if (name === process.env.N3_TEST_STALL_STAGE) {
    console.log("STALLED " + name);
    await new Promise(() => {});
  }
}
const page = {
  setDefaultTimeout() {}, on() {},
  async addInitScript() { await stage("addInitScript"); },
  async exposeFunction(_name, callback) {
    await stage("exposeFunction"); saved = callback;
  },
  async goto() {}, async waitForFunction() {},
  getByRole() { return { async click() { saved(); } }; },
};
const browser = {
  version() { return "test Chromium"; },
  async newContext() {
    await stage("newContext");
    return { async newPage() { await stage("newPage"); return page; } };
  },
  async newBrowserCDPSession() {
    await stage("cdpSession");
    return {
      async send() {
        await stage("cdpSend");
        return { gpu: { devices: [], featureStatus: {} } };
      },
      async detach() { await stage("cdpDetach"); },
    };
  },
};
export const chromium = {
  async launchServer() {
    keepalive = setInterval(() => {}, 1000);
    return {
      wsEndpoint() { return "ws://127.0.0.1/owned"; },
      process() { return { spawnargs: ["test Chromium"] }; },
      async close() { console.log("OWNED_CLOSED"); clearInterval(keepalive); },
      async kill() { console.log("OWNED_KILLED"); clearInterval(keepalive); },
    };
  },
  async connect() { return browser; },
};
`;

async function runnerFixture(t) {
  const root = await mkdtemp(join(tmpdir(), "n3-browser-lifecycle-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const runnerDirectory = join(root, "tools/benchmark/browser");
  const installation = join(
    root,
    ".cache/viewport-tools/node_modules/playwright",
  );
  await mkdir(runnerDirectory, { recursive: true });
  await mkdir(installation, { recursive: true });
  for (const name of ["runner.mjs", "policy.mjs"]) {
    await copyFile(
      new URL(`../browser/${name}`, import.meta.url),
      join(runnerDirectory, name),
    );
  }
  await writeFile(
    join(installation, "package.json"),
    JSON.stringify({ version: "1.63.0" }),
  );
  await writeFile(join(installation, "index.mjs"), playwright);
  return join(runnerDirectory, "runner.mjs");
}

async function bounded(promise, milliseconds) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("Runner did not close promptly")),
          milliseconds,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

function run(t, runner, phase, timeout = 30) {
  const child = spawn(
    process.execPath,
    [
      runner,
      "http://127.0.0.1:1234/token/measure.html",
      "headless",
      "1280",
      "800",
      "2",
      String(timeout),
    ],
    {
      env: { ...process.env, N3_TEST_STALL_STAGE: phase },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  let stdout = "";
  let stderr = "";
  const closed = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", (code, signal) => resolve({ code, signal }));
  });
  const stalled = new Promise((resolve, reject) => {
    child.stdout.on("data", (data) => {
      stdout += data;
      if (stdout.includes(`STALLED ${phase}`)) resolve();
    });
    closed.then(
      () =>
        reject(new Error(`Runner exited before the setup stall: ${stderr}`)),
      reject,
    );
  });
  child.stderr.on("data", (data) => {
    stderr += data;
  });
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill("SIGKILL");
      await closed;
    }
  });
  return { child, closed, stalled, output: () => ({ stdout, stderr }) };
}

test("SIGTERM closes the owned browser during every setup await", async (t) => {
  const runner = await runnerFixture(t);
  for (const phase of [
    "newContext",
    "newPage",
    "cdpSession",
    "cdpSend",
    "cdpDetach",
    "addInitScript",
    "exposeFunction",
  ]) {
    await t.test(phase, async (t) => {
      const session = run(t, runner, phase);
      await bounded(session.stalled, 4000);
      session.child.kill("SIGTERM");
      assert.deepEqual(await bounded(session.closed, 4000), {
        code: 1,
        signal: null,
      });
      assert.match(session.output().stdout, /OWNED_CLOSED/);
      assert.doesNotMatch(session.output().stdout, /OWNED_KILLED/);
      assert.match(session.output().stderr, /Browser measurement interrupted/);
    });
  }
});

test("watchdog closes the owned browser when setup never resolves", async (t) => {
  const runner = await runnerFixture(t);
  const session = run(t, runner, "newContext", 1);
  await bounded(session.stalled, 4000);
  assert.deepEqual(await bounded(session.closed, 4000), {
    code: 1,
    signal: null,
  });
  assert.match(session.output().stdout, /OWNED_CLOSED/);
  assert.match(session.output().stderr, /Browser measurement timed out/);
});
