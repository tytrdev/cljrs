//! Smoke test: every demo scene in docs/physics.html must parse + run
//! 10 frames without error. Catches typos in demo source before deploy.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn fresh() -> Env {
    let env = Env::new();
    builtins::install(&env);
    cljrs_physics::install(&env);
    env
}

fn run_demo(src: &str, label: &str) {
    let env = fresh();
    let forms = reader::read_all(src).unwrap_or_else(|e| panic!("{label}: read: {e}"));
    for f in forms {
        eval::eval(&f, &env).unwrap_or_else(|e| panic!("{label}: eval: {e}"));
    }
    // Call (frame!) 10 times; must always return a vector.
    let frame_sym = reader::read_all("(frame!)").unwrap().into_iter().next().unwrap();
    for i in 0..10 {
        let v = eval::eval(&frame_sym, &env)
            .unwrap_or_else(|e| panic!("{label}: frame {i}: {e}"));
        assert!(
            matches!(v, Value::Vector(_)),
            "{label}: frame {i}: expected vector, got {}",
            v.type_name()
        );
    }
}

#[test]
fn demo_2d_boxes() {
    run_demo(
        r#"
(require '[cljrs.physics.d2 :as p2])
(def world (p2/world {:gravity [0 -20]}))
(p2/add-body! world {:type :fixed :position [0 -6]
                     :collider {:shape :box :half-extents [14 0.4]}})
(p2/add-body! world {:type :fixed :position [-12 0] :rotation 0.3
                     :collider {:shape :box :half-extents [6 0.3]}})
(p2/add-body! world {:type :fixed :position [12 0] :rotation -0.3
                     :collider {:shape :box :half-extents [6 0.3]}})
(def bodies
  (vec
    (for [i (range 18)]
      (p2/add-body! world
                    {:type :dynamic
                     :position [(- (* i 1.2) 10) (+ 6 (mod i 3))]
                     :rotation (* i 0.13)
                     :collider {:shape :box
                                :half-extents [0.5 0.5]
                                :restitution 0.15
                                :friction 0.6}}))))
(defn frame! []
  (p2/step! world)
  (mapv (fn [b]
          (let [[x y] (p2/translation world b)
                r     (p2/rotation world b)]
            [x y r]))
        bodies))
        "#,
        "2d-boxes",
    );
}

#[test]
fn demo_2d_tower() {
    run_demo(
        r#"
(require '[cljrs.physics.d2 :as p2])
(def world (p2/world {:gravity [0 -25]}))
(p2/add-body! world {:type :fixed :position [0 -6]
                     :collider {:shape :box :half-extents [14 0.4]}})
(def bodies
  (vec
    (for [layer (range 14)
          col   (range 3)]
      (let [y  (+ -5 (* layer 1.02))
            x  (+ (- (* col 1.02) 1.02)
                  (if (zero? layer) 0.08 0))]
        (p2/add-body! world
                      {:type :dynamic
                       :position [x y]
                       :collider {:shape :box
                                  :half-extents [0.5 0.5]
                                  :friction 0.8
                                  :restitution 0.0}})))))
(defn frame! []
  (p2/step! world)
  (mapv (fn [b]
          (let [[x y] (p2/translation world b)
                r     (p2/rotation world b)]
            [x y r]))
        bodies))
        "#,
        "2d-tower",
    );
}

#[test]
fn demo_2d_balls() {
    run_demo(
        r#"
(require '[cljrs.physics.d2 :as p2])
(def world (p2/world {:gravity [0 -18]}))
(p2/add-body! world {:type :fixed :position [-10 -2] :rotation 0.6
                     :collider {:shape :box :half-extents [8 0.3]}})
(p2/add-body! world {:type :fixed :position [10 -2] :rotation -0.6
                     :collider {:shape :box :half-extents [8 0.3]}})
(p2/add-body! world {:type :fixed :position [0 -6]
                     :collider {:shape :box :half-extents [6 0.3]}})
(def balls
  (vec
    (for [i (range 24)]
      (p2/add-body! world
                    {:type :dynamic
                     :position [(- (mod i 6) 2.5)
                                (+ 3 (* 0.8 (quot i 6)))]
                     :linvel [(* 2 (- 0.5 (/ (mod i 5) 5.0))) 0]
                     :collider {:shape :ball
                                :radius 0.42
                                :restitution (+ 0.4 (* 0.05 (mod i 10)))
                                :friction 0.15}}))))
(defn frame! []
  (p2/step! world)
  (mapv (fn [b]
          (let [[x y] (p2/translation world b)]
            [x y 0.42]))
        balls))
        "#,
        "2d-balls",
    );
}

#[test]
fn demo_3d_stack() {
    run_demo(
        r#"
(require '[cljrs.physics.d3 :as p3])
(def world (p3/world {:gravity [0 -15 0]}))
(p3/add-body! world {:type :fixed :position [0 -4 0]
                     :collider {:shape :box :half-extents [10 0.5 10]}})
(def bodies
  (vec
    (for [i (range 5)
          j (range 5)]
      (p3/add-body! world
                    {:type :dynamic
                     :position [(- i 2) (+ 1 (* 0.2 i)) (- j 2)]
                     :collider {:shape :ball
                                :radius 0.45
                                :restitution 0.1
                                :friction 0.4}}))))
(defn frame! []
  (p3/step! world)
  (mapv (fn [b]
          (let [[x y z] (p3/translation world b)]
            [x y z 0.45]))
        bodies))
        "#,
        "3d-stack",
    );
}

#[test]
fn demo_3d_rain() {
    run_demo(
        r#"
(require '[cljrs.physics.d3 :as p3])
(def world (p3/world {:gravity [0 -18 0]}))
(p3/add-body! world {:type :fixed :position [0 -4 0]
                     :collider {:shape :box :half-extents [6 0.3 6]}})
(p3/add-body! world {:type :fixed :position [6 -2 0]
                     :collider {:shape :box :half-extents [0.3 2 6]}})
(p3/add-body! world {:type :fixed :position [-6 -2 0]
                     :collider {:shape :box :half-extents [0.3 2 6]}})
(p3/add-body! world {:type :fixed :position [0 -2 6]
                     :collider {:shape :box :half-extents [6 2 0.3]}})
(p3/add-body! world {:type :fixed :position [0 -2 -6]
                     :collider {:shape :box :half-extents [6 2 0.3]}})
(def balls
  (vec
    (for [i (range 60)]
      (let [ang (* i 0.73)
            r   (* 3 (sin (* i 0.3)))]
        (p3/add-body! world
                      {:type :dynamic
                       :position [(* r (cos ang))
                                  (+ 6 (* 0.2 i))
                                  (* r (sin ang))]
                       :collider {:shape :ball
                                  :radius 0.32
                                  :restitution 0.55
                                  :friction 0.2}})))))
(defn frame! []
  (p3/step! world)
  (mapv (fn [b]
          (let [[x y z] (p3/translation world b)]
            [x y z 0.32]))
        balls))
        "#,
        "3d-rain",
    );
}
