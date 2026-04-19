;; cljrs.music — tiny music-theory toolkit authored in cljrs.
;;
;; Loaded after cljrs_test.clj by `install_prelude`. Lives in the
;; `cljrs.music` namespace so callers reach it as
;; `cljrs.music/scale`, `cljrs.music/chord`, etc.
;;
;; All functions are pure data → data. No audio, no scheduling.
;; The browser side (docs/sequencer.html) reads vectors out of here
;; and plays them through Web Audio.
;;
;; Implementation note: cljrs resolves unqualified symbols against
;; the *caller's* namespace, not the defining namespace, so every
;; intra-library reference here is fully qualified as
;; cljrs.music/foo. Yes, it's verbose; it keeps callers in any ns
;; from having to (:refer ...) our helpers.

(ns cljrs.music)

;; ---------------------------------------------------------------
;; MIDI <-> Hz
;; ---------------------------------------------------------------
;; Standard A4 = MIDI 69 = 440 Hz tuning.
;;   hz = 440 * 2^((n - 69) / 12)

(defn midi->hz [n]
  (* 440.0 (pow 2.0 (/ (- n 69) 12.0))))

(defn hz->midi [f]
  (+ 69.0 (* 12.0 (/ (log (/ f 440.0)) (log 2.0)))))

;; ---------------------------------------------------------------
;; Note keyword parsing — :c4, :a#3, :eb5
;; ---------------------------------------------------------------

(def __pc-table
  {"c" 0  "d" 2  "e" 4  "f" 5  "g" 7  "a" 9  "b" 11})

(defn __char-at [s i] (subs s i (+ i 1)))

(defn __lower [c]
  ;; Normalize ASCII A-G to a-g; pass anything else through unchanged.
  (cond
    (= c "A") "a" (= c "B") "b" (= c "C") "c"
    (= c "D") "d" (= c "E") "e" (= c "F") "f"
    (= c "G") "g"
    :else c))

(defn note [kw]
  ;; Accept a keyword like :c4, :a#3, :eb5 and return a MIDI integer.
  ;; Form: letter, optional # or b, then octave digits.
  (let [s     (name kw)
        n     (count s)
        head  (cljrs.music/__lower (cljrs.music/__char-at s 0))
        pc    (get cljrs.music/__pc-table head)
        acc   (cond
                (= n 1) 0
                (= (cljrs.music/__char-at s 1) "#") 1
                (= (cljrs.music/__char-at s 1) "b") -1
                :else 0)
        oct-s (cond
                (= n 1) "4"
                (= acc 0) (subs s 1 n)
                :else (subs s 2 n))
        oct   (read-string oct-s)]
    (when (nil? pc)
      (throw (ex-info "note: unknown pitch class" {:kw kw})))
    (+ (* 12 (+ oct 1)) pc acc)))

;; ---------------------------------------------------------------
;; Scales
;; ---------------------------------------------------------------

(def __scale-table
  {:major       [0 2 4 5 7 9 11 12]
   :minor       [0 2 3 5 7 8 10 12]
   :aeolian     [0 2 3 5 7 8 10 12]
   :dorian      [0 2 3 5 7 9 10 12]
   :phrygian    [0 1 3 5 7 8 10 12]
   :lydian      [0 2 4 6 7 9 11 12]
   :mixolydian  [0 2 4 5 7 9 10 12]
   :locrian     [0 1 3 5 6 8 10 12]
   :pentatonic  [0 2 4 7 9 12]
   :blues       [0 3 5 6 7 10 12]})

(defn scale [root mode]
  (let [intervals (get cljrs.music/__scale-table mode)]
    (when (nil? intervals)
      (throw (ex-info "scale: unknown mode" {:mode mode})))
    (mapv (fn [i] (+ root i)) intervals)))

;; ---------------------------------------------------------------
;; Chords
;; ---------------------------------------------------------------

(def __chord-table
  {:maj   [0 4 7]
   :min   [0 3 7]
   :dim   [0 3 6]
   :aug   [0 4 8]
   :sus2  [0 2 7]
   :sus4  [0 5 7]
   :7     [0 4 7 10]
   :maj7  [0 4 7 11]
   :min7  [0 3 7 10]
   :dim7  [0 3 6 9]})

(defn chord [root quality]
  (let [intervals (get cljrs.music/__chord-table quality)]
    (when (nil? intervals)
      (throw (ex-info "chord: unknown quality" {:quality quality})))
    (mapv (fn [i] (+ root i)) intervals)))

;; ---------------------------------------------------------------
;; Progressions — diatonic-major degree resolution.
;; ---------------------------------------------------------------

(def __degree-offsets
  {1 0  2 2  3 4  4 5  5 7  6 9  7 11})

(defn progression [key qualities]
  (mapv (fn [pair]
          (let [degree  (first pair)
                quality (first (rest pair))
                off     (get cljrs.music/__degree-offsets degree)]
            (when (nil? off)
              (throw (ex-info "progression: degree must be 1-7"
                              {:degree degree})))
            (cljrs.music/chord (+ key off) quality)))
        qualities))

;; ---------------------------------------------------------------
;; Transpose
;; ---------------------------------------------------------------

(defn transpose [notes semitones]
  (mapv (fn [n] (+ n semitones)) notes))

;; ---------------------------------------------------------------
;; Arpeggiate — reorder a chord into a melodic pattern.
;; ---------------------------------------------------------------
;; :random is deterministic (no rand in cljrs yet) — it cycles
;; through a fixed perm, which is enough variety for a sequencer.

(defn __reverse-vec [xs]
  (let [n (count xs)]
    (mapv (fn [i] (nth xs (- (- n 1) i))) (range n))))

(defn __updown [xs]
  (let [n (count xs)]
    (if (<= n 1)
      xs
      ;; up then down without repeating top + bottom notes.
      (let [down-mid (mapv (fn [i] (nth xs (- (- n 2) i)))
                           (range (- n 2)))]
        (vec (concat xs down-mid))))))

(def __arp-perm [2 0 3 1 4 5 6])

(defn __pseudo-random [xs]
  (let [n (count xs)]
    (mapv (fn [i] (nth xs (mod (nth cljrs.music/__arp-perm
                                    (mod i (count cljrs.music/__arp-perm)))
                               n)))
          (range n))))

(defn arpeggiate [chord-notes pattern]
  (cond
    (= pattern :up)     (vec chord-notes)
    (= pattern :down)   (cljrs.music/__reverse-vec (vec chord-notes))
    (= pattern :updown) (cljrs.music/__updown (vec chord-notes))
    (= pattern :random) (cljrs.music/__pseudo-random (vec chord-notes))
    :else (throw (ex-info "arpeggiate: unknown pattern" {:pattern pattern}))))
