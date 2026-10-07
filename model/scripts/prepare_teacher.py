from pathlib import Path
from aegisx_model.module.lifecycle.teacher import Teacher
root=Path(__file__).resolve().parents[1]
Teacher.prepare(root / "runs/v09/data", root / "runs/v09/teacher-cache", root / "data/codebert")
