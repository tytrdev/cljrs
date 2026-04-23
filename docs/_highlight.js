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

/// Number span with source-position attrs so editor UIs (e.g. the
/// platformer's alt+drag scrubbable numbers) can locate the literal
/// in the underlying textarea.
function numSpan(start, text) {
  return `<span class="tok-number" data-s="${start}" data-l="${text.length}">${esc(text)}</span>`;
}

/// String span with source-position attrs — lets editor UIs attach
/// color-picker popovers to hex-color string literals. Hex-like strings
/// get an additional `tok-color` class plus a `--swatch` CSS variable
/// so UIs can render an in-line color preview.
function strSpan(start, text) {
  const m = text.match(/^"(#[0-9a-fA-F]{6})"$/);
  if (m) {
    return `<span class="tok-string tok-color" data-s="${start}" data-l="${text.length}" style="--swatch:${m[1]}">${esc(text)}</span>`;
  }
  return `<span class="tok-string" data-s="${start}" data-l="${text.length}">${esc(text)}</span>`;
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
      out += strSpan(i, src.slice(i, j));
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
      out += numSpan(i, src.slice(i, j));
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

const MOJO_KEYWORDS = new Set([
  "fn", "def", "var", "let", "if", "elif", "else", "for", "while",
  "break", "continue", "return", "pass", "raise", "try", "except",
  "finally", "with", "async", "await", "yield", "import", "from",
  "as", "in", "is", "not", "and", "or", "struct", "trait", "alias",
  "type", "owned", "inout", "borrowed", "ref", "lifetime", "mut",
  "True", "False", "None", "self", "Self",
]);
const MOJO_BUILTINS = new Set([
  // types
  "Int", "Int8", "Int16", "Int32", "Int64",
  "UInt", "UInt8", "UInt16", "UInt32", "UInt64",
  "Float16", "Float32", "Float64", "BFloat16",
  "Bool", "String", "StringLiteral",
  "List", "Dict", "Tuple", "Optional", "Variant",
  "SIMD", "DType", "Scalar", "Tensor", "Buffer",
  "InlinedFixedVector", "Pointer", "UnsafePointer", "AnyType",
  // stdlib
  "math", "print", "len", "range", "abs", "min", "max", "sum",
  "sqrt", "sin", "cos", "tan", "exp", "log", "pow",
  "floor", "ceil", "round", "isnan", "isinf",
  "abort", "debug_assert",
]);

/// Highlight Mojo.
function highlightMojo(src) {
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    // line comment
    if (c === "#") {
      let j = i;
      while (j < n && src[j] !== "\n") j++;
      out += span("tok-comment", src.slice(i, j));
      i = j;
      continue;
    }
    // f-string prefix: f"..." or f'...'
    if ((c === "f" || c === "F") && (src[i + 1] === '"' || src[i + 1] === "'")) {
      const quote = src[i + 1];
      // triple-quoted f-string
      if (src[i + 2] === quote && src[i + 3] === quote) {
        let j = i + 4;
        while (j < n && !(src[j] === quote && src[j + 1] === quote && src[j + 2] === quote)) j++;
        j = Math.min(n, j + 3);
        out += span("tok-string", src.slice(i, j));
        i = j;
        continue;
      }
      let j = i + 2;
      while (j < n) {
        if (src[j] === "\\" && j + 1 < n) { j += 2; continue; }
        if (src[j] === quote) { j++; break; }
        if (src[j] === "\n") break;
        j++;
      }
      out += span("tok-string", src.slice(i, j));
      i = j;
      continue;
    }
    // triple-quoted string
    if ((c === '"' || c === "'") && src[i + 1] === c && src[i + 2] === c) {
      const quote = c;
      let j = i + 3;
      while (j < n && !(src[j] === quote && src[j + 1] === quote && src[j + 2] === quote)) j++;
      j = Math.min(n, j + 3);
      out += span("tok-string", src.slice(i, j));
      i = j;
      continue;
    }
    // single/double string
    if (c === '"' || c === "'") {
      const quote = c;
      let j = i + 1;
      while (j < n) {
        if (src[j] === "\\" && j + 1 < n) { j += 2; continue; }
        if (src[j] === quote) { j++; break; }
        if (src[j] === "\n") break;
        j++;
      }
      out += span("tok-string", src.slice(i, j));
      i = j;
      continue;
    }
    // number: hex, binary, int, float, scientific, underscores
    if (/[0-9]/.test(c)) {
      let j = i + 1;
      if (c === "0" && (src[j] === "x" || src[j] === "X")) {
        j++;
        while (j < n && /[0-9a-fA-F_]/.test(src[j])) j++;
      } else if (c === "0" && (src[j] === "b" || src[j] === "B")) {
        j++;
        while (j < n && /[01_]/.test(src[j])) j++;
      } else {
        while (j < n && /[0-9_]/.test(src[j])) j++;
        if (src[j] === ".") {
          j++;
          while (j < n && /[0-9_]/.test(src[j])) j++;
        }
        if (src[j] === "e" || src[j] === "E") {
          j++;
          if (src[j] === "+" || src[j] === "-") j++;
          while (j < n && /[0-9_]/.test(src[j])) j++;
        }
      }
      out += span("tok-number", src.slice(i, j));
      i = j;
      continue;
    }
    // decorator
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
      if (MOJO_KEYWORDS.has(tok)) out += span("tok-keyword", tok);
      else if (MOJO_BUILTINS.has(tok)) out += span("tok-ident tok-builtin", tok);
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
  if (lang === "mojo") {
    return highlightMojo(src);
  }
  return esc(src);
}
