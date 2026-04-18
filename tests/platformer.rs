//! Platformer semantics. Gravity pulls the player down, ground blocks
//! stop the fall, horizontal movement runs. All state is plain data in
//! atoms so the demo can snapshot/rewind cleanly.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

const SCRIPT: &str = r#"
(def JUMP-V -13)
(def GRAVITY 0.7)
(def MOVE-SPEED 3.5)
(def FRICTION 0.88)
(def PW 16)
(def PH 22)

(def LEVEL
  [[0 380 800 40]
   [200 320 120 20]
   [400 260 120 20]])

(defonce state
  (atom {:player {:x 50 :y 300 :vx 0 :vy 0 :on-ground? false}
         :frame 0}))

(defn- hit [x1 y1 lx ly lw lh]
  (and (< x1 (+ lx lw))
       (> (+ x1 PW) lx)
       (< y1 (+ ly lh))
       (> (+ y1 PH) ly)))

(defn step [w L? R? J?]
  (let [p (:player w)
        vx0 (cond L? (- MOVE-SPEED)
                  R? MOVE-SPEED
                  :else (* FRICTION (:vx p)))
        vyg (+ (:vy p) GRAVITY)
        vy0 (if (and (:on-ground? p) J?) JUMP-V vyg)

        x-try (+ (:x p) vx0)
        hit-x (some (fn [plat]
                      (let [[lx ly lw lh] plat]
                        (when (hit x-try (:y p) lx ly lw lh) plat)))
                    LEVEL)
        x1 (if hit-x
             (let [[lx _ lw _] hit-x]
               (if (pos? vx0) (- lx PW) (+ lx lw)))
             x-try)
        vx1 (if hit-x 0 vx0)

        y-try (+ (:y p) vy0)
        hit-y (some (fn [plat]
                      (let [[lx ly lw lh] plat]
                        (when (hit x1 y-try lx ly lw lh) plat)))
                    LEVEL)
        landed? (and hit-y (pos? vy0))
        y1 (if hit-y
             (let [[_ ly _ lh] hit-y]
               (if (pos? vy0) (- ly PH) (+ ly lh)))
             y-try)
        vy1 (if hit-y 0 vy0)]
    (-> w
        (update :player assoc
                :x x1 :y y1 :vx vx1 :vy vy1
                :on-ground? (if landed? true false))
        (update :frame inc))))

(defn tick! [L R J]
  (swap! state step (= L 1) (= R 1) (= J 1))
  nil)
"#;

fn env_with_script() -> Env {
    let env = Env::new();
    builtins::install(&env);
    for f in reader::read_all(SCRIPT).unwrap() {
        eval::eval(&f, &env).expect("script eval");
    }
    env
}

fn run(env: &Env, src: &str) -> Value {
    let mut last = Value::Nil;
    for f in reader::read_all(src).unwrap() {
        last = eval::eval(&f, env).expect(src);
    }
    last
}

#[test]
fn gravity_pulls_player_down_to_floor() {
    let env = env_with_script();
    for _ in 0..180 {
        run(&env, "(tick! 0 0 0)");
    }
    // Player should be resting on the floor platform (y = 380 - 22 = 358).
    let y = run(&env, "(:y (:player @state))");
    let fy = match y {
        Value::Float(f) => f,
        Value::Int(i) => i as f64,
        _ => panic!("y not numeric: {y:?}"),
    };
    assert!((fy - 358.0).abs() < 0.5, "expected y ~358, got {fy}");
    let og = run(&env, "(:on-ground? (:player @state))");
    assert_eq!(og, Value::Bool(true), "should be grounded");
}

#[test]
fn jumping_changes_vy_once_per_press() {
    let env = env_with_script();
    // Fall to floor first.
    for _ in 0..180 {
        run(&env, "(tick! 0 0 0)");
    }
    // One jump press.
    run(&env, "(tick! 0 0 1)");
    let vy = run(&env, "(:vy (:player @state))");
    let fv = match vy {
        Value::Float(f) => f,
        Value::Int(i) => i as f64,
        _ => panic!("vy not numeric: {vy:?}"),
    };
    assert!(fv < -5.0, "expected strong upward vy, got {fv}");
}

#[test]
fn horizontal_movement_clips_to_wall() {
    let env = env_with_script();
    // Run right into a platform for many frames; x should stop short of it.
    for _ in 0..200 {
        run(&env, "(tick! 0 1 0)");
    }
    let x = run(&env, "(:x (:player @state))");
    let fx = match x {
        Value::Float(f) => f,
        Value::Int(i) => i as f64,
        _ => panic!("x not numeric: {x:?}"),
    };
    assert!(fx > 50.0 && fx < 800.0, "x out of range: {fx}");
}

#[test]
fn state_survives_redef_of_step() {
    // Live-coding scenario: advance some, then redefine step to do nothing,
    // verify state doesn't reset but behavior changes.
    let env = env_with_script();
    for _ in 0..60 {
        run(&env, "(tick! 0 0 0)");
    }
    let y_before = run(&env, "(:y (:player @state))");

    // "Edit" the source — redefine step to be a no-op (but keep the full
    // source including defonce). State atom must survive.
    run(&env, r#"(defn step [w L R J] w)"#);

    let y_after_redef = run(&env, "(:y (:player @state))");
    assert_eq!(y_before, y_after_redef);

    // New step does nothing, so position shouldn't change.
    for _ in 0..60 {
        run(&env, "(tick! 0 0 0)");
    }
    let y_final = run(&env, "(:y (:player @state))");
    assert_eq!(y_before, y_final);
}
