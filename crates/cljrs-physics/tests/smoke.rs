//! End-to-end: evaluate Clojure that uses the physics bindings and
//! verify a ball falls under gravity.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn run(src: &str) -> Value {
    let env = Env::new();
    builtins::install(&env);
    cljrs_physics::install(&env);
    let forms = reader::read_all(src).expect("read");
    let mut result = Value::Nil;
    for f in forms {
        result = eval::eval(&f, &env).expect(&format!("eval: {f}"));
    }
    result
}

fn as_floats(v: &Value) -> Vec<f64> {
    match v {
        Value::Vector(xs) => xs
            .iter()
            .map(|x| match x {
                Value::Float(f) => *f,
                Value::Int(i) => *i as f64,
                _ => panic!("non-numeric component: {x:?}"),
            })
            .collect(),
        _ => panic!("not a vector: {v:?}"),
    }
}

#[test]
fn world_2d_ball_falls() {
    let v = run(
        r#"
        (require '[cljrs.physics.d2 :as p2])
        (def w (p2/world {:gravity [0 -9.81]}))
        (def ball (p2/add-body! w
                                {:type :dynamic
                                 :position [0 10]
                                 :collider {:shape :ball :radius 0.5}}))
        (dotimes [_ 60] (p2/step! w))
        (p2/translation w ball)
        "#,
    );
    let xy = as_floats(&v);
    assert_eq!(xy.len(), 2);
    assert!(xy[1] < 10.0, "ball should have fallen, y = {}", xy[1]);
    assert!(xy[1] > 0.0, "ball shouldn't fall through floor, y = {}", xy[1]);
}

#[test]
fn world_2d_ball_on_floor_settles() {
    // Dynamic ball dropped onto a fixed box; after enough steps the
    // ball's vertical velocity should approach zero.
    let v = run(
        r#"
        (require '[cljrs.physics.d2 :as p2])
        (def w (p2/world {:gravity [0 -9.81]}))
        (def floor (p2/add-body! w
                                 {:type :fixed
                                  :position [0 -1]
                                  :collider {:shape :box :half-extents [100 1]}}))
        (def ball (p2/add-body! w
                                {:type :dynamic
                                 :position [0 5]
                                 :collider {:shape :ball
                                            :radius 0.5
                                            :restitution 0.0
                                            :friction 1.0}}))
        (dotimes [_ 240] (p2/step! w))
        (p2/linvel w ball)
        "#,
    );
    let v = as_floats(&v);
    // After 4s of simulation with no bounce, should be near rest.
    assert!(v[1].abs() < 2.0, "ball should have settled, vy = {}", v[1]);
}

#[test]
fn world_3d_ball_falls() {
    let v = run(
        r#"
        (require '[cljrs.physics.d3 :as p3])
        (def w (p3/world {:gravity [0 -9.81 0]}))
        (def ball (p3/add-body! w
                                {:type :dynamic
                                 :position [0 10 0]
                                 :collider {:shape :ball :radius 0.5}}))
        (dotimes [_ 60] (p3/step! w))
        (p3/translation w ball)
        "#,
    );
    let xyz = as_floats(&v);
    assert_eq!(xyz.len(), 3);
    assert!(xyz[1] < 10.0, "3d ball should have fallen, y = {}", xyz[1]);
}

#[test]
fn apply_impulse_changes_velocity() {
    let v = run(
        r#"
        (require '[cljrs.physics.d2 :as p2])
        (def w (p2/world {:gravity [0 0]}))
        (def b (p2/add-body! w
                             {:type :dynamic
                              :position [0 0]
                              :collider {:shape :ball :radius 0.5}}))
        (p2/apply-impulse! w b [5 0])
        (p2/step! w)
        (p2/linvel w b)
        "#,
    );
    let v = as_floats(&v);
    assert!(v[0] > 1.0, "impulse should give +x velocity, got {}", v[0]);
}

#[test]
fn body_count_reflects_insertions() {
    let v = run(
        r#"
        (require '[cljrs.physics.d2 :as p2])
        (def w (p2/world))
        (p2/add-body! w {:collider {:shape :ball :radius 1}})
        (p2/add-body! w {:collider {:shape :ball :radius 1}})
        (p2/add-body! w {:collider {:shape :ball :radius 1}})
        (p2/body-count w)
        "#,
    );
    assert_eq!(v, Value::Int(3));
}
