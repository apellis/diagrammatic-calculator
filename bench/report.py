"""Tables and comparisons of diagcalc-bench results (format diagcalc-bench/1).

    python3 bench/report.py summary NEW.json            # Markdown table
    python3 bench/report.py compare OLD.json NEW.json   # table with changes; exit 1 on a slowdown

A slowdown is a benchmark more than 10% slower than before, counting only benchmarks that take
at least 5 ms in both runs (shorter ones are within the noise of shared CI machines).
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


def compare(old, new):
    before = {key(r): r for r in old["results"]}
    slower = []
    print(f"Benchmarks compared with {old.get('label') or 'the previous run'}\n")
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
        print(f"\n{len(slower)} benchmark(s) more than 10% slower.")
    return not slower and all(r["check"] for r in new["results"])


def main(argv):
    if len(argv) == 3 and argv[1] == "summary":
        return 0 if summary(load(argv[2])) else 1
    if len(argv) == 4 and argv[1] == "compare":
        return 0 if compare(load(argv[2]), load(argv[3])) else 1
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
