// Proves the depth guard this vendored copy adds to braces 3.0.3, and that ordinary patterns
// still give 3.0.3's results. Run by `npm run vendor-check` and by the build.
// Published 3.0.3 overflowed the stack (RangeError) or accepted each hostile input on 2026-10-07;
// here each must throw the guard's SyntaxError. `node vendor/braces/b10x-check.mjs <dir>` checks
// only the braces package in <dir>, which is how that run on 3.0.3 is repeated.
//
// Each hostile case runs in a fresh child process. How deep V8 recurses before it overflows
// depends on whether the walker has already been optimised by earlier calls in the same process,
// so a shared process could hide a missing guard; a fresh one fails the same way every run.
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import * as inputs from "./b10x-hostile-inputs.mjs";

const require = createRequire(import.meta.url);
const self = fileURLToPath(import.meta.url);
const here = dirname(self);
const website = join(here, "..", "..");

// The ways the site's dependencies reach braces: micromatch and chokidar call braces(), with
// `expand: true` for brace expansion; parse and stringify are its other public walkers.
const ops = {
  compile: (braces, input) => braces(input),
  expand: (braces, input) => braces(input, { expand: true }),
  parse: (braces, input) => braces.parse(input),
  stringify: (braces, input) => braces.stringify(input),
};

if (process.argv[2] === "--case") {
  const [dir, op, name] = process.argv.slice(3);
  const braces = require(dir);
  const input = name === "nestedAst" ? inputs.nestedAst() : inputs[name];
  let outcome;
  try {
    ops[op](braces, input);
    outcome = { returned: true };
  } catch (error) {
    outcome = { name: error.constructor.name, message: String(error.message) };
  }
  // 3.0.3 itself writes to stdout on some inputs, so the outcome is the last line.
  process.stdout.write(`\n${JSON.stringify(outcome)}\n`);
  process.exit(0);
}

const hostile = [
  ...["nestedBraces", "nestedParens"].flatMap((name) => Object.keys(ops).map((op) => [op, name])),
  ...["compile", "expand", "stringify"].map((op) => [op, "nestedAst"]),
];
const honest = [
  ["compile", "**/_*.{js,jsx,ts,tsx,md,mdx}", ["**/_*.(js|jsx|ts|tsx|md|mdx)"]],
  ["compile", "app/{reading,writing}/**/*.{js,jsx}", ["app/(reading|writing)/**/*.(js|jsx)"]],
  ["expand", "page-{1..3}.js", ["page-1.js", "page-2.js", "page-3.js"]],
  ["expand", "a/{b,{c,d}}/e", ["a/b/e", "a/c/e", "a/d/e"]],
  ["compile", inputs.nestedBelowGuard, [inputs.nestedBelowGuard]],
  ["expand", inputs.nestedBelowGuard, [inputs.nestedBelowGuard]],
  ["stringify", inputs.nestedBelowGuard, inputs.nestedBelowGuard],
];
let failures = 0;
const fail = (message) => {
  console.error(message);
  failures += 1;
};

// The copies to check: this directory and every braces the lockfile installs, each of which must
// be this one; or only the directory named on the command line.
const copies = [];
if (process.argv[2]) {
  copies.push(["argument", resolve(process.argv[2])]);
} else {
  const { version } = JSON.parse(readFileSync(join(here, "package.json"), "utf8"));
  const lock = JSON.parse(readFileSync(join(website, "package-lock.json"), "utf8"));
  copies.push(["source", here]);
  for (const [key, entry] of Object.entries(lock.packages)) {
    if (!/(^|\/)node_modules\/braces$/.test(key)) continue;
    if (entry.version !== version) fail(`${key}: the lockfile installs braces ${entry.version}, not ${version}`);
    copies.push([key, join(website, key)]);
  }
  if (copies.length === 1) fail("package-lock.json installs no braces, so no installed copy was checked");
  // Packed from this directory, so a stale tarball fails here rather than at build.
  const files = ["index.js", ...readdirSync(join(here, "lib")).map((file) => `lib/${file}`)];
  for (const [label, dir] of copies.slice(1)) {
    for (const file of files) {
      let installed;
      try {
        installed = readFileSync(join(dir, file));
      } catch (error) {
        fail(`${label}/${file}: ${error.code} — is the tarball installed (npm ci)?`);
        continue;
      }
      if (!installed.equals(readFileSync(join(here, file)))) fail(`${label}/${file} differs from vendor/braces/${file}; repack`);
    }
  }
}

for (const [label, dir] of copies) {
  for (const [op, name] of hostile) {
    let outcome;
    try {
      const stdout = execFileSync(process.execPath, [self, "--case", dir, op, name], { encoding: "utf8", timeout: 10000 });
      outcome = JSON.parse(stdout.trim().split("\n").pop());
    } catch (error) {
      fail(`${label} ${op} ${name}: the case process failed (${error.signal ?? error.status ?? error.message})`);
      continue;
    }
    if (outcome.returned) {
      fail(`${label} ${op} ${name}: returned — a hostile input must be refused`);
    } else if (outcome.name !== "SyntaxError" || !/nesting depth exceeds/.test(outcome.message)) {
      fail(`${label} ${op} ${name}: ${outcome.name}: ${outcome.message} — expected the depth guard's SyntaxError`);
    }
  }
  const braces = require(dir);
  for (const [op, input, expected] of honest) {
    const shown = input.length > 40 ? `${input.slice(0, 20)}…(${input.length} chars)` : input;
    let result;
    try {
      result = ops[op](braces, input);
    } catch (error) {
      fail(`${label} ${op} ${shown}: ${error.constructor.name}: ${error.message}`);
      continue;
    }
    if (JSON.stringify(result) !== JSON.stringify(expected)) {
      fail(`${label} ${op} ${shown}: ${JSON.stringify(result)} ≠ ${JSON.stringify(expected)}`);
    }
  }
}
if (failures) {
  console.error(`braces vendor check: ${failures} failure(s)`);
  process.exit(1);
}
console.log(
  `braces vendor check: ${hostile.length} hostile inputs refused and ${honest.length} ordinary patterns unchanged — ` +
    copies.map(([label]) => label).join(", "),
);
