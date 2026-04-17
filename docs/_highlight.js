// Minimal syntax highlighting for Clojure and WGSL. Hand-rolled so the
// docs site stays dependency-free — no highlight.js, prism, etc.
//
// API: highlight(source, lang) -> HTML string (escaped, wrapped in spans).
//
// Token classes:
//   tok-comment     gray     ;; ...    (clj)   // ...   (wgsl)
//   tok-string      amber    "..."
//   tok-number      blue     123, 3.14, 0xff, 1e-5
//   tok-keyword     green    :kw                 (clj only)
//   tok-special     purple   defn fn let if ...  (per-lang keyword list)
//   tok-ident       default  everything else

const CLJ_SPECIAL = new Set([
  // Core special forms + prelude macros + GPU DSL constructs.
  "def", "defn", "defn-", "defmacro", "defn-native", "defn-gpu",
  "defn-gpu-pixel", "fn", "let", "loop", "recur", "if", "when",
  "when-not", "cond", "case", "if-let", "when-let", "do", "try",
  "catch", "finally", "throw", "ns", "require", "import",
  "and", "or", "->", "->>", "quote", "dotimes", "doseq",
]);

const WGSL_KEYWORDS = new Set([
  "fn", "let", "var", "const", "if", "else", "for", "while", "loop",
  "break", "continue", "return", "struct", "true", "false",
  "@group", "@binding", "@compute", "@workgroup_size", "@builtin",
  "@uniform", "@storage", "@location", "@vertex", "@fragment",
]);
const WGSL_TYPES = new Set([
  "i32", "u32", "f32", "f16", "bool", "vec2", "vec3", "vec4",
  "mat2x2", "mat3x3", "mat4x4", "array", "atomic", "ptr",
  "read", "write", "read_write",
]);
const WGSL_BUILTIN_FNS = new Set([
  "sin", "cos", "tan", "asin", "acos", "atan", "atan2",
  "exp", "log", "log2", "sqrt", "inverseSqrt", "pow",
  "abs", "min", "max", "clamp", "mix", "step", "smoothstep",
  "floor", "ceil", "round", "fract", "trunc", "sign",
  "length", "normalize", "dot", "cross", "distance",
  "select", "all", "any", "arrayLength",
]);

function esc(s) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function span(cls, text) {
  return `<span class="${cls}">${esc(text)}</span>`;
}

/// Highlight a Clojure-ish source string (cljrs dialect).
function highlightClj(src) {
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    // line comment
    if (c === ";") {
      let j = i;
      while (j < n && src[j] !== "\n") j++;
      out += span("tok-comment", src.slice(i, j));
      i = j;
      continue;
    }
    // string
    if (c === '"') {
      let j = i + 1;
      while (j < n) {
        if (src[j] === "\\" && j + 1 < n) { j += 2; continue; }
        if (src[j] === '"') { j++; break; }
        j++;
      }
      out += span("tok-string", src.slice(i, j));
      i = j;
      continue;
    }
    // keyword
    if (c === ":") {
      let j = i + 1;
      while (j < n && /[A-Za-z0-9_\-?!+*/<>=.]/.test(src[j])) j++;
      out += span("tok-keyword", src.slice(i, j));
      i = j;
      continue;
    }
    // number (including leading -)
    if (/[0-9]/.test(c) ||
        (c === "-" && i + 1 < n && /[0-9]/.test(src[i + 1]) &&
         (i === 0 || /[\s(\[{,]/.test(src[i - 1])))) {
      let j = i + 1;
      while (j < n && /[0-9.eE+\-xXa-fA-F]/.test(src[j])) j++;
      out += span("tok-number", src.slice(i, j));
      i = j;
      continue;
    }
    // whitespace / punctuation passthrough
    if (/[\s(){}\[\],]/.test(c)) {
      out += esc(c);
      i++;
      continue;
    }
    // symbol / identifier
    let j = i;
    while (j < n && !/[\s(){}\[\],;"]/.test(src[j])) j++;
    const tok = src.slice(i, j);
    if (CLJ_SPECIAL.has(tok)) {
      out += span("tok-special", tok);
    } else {
      out += span("tok-ident", tok);
    }
    i = j;
  }
  return out;
}

/// Highlight WGSL.
function highlightWgsl(src) {
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    // line comment
    if (c === "/" && src[i + 1] === "/") {
      let j = i;
      while (j < n && src[j] !== "\n") j++;
      out += span("tok-comment", src.slice(i, j));
      i = j;
      continue;
    }
    // block comment
    if (c === "/" && src[i + 1] === "*") {
      let j = i + 2;
      while (j < n && !(src[j] === "*" && src[j + 1] === "/")) j++;
      j = Math.min(n, j + 2);
      out += span("tok-comment", src.slice(i, j));
      i = j;
      continue;
    }
    // string
    if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"') j++;
      if (j < n) j++;
      out += span("tok-string", src.slice(i, j));
      i = j;
      continue;
    }
    // number
    if (/[0-9]/.test(c)) {
      let j = i + 1;
      while (j < n && /[0-9.eE+\-xXa-fA-FuUiIfF]/.test(src[j])) j++;
      out += span("tok-number", src.slice(i, j));
      i = j;
      continue;
    }
    // attribute (@something)
    if (c === "@") {
      let j = i + 1;
      while (j < n && /[A-Za-z0-9_]/.test(src[j])) j++;
      out += span("tok-special", src.slice(i, j));
      i = j;
      continue;
    }
    // identifier
    if (/[A-Za-z_]/.test(c)) {
      let j = i + 1;
      while (j < n && /[A-Za-z0-9_]/.test(src[j])) j++;
      const tok = src.slice(i, j);
      if (WGSL_KEYWORDS.has(tok)) out += span("tok-special", tok);
      else if (WGSL_TYPES.has(tok)) out += span("tok-keyword", tok);
      else if (WGSL_BUILTIN_FNS.has(tok)) out += span("tok-ident tok-builtin", tok);
      else out += span("tok-ident", tok);
      i = j;
      continue;
    }
    out += esc(c);
    i++;
  }
  return out;
}

export function highlight(src, lang) {
  if (lang === "clj" || lang === "clojure" || lang === "cljrs") {
    return highlightClj(src);
  }
  if (lang === "wgsl") {
    return highlightWgsl(src);
  }
  return esc(src);
}
