import numpy as np
import pytest

from aegisx_model.core import ModelError
from aegisx_model.module.data import Dataset
from aegisx_model.module.features import Features
from aegisx_model.module.network import Network
from aegisx_model.module.train import Trainer


def test_feature_boundary ():

    features = Features()
    assert features.normalize([[0] * 296]).tolist() == [[0] * 296]
    assert features.normalize([[1e12] * 296]).tolist() == [[1] * 296]
    with pytest.raises(ModelError): features.normalize([[float("nan")] * 296])
    with pytest.raises(ModelError): features.normalize([[-1] * 296])
    with pytest.raises(ModelError): features.normalize([[0] * 39])


def test_sessions_do_not_leak ():

    data = Dataset.demo(42)
    train, validation, test = data.split(42)
    assert set(data.groups[train]).isdisjoint(data.groups[test])
    assert set(data.groups[validation]).isdisjoint(data.groups[test])
    assert set(data.groups[train]).isdisjoint(data.groups[validation])
    assert np.array_equal(data.values, Dataset.demo(42).values)
    assert Network().parameters_count() == 1288449


def test_training_exports_equivalent_model ( tmp_path ):

    report = Trainer().run(Dataset.demo(42), tmp_path, 3, 42)
    assert report["max_onnx_error"] < 1e-5
    assert report["source"] == "synthetic_demo"
    assert (tmp_path / "model.onnx").is_file()
    assert (tmp_path / "checkpoint.pt").is_file()



def test_prepared_features_are_validated_without_a_second_matrix_copy():
    import numpy as np
    import pytest
    from aegisx_model.module.features import Features
    from aegisx_model.module.data import Dataset
    from aegisx_model.core import ModelError
    rows = np.zeros((2, len(Features().names)), dtype=np.float32)
    data = Dataset(rows, [0, 1], ["a", "b"], "unit", normalized=True)
    assert data.values is rows
    rows[0, 0] = 1.01
    with pytest.raises(ModelError, match="Normalized features"):
        Dataset(rows, [0, 1], ["a", "b"], "unit", normalized=True)
