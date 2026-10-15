#!/bin/sh
set -eu

python3 -c "import rustic_audio_tool as r; assert hasattr(r, 'PttAudioEngine')"
