import json
import statistics
import sys
from pathlib import Path


def rows ( path ):

    data = json.loads(Path(path).read_text())

    return data["trials"]


def median ( items, key ):

    values = [item[key] for item in items if item.get(key) is not None]

    return statistics.median(values) if values else float("nan")


def summarize ( label, path ):

    trials = rows(path)
    out = []

    for connections in sorted({row["connections"] for row in trials}):

        for target in sorted({row["target"] for row in trials}):

            items = [row for row in trials if row["target"] == target and row["connections"] == connections]

            if not items: continue

            out.append((label, connections, target, median(items, "requests_per_second"), median(items, "p50_ms"), median(items, "p99_ms"), median(items, "cpu_us_per_request"), median(items, "user_us_per_request"), median(items, "sys_us_per_request"), median(items, "pss_bytes") / 1e6))

    return out


if __name__ == "__main__":

    print(f"{'run':14} {'conns':>5} {'target':8} {'req/s':>9} {'p50':>6} {'p99':>7} {'cpu':>6} {'user':>5} {'sys':>5} {'pss':>6}")

    for path in sys.argv[1:]:

        for label, connections, target, rps, p50, p99, cpu, user, system, pss in summarize(Path(path).stem, path):

            print(f"{label:14} {connections:>5} {target:8} {rps:>9.0f} {p50:>6.2f} {p99:>7.2f} {cpu:>6.1f} {user:>5.1f} {system:>5.1f} {pss:>6.1f}")
