//! Timeline + input recording on top of the phase-1 platformer:
//! - each tick! records both state and the input that produced it
//! - frame-step! bundles advance + readout + ghost into one call
//! - scrubbed replay uses recorded inputs, not currently-held keys
//! - changing constants mid-session changes the replay ghost

use cljrs::{builtins, env::Env, eval, reader, value::Value};

const SCRIPT: &str = r#"
(def JUMP-V -13)
(def GRAVITY 0.7)
(def MOVE-SPEED 3.5)
(def FRICTION 0.88)
(def PW 16) (def PH 22)
(def HISTORY-LEN 30)
(def PREDICT-LEN 8)

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

(defn- push-history [h entry]
  (let [h' (conj h entry) n (count h')]
    (if (> n HISTORY-LEN) (vec (drop (- n HISTORY-LEN) h')) h')))

(defn tick! [L R J]
  (swap! state step (= L 1) (= R 1) (= J 1))
  (swap! history push-history {:state @state :input [L R J]})
  nil)

(defn- predict-from [w inputs]
  (loop [w w xs inputs out []]
    (if (empty? xs) out
      (let [[L R J] (first xs)
            w' (step w (= L 1) (= R 1) (= J 1))
            p (:player w')]
        (recur w' (rest xs) (-> out (conj (:x p)) (conj (:y p))))))))

(defn frame-step! [advance k L R J]
  (when (= advance 1) (tick! L R J))
  (let [h @history n (count h)
        live? (< k 0)
        entry (when (and (not live?) (< k n)) (nth h k))
        s (if live? @state (if entry (:state entry) @state))
        p (:player s)
        inputs (if live?
                 (vec (repeat PREDICT-LEN [L R J]))
                 (vec (take PREDICT-LEN (map :input (drop (inc k) h)))))
        ghost (predict-from s inputs)
        lp (:player @state)]
    (-> [(:x p) (:y p)
         (if (:on-ground? p) 1.0 0.0)
         (:vx p) (:vy p)
         (:frame s) n
         (:x lp) (:y lp)]
        (into ghost))))
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
fn history_entries_contain_state_and_input() {
    let env = env_with_script();
    run(&env, "(tick! 1 0 0)");
    run(&env, "(tick! 0 1 0)");
    let e0_input = run(&env, "(:input (nth @history 0))");
    let e1_input = run(&env, "(:input (nth @history 1))");
    // these should round-trip exactly as recorded
    assert_eq!(format!("{e0_input:?}"), "[1 0 0]");
    assert_eq!(format!("{e1_input:?}"), "[0 1 0]");
}

#[test]
fn frame_step_advance_1_ticks() {
    let env = env_with_script();
    run(&env, "(frame-step! 1 -1 0 0 0)");
    run(&env, "(frame-step! 1 -1 0 0 0)");
    assert_eq!(run(&env, "(count @history)"), Value::Int(2));
}

#[test]
fn frame_step_advance_0_does_not_tick() {
    let env = env_with_script();
    run(&env, "(frame-step! 0 -1 0 0 0)");
    run(&env, "(frame-step! 0 -1 0 0 0)");
    assert_eq!(run(&env, "(count @history)"), Value::Int(0));
}

#[test]
fn scrub_ghost_uses_recorded_inputs_not_live_args() {
    // Play a sequence where input is "move right" for 8 frames.
    let env = env_with_script();
    for _ in 0..8 {
        run(&env, "(tick! 0 1 0)");
    }
    // Scrub to frame 0. Ask frame-step! for the ghost, but pass L=1 as
    // the live "currently held" key. The ghost should IGNORE that and
    // replay the recorded right-moves from frame 1 onward.
    let v = run(&env, "(frame-step! 0 0 1 0 0)");
    // out[0..8] is state readout; scrubbed state at k=0 has the player
    // after the first recorded tick (which moved right), so x should be
    // greater than the start (50).
    if let Value::Vector(xs) = v {
        // ghost starts at index 9, pairs of (x, y)
        // frame 1 of replay: one more rightward step; x should exceed frame 0's x
        let ghost_x0 = to_f64(&xs[9]);
        let scrubbed_x = to_f64(&xs[0]);
        assert!(
            ghost_x0 > scrubbed_x,
            "ghost should replay rightward motion, got scrub_x={scrubbed_x} ghost_x0={ghost_x0}"
        );
    } else {
        panic!("not a vector: {v:?}");
    }
}

#[test]
fn changing_jumpv_changes_scrubbed_ghost_arc() {
    // The BV invariant: rewind to a moment, change JUMP-V, the
    // predicted replay arc reflects the new value because step
    // reads live globals.
    // Start with a lot of history capacity so indexing stays stable.
    let env = env_with_script();
    run(&env, "(def HISTORY-LEN 500)");
    for _ in 0..180 {
        run(&env, "(tick! 0 0 0)"); // settle on floor
    }
    // Record one jump press.
    run(&env, "(tick! 0 0 1)");
    for _ in 0..8 {
        run(&env, "(tick! 0 0 0)"); // coast
    }
    // Scrub to the frame BEFORE the jump press. Replay then starts
    // with the recorded jump input, which lets a new JUMP-V reshape
    // the arc.
    let scrub_k = 179i64;
    let v1 = run(&env, &format!("(frame-step! 0 {scrub_k} 0 0 0)"));
    run(&env, "(def JUMP-V -25)");
    let v2 = run(&env, &format!("(frame-step! 0 {scrub_k} 0 0 0)"));
    // Compare ghost y at frame 5 of the replay.
    // ghost starts at offset 9; each frame is (x, y); frame 5 y is at offset 9 + 5*2 + 1 = 20.
    let y5_a = if let Value::Vector(xs) = v1 { to_f64(&xs[20]) } else { panic!() };
    let y5_b = if let Value::Vector(xs) = v2 { to_f64(&xs[20]) } else { panic!() };
    assert!(
        y5_b < y5_a,
        "stronger JUMP-V should send the ghost higher (lower y), got {y5_a} -> {y5_b}"
    );
}
