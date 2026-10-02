import { mountN3 } from "./n3.js";

const status = document.querySelector("#status");
const errorBox = document.querySelector("#error");
const report = (error) => {
  errorBox.hidden = false;
  errorBox.textContent = String(error.message ?? error);
};
try {
  await mountN3(document.querySelector("#editor"), {
    onState(state) {
      if (state.ready) status.hidden = true;
    },
    onError: report,
  });
} catch (error) {
  status.textContent = "Unable to start";
  report(error);
}
