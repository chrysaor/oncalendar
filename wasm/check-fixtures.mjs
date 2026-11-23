import { readFileSync } from "node:fs";
import { load } from "./demo/oncalendar.mjs";

const [wasmPath, expectedPath] = process.argv.slice(2);
if (!wasmPath || !expectedPath) {
  console.error("usage: node wasm/check-fixtures.mjs <oncalendar.wasm> <expected.tsv>");
  process.exit(2);
}

const lines = readFileSync(expectedPath, "utf8").split(/\r?\n/);
const iterations = Number(/iterations=(\d+)/.exec(lines[0] ?? "")?.[1]);
if (!Number.isInteger(iterations)) {
  console.error(`${expectedPath}: no iterations=N in the first line`);
  process.exit(2);
}

const module = await load(readFileSync(wasmPath));
const wholeSecond = (utc) => utc.replace(/\.\d+Z$/, "Z");

function describe(actual) {
  switch (actual.status) {
    case "ok": {
      const elapses = actual.elapses.map((elapse) => wholeSecond(elapse.utc));
      return `ok ${actual.normalized} ${elapses.length ? elapses.join(",") : "never"}`;
    }
    case "invalid":
      return `rejected by parse at byte ${actual.offset}`;
    case "unknown-zone":
      return `rejected by upcoming_with ${actual.normalized}`;
    default:
      return `bad request: ${actual.message}`;
  }
}

function agrees(status, normalized, elapses, actual) {
  switch (status) {
    case "error":
      return actual.status === "invalid" || actual.status === "unknown-zone";
    case "ok":
      return actual.status === "ok" && describe(actual) === `ok ${normalized} ${elapses}`;
    case "unresolved":
      return actual.status === "ok" && (normalized === "-" || actual.normalized === normalized);
    default:
      return false;
  }
}

let rows = 0;
const differences = [];
lines.forEach((line, index) => {
  if (line === "" || line.startsWith("#")) {
    return;
  }
  rows += 1;
  const [zone, base, expression, status, normalized, elapses] = line.split("\t");
  const actual = module.evaluate(zone, base, expression, iterations);
  if (!agrees(status, normalized, elapses, actual)) {
    differences.push(
      `expected.tsv:${index + 1} [${zone}, ${base}] ${expression.slice(0, 80)}\n` +
        `    expected: ${status} ${normalized} ${elapses}\n` +
        `    actual:   ${describe(actual)}`,
    );
  }
});

console.log(`wasm fixtures: ${rows} rows, differing: ${differences.length}`);
if (differences.length > 0) {
  console.log(differences.slice(0, 20).join("\n"));
  process.exit(1);
}
