// docs/_ui.js — Preact bridge for the cljrs UI library.
//
// Loaded as an ES module from any UI page. Imports the vendored Preact
// build and exposes a tiny `mount`/`hydrate` API on `window.cljrsUi`
// that the wasm-side ui_bridge calls via `Reflect::get`.
//
// Hiccup tree shape (built in ui_bridge.rs::hiccup_to_js):
//   [tag, propsObj, ...children]
//      tag   = string (keyword or symbol name)
//      props = plain object; values may be strings/numbers/booleans/null
//              or wasm-wrapped JS functions (event handlers).
//      child = string | number | array (recursive) | null
//
// We translate that into Preact vnodes via `h(tag, props, ...kids)`
// then call `render` (mount) or `hydrate` (hydration).

import { h, render, hydrate } from "./vendor/preact.js";

function toVNode(node) {
  if (node == null) return null;
  if (typeof node === "string" || typeof node === "number") return node;
  if (typeof node === "boolean") return null;
  if (Array.isArray(node)) {
    if (node.length === 0) return null;
    const head = node[0];
    // If the first element is itself an array, this is a list of children
    // rather than a single element — splice them.
    if (Array.isArray(head) || head == null || typeof head === "object") {
      return node.map(toVNode);
    }
    // Treat anything stringy as a tag.
    if (typeof head === "string") {
      const tag = head;
      const rest = node.slice(1);
      let props = {};
      let kids = rest;
      if (rest.length > 0 && rest[0] && typeof rest[0] === "object" && !Array.isArray(rest[0])) {
        props = rest[0] || {};
        kids = rest.slice(1);
      }
      // Map :on-click → onClick etc. Preact uses camelCased event props.
      const fixedProps = {};
      for (const k of Object.keys(props)) {
        if (k.startsWith("on-")) {
          // on-click → onClick
          const ev = k.slice(3).split("-").map((seg, i) =>
            i === 0 ? seg : seg.charAt(0).toUpperCase() + seg.slice(1)
          ).join("");
          fixedProps["on" + ev.charAt(0).toUpperCase() + ev.slice(1)] = props[k];
        } else if (k === "class") {
          fixedProps["className"] = props[k];
        } else if (k === "style" && typeof props[k] === "string") {
          fixedProps["style"] = props[k];
        } else {
          fixedProps[k] = props[k];
        }
      }
      const flatKids = [];
      // Walk one child slot, splicing nested child-lists (from cljrs
      // `(map ...)` results that arrive here as a JS array of hiccup
      // vectors) but stopping at any value that has already become a
      // vnode (h() returns an Object — re-entering toVNode on it
      // would coerce it to "[object Object]").
      const pushKid = (c) => {
        if (c == null || c === false) return;
        if (Array.isArray(c)) {
          if (c.length === 0) return;
          const head = c[0];
          // Element form `[tag ...]` — first slot is a string tag.
          if (typeof head === "string") {
            flatKids.push(toVNode(c));
            return;
          }
          // Otherwise it's a list of children to splice in.
          for (const cc of c) pushKid(cc);
          return;
        }
        if (typeof c === "string" || typeof c === "number") {
          flatKids.push(c);
          return;
        }
        // Already a Preact vnode (object) — pass through.
        flatKids.push(c);
      };
      for (const c of kids) pushKid(c);
      return h(tag, fixedProps, ...flatKids);
    }
    return node.map(toVNode);
  }
  return String(node);
}

function getRoot(rootId) {
  const el = document.getElementById(rootId);
  if (!el) {
    console.error("cljrsUi: root element not found:", rootId);
  }
  return el;
}

window.cljrsUi = {
  h,
  render,
  hydrate,
  toVNode,
  mount(rootId, tree) {
    const el = getRoot(rootId);
    if (!el) return;
    render(toVNode(tree), el);
  },
  hydrate(rootId, tree) {
    const el = getRoot(rootId);
    if (!el) return;
    hydrate(toVNode(tree), el);
  },
};
