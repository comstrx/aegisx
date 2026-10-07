import re
from .features import Features


class Content:
    def __init__ ( self, spec=None ):
        self.spec = spec or Features().spec
        self.buckets = self.spec["lexical"]["buckets_per_width"]
        self.patterns = [[value.encode("ascii") for value in items] for items in self.spec["content_patterns"]]

    @staticmethod
    def decode ( value ):
        value = value.replace(b"+", b" ")
        return re.sub(rb"%([0-9a-fA-F]{2})", lambda match: bytes([int(match[1], 16)]), value)

    def extract ( self, sample, total=None ):
        text = self.decode(self.decode(sample)).lower()
        size = max(1, len(text))
        def count ( choices ): return sum(value in choices for value in text)
        punctuation = b"""!"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"""
        rows = [
            len(sample), sum(value > 127 for value in text) / size,
            sum(value < 32 or value == 127 for value in text) / size,
            count(b"0123456789") / size, count(punctuation) / size,
            sample.count(b"%"), count(b"""'"`"""), count(b"<>{}[]();|&"),
        ]
        for patterns in self.patterns:
            rows.append(sum(self.occurrences(text, pattern) for pattern in patterns))
        rows.extend([int((len(sample) if total is None else total) > len(sample)), int(bool(sample))])
        for width in (2, 3):
            buckets = [0] * self.buckets
            for index in range(len(text) - width + 1):
                state = 2166136261
                for value in text[index:index + width]: state = ((state ^ value) * 16777619) & 0xffffffff
                buckets[(state ^ (state >> 16)) & (self.buckets - 1)] += 1
            rows.extend(value / max(1, len(text) - width + 1) for value in buckets)
        return rows

    @staticmethod
    def occurrences ( text, pattern ):
        count, start = 0, 0
        while (index := text.find(pattern, start)) != -1:
            count += 1
            start = index + 1
        return count

    def row ( self, sample, total=None ):
        values = self.extract(sample, total)
        return [0] * 16 + values[:16] + [0] * 8 + values[16:]
