import numpy as np
import pytest
from aegisx_model.core import ModelError
from aegisx_model.module.metrics import Metrics
from aegisx_model.module.sources import Sources

def test_exact_calibration_matches_every_boundary_and_respects_ties():
    rng=np.random.default_rng(7)
    scores=np.round(rng.random(1200),3)
    labels=rng.integers(0,2,1200)
    allowed=[Metrics.report(labels,scores,value) for value in np.unique(np.r_[scores,1.0])]
    allowed=[row for row in allowed if row["false_positive_rate"]<=.01]
    best=max(allowed,key=lambda row:(row["recall"],row["precision"],row["threshold"]))
    assert Metrics.calibrate(labels,scores,.01)==best["threshold"]

def test_impossible_calibration_is_explicit():
    with pytest.raises(ModelError): Metrics.calibrate([0,1],[1.,1.],0)

def test_cross_source_templates_are_joined_and_conflicting_labels_are_removed():
    data=Sources.build([(b"id=123",0,"a"),(b"id=456",0,"b"),(b"conflict",0,"a"),(b"conflict",1,"b")],{},"http_corpus")
    assert len(data.values)==2
    assert data.groups[0]==data.groups[1]
    assert data.provenance["conflicting_payloads_removed"]==1
