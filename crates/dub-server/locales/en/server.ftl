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

## Glossary, hardware, job records, LLM providers

glossary-bad-format = format “{ $format }”: json or tsv
glossary-series-unreadable = the series glossary is unreadable: { $error }
glossary-no-text = the project has no text: the glossary is built from recognized speech, so analyze first
glossary-lines = glossary: { $count } { $count ->
    [one] line
   *[other] lines
} of text
glossary-no-llm = glossary: the LLM is unavailable: { $error }
glossary-failed = glossary: { $error }
glossary-proposed = glossary: { $count } { $count ->
    [one] entry
   *[other] entries
} proposed
glossary-series-profile-missing = series profile “{ $slug }” not found; its glossary is not applied
glossary-series-slug-unreadable = the glossary of series “{ $slug }” is unreadable: { $error }
glossary-series-applied = series glossary “{ $slug }”: { $count } { $count ->
    [one] entry
   *[other] entries
}, { $added } added to the project
hw-no-nvidia = no NVIDIA GPU
jobs-serialize-record = serializing job.json: { $error }
jobs-record-missing = { $file } is not in { $dir }
llm-llama-server-missing = llama-server not found ({ $path })
llm-gemma-missing = the Gemma GGUF was not found ({ $path })
llm-mmproj-missing = the Gemma vision projector (mmproj) was not found ({ $path })
llm-chat-client = chat client: { $error }
llm-local-no-text-model = the local server ({ $url }) is chosen for translation, but no model is selected
llm-local-no-vision-model = the local server ({ $url }) is chosen for vision, but no model is selected
llm-local-client = local server client: { $error }
llm-local-label = local server { $url } · { $model }
llm-openrouter-no-key = OpenRouter is chosen, but no key is set
llm-openrouter-no-text-model = OpenRouter is chosen for translation, but no model is selected
llm-openrouter-no-vision-model = OpenRouter is chosen for vision, but no model is selected
llm-openrouter-not-text = the OpenRouter model { $model } does not answer in text; choose another one for translation
llm-openrouter-not-vision = the OpenRouter model { $model } does not accept images; choose a vision model
llm-vision-missing = none ({ $reason })
llm-pair = translation: { $text }; vision: { $vision }

## Media tools and OCR

common-ffmpeg-exit = ffmpeg exit code { $code }:
    { $tail }
media-ffprobe-start = ffprobe failed to start: { $error }
media-ffprobe-exit = ffprobe exited with code { $code }: { $stderr }
media-ffprobe-no-streams = ffprobe: no streams
media-no-streams = the input has neither a video nor an audio stream
media-no-duration = the duration could not be determined
media-no-wav = ffmpeg did not create the wav
media-ffmpeg-hung = ffmpeg did not finish in { $seconds }s and was killed (it hung)
media-ffprobe-duration-exit = ffprobe duration exit code { $code }
media-env-filter-script = envelope filter script: { $error }
media-no-sample-rate = { $path }: the sample rate was not read ({ $stderr })
ocr-no-blur = { $error }; no blur
ocr-models-missing = the OCR models were not found
ocr-detection-failed = OCR detection failed ({ $error })
ocr-summary = OCR: { $regions } { $regions ->
    [one] region
   *[other] regions
}, { $localize } to localize, { $bands } band { $bands ->
    [one] span
   *[other] spans
}, sub_y={ $sub_y }

## Hardware presets

common-write = writing { $what }: { $error }
preset-rtx5090-title = RTX 5090 (32 GB)
preset-rtx4090-title = RTX 4090 (24 GB)
preset-top-subtitle = Maximum quality: the top quants locally
preset-gpu16-title = 16 GB GPU
preset-gpu16-subtitle = High quality (4080/4070 Ti and similar)
preset-gpu12-title = 12 GB GPU
preset-gpu12-subtitle = Balanced (3060/4070 and similar)
preset-gpu8-title = 8 GB GPU
preset-gpu8-subtitle = Economical: light quants (3060 Ti/4060)
preset-weak-nvidia-cloud-title = Weak NVIDIA + cloud
preset-weak-nvidia-cloud-subtitle = The heavy work (translation/vision/voicing) in OpenRouter, separation and ASR on your GPU
preset-cloud-title = CPU + cloud (no NVIDIA)
preset-cloud-subtitle = The heavy work in OpenRouter, the local work on the processor; runs without a graphics card (a key is needed)
preset-custom-title = Custom
preset-custom-subtitle = I will set every parameter myself
preset-reason-no-gpu = No NVIDIA GPU found, so the “CPU + cloud” mode: the heavy work in OpenRouter, the local work on the processor (slower, but it works)
preset-reason-top-card = { $gpu } detected: the maximum quants
preset-reason-by-vram = { $gpu } · { $vram } GB VRAM: { $preset }
preset-reason-low-vram = { $gpu } · { $vram } GB VRAM is little for local models; the cloud is more reliable
preset-unknown = unknown preset: { $id }
