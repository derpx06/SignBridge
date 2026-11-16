#!/usr/bin/env bash
# Downloads the on-device models into ~/.cache/voicio (or $VOICIO_MODELS).
#   fetch-models.sh            dictation models (VAD, Parakeet 110M, S1-mini)
#   fetch-models.sh --meeting  also the speaker-diarization models used by the web page
set -euo pipefail

DIR="${VOICIO_MODELS:-$HOME/.cache/voicio}"
# The folder was ~/.cache/whispr-voice-lab before the rename: move it once (or keep using it if it cannot move).
OLD=$HOME/.cache/whispr-voice-lab
if [ -z "${VOICIO_MODELS:-}" ] && [ ! -e "$DIR" ] && [ -d "$OLD" ]; then mv -T "$OLD" "$DIR" 2>/dev/null || DIR=$OLD; fi
GH=https://github.com/k2-fsa/sherpa-onnx/releases/download
HF=https://huggingface.co
mkdir -p "$DIR"

fetch() { # url target
  local url=$1 target=$DIR/$2
  [ -e "$target" ] && return
  mkdir -p "$(dirname "$target")"
  echo "downloading $url"
  curl -fsSL --retry 3 -o "$target.part" "$url"
  mv "$target.part" "$target"
}

fetch_tar() { # url extracted-dir-name
  [ -d "$DIR/$2" ] && return
  fetch "$1" "$2.tar.bz2"
  tar -xjf "$DIR/$2.tar.bz2" -C "$DIR"
  rm "$DIR/$2.tar.bz2"
}

fetch "$GH/asr-models/silero_vad.onnx" silero_vad.onnx
fetch_tar "$GH/asr-models/sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8.tar.bz2" \
  sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8
# "S1-mini" by "Superwhisper" (Apache-2.0 + naming clause): transcript cleanup.
fetch "$HF/superwhisper/s1-mini-GGUF/resolve/main/s1-mini-q4_k_m.gguf" s1-mini/s1-mini-q4_k_m.gguf
fetch "$HF/superwhisper/s1-mini-GGUF/resolve/main/LICENSE" s1-mini/LICENSE
fetch "$HF/superwhisper/s1-mini-GGUF/resolve/main/NOTICE" s1-mini/NOTICE
# Test clips for `voicio-engine --selftest`.
for n in 0 1; do
  fetch "$HF/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/test_wavs/$n.wav" "test_wavs/$n.wav"
done

if [ "${1:-}" = "--meeting" ]; then
  fetch_tar "$GH/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2" \
    sherpa-onnx-pyannote-segmentation-3-0
  fetch "$GH/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx" \
    3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx
fi
echo "models ready in $DIR"
