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

## Shared messages, compositing, downloads, dub timing, endpoints, preview frame

common-read = reading { $path }: { $error }
common-parse = parsing { $path }: { $error }
common-ffmpeg-start = starting ffmpeg: { $error }
compose-summary = composite: titles={ $titles } (bbox), blur { $boxes } { $boxes ->
    [one] box
   *[other] boxes
}, sub_px={ $sub_px }
compose-taglines-no-mt = taglines: machine translation is unavailable ({ $error }) -> blur only
compose-taglines-translating = taglines: translating the title card text
compose-taglines-failed = taglines: the translation failed; blur only
downloads-interrupted = the download stopped when the app closed; what was fetched is kept in .part and resumes from there
downloads-nothing-to-download = none of the selected ids is a downloadable component
downloads-busy = a download is already running
downloads-thread-failed = the download thread did not start: { $error }
timing-words-not-recognized = word timings of the dub were not recognized for { $count } { $count ->
    [one] line
   *[other] lines
} ({ $examples }); their words are highlighted by length
openrouter-catalog-failed = OpenRouter catalogue: { $error }
openrouter-empty-key = the key is empty
llm-server-no-address = no server address is set
llm-server-no-answer = the server { $base } did not answer: { $reason }
llm-server-status = { $endpoint } answered { $status }
llm-server-not-json = { $endpoint } did not return JSON: { $error }
llm-server-not-model-list = { $endpoint } did not return an OpenAI model list (no data field)
remix-start = remixing { $count } { $count ->
    [one] line
   *[other] lines
} → { $instruction }
remix-no-llm = remix: the LLM is unavailable: { $error }
remix-lines-changed = remix: the lines changed while the remix ran; the project was left as it was, run the remix again
frame-empty-ass = empty ASS: { $error }
frame-read-preview = reading the preview frame: { $error }
frame-read-source = reading the source frame: { $error }
