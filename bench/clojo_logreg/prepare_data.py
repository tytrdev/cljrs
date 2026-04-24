#!/usr/bin/env python3
"""
Download the Wisconsin breast cancer dataset from sklearn, z-score
each feature, split into train/test, and dump the four arrays as raw
little-endian f64 binary files Mojo can mmap directly.

Usage:
    uv run --with scikit-learn --with numpy \
        bench/clojo_logreg/prepare_data.py

Writes:
    bench/clojo_logreg/data/X_train.f64
    bench/clojo_logreg/data/y_train.f64
    bench/clojo_logreg/data/X_test.f64
    bench/clojo_logreg/data/y_test.f64
    bench/clojo_logreg/data/meta.json

Also trains sklearn's own LogisticRegression on the same split so we
have a baseline accuracy to compare against.
"""
import json
import os
import sys

import numpy as np
from sklearn.datasets import load_breast_cancer
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import StandardScaler

OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data")
os.makedirs(OUT_DIR, exist_ok=True)


def main():
    ds = load_breast_cancer()
    X, y = ds.data.astype(np.float64), ds.target.astype(np.float64)
    print(f"dataset: {X.shape[0]} samples × {X.shape[1]} features, "
          f"positive={int(y.sum())}/{len(y)}")

    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.2, random_state=42, stratify=y,
    )
    # z-score using train stats; test gets the same transform.
    sc = StandardScaler().fit(X_train)
    X_train_s = sc.transform(X_train)
    X_test_s = sc.transform(X_test)

    # Write raw f64 little-endian. Row-major, contiguous.
    for name, arr in [
        ("X_train", X_train_s),
        ("y_train", y_train),
        ("X_test", X_test_s),
        ("y_test", y_test),
    ]:
        path = os.path.join(OUT_DIR, f"{name}.f64")
        arr.astype("<f8").tobytes()  # validate
        with open(path, "wb") as f:
            f.write(arr.astype("<f8").tobytes())
        print(f"wrote {path}  shape={arr.shape}  {arr.nbytes} bytes")

    # Train sklearn baseline on the same split.
    base = LogisticRegression(max_iter=1000, C=1e6)  # near-unregularized
    base.fit(X_train_s, y_train)
    base_train_acc = base.score(X_train_s, y_train)
    base_test_acc = base.score(X_test_s, y_test)
    print(f"sklearn LR: train={base_train_acc:.4f} test={base_test_acc:.4f}")

    meta = {
        "n_train": int(X_train_s.shape[0]),
        "n_test": int(X_test_s.shape[0]),
        "n_features": int(X_train_s.shape[1]),
        "feature_names": list(ds.feature_names),
        "positive_class_name": str(ds.target_names[1]),
        "negative_class_name": str(ds.target_names[0]),
        "sklearn_train_acc": float(base_train_acc),
        "sklearn_test_acc": float(base_test_acc),
        "sklearn_iters": 1000,
        "sklearn_C": 1e6,
        "scaler_mean": sc.mean_.tolist(),
        "scaler_scale": sc.scale_.tolist(),
    }
    with open(os.path.join(OUT_DIR, "meta.json"), "w") as f:
        json.dump(meta, f, indent=2)
    print(f"wrote {os.path.join(OUT_DIR, 'meta.json')}")


if __name__ == "__main__":
    main()
