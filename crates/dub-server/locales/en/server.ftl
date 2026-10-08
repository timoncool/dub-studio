asr-windowed = transcribing in windows on the GPU ({ $speakers } { $speakers ->
    [one] speaker
   *[other] speakers
})

## Separation, OCR, benchmark, casting library, cloud ASR and TTS

atomic-extract-separation-audio = extracting 44.1k audio for separation
atomic-separating = separating ({ $model })
atomic-separation-failed = separation: { $error }
ocr-detecting-burned-text = detecting burned-in text ({ $model })
bench-stage = ⏱ { $name }: { $seconds }s | GPU ~{ $gpu_avg }% (peak { $gpu_max }%) | CPU ~{ $cpu_avg }% | VRAM { $vram }MB | bound by: { $bound }
bench-total = ⏱ { $label } TOTAL: { $seconds }s
casting-no-casting-json = the project has no casting.json (casting has not been run)
casting-profile-delete-failed = deleting profile { $slug }: { $error }
casting-profile-not-found = profile “{ $slug }” not found
cloud-asr-no-key = cloud ASR is on, but no OpenRouter key is set
cloud-asr-no-model = no OpenRouter STT model is selected in the settings
cloud-asr-failed = cloud ASR: { $error }
cloud-asr-empty = cloud STT returned an empty transcript
cloud-tts-no-key = cloud TTS is on, but no OpenRouter key is set
cloud-tts-no-model = no TTS model is selected in the settings (Cloud models · OpenRouter)
cloud-tts-no-voice = no TTS voice is set in the settings (each model has its own voices)
cloud-tts-failed = cloud TTS: { $error }
cloud-tts-too-short = cloud TTS: the audio is too short ({ $bytes } { $bytes ->
    [one] byte
   *[other] bytes
})
cloud-tts-read-wav = reading the cloud wav: { $error }
