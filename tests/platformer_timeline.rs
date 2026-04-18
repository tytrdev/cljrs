//! Timeline + predict extensions on top of the phase-1 platformer:
//! - history buffer grows per frame, capped at HISTORY-LEN
//! - snapshot-at returns a past state readout
//! - predict runs step forward n times purely, no atom mutation
//! - fork-from! resets state to a historical snapshot

use cljrs::{builtins, env::Env, eval, reader, value::Value};

const SCRIPT: &str = r#"
(def JUMP-V -13)
(def GRAVITY 0.7)
(def MOVE-SPEED 3.5)
(def FRICTION 0.88)
(def PW 16) (def PH 22)
(def HISTORY-LEN 20)
(def PREDICT-LEN 10)

(def LEVEL [[0 380 800 40]])

(def PLAYER-START {:x 50 :y 300 :vx 0 :vy 0 :on-ground? false})

(defonce state   (atom {:player PLAYER-START :frame 0}))
(defonce history (atom []))

(defn- hit? [x y lx ly lw lh]
  (and (< x (+ lx lw)) (> (+ x PW) lx)
       (< y (+ ly lh)) (> (+ y PH) ly)))

(defn- first-hit [x y]
  (some (fn [plat]
          (let [[lx ly lw lh] plat]
            (when (hit? x y lx ly lw lh) plat))) LEVEL))

(defn step [w L? R? J?]
  (let [p (:player w)
        vx0 (cond L? (- MOVE-SPEED) R? MOVE-SPEED :else (* FRICTION (:vx p)))
        vyg (+ (:vy p) GRAVITY)
        vy0 (if (and (:on-ground? p) J?) JUMP-V vyg)
        x-try (+ (:x p) vx0)
        hx (first-hit x-try (:y p))
        x1 (if hx (let [[lx _ lw _] hx] (if (pos? vx0) (- lx PW) (+ lx lw))) x-try)
        vx1 (if hx 0 vx0)
        y-try (+ (:y p) vy0)
        hy (first-hit x1 y-try)
        landed? (and hy (pos? vy0))
        y1 (if hy (let [[_ ly _ lh] hy] (if (pos? vy0) (- ly PH) (+ ly lh))) y-try)
        vy1 (if hy 0 vy0)]
    (-> w
        (update :player assoc :x x1 :y y1 :vx vx1 :vy vy1 :on-ground? (if landed? true false))
        (update :frame inc))))

(defn- push-history [h s]
  (let [h' (conj h s) n (count h')]
    (if (> n HISTORY-LEN) (vec (drop (- n HISTORY-LEN) h')) h')))

(defn tick! [L R J]
  (swap! state step (= L 1) (= R 1) (= J 1))
  (swap! history push-history @state)
  nil)

(defn- predict-from [w L? R? J? n]
  (loop [w w i 0 out []]
    (if (= i n) out
      (let [w' (step w L? R? J?)
            p (:player w')]
        (recur w' (inc i) (-> out (conj (:x p)) (conj (:y p))))))))

(defn predict [L R J]
  (predict-from @state (= L 1) (= R 1) (= J 1) PREDICT-LEN))

(defn predict-at [k L R J]
  (let [h @history n (count h)
        base (if (and (>= k 0) (< k n)) (nth h k) @state)]
    (predict-from base (= L 1) (= R 1) (= J 1) PREDICT-LEN)))

(defn fork-from! [k]
  (let [h @history n (count h)]
    (when (and (>= k 0) (< k n)) (reset! state (nth h k)))
    nil))
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

fn to_f64(v: &Value) -> f64 {
    match v {
        Value::Float(f) => *f,
        Value::Int(i) => *i as f64,
        _ => panic!("not numeric: {v:?}"),
    }
}

#[test]
fn history_grows_and_caps() {
    let env = env_with_script();
    for _ in 0..5 {
        run(&env, "(tick! 0 0 0)");
    }
    assert_eq!(run(&env, "(count @history)"), Value::Int(5));
    for _ in 0..50 {
        run(&env, "(tick! 0 0 0)");
    }
    // capped at HISTORY-LEN = 20
    assert_eq!(run(&env, "(count @history)"), Value::Int(20));
}

#[test]
fn predict_returns_future_positions_without_mutating() {
    let env = env_with_script();
    // Fall to floor first so predict starts from a stable pose.
    for _ in 0..180 {
        run(&env, "(tick! 0 0 0)");
    }
    let y_before = to_f64(&run(&env, "(:y (:player @state))"));
    let predicted = run(&env, "(predict 0 0 1)");
    let y_after = to_f64(&run(&env, "(:y (:player @state))"));
    assert!((y_before - y_after).abs() < 1e-9, "predict must not mutate state");

    if let Value::Vector(xs) = predicted {
        // 10 predicted frames × (x, y) = 20 floats
        assert_eq!(xs.len(), 20);
        // First pair should have y < y_before (upward jump)
        let y0 = to_f64(&xs[1]);
        assert!(y0 < y_before, "first predicted y {y0} should be above start {y_before}");
    } else {
        panic!("predict did not return a vector: {predicted:?}");
    }
}

#[test]
fn fork_from_rewinds_state_to_snapshot() {
    let env = env_with_script();
    for _ in 0..10 {
        run(&env, "(tick! 0 1 0)"); // move right
    }
    let snap_x = to_f64(&run(&env, "(:x (:player (nth @history 2)))"));
    run(&env, "(fork-from! 2)");
    let now_x = to_f64(&run(&env, "(:x (:player @state))"));
    assert!((snap_x - now_x).abs() < 1e-9, "after fork state should equal snapshot");
}

#[test]
fn predict_sees_live_constant_changes() {
    // BV test: predicted trajectory depends on the *current* value of
    // JUMP-V, not the value when the atom was last updated.
    let env = env_with_script();
    for _ in 0..180 {
        run(&env, "(tick! 0 0 0)");
    }
    let with_default = run(&env, "(predict 0 0 1)");
    run(&env, "(def JUMP-V -25)"); // change jump strength
    let with_bigger = run(&env, "(predict 0 0 1)");
    // With a stronger jump, the player goes higher (lower y) at frame 5 of the
    // prediction.
    let y5_a = if let Value::Vector(xs) = &with_default { to_f64(&xs[11]) } else { panic!() };
    let y5_b = if let Value::Vector(xs) = &with_bigger { to_f64(&xs[11]) } else { panic!() };
    assert!(y5_b < y5_a, "stronger jump should predict higher position: {y5_b} vs {y5_a}");
}
