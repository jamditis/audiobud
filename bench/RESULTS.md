# AudioBud engine benchmark (milestone A)

Machine: Legion, RTX 4080 Super, Windows 11. Date: 2026-06-21.
Read set: `bench/passage.txt` (6 diverse sentences: pangram, proper nouns, homophones, varied length). Each read once per engine via the plain transcribe hotkey (Ctrl+Alt+Space), post-processing OFF (pure engine output, no API calls). Transcripts pulled from the app's `transcription_history` SQLite table; latencies from the app log (`Transcription completed in Nms`); WER from `scripts/wer.ts`.

## Results

| Engine (model id) | Backend | Size on disk | Avg latency (warm) | Avg WER | Reliable over the run? |
|---|---|---|---|---|---|
| Parakeet V3 (`parakeet-tdt-0.6b-v3`) | DirectML/ONNX | 640 MB | 594 ms (422-1092) | 0.0595 | Yes - 6/6 transcribed |
| Canary 180M flash (`canary-180m-flash`) | DirectML/ONNX | 204 MB | n/a | n/a | NO - 7/7 empty output (broken) |
| Whisper turbo (`turbo`) | Vulkan/whisper.cpp | 1549 MB | ~290 ms (172-484; cold 4051) | ~0.044 | Works, but hallucinated on silence once |

Parakeet V3 per-sentence WER: s1 0.000, s2 0.214 ("Joe Amditis" -> "Jo M Dietz"), s3 0.143 ("They're" -> "They are"), s4 0.000, s5 0.000, s6 0.000. The two non-zero scores are a proper-noun miss and a contraction expansion; on ordinary content WER is effectively 0.

Cold-start (first transcription after model load): Parakeet V3 ~2456 ms, Whisper turbo ~4051 ms. Both warm up to sub-second afterward.

## Default chosen: `parakeet-tdt-0.6b-v3`

Reasons:
1. It is the smallest engine that actually works on this machine. Canary (the only smaller model, 204 MB) produces empty output on this build's DirectML path; Whisper turbo works but is 1.5 GB - a poor default for a lightweight local dictation app.
2. Accuracy is strong (5.95% avg WER, ~0% on non-proper-noun content) and latency is sub-second warm.
3. Confirmed by the user (2026-06-21): "lets stick with parakeet v3."

## Known finding (to file as an issue once the AudioBud repo exists)

Canary 180M flash (`canary-180m-flash`) loads successfully (model loaded in ~820 ms, vocab 2973 tokens) but every transcription returns an empty string when `ort_accelerator = directml`. The app's own UI flags DirectML as experimental ("models may fail to transcribe"); Canary appears to be one such failure. Re-test on a CPU execution provider and on a future ort build before shipping Canary as a selectable option. Tracked here until the repo is created (milestone C).
