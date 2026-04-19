//! Integration tests for cljrs.ui — pure hiccup → HTML rendering.
//! The mount/hydrate side is wasm-only and not exercised here.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn fresh_env() -> Env {
    let env = Env::new();
    builtins::install(&env);
    env
}

fn render(hiccup_src: &str) -> String {
    let env = fresh_env();
    let src = format!("(cljrs.ui/render-html {hiccup_src})");
    let forms = reader::read_all(&src).expect("parse");
    let mut last = Value::Nil;
    for f in forms {
        last = eval::eval(&f, &env).expect("eval");
    }
    match last {
        Value::Str(s) => s.to_string(),
        other => panic!("expected string, got {}", other.to_pr_string()),
    }
}

#[test]
fn simple_div() {
    assert_eq!(render("[:div \"hi\"]"), "<div>hi</div>");
}

#[test]
fn nested() {
    assert_eq!(
        render("[:div [:span \"a\"] [:span \"b\"]]"),
        "<div><span>a</span><span>b</span></div>"
    );
}

#[test]
fn props_and_class() {
    let out = render("[:div {:class \"x\" :id \"y\"} \"hi\"]");
    assert!(out.starts_with("<div"));
    assert!(out.ends_with(">hi</div>"));
    assert!(out.contains(" class=\"x\""));
    assert!(out.contains(" id=\"y\""));
}

#[test]
fn html_escaping_in_text() {
    assert_eq!(
        render("[:p \"<script>alert(1)</script>\"]"),
        "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>"
    );
}

#[test]
fn html_escaping_in_attrs() {
    let out = render("[:a {:href \"x\\\"y\"} \"go\"]");
    assert!(out.contains("href=\"x&quot;y\""));
}

#[test]
fn skips_event_handlers() {
    // `:on-click` should not appear in the rendered HTML.
    let env = fresh_env();
    // Define an actual fn so the value is callable, not just a symbol.
    eval::eval(
        &reader::read_all("(defn noop [] nil)").unwrap()[0],
        &env,
    )
    .unwrap();
    let src = "(cljrs.ui/render-html [:button {:on-click noop} \"go\"])";
    let v = eval::eval(&reader::read_all(src).unwrap()[0], &env).unwrap();
    let s = match v {
        Value::Str(s) => s.to_string(),
        _ => unreachable!(),
    };
    assert_eq!(s, "<button>go</button>");
    assert!(!s.contains("on-click"));
}

#[test]
fn void_elements_self_close() {
    assert_eq!(render("[:br]"), "<br/>");
    let out = render("[:img {:src \"a.png\" :alt \"x\"}]");
    assert!(out.starts_with("<img") && out.ends_with("/>"));
    assert!(out.contains(" src=\"a.png\""));
    assert!(out.contains(" alt=\"x\""));
}

#[test]
fn nil_children_skipped() {
    assert_eq!(
        render("[:div \"a\" nil \"b\"]"),
        "<div>ab</div>"
    );
}

#[test]
fn numbers_render() {
    assert_eq!(render("[:span 42]"), "<span>42</span>");
}

#[test]
fn style_map() {
    let out = render("[:div {:style {:color \"red\"}} \"x\"]");
    assert!(out.contains("style=\"color:red\""));
}

#[test]
fn boolean_attrs() {
    let out = render("[:input {:disabled true :type \"text\"}]");
    assert!(out.contains(" disabled"));
    assert!(!out.contains("disabled=\"true\""));
    assert!(out.contains(" type=\"text\""));
}

#[test]
fn renders_seq_of_children() {
    // map returns a lazy seq; render-html should splice it.
    let env = fresh_env();
    let src = r#"
      (cljrs.ui/render-html
        [:ul (map (fn [x] [:li (str x)]) [1 2 3])])
    "#;
    let v = eval::eval(&reader::read_all(src).unwrap()[0], &env).unwrap();
    let s = match v { Value::Str(s) => s.to_string(), _ => unreachable!() };
    assert_eq!(s, "<ul><li>1</li><li>2</li><li>3</li></ul>");
}
