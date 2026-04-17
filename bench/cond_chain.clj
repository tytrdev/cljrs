;; A cond-based dispatch chain. Exercises the macro system (cond expands
;; recursively into nested if-forms) and the if special form.
(defmacro cond [& clauses]
  (if (empty? clauses)
    nil
    `(if ~(first clauses)
       ~(nth clauses 1)
       (cond ~@(rest (rest clauses))))))

(defn classify [n]
  (cond
    (= n 1)  :one
    (= n 2)  :two
    (= n 3)  :three
    (= n 4)  :four
    (= n 5)  :five
    (= n 6)  :six
    (= n 7)  :seven
    (= n 8)  :eight
    (= n 9)  :nine
    :else    :other))

;; one bench iteration: classify each number 1..10 once
(loop [i 1 acc 0]
  (if (> i 10)
    acc
    (recur (+ i 1)
           (if (= (classify i) :other) (+ acc 1) acc))))
