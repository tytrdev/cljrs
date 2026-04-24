# Transpiler error source locations — design + patch

## Problem

`cljrs-mojo`'s `emit()` returns `Result<String, String>`. On failure the
error string quotes the offending form (e.g. `"defn arg vector expected:
(defn-mojo smoothstep ...)"`) but has no line/column, so IDEs and the
live Clojo playground can't jump to the mistake.

## Available machinery

`src/reader.rs` already tracks `(line, col)` for every list form read
from source, via a thread-local side table:

```rust
pub static FORM_LOCATIONS: RefCell<HashMap<usize, (u32, u32)>>
pub fn lookup_location(list: &Arc<Vec<Value>>) -> Option<(u32, u32)>
```

`lookup_location` takes the `Arc<Vec<Value>>` that backs a list form and
returns the 1-based `(line, col)` at which its opening `(` was read.
Synthesized forms (macro output, gensyms) return `None`.

## Proposed format

Prepend `line N col M: ` to the existing message whenever a location is
available:

```
line 3 col 1: defn arg vector expected: (defn-mojo smoothstep ...)
```

Unknown-location errors (read errors before FORM_LOCATIONS is
populated, synthetic-form errors) keep the current format.

## Minimal-surface patch

The cleanest fix lives entirely in `crates/cljrs-mojo/src/lib.rs::emit`
and requires NO changes to `tier1/2/3` — we annotate per top-level form
at the module-lowering boundary. `tier1::lower_module` already calls
`lower_top` per form, but swallows the per-form context on error. Swap
that loop into `emit` so we know which top-level form failed.

```rust
// crates/cljrs-mojo/src/lib.rs

pub fn emit(src: &str, tier: Tier) -> Result<String, String> {
    let forms = cljrs::reader::read_all(src)
        .map_err(|e| format!("read error: {e}"))?;

    // Lower per-form so we can attribute errors to the originating
    // top-level form's source location.
    let mut module = tier1::lower_module_annotated(&forms)
        .map_err(|(form, msg)| annotate_error(form, msg))?;

    match tier {
        Tier::Readable => {}
        Tier::Optimized => tier2::optimize(&mut module),
        Tier::Max => tier3::specialize(&mut module),
    }
    add_elementwise_imports(&mut module, tier);
    Ok(print_module(&module, tier))
}

fn annotate_error(form: Option<&cljrs::value::Value>, msg: String) -> String {
    use cljrs::reader::lookup_location;
    let Some(form) = form else { return msg };
    // Only list forms have locations in the side table.
    if let cljrs::value::Value::List(list) = form {
        if let Some((line, col)) = lookup_location(list) {
            return format!("line {line} col {col}: {msg}");
        }
    }
    msg
}
```

And in `tier1.rs` add a small wrapper alongside `lower_module`:

```rust
/// Like `lower_module`, but on error returns the offending top-level
/// form so the caller can attribute a source location.
pub fn lower_module_annotated(
    forms: &[Value],
) -> std::result::Result<MModule, (Option<&Value>, String)> {
    let ctx = Ctx::default();
    let mut items = Vec::new();
    for form in forms {
        match lower_top(&ctx, form) {
            Ok(mut v) => items.append(&mut v),
            Err(e) => return Err((Some(form), e)),
        }
    }
    // ... method attachment + finalization — propagate as (None, msg)
    //     since these errors aren't tied to a single top-level form.
    let pending = std::mem::take(&mut *ctx.methods.borrow_mut());
    for (sname, m) in pending {
        let mut placed = false;
        for it in items.iter_mut() {
            if let MItem::Struct { name, methods, .. } = it {
                if name == &sname {
                    methods.push(m.clone());
                    placed = true;
                    break;
                }
            }
        }
        if !placed {
            return Err((None, format!(
                "defn-method-mojo: no struct named `{sname}` \
                 defined before this method"
            )));
        }
    }
    let imports = ctx.take_imports();
    Ok(MModule { imports, items })
}
```

## Deeper (follow-up) improvement

The patch above only annotates *which top-level form* errored. For
nested errors (a bad argument in a nested call, etc.) the location will
be the enclosing `defn-mojo`'s line — close enough for most cases but
not always the precise offending sub-form.

For sub-form precision, thread `&Value` through the internal `lower_*`
helpers and annotate at each return site that knows the offending
sub-form. That's a larger patch touching tier1/2/3 and should be
landed separately.

## Test

`crates/cljrs-mojo/tests/error_locations.rs` (checked in by this
agent, marked `#[ignore]`) pins the expected format so the fix can
de-`#[ignore]` the tests as acceptance criteria.

## Acceptance

1. Apply the patch in `error_locations.patch`.
2. Remove `#[ignore]` from `tests/error_locations.rs`.
3. `cargo test -p cljrs-mojo` passes.
