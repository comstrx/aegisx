#!/usr/bin/env python3

import re
import sys

from pathlib import Path

STATUSES = ("done", "wip", "todo", "gap", "skip")


def main ():

    path = Path(sys.argv[1])
    text = path.read_text()
    counts = {}
    family = None

    for line in text.splitlines():

        if line.startswith("## "):

            family = next(( name for name in ("nginx", "Caddy") if line[3:].startswith(name) ), None)

            continue

        cells = [cell.strip() for cell in line.split("|")]

        if family and len(cells) >= 4 and cells[2] in STATUSES: counts.setdefault(family, dict.fromkeys(STATUSES, 0))[cells[2]] += 1

    lines = ["| against | done | wip | todo | gap | skip | coverage |", "|---|--:|--:|--:|--:|--:|--:|"]
    total = dict.fromkeys(STATUSES, 0)

    for family, tally in [*counts.items(), ( "both", total )]:

        if family != "both":

            for status in STATUSES: total[status] += tally[status]

        counted = tally["done"] + tally["wip"] + tally["todo"] + tally["gap"]

        lines.append(f"| {family} | {tally['done']} | {tally['wip']} | {tally['todo']} | {tally['gap']} | {tally['skip']} | {tally['done'] * 100 / max(counted, 1):.1f} % |")

    block = "<!-- coverage:begin -->\n" + "\n".join(lines) + "\n<!-- coverage:end -->"

    path.write_text(re.sub(r"<!-- coverage:begin -->.*<!-- coverage:end -->", block, text, flags = re.S))

    print("\n".join(lines))


if __name__ == "__main__":

    main()
