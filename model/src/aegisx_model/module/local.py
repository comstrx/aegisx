import csv
import json
import sqlite3

import numpy as np

from ..core import ModelError
from .features import Features


class LocalData:

    @staticmethod
    def export ( database, labels, output ):

        if not database.is_file(): raise ModelError("Lifecycle database does not exist")
        schema = Features()
        rows = []
        seen = set()
        with labels.open(newline="") as stream, sqlite3.connect(database.resolve().as_uri() + "?mode=ro", uri=True) as connection:
            connection.execute("PRAGMA query_only=ON")
            for number, label in enumerate(csv.DictReader(stream), 2):
                request_id, group = label.get("request_id"), label.get("group")
                if not request_id or request_id in seen or not group or label.get("label") not in {"0", "1"}:
                    raise ModelError(f"Invalid or duplicate independent label at CSV line {number}")
                seen.add(request_id)
                events = connection.execute(
                    "SELECT payload FROM events WHERE request_id=? AND stage='analyzed' ORDER BY sequence",
                    (request_id,),
                ).fetchall()
                if len(events) != 1: raise ModelError(f"Expected one retained analysis snapshot at label line {number}")
                try:
                    details = json.loads(events[0][0])["details"]
                    values = np.asarray(details["features"], dtype=np.float32)
                    if details["feature_version"] != schema.version or values.shape != (len(schema.names),):
                        raise ValueError("Unsupported feature snapshot")
                    if not np.isfinite(values).all() or (values < 0).any() or (values > 1).any():
                        raise ValueError("Invalid normalized features")
                except (ValueError, KeyError, TypeError) as error:
                    raise ModelError(f"Invalid retained features at label line {number}") from error
                rows.append({
                    "features": dict(zip(schema.names, (values * schema.scales).tolist(), strict=True)),
                    "label": int(label["label"]), "group": group,
                })
        if not rows: raise ModelError("No independent labels supplied")
        output.parent.mkdir(parents=True, exist_ok=True)
        temporary = output.with_suffix(output.suffix + ".tmp")
        temporary.write_text("".join(json.dumps(row) + "\n" for row in rows))
        temporary.replace(output)

        return {"rows": len(rows), "output": str(output), "labels": "independent_csv", "feature_version": schema.version}
