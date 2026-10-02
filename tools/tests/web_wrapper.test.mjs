import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { test } from "node:test";

const wrapperURL = new URL("../../web/n3.js", import.meta.url);
const hostKey = Symbol.for("n3.wrapper.test.host");
let fixtureSerial = 0;

function event(name, detail) {
  return Object.assign(new Event(name, { cancelable: true }), { detail });
}

function deferredFile(name = "pending.n3.json") {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return {
    file: { name, size: 2, arrayBuffer: () => promise },
    finish: () => resolve(new TextEncoder().encode("{}").buffer),
  };
}

const settle = () => new Promise((resolve) => setImmediate(resolve));

// Only the DOM and generated WASM module are substituted. Tests import and run
// the maintained wrapper itself, without copying or rewriting its implementation.
async function mount(t) {
  const fixture = {
    state: { ready: true, dirty: false, objects: 0 },
    actions: [],
    errors: [],
    confirm: true,
    snapshotError: false,
    newError: false,
    loadError: false,
  };
  class Element extends EventTarget {
    constructor(tagName) {
      super();
      this.tagName = tagName;
      this.style = {};
      this.children = [];
      this.files = [];
      this.clientWidth = 800;
      this.clientHeight = 600;
    }
    setAttribute() {}
    append(child) {
      child.remove();
      this.children.push(child);
      child.parent = this;
    }
    remove() {
      if (this.parent) {
        this.parent.children.splice(this.parent.children.indexOf(this), 1);
        this.parent = undefined;
      }
    }
    focus() {
      fixture.document.activeElement = this;
    }
    click() {
      this.clicked = true;
    }
    set value(value) {
      if (value === "") this.files = [];
    }
  }
  fixture.document = {
    body: new Element("body"),
    createElement: (tag) => new Element(tag),
  };
  fixture.window = Object.assign(new EventTarget(), {
    isSecureContext: true,
    confirm: () => fixture.confirm,
  });
  fixture.app = {
    command(id) {
      fixture.actions.push(["command", id]);
      fixture.state.dirty = true;
      fixture.state.objects += 1;
    },
    new_document() {
      if (fixture.newError) throw new Error("Active edit prevents replacement");
      fixture.actions.push(["new"]);
      fixture.state.dirty = false;
      fixture.state.objects = 0;
    },
    load_file(name, _bytes, append) {
      if (fixture.loadError) throw new Error("Invalid document");
      fixture.actions.push(["load", name, append]);
    },
    snapshot() {
      if (fixture.snapshotError) throw new Error("Host unavailable");
      return JSON.stringify(fixture.state);
    },
    resize() {},
    destroy() {
      fixture.actions.push(["destroy"]);
    },
    free() {
      fixture.actions.push(["free"]);
    },
  };
  fixture.start = (canvas) => {
    fixture.canvas = canvas;
    queueMicrotask(() => canvas.dispatchEvent(event("n3-ready")));
    return fixture.app;
  };
  const globals = {
    document: fixture.document,
    window: fixture.window,
    navigator: { gpu: {} },
    ResizeObserver: class {
      observe() {}
      disconnect() {}
    },
  };
  const previous = Object.fromEntries(
    Object.keys(globals).map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { configurable: true, value });
  }
  globalThis[hostKey] = fixture;
  const serial = ++fixtureSerial;
  const mockURL = `n3-wrapper-test:wasm-${serial}`;
  const hooks = registerHooks({
    resolve(specifier, context, nextResolve) {
      if (
        specifier === "./pkg/n3.js" &&
        new URL(context.parentURL).pathname === wrapperURL.pathname
      ) {
        return { url: mockURL, shortCircuit: true };
      }
      return nextResolve(specifier, context);
    },
    load(url, context, nextLoad) {
      if (url === mockURL) {
        return {
          format: "module",
          shortCircuit: true,
          source: `export default async function init() {}
            export function start(canvas) {
              return globalThis[Symbol.for("n3.wrapper.test.host")].start(canvas);
            }`,
        };
      }
      return nextLoad(url, context);
    },
  });
  t.after(() => {
    fixture.controller?.destroy();
    hooks.deregister();
    delete globalThis[hostKey];
    for (const [key, descriptor] of Object.entries(previous)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  });
  const { mountN3 } = await import(`${wrapperURL.href}?fixture=${serial}`);
  fixture.controller = await mountN3(new Element("main"), {
    onError: (error) => fixture.errors.push(error.message),
  });
  fixture.request = async (name) => {
    fixture.canvas.dispatchEvent(event("n3-request", name));
    await settle();
  };
  fixture.unload = () => {
    const unload = event("beforeunload");
    fixture.window.dispatchEvent(unload);
    return unload;
  };
  fixture.publish = () =>
    fixture.canvas.dispatchEvent(
      event("n3-state", JSON.stringify(fixture.state)),
    );
  fixture.pickers = () =>
    fixture.document.body.children.filter((child) => child.tagName === "input");
  return fixture;
}

