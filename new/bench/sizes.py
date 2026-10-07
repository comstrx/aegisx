#!/usr/bin/env python3

import re
import sys


def main ():

    path = sys.argv[1]
    wanted = re.compile(sys.argv[2]) if len(sys.argv) > 2 else None
    limit = int(sys.argv[3]) if len(sys.argv) > 3 else 40
    header = re.compile(r"print-type-size type: `(.*)`: (\d+) bytes, alignment: (\d+) bytes")
    seen = {}

    with open(path, errors = "replace") as source:

        for line in source:

            match = header.match(line)

            if not match: continue

            name, size = match.group(1), int(match.group(2))

            if wanted and not wanted.search(name): continue

            seen[name] = max(size, seen.get(name, 0))

    for name, size in sorted(seen.items(), key = lambda item: -item[1])[:limit]:

        print(f"{size:>8}  {name[:190]}")


if __name__ == "__main__":

    main()
