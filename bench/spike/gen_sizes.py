"""Writes synthetic traces in the spike-replay format to compare backends as polynomials grow:
for each size T, products of two random polynomials with T terms in 5 variables, divisions of
(x_i - x_j) * A by x_i - x_j (exact) and of a random A (not exact). One file per size:

    python3 gen_sizes.py DIR      # writes DIR/size-T.txt
"""

import itertools
import os
import random
import sys

N = 5
rng = random.Random(1)


def monomials(d):
    return [m for m in itertools.product(range(d + 1), repeat=N) if sum(m) <= d]


def rand_poly(t, d):
    ms = rng.sample(monomials(d), t)
    return {m: rng.choice([-9, -5, -3, -2, -1, 1, 2, 3, 5, 9]) for m in ms}


def times_diff(a, i, j):
    out = {}
    for m, c in a.items():
        for v, s in ((i, 1), (j, -1)):
            e = list(m)
            e[v] += 1
            e = tuple(e)
            out[e] = out.get(e, 0) + s * c
    return {m: c for m, c in out.items() if c}


def fmt(p):
    return " ".join([str(len(p))] + [f"{c} " + " ".join(map(str, m)) for m, c in p.items()])


for t, d in ((4, 4), (16, 5), (64, 7), (256, 9), (1024, 12)):
    k = max(5, 20000 // t)
    with open(os.path.join(sys.argv[1], f"size-{t}.txt"), "w") as f:
        for _ in range(k):
            a, b = rand_poly(t, d), rand_poly(t, d)
            i, j = sorted(rng.sample(range(N), 2))
            print(f"mul {N} {fmt(a)} {fmt(b)}", file=f)
            print(f"div {N} {i} {j} {fmt(times_diff(a, i, j))}", file=f)
            print(f"div {N} {i} {j} {fmt(b)}", file=f)