test("unload observes unsaved commands before the next rendered status", async (t) => {
  const fixture = await mount(t);
  assert.equal(fixture.unload().defaultPrevented, false);
  fixture.controller.command("insert.cube");
  assert.equal(fixture.unload().defaultPrevented, true);
});

test("unload prefers fresh clean state and retains dirty fallback on host failure", async (t) => {
  const fixture = await mount(t);
  fixture.state.dirty = true;
  fixture.publish();
  fixture.state.dirty = false;
  assert.equal(fixture.unload().defaultPrevented, false);
  fixture.snapshotError = true;
  assert.equal(fixture.unload().defaultPrevented, true);
  fixture.controller.destroy();
  assert.equal(fixture.unload().defaultPrevented, false);
});

test("confirmed New supersedes a pending file read", async (t) => {
  const fixture = await mount(t);
  const read = deferredFile();
  const pending = fixture.controller.open(read.file);
  await fixture.request("new");
  read.finish();
  assert.equal(await pending, false);
  assert.deepEqual(fixture.actions, [["new"]]);
});

test("cancelled or rejected New preserves a pending file read", async (t) => {
  const fixture = await mount(t);
  for (const mode of ["cancel", "reject"]) {
    fixture.state.dirty = true;
    fixture.confirm = mode !== "cancel";
    fixture.newError = mode === "reject";
    const read = deferredFile(`${mode}.n3.json`);
    const pending = fixture.controller.open(read.file);
    await fixture.request("new");
    fixture.confirm = true;
    fixture.newError = false;
    read.finish();
    assert.equal(await pending, true);
  }
  assert.deepEqual(fixture.actions, [
    ["load", "cancel.n3.json", false],
    ["load", "reject.n3.json", false],
  ]);
  assert.deepEqual(fixture.errors, ["Active edit prevents replacement"]);
});

test("file picker cleans up after success, decode failure, empty selection and cancel", async (t) => {
  const fixture = await mount(t);
  for (const outcome of ["success", "failure", "empty", "cancel"]) {
    await fixture.request("open");
    const [picker] = fixture.pickers();
    assert.ok(picker?.clicked);
    fixture.loadError = outcome === "failure";
    if (outcome === "cancel") picker.dispatchEvent(event("cancel"));
    else {
      if (outcome !== "empty")
        picker.files = [new File(["{}"], `${outcome}.n3.json`)];
      picker.dispatchEvent(event("change"));
    }
    await settle();
    assert.deepEqual(fixture.pickers(), [], `${outcome} retained a picker`);
    assert.deepEqual(
      picker.files,
      [],
      `${outcome} retained its file selection`,
    );
  }
  assert.deepEqual(fixture.errors, ["Invalid document"]);
});

test("destroy removes open pickers and ignores changes or read completions afterwards", async (t) => {
  const fixture = await mount(t);
  await fixture.request("open");
  const [reading] = fixture.pickers();
  const read = deferredFile();
  reading.files = [read.file];
  reading.dispatchEvent(event("change"));
  await fixture.request("import");
  const waiting = fixture.pickers().find((picker) => picker !== reading);
  assert.ok(waiting);
  fixture.controller.destroy();
  assert.deepEqual(fixture.pickers(), []);
  assert.deepEqual(reading.files, []);
  assert.deepEqual(waiting.files, []);
  waiting.files = [new File(["{}"], "late.n3.json")];
  waiting.dispatchEvent(event("change"));
  read.finish();
  await settle();
  assert.deepEqual(fixture.errors, []);
  assert.deepEqual(fixture.actions, [["destroy"], ["free"]]);
});
