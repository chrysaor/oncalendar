import { load } from "./oncalendar.mjs";

const element = (id) => document.getElementById(id);
const form = element("form");
const expressionInput = element("expression");
const zoneSelect = element("zone");
const countSelect = element("count");
const status = element("status");
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function nowAsBase() {
  return new Date().toISOString().replace("T", " ").replace("Z", " UTC");
}

function charIndex(text, byteOffset) {
  return decoder.decode(encoder.encode(text).subarray(0, byteOffset)).length;
}

function fromNow(utc) {
  const minutes = Math.round((Date.parse(utc) - Date.now()) / 60000);
  if (minutes < 1) {
    return "under a minute";
  }
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const parts = [];
  if (days > 0) parts.push(`${days} d`);
  if (hours > 0) parts.push(`${hours} h`);
  if (days === 0 && minutes % 60 > 0) parts.push(`${minutes % 60} min`);
  return `in ${parts.join(" ")}`;
}

function show(id, visible) {
  element(id).hidden = !visible;
}

function setStatus(text, kind) {
  status.textContent = text;
  status.className = `status ${kind}`;
}

function renderError(expression, offset, message) {
  const index = charIndex(expression, offset);
  const at = expression.codePointAt(index);
  const atText = at === undefined ? " " : String.fromCodePoint(at);
  element("error-message").textContent = message;
  element("error-before").textContent = expression.slice(0, index);
  element("error-at").textContent = atText;
  element("error-at").title = at === undefined ? "end of input" : `byte ${offset}`;
  element("error-after").textContent = expression.slice(index + atText.length);
}

function renderElapses(elapses, base, zone) {
  const rows = elapses.map(({ utc, local }) => {
    const row = document.createElement("tr");
    for (const text of [local, utc, fromNow(utc)]) {
      const cell = document.createElement("td");
      cell.textContent = text;
      row.append(cell);
    }
    return row;
  });
  element("elapses").replaceChildren(...rows);
  element("elapses-caption").textContent =
    `After ${base.replace(/\.\d+ /, " ")}, with ${zone} as the local time zone.`;
  show("elapses-table", elapses.length > 0);
  show("never", elapses.length === 0);
}

function saveState(expression, zone) {
  const params = new URLSearchParams({ expression, zone });
  history.replaceState(null, "", `#${params}`);
}

function readState(zones) {
  const params = new URLSearchParams(location.hash.slice(1));
  const browserZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const zone = [params.get("zone"), browserZone].find((name) => zones.includes(name)) ?? "UTC";
  return { expression: params.get("expression") ?? expressionInput.value, zone };
}

function render(module) {
  const expression = expressionInput.value;
  const zone = zoneSelect.value;
  const base = nowAsBase();
  const result = module.evaluate(zone, base, expression, Number(countSelect.value));
  saveState(expression, zone);

  show("normalized-block", result.status === "ok" || result.status === "unknown-zone");
  show("error-block", result.status === "invalid");
  show("elapses-block", result.status === "ok");
  element("normalized").textContent = result.normalized ?? "";

  switch (result.status) {
    case "ok":
      setStatus("Valid expression.", "valid");
      renderElapses(result.elapses, base, zone);
      break;
    case "invalid":
      setStatus("Not a valid expression.", "invalid");
      renderError(expression, result.offset, result.message);
      break;
    case "unknown-zone":
      setStatus(`Parsed, but cannot compute elapses: ${result.message}`, "invalid");
      break;
    default:
      setStatus(result.message, "invalid");
  }
}

async function start() {
  let module;
  try {
    const response = await fetch("oncalendar.wasm");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    module = await load(await response.arrayBuffer());
  } catch (error) {
    setStatus(
      `Could not load oncalendar.wasm (${error.message}). Build it with scripts/check-wasm.sh and serve this folder over HTTP.`,
      "invalid",
    );
    return;
  }

  const zones = module.zones();
  zoneSelect.replaceChildren(
    ...zones.map((name) => {
      const option = document.createElement("option");
      option.textContent = name;
      return option;
    }),
  );
  const state = readState(zones);
  expressionInput.value = state.expression;
  zoneSelect.value = state.zone;

  for (const control of [expressionInput, zoneSelect, countSelect]) {
    control.disabled = false;
    control.addEventListener("input", () => render(module));
  }
  form.addEventListener("submit", (event) => event.preventDefault());
  for (const button of document.querySelectorAll("[data-example]")) {
    button.addEventListener("click", () => {
      expressionInput.value = button.dataset.example;
      render(module);
      expressionInput.focus();
    });
  }
  setInterval(() => {
    if (document.visibilityState === "visible") {
      render(module);
    }
  }, 30000);
  render(module);
}

start();
