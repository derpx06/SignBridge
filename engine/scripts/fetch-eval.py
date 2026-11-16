# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyarrow", "soundfile", "numpy"]
# ///
"""Dev tooling: turns LibriSpeech test-clean into wav+txt pairs for `voicio-engine --eval`.

    uv run engine/scripts/fetch-eval.py [count]

Writes an evenly spread subset (default 300 utterances) to ~/.cache/voicio/eval/librispeech-test-clean.
"""
import io
import sys
import urllib.request
from pathlib import Path

import numpy as np
import pyarrow.parquet as pq
import soundfile

EVAL = Path.home() / ".cache" / "voicio" / "eval"
URL = "https://huggingface.co/datasets/openslr/librispeech_asr/resolve/main/all/test.clean/0000.parquet"


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 300
    parquet = EVAL / "test-clean.parquet"
    if not parquet.exists():
        EVAL.mkdir(parents=True, exist_ok=True)
        print(f"downloading {URL} (~350 MB)", flush=True)
        urllib.request.urlretrieve(URL, parquet.with_suffix(".part"))
        parquet.with_suffix(".part").rename(parquet)
    table = pq.read_table(parquet, columns=["id", "audio", "text"]).to_pylist()
    out = EVAL / "librispeech-test-clean"
    out.mkdir(exist_ok=True)
    for row in table[:: max(1, len(table) // count)][:count]:
        samples, rate = soundfile.read(io.BytesIO(row["audio"]["bytes"]), dtype="int16")
        assert rate == 16_000 and samples.ndim == 1
        soundfile.write(out / f"{row['id']}.wav", np.asarray(samples), rate, subtype="PCM_16")
        (out / f"{row['id']}.txt").write_text(row["text"].lower() + "\n")
    print(f"wrote {count} utterances to {out}")


if __name__ == "__main__":
    main()
