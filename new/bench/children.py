import collections
import re
import sys


def clean ( symbol ):

    symbol = re.sub(r"\+0x[0-9a-f]+$", "", symbol)
    symbol = re.sub(r"::h[0-9a-f]{16}$", "", symbol)

    return symbol


def main ():

    parents = sys.argv[1:]
    total = 0
    tables = {parent: collections.Counter() for parent in parents}
    frames = []

    def flush ():

        nonlocal total

        if not frames: return

        total += 1

        for parent in parents:

            position = next((index for index, frame in enumerate(frames) if parent in frame), None)

            if position is None: continue

            tables[parent]["(self)" if position == 0 else frames[position - 1]] += 1

        frames.clear()

    for line in sys.stdin:

        if not line.strip(): flush(); continue

        if not line.startswith("\t") and not line.startswith(" "): continue

        parts = line.strip().split(" ", 1)

        if len(parts) == 2 and re.match(r"^[0-9a-f]+$", parts[0]): frames.append(clean(parts[1].rsplit(" (", 1)[0]))

    flush()

    if not total: print("no samples"); return

    print(f"samples {total}")

    for parent, table in tables.items():

        inside = sum(table.values())

        print(f"\n## under `{parent}`: {inside * 100 / total:.2f}% of all samples")

        for symbol, count in table.most_common(45): print(f"{count * 100 / total:6.2f}%  {symbol[:170]}")


if __name__ == "__main__":

    main()
