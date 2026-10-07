import collections
import re
import sys


def clean ( symbol ):

    symbol = re.sub(r"\+0x[0-9a-f]+$", "", symbol)
    symbol = re.sub(r"::h[0-9a-f]{16}$", "", symbol)

    return symbol[:140]


def main ():

    leaves = set(sys.argv[1:]) or {"__memcpy_avx_unaligned_erms", "__memmove_avx_unaligned_erms", "mi_page_malloc_zero", "_mi_page_malloc_zero", "mi_free_ex"}
    total = 0
    inclusive = collections.Counter()
    exclusive = collections.Counter()
    callers = {leaf: collections.Counter() for leaf in leaves}
    frames = []

    def flush ():

        nonlocal total

        if not frames: return

        total += 1
        exclusive[frames[0]] += 1

        for symbol in set(frames): inclusive[symbol] += 1

        for leaf in leaves:

            if frames[0] == leaf:

                parent = next((frame for frame in frames[1:] if frame != leaf), "?")
                callers[leaf][parent] += 1

        frames.clear()

    for line in sys.stdin:

        if not line.strip(): flush(); continue

        if not line.startswith("\t") and not line.startswith(" "): continue

        parts = line.strip().split(" ", 1)

        if len(parts) == 2 and re.match(r"^[0-9a-f]+$", parts[0]):

            symbol = parts[1].rsplit(" (", 1)[0]
            frames.append(clean(symbol))

    flush()

    if not total: print("no samples"); return

    print(f"samples {total}")
    print()
    print("## inclusive (sample appears anywhere in the stack)")

    for symbol, count in inclusive.most_common(60): print(f"{100 * count / total:6.2f}%  {symbol}")

    print()
    print("## exclusive (leaf)")

    for symbol, count in exclusive.most_common(25): print(f"{100 * count / total:6.2f}%  {symbol}")

    for leaf in leaves:

        print()
        print(f"## callers of {leaf}")

        for symbol, count in callers[leaf].most_common(20): print(f"{100 * count / total:6.2f}%  {symbol}")


main()
