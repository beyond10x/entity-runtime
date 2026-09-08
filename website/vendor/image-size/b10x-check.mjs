// Proves the three loop guards this vendored copy adds to image-size 2.0.2, and that honest
// files of the same three formats still parse. Run by `npm run vendor-check` and by the build.
// Each hostile input hung pristine 2.0.2 (killed after 5 s) on 2026-09-09; here each must throw.
import { imageSize } from "./dist/index.mjs";
import { createRequire } from "node:module";
import * as inputs from "./b10x-hostile-inputs.mjs";

const require = createRequire(import.meta.url);
const cjs = require("./dist/index.cjs");
// The copy Docusaurus actually loads: the tarball `package.json` points at, installed under
// node_modules. Packed from this directory, so a stale tarball fails here rather than at build.
const installed = await import("image-size");

const hostile = ["icnsZeroEntry", "heifZeroIspe", "jxlZeroJxlp"];
const honest = { icnsHonest: { width: 128, height: 128 }, heifHonest: { width: 64, height: 48 } };
let failures = 0;

for (const [label, size] of [["esm", imageSize], ["cjs", cjs.imageSize], ["installed", installed.imageSize]]) {
  for (const name of hostile) {
    const started = Date.now();
    try {
      const result = size(inputs[name]);
      console.error(`${label} ${name}: returned ${JSON.stringify(result)} — a hostile input must be refused`);
      failures += 1;
    } catch (error) {
      const elapsed = Date.now() - started;
      if (!(error instanceof TypeError) || elapsed > 1000) {
        console.error(`${label} ${name}: ${error.constructor.name} after ${elapsed}ms — expected a prompt TypeError`);
        failures += 1;
      }
    }
  }
  for (const [name, expected] of Object.entries(honest)) {
    const result = size(inputs[name]);
    if (result.width !== expected.width || result.height !== expected.height) {
      console.error(`${label} ${name}: ${JSON.stringify(result)} ≠ ${JSON.stringify(expected)}`);
      failures += 1;
    }
  }
}
if (failures) {
  console.error(`image-size vendor check: ${failures} failure(s)`);
  process.exit(1);
}
console.log("image-size vendor check: 3 hostile inputs refused and 2 honest files parsed — esm, cjs and the installed tarball");
