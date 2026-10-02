import assert from "node:assert/strict";
import test from "node:test";
import {
  clearExternalControl,
  parseOptions,
  recordedLaunchArguments,
} from "../browser/policy.mjs";
import { observeRun } from "../browser/observations.mjs";

test("runner admits only explicit isolated local measurements and bounded dimensions", () => {
  const valid = [
    "http://127.0.0.1:1234/token/measure.html",
    "headed",
    "1280",
    "800",
    "2",
    "600",
  ];
  const result = parseOptions(valid);
  assert.equal(result.timeout, 600000);
  assert.equal(result.width, 1280);
  for (const [index, value] of [
    [0, "https://127.0.0.1/page"],
    [0, "http://example.com/page"],
    [0, "http://user:password@127.0.0.1/page"],
    [1, "manual"],
    [2, "63"],
    [2, "8193"],
    [2, "1280.5"],
    [3, "NaN"],
    [4, "0"],
    [4, "Infinity"],
    [4, "4.1"],
    [5, "7201"],
  ]) {
    const arguments_ = [...valid];
    arguments_[index] = value;
    assert.throws(() => parseOptions(arguments_));
  }
  assert.throws(() => parseOptions(valid.slice(1)));
});

test("ambient remote or debugger aliases cannot redirect the owned browser", () => {
  const environment = {
    PATH: "preserved",
    SELENIUM_REMOTE_URL: "remote",
    PWDEBUG: "1",
    npm_config_selenium_remote_url: "remote",
    npm_package_config_selenium_remote_url: "remote",
    npm_config_pwdebug: "1",
    npm_package_config_pwdebug: "1",
  };
  clearExternalControl(environment);
  assert.deepEqual(environment, { PATH: "preserved" });
  assert.deepEqual(
    recordedLaunchArguments([
      "--headless",
      "--user-data-dir=/private/profile",
      "--force-device-scale-factor=2",
    ]),
    [
      "--headless",
      "--user-data-dir=[temporary-profile]",
      "--force-device-scale-factor=2",
    ],
  );
});

function environment() {
  return {
    window: Object.assign(new EventTarget(), { devicePixelRatio: 2 }),
    document: new EventTarget(),
    canvas: { width: 1280, height: 800 },
    width: 1280,
    height: 800,
  };
}

test("focus, visibility, and transient size changes cannot be hidden by recovery", () => {
  const env = environment();
  const run = observeRun(env);
  env.window.dispatchEvent(new Event("blur"));
  env.document.dispatchEvent(new Event("visibilitychange"));
  env.document.dispatchEvent(new Event("visibilitychange"));
  env.canvas.width = 640;
  run.sample();
  env.canvas.width = 1280;
  assert.deepEqual(run.finish(), {
    visibility_changes: 2,
    focus_losses: 1,
    size_changed: true,
    comparable: false,
  });
});

test("clean runs stay comparable; DPR changes are sampled at finish and listeners are removed", () => {
  const env = environment();
  const first = observeRun(env);
  const clean = first.finish();
  assert.deepEqual(clean, {
    visibility_changes: 0,
    focus_losses: 0,
    size_changed: false,
    comparable: true,
  });
  env.window.dispatchEvent(new Event("blur"));
  assert.deepEqual(first.finish(), clean);
  const next = observeRun(env);
  env.window.devicePixelRatio = 1;
  assert.equal(next.finish().size_changed, true);
});
