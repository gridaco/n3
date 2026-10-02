// Experimental application adapter. Rust owns document edits, input and rendering.
import init, { start } from "./pkg/n3.js";

let moduleInitialization;

/** Mount one editor per WASM module instance. Use an iframe for independent editors. */
export async function mountN3(
  container,
  { onState = () => {}, onError = console.error } = {},
) {
  const canvas = document.createElement("canvas");
  canvas.className = "n3-canvas";
  canvas.tabIndex = 0;
  canvas.setAttribute("aria-label", "N3 mesh editor");
  canvas.style.cssText =
    "display:block;width:100%;height:100%;outline:none;touch-action:none";
  container.append(canvas);
  const abort = new AbortController();
  const { signal } = abort;
  let app;
  let destroyed = false;
  let loadGeneration = 0;
  let resizeObserver;
  const pickers = new Set();
  let state = { ready: false, objects: 0, selected: 0, dirty: false };
  const report = (error) =>
    onError(error instanceof Error ? error : new Error(String(error)));
  // WASM events may be emitted while Rust holds its workspace. Run host effects
  // after that borrow is released, including callbacks supplied by an embedding UI.
  canvas.addEventListener(
    "n3-state",
    (event) => {
      state = JSON.parse(event.detail);
      queueMicrotask(() => {
        if (!destroyed) onState(state);
      });
    },
    { signal },
  );
  canvas.addEventListener(
    "n3-error",
    (event) => queueMicrotask(() => report(event.detail)),
    { signal },
  );
  const download = (text, filename) => {
    const url = URL.createObjectURL(
      new Blob([text], { type: "application/json" }),
    );
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = filename;
    document.body.append(anchor);
    anchor.click();
    anchor.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    // A download request is not proof that the browser wrote the file. Keep dirty.
  };
  const confirmReplace = () =>
    !JSON.parse(app.snapshot()).dirty ||
    window.confirm(
      "Discard unsaved changes? Download the document first if you want to keep it.",
    );
  const openPicker = (append) => {
    const picker = document.createElement("input");
    const pickerAbort = new AbortController();
    const removePicker = () => {
      pickerAbort.abort();
      picker.value = "";
      picker.remove();
      pickers.delete(removePicker);
    };
    picker.type = "file";
    picker.accept = ".json,.obj,.gltf,.glb";
    picker.hidden = true;
    document.body.append(picker);
    pickers.add(removePicker);
    picker.addEventListener("cancel", removePicker, {
      once: true,
      signal: pickerAbort.signal,
    });
    picker.addEventListener(
      "change",
      async () => {
        try {
          const file = picker.files[0];
          if (!file) return;
          await controller.open(file, { append });
        } catch (error) {
          if (!destroyed) report(error);
        } finally {
          removePicker();
        }
      },
      { once: true, signal: pickerAbort.signal },
    );
    try {
      picker.click();
    } catch (error) {
      removePicker();
      throw error;
    }
  };
  canvas.addEventListener(
    "n3-request",
    (event) => {
      queueMicrotask(() => {
        if (destroyed) return;
        try {
          switch (event.detail) {
            case "open":
              openPicker(false);
              break;
            case "import":
              openPicker(true);
              break;
            case "save":
              controller.download();
              break;
            case "new":
              if (confirmReplace()) {
                app.new_document();
                loadGeneration += 1;
              }
              break;
            case "settings":
              download(app.settings_json(), "settings.json");
              break;
            case "quit":
              report(
                "Close this browser tab to leave N3. Download your document first.",
              );
              break;
          }
        } catch (error) {
          report(error);
        }
      });
    },
    { signal },
  );
  window.addEventListener(
    "beforeunload",
    (event) => {
      let dirty = state.dirty;
      try {
        if (app) dirty = JSON.parse(app.snapshot()).dirty;
      } catch {
        // Startup or a host failure can prevent a snapshot. Retain the last
        // known warning instead of discarding it with the failed query.
      }
      if (dirty) {
        event.preventDefault();
        event.returnValue = "";
      }
    },
    { signal },
  );
  canvas.addEventListener("dragover", (event) => event.preventDefault(), {
    signal,
  });
  canvas.addEventListener(
    "drop",
    (event) => {
      event.preventDefault();
      const file = event.dataTransfer.files[0];
      if (file)
        controller
          .open(file, { append: !file.name.toLowerCase().endsWith(".n3.json") })
          .catch(report);
    },
    { signal },
  );
  const controller = {
    canvas,
    command(id) {
      app.command(id);
      canvas.focus();
    },
    async open(file, { append = false } = {}) {
      const generation = ++loadGeneration;
      if (file.size > 64 * 1024 * 1024)
        throw new Error("Files must be 64 MiB or smaller");
      const bytes = new Uint8Array(await file.arrayBuffer());
      if (destroyed) throw new Error("Editor has been destroyed");
      if (generation !== loadGeneration) return false;
      // Recheck after the asynchronous read so edits made during it are protected.
      if (!append && !confirmReplace()) return false;
      app.load_file(file.name, bytes, append);
      canvas.focus();
      return true;
    },
    download(filename = "Untitled.n3.json") {
      download(app.document_json(), filename);
    },
    documentJSON() {
      return app.document_json();
    },
    snapshot() {
      return JSON.parse(app.snapshot());
    },
    destroy() {
      if (destroyed) return;
      destroyed = true;
      loadGeneration += 1;
      resizeObserver?.disconnect();
      abort.abort();
      for (const removePicker of pickers) removePicker();
      app?.destroy();
      app?.free();
      canvas.remove();
    },
  };
  try {
    if (!window.isSecureContext || !navigator.gpu) {
      throw new Error(
        "N3 requires WebGPU in a secure context. Use a WebGPU-capable browser over HTTPS or localhost.",
      );
    }
    moduleInitialization ??= init();
    await moduleInitialization;
    await new Promise((resolve, reject) => {
      canvas.addEventListener("n3-ready", resolve, { once: true, signal });
      canvas.addEventListener(
        "n3-error",
        (event) => reject(new Error(event.detail)),
        { once: true, signal },
      );
      app = start(canvas);
    });
    const resize = () => {
      if (container.clientWidth > 0 && container.clientHeight > 0) {
        app.resize(container.clientWidth, container.clientHeight);
      }
    };
    resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    resize();
    canvas.focus();
    return controller;
  } catch (error) {
    controller.destroy();
    throw error;
  }
}
