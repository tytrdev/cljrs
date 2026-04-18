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
fn demo_2d_pinball() {
    run_demo(
        r#"
(require '[cljrs.physics.d2 :as p2])
(def world (p2/world {:gravity [0 -22]}))
(p2/add-body! world {:type :fixed :position [0 -6]
                     :collider {:shape :box :half-extents [12 0.3]
                                :restitution 0.95}})
(p2/add-body! world {:type :fixed :position [-10 0] :rotation 0.9
                     :collider {:shape :box :half-extents [8 0.3]
                                :restitution 0.95}})
(p2/add-body! world {:type :fixed :position [10 0] :rotation -0.9
                     :collider {:shape :box :half-extents [8 0.3]
                                :restitution 0.95}})
(p2/add-body! world {:type :fixed :position [0 2] :rotation 0
                     :collider {:shape :ball :radius 0.9
                                :restitution 1.0}})
(def balls
  (vec
    (for [i (range 30)]
      (p2/add-body! world
                    {:type :dynamic
                     :position [(- (mod i 6) 2.5) (+ 6 (* 0.3 (quot i 6)))]
                     :linvel [(* 5 (- 0.5 (/ (mod i 7) 7.0)))
                              (* -3 (/ (mod i 3) 3.0))]
                     :collider {:shape :ball
                                :radius 0.35
                                :restitution 0.95
                                :friction 0.02
                                :density 0.6}}))))
(defn frame! []
  (p2/step! world)
  (mapv (fn [b]
          (let [[x y] (p2/translation world b)]
            [x y 0.35]))
        balls))
        "#,
        "2d-pinball",
    );
}

#[test]
fn demo_3d_explosion() {
    run_demo(
        r#"
(require '[cljrs.physics.d3 :as p3])
(def world (p3/world {:gravity [0 -6 0]}))
(p3/add-body! world {:type :fixed :position [0 -5 0]
                     :collider {:shape :box :half-extents [14 0.3 14]
                                :restitution 0.6}})
(p3/add-body! world {:type :fixed :position [14 0 0]
                     :collider {:shape :box :half-extents [0.3 6 14]
                                :restitution 0.8}})
(p3/add-body! world {:type :fixed :position [-14 0 0]
                     :collider {:shape :box :half-extents [0.3 6 14]
                                :restitution 0.8}})
(p3/add-body! world {:type :fixed :position [0 0 14]
                     :collider {:shape :box :half-extents [14 6 0.3]
                                :restitution 0.8}})
(p3/add-body! world {:type :fixed :position [0 0 -14]
                     :collider {:shape :box :half-extents [14 6 0.3]
                                :restitution 0.8}})
(def balls
  (vec
    (for [i (range 90)]
      (let [phi  (* i 2.399963)
            y    (- 1.0 (/ (* i 2.0) 89.0))
            r    (sqrt (max 0.0001 (- 1.0 (* y y))))
            dx   (* r (cos phi))
            dz   (* r (sin phi))
            speed 18.0]
        (p3/add-body! world
                      {:type :dynamic
                       :position [(* 0.2 dx) (+ 1 (* 0.2 y)) (* 0.2 dz)]
                       :linvel [(* speed dx) (* speed y) (* speed dz)]
                       :collider {:shape :ball
                                  :radius 0.28
                                  :restitution 0.75
                                  :friction 0.05
                                  :density 0.8}})))))
(defn frame! []
  (p3/step! world)
  (mapv (fn [b]
          (let [[x y z] (p3/translation world b)]
            [x y z 0.28]))
        balls))
        "#,
        "3d-explosion",
    );
}

#[test]
fn demo_3d_orbit() {
    run_demo(
        r#"
(require '[cljrs.physics.d3 :as p3])
(def world (p3/world {:gravity [0 0 0]}))
(p3/add-body! world {:type :fixed :position [0 0 0]
                     :collider {:shape :ball :radius 0.8
                                :restitution 0.3}})
(def balls
  (vec
    (for [i (range 35)]
      (let [ring (quot i 12)
            r    (+ 3 (* 1.3 ring))
            th   (* (mod i 12) 0.5236)
            x    (* r (cos th))
            z    (* r (sin th))
            y    (* 0.15 (- (mod i 3) 1))
            speed (* 1.3 (sqrt (/ 12.0 r)))
            vx   (* speed (- (sin th)))
            vz   (* speed (cos th))]
        (p3/add-body! world
                      {:type :dynamic
                       :position [x y z]
                       :linvel [vx 0 vz]
                       :collider {:shape :ball
                                  :radius 0.28
                                  :restitution 0.6
                                  :friction 0.1
                                  :density 1.0}})))))
(defn frame! []
  (doseq [b balls]
    (let [[x y z] (p3/translation world b)
          r2 (max 1.0 (+ (* x x) (* y y) (* z z)))
          k  (/ 3.2 r2)]
      (p3/apply-impulse! world b
                         [(* k (- x)) (* k (- y)) (* k (- z))])))
  (p3/step! world)
  (mapv (fn [b]
          (let [[x y z] (p3/translation world b)]
            [x y z 0.28]))
        balls))
        "#,
        "3d-orbit",
    );
}
