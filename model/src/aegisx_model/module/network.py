import json
from importlib.resources import files
from torch import nn


class Network ( nn.Module ):

    def __init__ ( self ):

        super().__init__()
        self.architecture = json.loads(files("aegisx_model").joinpath("architecture.json").read_text())
        sizes = self.architecture["layers"]
        layers = []
        for index, (left, right) in enumerate(zip(sizes, sizes[1:], strict=False)):
            layers.append(nn.Linear(left, right))
            if index < len(sizes) - 2: layers.append(nn.ReLU())
        self.layers = nn.Sequential(*layers)

    def forward ( self, values ):

        return self.layers(values)

    def parameters_count ( self ) -> int:

        return sum(value.numel() for value in self.parameters())
