"""Tables and comparisons of diagcalc-bench results (format diagcalc-bench/1).

    python3 bench/report.py summary NEW.json            # Markdown table
    python3 bench/report.py compare OLD.json... NEW.json   # table with changes against OLD

The baseline of a benchmark is the median of its times in the OLD runs. A slowdown is a benchmark
more than 10% slower than its baseline, counting only benchmarks that take at least 5 ms in both
(shorter ones are within the noise of shared CI machines). Slowdowns are reported in the table
and as a GitHub Actions warning, but do not make the exit status nonzero: the runners of shared
CI machines differ by 20-100% from one night to the next on unchanged code, so a timing alone
is not evidence of a regression. The exit status is 1 only when a result check fails (an answer
changed), which is always a real error.
"""

import json
import sys

THRESHOLD = 1.10
FLOOR = 0.005


def load(path):
    with open(path) as f:
        data = json.load(f)
    assert data["format"] == "diagcalc-bench/1", path
    return data


def key(r):
    return (r["id"], r["context"], r["op"])


def fmt_us(r):
    return f"{r['per_op_us']:.1f}"


def summary(new):
    print(f"Benchmarks ({new['platform']}, {'quick' if new['quick'] else 'full'}), "
          f"total {new['total_seconds']:.1f} s\n")
    print("| | Context | Operation | Count | µs per op | Terms | Check |")
    print("|---|---|---|---:|---:|---:|---|")
    for r in new["results"]:
        print(f"| {r['id']} | `{r['context']}` | {r['op']} | {r['count']} | {fmt_us(r)} | "
              f"{r['mean_terms']:.1f} | {'ok' if r['check'] else '**failed**'} |")
    return all(r["check"] for r in new["results"])


def compare(olds, new):
    times = {}
    for old in olds:
        for r in old["results"]:
            times.setdefault(key(r), []).append(r)
    before = {}
    for k, rs in times.items():
        rs = sorted(rs, key=lambda r: r["seconds"])
        before[k] = rs[(len(rs) - 1) // 2]  # lower median, an actual run's record
    slower = []
    labels = [old.get("label") or "a previous run" for old in olds]
    what = (labels[0] if len(labels) == 1
            else f"the median of {len(labels)} runs ({labels[0]} to {labels[-1]})")
    print(f"Benchmarks compared with {what}\n")
    print("| | Context | Operation | Before (µs) | Now (µs) | Change | Check |")
    print("|---|---|---|---:|---:|---:|---|")
    for r in new["results"]:
        b = before.get(key(r))
        change = ""
        if b and b["seconds"] > 0:
            ratio = r["seconds"] / b["seconds"]
            change = f"{(ratio - 1) * 100:+.0f}%"
            if ratio > THRESHOLD and min(r["seconds"], b["seconds"]) >= FLOOR:
                slower.append(r)
                change = f"**{change}**"
        print(f"| {r['id']} | `{r['context']}` | {r['op']} | {fmt_us(b) if b else '–'} | "
              f"{fmt_us(r)} | {change} | {'ok' if r['check'] else '**failed**'} |")
    if slower:
        print(f"\n{len(slower)} benchmark(s) more than 10% slower than the baseline "
              "(timing only; see the module docstring).")
        print(f"::warning title=Benchmarks::{len(slower)} benchmark(s) more than 10% slower "
              "than the baseline", file=sys.stderr)
    return all(r["check"] for r in new["results"])


def main(argv):
    if len(argv) == 3 and argv[1] == "summary":
        return 0 if summary(load(argv[2])) else 1
    if len(argv) >= 4 and argv[1] == "compare":
        return 0 if compare([load(p) for p in argv[2:-1]], load(argv[-1])) else 1
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
