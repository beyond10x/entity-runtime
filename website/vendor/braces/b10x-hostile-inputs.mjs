// Hostile inputs: each nests deeper than the recursive walkers of braces 3.0.3 survive.
// braces refuses input above 10,000 characters; 4,999 levels around one character is 9,999.
const DEPTH = 4999;
export const nestedBraces = "{".repeat(DEPTH) + "a" + "}".repeat(DEPTH);
export const nestedParens = "(".repeat(DEPTH) + "a" + ")".repeat(DEPTH);

// compile, expand and stringify also accept a caller-built AST, which no length limit bounds.
// A fresh tree on every call, because expand writes a queue into each node it visits.
export const nestedAst = () => {
  let ast = { type: "root", nodes: [] };
  for (let i = 0; i < 10000; i += 1) ast = { type: "root", nodes: [ast] };
  return ast;
};

// Honest nesting just inside the guard: 99 containers put the innermost text 100 calls below the
// root, the deepest compile walk the guard admits.
export const nestedBelowGuard = "{".repeat(99) + "a" + "}".repeat(99);
