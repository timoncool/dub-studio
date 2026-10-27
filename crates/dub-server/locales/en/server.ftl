asr-windowed = transcribing in windows ({ $speakers } { $speakers ->
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
downloads-panicked = the download stopped on an internal error: { $error }
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
llm-agent-label = Agent (MCP)
llm-agent-no-port = the server's port is not known yet; try again in a moment
llm-agent-absent = the language model is the connected agent, but no agent is connected now: connect one (Settings, Agent) or choose another LLM provider
llm-agent-no-answer = the agent did not answer within { $minutes } min
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
media-unreadable = { $file } cannot be read as video or audio: the file is damaged or not fully downloaded (ffprobe: { $stderr })
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

## Service port, shortening, frontend, launch defaults

common-corrupt = { $path } is damaged: { $error }
common-create-dir = folder { $path }: { $error }
service-bad-port = { $value }: not a port number (1..65535 expected)
service-exe-path = the service exe path: { $error }
service-request-not-sent = the connection was accepted, but the request was not sent: { $error }
service-no-health-answer = the connection was accepted, but /health did not answer: { $error }
service-not-http = the answer is not HTTP
service-health-status = an HTTP server; /health answered { $code }
service-health-not-dub-studio = an HTTP server; /health did not answer with a Dub Studio body
service-health-not-json = an HTTP server; /health did not answer with JSON
service-health-no-fields = /health calls itself { $app }, but without the service fields: { $error }
service-other-app = another application ({ $app })
service-health-no-app = an HTTP server; /health without an application name
service-port-reserved = the system does not give out the port, though nobody accepts connections on it (it may be in a range Windows reserves: { $command })
service-port-dub-studio = Dub Studio { $version } is on it ({ $executable })
service-port-other = another process holds it: { $what }
service-port-busy = Port 127.0.0.1:{ $port } has been busy for { $seconds } s: { $who }. Error: { $error }.

    Close the program that holds it, or set another port with the { $env } environment variable (for example { $env }={ $other_port }).
shorten-line-done = shortening { $n }/{ $total }: { $from } -> { $to } characters
shorten-line-rejected = shortening { $n }/{ $total }: the answer was not accepted ({ $reason })
shorten-line-no-answer = shortening { $n }/{ $total }: the LLM did not answer: { $error }
shorten-auto-start = { $count } { $count ->
    [one] line does
   *[other] lines do
} not fit the slot; shortening the translation and voicing only those
shorten-higgs-unloaded = Higgs is unloaded while the translation is shortened
shorten-auto-no-llm = shortening the translation was skipped: the LLM is unavailable: { $error }
shorten-none-shortened = shortening: none of the { $count } lines got shorter; they stay as voiced
shorten-done = { $count } of { $total } { $total ->
    [one] line
   *[other] lines
} shortened
shorten-nothing = nothing to shorten: every line fits its slot
shorten-no-llm = shortening: the LLM is unavailable: { $error }
shorten-start = shortening { $count } { $count ->
    [one] line
   *[other] lines
}: { $provider }
shorten-all-failed = shortening failed: the LLM answered none of the lines ({ $id }: { $error })
spa-not-built = the frontend is not built
settings-bad-speaker-count = speaker_count: a whole number from 0 to { $max } is expected (0 means automatic)
settings-bad-vo-gain = vo_gain_db={ $value }: a number from { $min } to { $max } dB is expected
settings-bad-src-lang = src_lang={ $value }: neither a language code nor "auto"
settings-bad-tgt-lang = tgt_lang={ $value }: not a language code
settings-bad-casting-ref = casting_ref={ $value }: not a casting profile slug
settings-style-too-long = tr_style_custom is longer than { $max } characters
settings-too-many-slots = { $name }: more than { $max } slots
settings-unknown-field = unknown launch defaults field: { $key }
settings-read-failed = reading the launch defaults: { $error }
settings-patch-not-object = the PATCH /settings/launch body is an object of fields
settings-write-failed = writing the launch defaults: { $error }

## Takes, translation, TTS text, MCP arguments

takes-delete-old = deleting the old take { $path }: { $error }
takes-missing = take { $take } is not in the history of line { $line }
translate-no-vision = the content type was not determined: vision is unavailable ({ $error })
translate-subs-already-translated = the subtitles are already in the target language -> no machine translation (voicing only)
translate-transcribe-only = transcribe: tgt=the source text, no translation
translate-same-language = same language -> no machine translation (tgt=the source)
translate-no-llm = translation failed: the LLM is unavailable: { $error }
translate-ctx-pass = context pass: vision layout/scene + translating the transcript
translate-failed = translation failed: { $error }
translate-untranslated = translation failed: { $left } of { $total } { $total ->
    [one] line
   *[other] lines
} stayed in the source language (details in the log and logs/llama-server.log)
translate-untranslated-auto = translation failed: { $left } of { $total } { $total ->
    [one] line
   *[other] lines
} stayed in the source language; if the speech in the video is already in the target language, set the original language and no translation is needed (details in the log and logs/llama-server.log)
translate-done = translation ready: { $done }/{ $total } lines, titles={ $titles }
translate-left-untranslated = { $left } of { $total } { $total ->
    [one] line
   *[other] lines
} stayed in the source language (details in logs/llama-server.log)
translate-coverage-retry = translation coverage: { $count } { $count ->
    [one] line
   *[other] lines
} without a translation; translating them again
translate-coverage-failed = translation coverage: translating again failed ({ $error })
translate-coverage-left = translation coverage: { $count } left without a translation
tts-silent-after-cleanup = no speech left after cleaning the text, so silence: { $count } { $count ->
    [one] line
   *[other] lines
} ({ $lines })
tts-markup-ignored = the voice engine does not read this markup, so it was taken off: { $count } { $count ->
    [one] line
   *[other] lines
} ({ $lines })
mcp-bad-speaker-count = speaker_count: a whole number from 0 to { $max } is expected
mcp-speakers-without-diarize = speaker_count above 1 does not go with diarize=false

## Routes: projects, voices, setup, translation, export, jobs, alignment

project-serialize = serializing project.json: { $error }
project-delete-failed = deleting the project { $path }: { $error }
project-delete-in-use = a file of the project is in use (the window or another program is reading it): { $error }
project-restore-exists = The project is already in place.
project-restore-gone = The project is not in the recycle bin: it was emptied, or the project was not deleted from here.
project-restore-finder = On macOS a deleted project is taken back from the Bin in Finder: select it there and choose Put Back.
setup-no-ids = ids is empty
setup-fetching-missing = Fetching the models this feature still needs…
setup-diarization-missing = The diarization model did not download ({ $error }); the analysis goes on without telling the speakers apart
setup-pick-model-files = Model file(s)
setup-pick-models-folder = Folder with ready models
setup-no-folder = there is no folder { $path }
voices-not-found = voice { $name } was not found in voices/
voices-no-vocals = no vocals to measure F0 on; analyze first
voices-speakers-changed = voices by slots: the speakers changed while the voice was measured; the project was left as it was, run it again
analyze-args-not-object = analyze args: an object was expected
cost-analyze = OpenRouter: ${ $spent } spent on the analysis (${ $total } used in total)
cost-run = OpenRouter: ${ $spent } spent on the run (${ $total } used in total)
translate-lines-to = Translating { $count } { $count ->
    [one] line
   *[other] lines
} → { $lang }
translate-note = translation: { $note }
translate-titles-failed = the titles were not translated ({ $error }); they stay in the source language in the video
translate-titles-error = translating the titles: { $error }
translate-lines-changed = translation: the lines or titles changed while the translation ran; the project was left as it was, run the translation again
export-pick-folder = Where to save the results
export-copy-failed = copying to { $path }: { $error }
jobs-wait-not-number = wait: a number of seconds was expected
jobs-unknown-kind = unknown job kind in job.json: { $kind }
jobs-not-resumable = a { $kind } job cannot be resumed
align-no-source-text = aligning to speech: the lines have no text in the original language (the subtitles were imported in the target language)
align-no-vocals = aligning to speech: the project has no vocal track; analyze first
align-recognizing = aligning to speech: recognizing the words
align-recognition-failed = aligning to speech: recognition: { $error }
align-no-speech = aligning to speech: no speech was recognized; the timings were not changed
align-mismatch = aligning to speech: the lines did not match the speech ({ $share }% matched); the timings were not changed
align-lines-changed = aligning to speech: the lines changed during recognition; the timings were not changed, run the alignment again
align-done = aligned to speech: { $share }% of the lines by words, timing changed for { $changed }; offset { $offset } s

## Analysis

analyze-serialize = serializing { $what }: { $error }
analyze-read-model = reading the model { $path }: { $error }
analyze-stage-cache-unreadable = the cache of stage { $stage } is unreadable ({ $error }); recomputing
analyze-checkpoint-not-saved = the checkpoint of stage { $stage } was not saved: { $error }
analyze-diarize-continuous = diarization: { $speakers } { $speakers ->
    [one] speaker
   *[other] speakers
} expected, one continuous pass over the whole recording without resetting labels at hour boundaries
analyze-diarize-fragments = diarization: { $speakers } { $speakers ->
    [one] speaker
   *[other] speakers
} expected, matching voices across fragments
analyze-wespeaker-needed = WeSpeaker is needed for a set number of speakers: { $error }
analyze-align-skipped = aligning to speech skipped: the subtitles are in the target language and the speech in the original one
analyze-align-recognizing = aligning the subtitles to speech: recognizing the words
analyze-align-recognition-failed = aligning the subtitles: speech recognition: { $error }
analyze-align-no-speech = aligning to speech: no speech was recognized; the file’s timings are kept
analyze-align-mismatch = the subtitles did not match the speech ({ $share }% of the lines matched); the file’s timings are kept
analyze-align-done = the subtitles are aligned to speech: { $share }% of the lines by words, the rest shifted with their neighbours; file offset { $offset } s
analyze-more = { $count } more
analyze-window-plan = a long track of { $duration }s: a plan of { $windows } { $windows ->
    [one] window
   *[other] windows
} (the first ~{ $first }s); windowed ASR is not active yet, processing in one piece
analyze-audio-cached = audio from the cache (the source did not change); skipping ffmpeg
analyze-extracting-audio = extracting the audio (ffmpeg -> 16k mono)
analyze-stems-stale = the stems were made from an earlier extraction; separating again
analyze-separating = separating the vocals ({ $model }): a clean voice for diarization/ASR
analyze-separation-cached = separation from the cache (the stems are already made)
analyze-separation-failed = separation failed ({ $error }); diarization/ASR on the raw audio
analyze-separator-missing = { $model } was not found; diarization/ASR on the raw audio
analyze-diarizing = diarization ({ $model })
analyze-diarization-cached = diarization from the cache
analyze-diarization-count-failed = diarization with a set number of speakers: { $error }
analyze-diarization-failed = diarization failed ({ $error }); going on with a single speaker
analyze-diarization-model-missing-count = the { $model } model was not found: the set number of speakers cannot be applied
analyze-subs-no-diarization = subtitles: no diarization (the whole clip as one piece)
analyze-diarization-model-missing = the diarization model was not found; going on with a single speaker
analyze-fewer-speakers = { $expected } { $expected ->
    [one] speaker was
   *[other] speakers were
} expected, { $found } told apart: the missing voices were not added
analyze-speakers-matched = voices matched across fragments: { $speakers } { $speakers ->
    [one] speaker
   *[other] speakers
}
analyze-transcript-cached = transcript from the cache: { $segments } { $segments ->
    [one] segment
   *[other] segments
}
analyze-read-subs = reading the subtitles { $path }: { $error }
analyze-subs-empty = the subtitles are unrecognized or empty: { $path }
analyze-subs-imported = subtitles imported: { $lines } { $lines ->
    [one] line
   *[other] lines
}, { $speakers } { $speakers ->
    [one] speaker
   *[other] speakers
}
analyze-cloud-asr = transcribing in the cloud (OpenRouter STT)
analyze-cloud-stt-failed = cloud STT: { $error }
analyze-cloud-done = cloud: { $lines } { $lines ->
    [one] line
   *[other] lines
}, { $speakers } { $speakers ->
    [one] speaker
   *[other] speakers
}
analyze-hallucination-filter = ASR hallucination filter: { $error }
analyze-asr-to-crisperwhisper = language "{ $lang }": Parakeet does not know it, recognising with CrisperWhisper
analyze-asr-no-words = The video has { $speech } s of speech and Parakeet heard { $words } words in it: Parakeet knows only 25 European languages. Pick the source language in the window instead of Auto — for a language outside Europe the studio switches recognition to Whisper.
analyze-hidden-hallucinations = ASR hallucination lines hidden (no voice): { $count }: { $lines }
analyze-hidden-by-text = ASR titles and sounds hidden by their text (there is sound in the interval, the voice is not separated from the music): { $count }: { $lines }
analyze-voiced-suspects = look like hallucinations, but there is a voice, so they are kept with a mark: { $count }: { $lines }
analyze-merged-fragments = merging fragments: { $before } -> { $after } segments
analyze-characters-by-voice = characters by voice: { $count }
analyze-segments-speakers = { $segments } { $segments ->
    [one] segment
   *[other] segments
}, { $speakers } { $speakers ->
    [one] speaker
   *[other] speakers
}
analyze-project-unparsable = project.json cannot be parsed ({ $error }): it holds the project glossary, so the analysis stopped to keep it
analyze-glossary-fixed = glossary: { $count } term recognition { $count ->
    [one] error
   *[other] errors
} fixed
analyze-no-speech-nodub = no speech segments; keeping the original track (nodub)
analyze-auto-nodub = auto: no speech worth dubbing -> NODUB (the original + localized on-screen text)
analyze-casting-style = character descriptions from the casting profile -> translation style ({ $chars } chars)
analyze-empty-removed = segments without words removed: { $count }
analyze-translation-cached = translation from the cache
analyze-ocr-cached = on-screen text detection from the cache
analyze-audio-no-ocr = audio mode: no video, so no on-screen text detection is needed
analyze-ocr-off = burned-in text detection is off (the checkbox)
analyze-content-type = content type (auto): { $kind }
analyze-casting-cached = casting from the cache
analyze-casting-checkpoint = the casting checkpoint was not saved: { $error }
analyze-profile-voices-missing = the profile voices were not found in voices/ -> clone: { $voices }
analyze-profile-voices-applied = the carried-over profile voices are applied to the dub ({ $count } { $count ->
    [one] character
   *[other] characters
})
analyze-audio-no-casting = audio mode: no video, so no character casting is needed
analyze-cache-not-saved = cache.json could not be saved: { $error } (not critical)

## Casting, recording, voice library

casting-relabel = relabelling by voice: { $count } { $count ->
    [one] character
   *[other] characters
} by voice (diarization found { $found })
casting-skipped-no-speech = casting skipped: no speech segments
casting-skipped-no-speakers = casting skipped: no speakers
casting-speakers-ranked = speaking characters: { $count } (ranked by speaking time)
casting-faces-bound = faces bound to speakers: { $count } of { $total }
casting-bit-part-skipped = { $character }: speaker { $speaker } has 1 line and neither a face nor a voice -> skipped (a bit part not worth casting)
casting-character-face = { $character }: speaker { $speaker }, { $lines } { $lines ->
    [one] line
   *[other] lines
}, { $seconds }s of speech, face: yes
casting-character-no-face = { $character }: speaker { $speaker }, { $lines } { $lines ->
    [one] line
   *[other] lines
}, { $seconds }s of speech, face: no
casting-profile-other-type = the profile is of another type ({ $previous } ≠ { $current }); cross-matching skipped
casting-cross-episode = across episodes: names/voices carried over: { $count }
casting-saved = casting.json is ready: { $count } { $count ->
    [one] character
   *[other] characters
}
casting-save-failed = casting.json could not be written: { $error }
casting-faces-collected = faces collected: { $faces } ({ $samples } samples)
casting-face-clusters = face identities (clusters by vector): { $count }
casting-anime-detector-missing = the anime detector was not found ({ $path }); no avatars
casting-anime-detector-failed = the anime detector did not load: { $error }; no avatars
casting-scrfd-missing = SCRFD was not found; no avatars (casting by voice)
casting-scrfd-failed = SCRFD did not load: { $error }; no avatars
casting-embedder-missing = { $model } was not found ({ $path }); avatars without an embedding
casting-embedder-failed = { $model } did not load: { $error }; avatars without an embedding
casting-no-frame = ffmpeg did not extract the frame
casting-voice-no-vocals = voice embedding skipped: no clean vocals
casting-voice-no-model = voice skipped: no WeSpeaker model ({ $path })
casting-voice-model-failed = WeSpeaker did not load: { $error }; voice skipped
casting-voice-trim-failed = voice sample { $speaker }: trimming failed: { $error }
casting-voice-embedding-failed = voice { $speaker }: embedding failed: { $error }
casting-voice-embeddings = voice embeddings: { $count }
casting-applying-profile = applying the library profile: { $slug }
casting-library-profile-missing = the library profile “{ $slug }” was not found; nothing applied
record-busy = Recording is already running
record-mic-not-found = Microphone “{ $name }” was not found: it is unplugged or renamed; choose another one
record-no-mic = No microphone found
record-mic-config = microphone configuration: { $error }
record-create-wav = creating the wav: { $error }
record-format-unsupported = format { $format } is not supported
record-mic-open = opening the microphone: { $error }
record-mic-start = starting the microphone: { $error }
record-not-recording = Nothing is being recorded
voices-not-file-list = { $page }: not a file list: { $error }
voices-list-endless = { $page }: the dataset file list does not end
voices-too-large = { $url }: more than the pinned { $size } bytes
voices-size-mismatch = { $url }: { $got } bytes arrived, { $size } are pinned
voices-sha-mismatch = { $url }: SHA-256 { $got } does not match the pinned { $want }
voices-bad-name = a bad name
voices-not-in-dataset = voice { $name } is not in the dataset { $dataset }
voices-bad-size = { $name }: the size in the catalogue is not valid
voices-no-sha = { $name }: no SHA-256 in the catalogue
voices-pack-downloading = downloading the voice pack
voices-pack-folder = the voice pack goes into the program's voices folder, and the voices are now in { $path } (DUBENGINE_VOICES)
voices-pack-taken = taken from { $app }: { $count } { $count ->
    [one] file
   *[other] files
}
voices-pack-done = done: { $count } { $count ->
    [one] file
   *[other] files
}

## Downloads by link

url-no-fetch = there is no download { $id }
url-stopped = stopped
url-already-downloading = this link is already downloading: { $id }
url-probe-not-json = the yt-dlp -J answer is not JSON: { $error }; { $stderr }
url-interrupted = the download stopped when the studio closed; what was fetched is in its folder and resumes from there
url-not-a-link = “{ $url }” is not a link: { $error }
url-not-http = “{ $url }” is not an http(s) link
url-bad-subs-lang = “{ $lang }” is not a subtitle language code
url-cookies-not-file = { $path } is not a cookies.txt file of up to 1 MB
url-cookies-empty-or-large = cookies.txt is empty or larger than 1 MB
url-thread-failed = the download thread did not start: { $error }
url-cookies-gone = this download’s cookies.txt is gone from its folder; start the download again with cookies
url-disk-space = about { $need } MB is needed, { $free } MB is free
url-no-subs = the video has no human-made “{ $lang }” subtitles (there are: { $have })
url-no-subs-none = the video has no human-made “{ $lang }” subtitles (there are none at all)
url-no-media-file = yt-dlp finished, but there is no video file in { $path }: { $stderr }
url-subs-failed = the “{ $lang }” subtitles did not download ({ $code }): { $error }
url-ytdlp-start = starting yt-dlp: { $error }
url-ytdlp-wait = waiting for yt-dlp: { $error }
url-still-downloading = { $id } is still downloading
url-cancelled-removed = { $id } was cancelled and what was fetched is removed: start a new download
url-still-stopping = { $id } is still stopping; try again in a couple of seconds
url-cancel-first = { $id } is still downloading; cancel it first
url-no-subs-file = yt-dlp finished without a subtitle file in { $path }: { $stderr }
url-no-parent = { $path } has no parent folder
url-subs-empty = the subtitles { $path } have no lines at all
url-ytdlp-missing = the ytdlp component is not downloaded
url-bad-quality = quality “{ $quality }”: best, 1080, 720, 480 or audio

## yt-dlp

ytdlp-update-missing = the yt-dlp update { $version } ({ $path }) is missing; the pinned { $pinned } runs
ytdlp-http-client = HTTP client: { $error }
ytdlp-tag-not-version = the yt-dlp release tagged “{ $tag }” is not a version
ytdlp-no-checksum = { $url } has no { $file }
ytdlp-new-says = the new yt-dlp { $tag } calls itself “{ $said }”; { $current } stays
ytdlp-new-failed = the new yt-dlp { $tag } did not start: { $error }; { $current } stays
ytdlp-too-large = { $url }: more than { $limit } bytes
ytdlp-sha-mismatch = { $url }: SHA-256 { $got } does not match SHA2-256SUMS { $want }
ytdlp-exit-code = exit code { $code }: { $stderr }
ytdlp-component-missing = the ytdlp component (yt-dlp and deno) is not downloaded
ytdlp-no-ffmpeg = no ffmpeg: neither the ffmpeg component nor ffmpeg.exe in PATH
ytdlp-start = starting { $program }: { $error }
ytdlp-timeout = yt-dlp did not answer in { $seconds } s: { $stderr }
ytdlp-playlist = the link is a playlist or a channel ({ $count } { $count ->
    [one] video
   *[other] videos
}), not a single video
ytdlp-redirect-only = the link has no video itself, only a link onward
ytdlp-live = a live stream ({ $status })
ytdlp-thumbnail-not-image = the thumbnail is not an image ({ $mime })
ytdlp-thumbnail-too-large = the thumbnail is larger than 4 MB
ytdlp-failed-silently = yt-dlp failed without a message

## Render: voicing, QC, mix, mux

render-input = input { $width }x{ $height } dur={ $duration }s
render-nodub-original = nodub: the original audio track
render-done-audio = done (audio only) -> { $path }
render-building-ass = building the ASS (titles + dubbed subtitles)
render-burning = burning in the subtitles + blur (ffmpeg + libass, NVENC)
render-burn-off = subtitles/titles are off (subs.burn=off)
render-encode-cpu-mode = the studio runs on the processor, so the video is encoded on the processor (x264), slower than on the graphics card
render-encode-cpu-driver = NVIDIA driver { $have } is older than { $need }, which encoding on the graphics card needs, so the video is encoded on the processor (x264), slower. Update the driver from NVIDIA's site to encode on the card
render-muxing = muxing video + audio
render-track-dub = { $lang } (dub)
render-track-original = { $lang } (original)
render-two-tracks = two tracks: { $dub } + { $original } -> { $container }
render-multitrack-failed = the multi-track mux failed ({ $error }) -> one track
render-subtitle-tracks = subtitles as mkv tracks: { $tracks }
render-subtitle-tracks-failed = subtitles as mkv tracks: { $error }
render-mp4-companion-failed = the mp4 companion was not built ({ $error }); the player opens the mkv (VLC works)
render-done = done -> { $path }
render-dub-audio-done = the dub audio is ready
render-synth-thread-ended = the synthesis thread ended without a result
render-synth-timeout = synthesis timed out after >{ $seconds }s; cancelled, the engine is free
render-engine-stuck = synthesis does not cancel after >{ $seconds }s; the render is stopped (the engine hung in the DLL)
render-higgs-load-failed = loading the Higgs DLL: { $error }
render-higgs-gpu-old = The local Higgs voice cannot run on { $card }: it needs an NVIDIA card from Maxwell on (compute capability { $min }, GTX 900 or newer). Choose the "Google · direct" or "OpenRouter" voice.
render-higgs-driver-old = The local Higgs voice needs NVIDIA driver { $min } or newer, and this one is older: { $card }. Update the driver or choose the "Google · direct" or "OpenRouter" voice.
render-higgs-engine-failed = Higgs did not speak a single line: the engine failed before any sound ({ $error })
render-higgs-no-gpu = The local Higgs voice needs an NVIDIA card with CUDA, and none was found ({ $card }). Choose the "Google · direct" or "OpenRouter" voice.
render-defect-runaway = runaway
render-defect-cutoff = cut off
render-defect-silence = silence
render-defect-hum = hum
render-defect-repeat = a repeated piece
render-defect-end-burst = a click or an extra word at the end
render-defect-padding = mostly silence
render-defect-whistle = a whistle
render-defect-hiss = hiss in the pauses
render-cloud-defect = ⚠ segment { $line }: the cloud voicing has a defect ({ $defect }); regenerate the line in the editor
render-voice-gender = ⚠ speaker { $speaker } has a { $want } voice, but the dub voice is { $got }; pick another voice
render-gender-male = male
render-gender-female = female
render-recognition-no-answer = recognition returned no answer
render-second-pass-overflow = the second voicing pass asked for shortening, which is off in it
render-no-translated-lines = no translated lines -> silence, the original track
render-takes-of-removed-lines = take histories of removed lines cleared: { $count }
render-extracting-audio = extracting the audio (ffmpeg 44.1k stereo)
render-separator-missing = the separation engine was not found -> no background (keep_music off)
render-ref-from-mix = the clone reference comes from the unseparated mix: the original background sounds in it too
render-emotion-ref-failed = segment { $segment }: the emotion reference was not cut ({ $error }); using the speaker’s identity reference
render-cloud-voices = cloud voices by speaker: { $voices }
render-synth-keys-reset = { $error }; the synthesis keys start over
render-synthesizing = synthesizing { $count } of { $total } { $total ->
    [one] segment
   *[other] segments
}
render-voicing-cached = voicing from the cache: { $count } { $count ->
    [one] segment
   *[other] segments
}
render-cloud-tts-parallel = cloud TTS: { $count } { $count ->
    [one] segment
   *[other] segments
} in { $threads } parallel threads
render-cloud-tts-ready = cloud TTS: pre-synthesis ready ({ $count } { $count ->
    [one] segment
   *[other] segments
})
render-takes-quarantined = { $error }; the take history of line { $line } is set aside in { $path } and started over
render-pinned-take = line { $line }: the pinned take plays; new voicing does not replace it
render-take-unpinned = line { $line }: the take is unpinned because the line’s text changed
render-selected-take = line { $line }: the chosen take { $take } plays; new voicing does not replace it
render-write-cloud-segment = writing the cloud seg{ $line }: { $error }
render-cloud-tts-failed = ⚠ segment { $line }: cloud TTS failed ({ $error }); the original is kept
render-loading-higgs = loading Higgs
render-failures-kept-generated = ⚠ segment { $line }: { $attempts } synthesis failures ({ $error }); the generated voicing is used (range { $range } dB)
render-failures-kept-original = ⚠ segment { $line }: { $attempts } synthesis failures/timeouts ({ $error }); the original line is kept
render-regenerating = segment { $line }: { $error }; regenerating ({ $attempt }/{ $attempts })
render-defects-kept-generated = ⚠ segment { $line }: all { $attempts } attempts have a defect ({ $defect }); the generated voicing is used (range { $range } dB)
render-silent-kept-original = ⚠ segment { $line }: { $attempts } attempts without sound; the original is used
render-retry-alt-ref = another reference
render-retry-temperature = higher temperature
render-defect-regenerating = segment { $line }: a synthesis defect ({ $defect }); regenerating ({ $via } { $attempt }/{ $attempts })
render-write-segment = writing seg{ $line }: { $error }
render-too-many-artifacts = TTS: too many failed voicing attempts ({ $in_a_row } lines in a row, { $retries } retries in total); regenerating does not help. Causes: { $causes }; the last: { $last }. Stopped at segment { $line }.
render-cause-engine = engine error
render-multi-take = segment { $line }: multi-take; the take closest to the slot is chosen ({ $deviation }s off)
render-stretch-over-cap = segment { $line }: needs a stretch of x{ $needed } (slot { $slot }s), cap x{ $cap }; the text is faster than normal
render-lead-into-silence = starting in the silence: { $lines } { $lines ->
    [one] line starts
   *[other] lines start
} earlier, { $seconds } s in all, instead of being sped up
render-silence-trimmed = trimming TTS silence: { $seconds } s cut from { $lines } { $lines ->
    [one] line
   *[other] lines
} ({ $pauses } s of it pauses); trimming brought the speed-up within the cap for { $into_cap } { $into_cap ->
    [one] line
   *[other] lines
}
render-fit-summary = fitting: { $over }/{ $total } segments above the cap ({ $share }%)
render-fit-summary-drift = fitting: { $over }/{ $total } segments above the cap ({ $share }%), sync caught up on { $drift }
render-qc-start = QC: checking { $count } { $count ->
    [one] line
   *[other] lines
} by transcription
render-qc-unheard = QC: { $count } of { $total } lines were not checked; recognition failed: { $reason }
render-qc-mismatch = QC: { $count } { $count ->
    [one] line does
   *[other] lines do
} not match the translation; resynthesizing
render-qc-resynthesized = QC: segment { $line } resynthesized (attempt { $attempt })
render-qc-unconfirmed = ⚠ QC: segment { $line } (“{ $text }”) could not be confirmed; check the line by hand
render-qc-kept-mismatch = ⚠ QC: segment { $line } does not match the translated text; the generated voicing is kept
render-qc-resynth-unheard = QC: { $count } resynthesized { $count ->
    [one] line was
   *[other] lines were
} not checked; recognition failed: { $reason }
render-qc-summary = QC result: { $fixed }/{ $total } fixed, { $flagged } still flagged, { $unheard } not checked
render-qc-all-confirmed = QC: every line is confirmed by transcription ✓
render-qc-rest-confirmed = QC: the other lines are confirmed by transcription
render-laying-out = laying the dub out on the timeline
render-peak-limiter = peak limiter: { $lines } { $lines ->
    [one] line
   *[other] lines
}, { $samples } samples above the { $ceiling } ceiling brought down without clipping
render-tempo-fit = tempo fit of the whole track x{ $factor }
render-voiceover-envelope = voiceover: the original at { $db } dB UNDER the translation, full in the pauses (dynamic envelope, { $blocks } blocks)
render-voiceover-flat = voiceover: the envelope is unavailable -> flat attenuation
render-mix-no-ducking = mixing: instrumental + dub vocals (ducking OFF, full background)
render-mix-ducking = mixing: instrumental + dub vocals (ducking ON, envelope, { $blocks } blocks)
render-mix-sidechain = the envelope is unavailable -> sidechain ducking
render-mix-plain = sidechain is unavailable -> a plain mix
render-loudness-off = loudness evening is off: the mix as it is
render-loudness-normalizing = normalizing loudness (EBU R128, true peak)
render-loudnorm-skipped = loudnorm skipped ({ $error })
render-track-gain = track gain { $db } dB
render-dub-timings-not-written = dub timings for the subtitles were not written: the layout ({ $lines } lines, { $spans } spans) did not match the segments ({ $segments }); the subtitles follow the original timings
render-dub-timing-mismatch = dub timings: line { $line } of the synthesis view does not match project segment #{ $index }
render-word-timings = word timings of the subtitles: recognizing { $count } dub { $count ->
    [one] line
   *[other] lines
}
render-refs-unchecked = checking references: { $count } { $count ->
    [one] candidate is
   *[other] candidates are
} accepted unchecked; recognition failed: { $error }
render-refs-all-failed = ⚠ speaker { $speaker }: no reference candidate passed the check (heard: “{ $heard }”); taking the best by score
render-speaker-ref-failed = speaker { $speaker } reference: { $error }
render-speaker-ref = speaker { $speaker } reference: “{ $text }” ({ $seconds }s, { $candidates } { $candidates ->
    [one] candidate
   *[other] candidates
}, check ok)
render-speaker-ref-unchecked = speaker { $speaker } reference: “{ $text }” ({ $seconds }s, { $candidates } { $candidates ->
    [one] candidate
   *[other] candidates
}, check ⚠ failed)

## Setup: components

setup-comp-higgs-purpose = Voices the translation in the original actors' voices
setup-comp-higgs-engine-name = audio.cpp engine
setup-comp-higgs-engine-purpose = The program that runs the Higgs voice and CrisperWhisper recognition
setup-comp-gemma-purpose = Translates the speech and reads the text in the frame
setup-comp-gemma-q5-0-purpose = Translation and on-screen text — more accurate than q4_0
setup-comp-gemma-q6-k-purpose = Translation and on-screen text — more accurate still
setup-comp-gemma-q8-0-purpose = Translation and on-screen text — the most accurate and the heaviest
setup-comp-parakeet-purpose = Recognises the speech in the video, with the timing of every word
setup-comp-transcribe-engine-purpose = The transcribe.cpp recognition engine on the GPU; Parakeet runs on it
setup-comp-transcribe-engine-cpu-purpose = The transcribe.cpp recognition engine on the processor, for a computer without NVIDIA; Parakeet runs on it, slower
setup-comp-transcribe-engine-cuda12-purpose = The transcribe.cpp recognition engine on an older NVIDIA card or driver (CUDA 12.9); Parakeet runs on it
setup-comp-higgs-q6-k-purpose = Voices — a lighter variant than Q8_0
setup-comp-higgs-q4-k-m-purpose = Voices — the lightest variant
setup-comp-parakeet-ultra-purpose = Speech recognition — a fine-tuned version with fewer mistakes
setup-comp-crisperwhisper-purpose = Speech recognition in 99 languages with timestamps, beyond Parakeet's 25 European ones. Licence: nyra health Non-Commercial (LICENSE.md beside the weights)
setup-comp-langid-purpose = Hears the language of the recording when the source language is left on Auto, for CrisperWhisper
setup-comp-sortformer-name = Who is speaking (up to 8 voices)
setup-comp-sortformer-purpose = Splits the lines by speaker, so each one gets their own voice
setup-comp-roformer-purpose = Separates the voice from music and noise, so the background stays
setup-comp-roformer-q5-purpose = Voice separation — a lighter variant than Q8_0
setup-comp-roformer-q4-purpose = Voice separation — the lightest variant
setup-comp-casting-name = Character casting (faces and voices)
setup-comp-casting-purpose = Recognises characters by face and voice, so each keeps the same voice
setup-comp-bsroformer-engine-name = Voice separation engine (NVIDIA)
setup-comp-bsroformer-engine-purpose = The program that separates the voice on the graphics card
setup-comp-bsroformer-engine-cpu-name = Voice separation engine (processor)
setup-comp-bsroformer-engine-cpu-purpose = For a computer without NVIDIA: runs on the processor, slower
setup-comp-llama-name = Translation server (llama.cpp)
setup-comp-llama-purpose = The program that runs Gemma for translation and on-screen text
setup-comp-onnxruntime-purpose = Needed for recognising speech, on-screen text and speakers
setup-comp-onnxruntime-gpu-purpose = So that speech and speaker recognition run on the graphics card, not the processor
setup-comp-ffmpeg-purpose = Reads and assembles video and audio
setup-comp-ytdlp-name = Download by link (yt-dlp + deno)
setup-comp-ytdlp-purpose = Download a video by link (YouTube and the other yt-dlp sites) into a new project
setup-comp-cuda-runtime-purpose = NVIDIA libraries the engines use on the graphics card; no separate CUDA install needed
setup-comp-cudnn-purpose = Needed for speech and speaker recognition on the graphics card
setup-comp-vcruntime-purpose = Windows system libraries for the engines (included)
setup-comp-ocr-name = On-screen text (OCR)
setup-comp-ocr-purpose = Finds the text burnt into the video, to cover it with the translation (included)
setup-comp-nvidia-driver-name = NVIDIA driver
setup-comp-nvidia-driver-purpose = Needed to work on the graphics card; installed from NVIDIA's site, not by the app

## Setup: downloads and installation

setup-http-status = { $url }: status { $status }
setup-write = writing: { $error }
setup-not-zip = not a zip: { $error }
setup-zip-entry = zip entry: { $error }
setup-open = opening { $path }: { $error }
setup-verifying = Checking the SHA-256 of { $file }…
setup-install-record = the install record: { $error }
setup-disk-space = not enough space: { $need } GB needed, { $free } GB free ({ $path })
setup-unknown-component = there is no component { $id }
setup-not-removable = { $id } is not installed by the app
setup-component-busy = the component is downloading now; pause the download
setup-paused = the download is paused
setup-rate-limited = { $url }: the server has answered { $status } for { $minutes } min
setup-proxy-scheme = proxy { $proxy }: the scheme { $scheme } does not suit downloads (http, https, socks4, socks5)
setup-proxy-no-host = proxy { $proxy }: no host
setup-proxy-no-port = proxy { $proxy }: no port
setup-proxy-credentials = proxy { $proxy }: model downloads (ureq) cannot pass such a login or password to the proxy: it has / ? #, a space, non-ASCII or (for SOCKS5) a colon in the password; cloud requests through this proxy work, downloads need a password without these characters
setup-start-failed = { $url }: could not start in { $retries } attempts: { $error }
setup-chunk-manifest = the chunk manifest: { $error }
setup-range-incomplete = an incomplete range: { $got }/{ $want } bytes
setup-range-failed = range { $start }-{ $end } after { $retries } attempts: { $error }
setup-range-status = range { $start }-{ $end }: status { $status } (206 expected)
setup-range-read = reading the range: { $error }
setup-create = creating { $path }: { $error }
setup-read = reading: { $error }
setup-download-failed = { $url } after { $retries } attempts: { $error }
setup-size-mismatch = { $file }: { $got } bytes downloaded, { $want } pinned; the file is removed and the next attempt starts over
setup-hash-mismatch = { $file }: SHA-256 { $got } does not match the pinned { $want }; the file is removed and the next attempt starts over
setup-unpacking = Unpacking { $file }…
setup-rename = renaming { $path }: { $error }
setup-waiting-other = Waiting for another download of the same components…
setup-delete = deleting { $path }: { $error }
setup-downloading = Downloading the models…
setup-source-changed = { $url }: the server gives { $got } bytes, { $want } are pinned; the source changed
setup-manifest-write = manifest { $path }: { $error }
setup-archive-no-files = the archive { $path } has none of the needed files
setup-wheel-not-zip = the wheel is not a zip: { $error }
setup-wheel-entry = wheel entry: { $error }
setup-archive-no-dll = the archive { $path } has no libraries
setup-unpack = unpacking { $path }: { $error }
setup-finalize = finishing { $path }: { $error }

## Settings applied after analysis

post-analyze-bad-vo-gain = vo_gain: a number of dB was expected, { $value } came
post-analyze-bad-flag = { $name }: 0 or 1 was expected, { $value } came
post-analyze-bad-container = container: mp4 or mkv was expected, { $value } came
post-analyze-bad-voice-slots = voice_slots: an object {"{"}male:[…], female:[…]{"}"} was expected
post-analyze-edit-failed = the setting after analysis { $edit }: { $error }

## Messages of the engines and libraries: glossary, LLM, ASR, translation, separation, faces, TTS, captions, OCR

glossary-over-limit = the glossary has { $total } entries, more than { $max }
glossary-empty-term = entry { $entry } has no term
glossary-field-too-long = entry { $entry } (“{ $term }”): a field is longer than { $max } characters
glossary-duplicate = the term “{ $term }” is listed twice (entries { $first } and { $second })
glossary-tsv-keep = line { $line }: keep is “{ $value }”, 1 or 0 was expected
glossary-tsv-empty-term = line { $line } has no term
glossary-one-of = send either entries or TSV, not both
glossary-nothing = nothing to save: no entries and no TSV
llm-spawn-failed = llama-server did not start: { $error }
llm-gguf-missing = the GGUF model was not found ({ $path })
llm-log-file = llama-server log { $path }: { $error }
llm-exited-early = llama-server exited before it was ready ({ $status }); stderr: { $stderr }
llm-not-ready = llama-server did not come up in { $secs } s (port { $port }); stderr: { $stderr }
llm-no-gpu-device = the local Gemma needs a graphics card, and the llama.cpp build sees none ({ $detail }). Choose OpenRouter or your own server for translation and vision, or check the graphics driver.
llm-http = the request to the model failed: { $error }
llm-api = the model API: { $error }
llm-rejected = the model API refused the request ({ $status }): { $body }
llm-empty-answer = the model { $model } returned an empty answer (no reason given)
llm-empty-answer-reason = the model { $model } returned an empty answer (finish_reason={ $reason })
llm-cut-short = the model { $model } hit the limit of { $max_tokens } { $max_tokens ->
    [one] token
   *[other] tokens
} (finish_reason=length): the answer is incomplete; it probably spends the budget on reasoning — choose a model without mandatory thinking
llm-prompt-cut = the server read only { $read } { $read ->
    [one] token
   *[other] tokens
} of a request of { $chars } { $chars ->
    [one] character
   *[other] characters
} and dropped its start — its context is too small: raise num_ctx in Ollama or the model's Context Length in LM Studio
asr-engine = speech recognition: { $error }
asr-wav-read = could not read the wav { $path }: { $error }
asr-resample = resampling: { $error }
asr-speaker-count = the number of speakers must be from 1 to { $max }
speakers-embedding = voice of speaker { $speaker }: { $error }
speakers-no-sample = speaker { $speaker }: no speech piece of at least 0.3 seconds to match the voice
speakers-too-many-voices = { $given } { $given ->
    [one] speaker
   *[other] speakers
} given, but the piece has { $found } different { $found ->
    [one] voice
   *[other] voices
}: they could not be merged safely by voice
speakers-bad-embedding = the voice model returned empty or invalid voice features
speakers-dimension-changed = the size of the voice features changed
speakers-more-voices = the piece has more voices than the given number of speakers
speakers-unmatched = the voices of the piece could not be matched reliably with the given number of speakers
translate-format-json = translation: the model answers with a JSON object by schema ({ $model })
translate-format-json-probe = translation: trying answers by JSON schema ({ $model }); if refused, numbered lines
translate-format-numbered = translation: numbered lines; the model { $model } does not declare structured_outputs in the OpenRouter catalogue
translate-schema-ignored = translation: { $model } accepted the JSON schema but answered with numbered lines; numbered lines from now on
translate-schema-ignored-server = translation: the server accepted the JSON schema but answered with numbered lines; numbered lines from now on
translate-schema-refused = translation: { $model } refused answers by JSON schema ({ $status }: { $body }); numbered lines from now on
translate-schema-refused-server = translation: the server refused answers by JSON schema ({ $status }: { $body }); numbered lines from now on
translate-line-flawed = translation: line { $line } keeps a translation with a remark: { $reason }
translate-line-failed = translation: line { $line } is not translated: { $reason }
translate-line-reason = { $line }: { $reason }
translate-lines-rejected = translation: { $bad } of { $total } { $total ->
    [one] line
   *[other] lines
} failed the check ({ $reasons })
translate-batch-stopped = translation: the batch of lines { $first }..{ $last } failed ({ $error }); the translation stopped
translate-batch-failed = translation: the batch of lines { $first }..{ $last } failed ({ $error })
translate-layout-no-vision = frame layout: skipped (no vision model is chosen or available)
translate-layout-not-needed = frame layout: skipped (subtitles are not burned in, no layout is needed)
translate-layout = frame layout: sub_style={ $sub_style } titles={ $titles } brands={ $brands }
translate-layout-failed = frame layout: skipped ({ $error })
translate-scene-failed = scene context: skipped ({ $error })
translate-scene-no-vision = scene context: skipped (no vision model is chosen or available)
translate-audio-failed = audio context: skipped ({ $error })
translate-context-trimmed = translation: the context block of { $chars } { $chars ->
    [one] character
   *[other] characters
} is cut to { $budget } (n_ctx guard)
translate-names-skipped = translation: the automatic name glossary is skipped ({ $error })
translate-chunks = translation: { $lines } { $lines ->
    [one] line
   *[other] lines
} -> { $chunks } { $chunks ->
    [one] chunk
   *[other] chunks
} (glossary: { $terms } { $terms ->
    [one] term
   *[other] terms
}, { $names } { $names ->
    [one] name
   *[other] names
})
translate-pass-done = translation: done, { $translated } translated ({ $flawed } with a remark), { $untranslated } left in the source language
glossary-pass = glossary: model pass { $pass }/{ $passes }
glossary-schema-refused = glossary: the server refused answers by JSON schema ({ $status }); asking for JSON as text
content-type-decided = content type: { $decided ->
    [anime] animation
   *[other] live action
} ({ $votes } { $votes ->
    [one] clear answer
   *[other] clear answers
} of { $frames } frames)
line-missing = missing from the answer
line-cut = the answer was cut by the token limit
line-untranslated = not in the target language
line-echo = repeats the source
line-too-short = too short ({ $got } < { $min })
line-too-long = too long ({ $got } > { $max })
line-loop = loops on “{ $gram }”
line-term-missing = the glossary term “{ $term }” is missing
line-gender = the speaker speaks of themselves as the other gender: “{ $form }”
line-latin = the source word “{ $word }” is left untranslated
translate-frame = extracting a video frame: { $error }
translate-audio = audio context: { $error }
translate-empty = the model translated none of { $lines } { $lines ->
    [one] line
   *[other] lines
}; the last cause: { $reason }
translate-empty-no-reason = the model translated none of { $lines } { $lines ->
    [one] line
   *[other] lines
}
translate-contract = the model's answer: { $problem }
translate-contract-answer = the model's answer: { $problem }; the answer: { $answer }
answer-no-json-object = it has no JSON object
answer-not-json = it is not valid JSON ({ $error })
answer-no-terms = it has no terms list
remix-failed = remix: { $error }
sep-engine-missing = the separation engine was not found ({ $path })
sep-model-missing = the separation model was not found ({ $path })
sep-spawn = starting the separation engine: { $error }
sep-engine-failed = the separation engine failed (exit code { $code }): { $tail }
sep-engine-killed = the separation engine was stopped without an exit code: { $tail }
sep-no-output = the separation engine made no vocal stem ({ $path })
sep-audio-io = separation audio: { $error }
sep-fell-back-to-cpu = the voice separation build for the graphics card found no card and ran on the processor ({ $why }). Choose «CPU» or «Auto» for voice separation in the settings, or check the graphics driver.
faces-model-missing = the model was not found ({ $path })
ffmpeg-exit = ffmpeg exit code { $code }: { $tail }
ffmpeg-killed = ffmpeg was stopped without an exit code: { $tail }
onnx-runtime = the ONNX runtime: { $error }
files-error = files: { $error }
model-no-outputs = the model returned no outputs
faces-output-shape = { $model }: the model output has an unexpected shape { $shape }
faces-output-count = { $model }: { $expected } model outputs were expected, { $got } came
faces-crop-size = the face embedding expects a 112x112 aligned crop, got { $width }x{ $height }
faces-sample-rate = the voice clip must be { $expected } Hz, it is { $got } Hz
faces-clip-too-short = the voice clip is too short to analyse
tts-library-load = the TTS engine did not load: { $error }
tts-cancelled = the generation was cancelled
tts-generation = the generation failed: { $error }
tts-invalid-param = invalid TTS parameter: { $error }
tts-streaming-unsupported = this engine DLL does not support streaming
render-higgs-model-failed = loading the Higgs model: { $error }
captions-write-ass = writing the subtitle file: { $error }
captions-filter-script = writing the ffmpeg filter script: { $error }
captions-ffmpeg-wait = waiting for ffmpeg: { $error }
captions-ffmpeg-timeout = ffmpeg did not finish in { $secs } s and was stopped (it hung): { $tail }
captions-burn-failed = burning the subtitles in failed: { $tail }
captions-nvenc-driver-old = the NVIDIA driver is too old for video encoding on the graphics card (NVENC): version { $need } or newer is needed. Update the driver from NVIDIA's site and render again
captions-frame-failed = the preview frame failed: { $tail }
ocr-no-dictionary = the text recognition model has no character dictionary
ocr-internal = text recognition: internal error ({ $error })
update-title = Dub Studio update
quit-title = Close Dub Studio?
quit-running = The studio is still working: { $n } task(s) queued or running. Closing now stops them; what is already done stays in the project, and the dubbing goes on from there with Continue.
quit-button-close = Close
quit-button-stay = Keep open
update-open-page = Version { $version } is out. Open the download page?
update-install-now = Version { $version } is out. Update now? The app restarts.
update-button-open = Open
update-button-install = Update
update-button-later = Later
update-open-failed = Could not open { $url }: { $error }
update-failed = The update did not install: { $error }
drama-needs-cuda = The Drama 3 voice needs an NVIDIA card with CUDA, and none was found ({ $card }). Choose Higgs.
drama-needs-vram = Drama 3 needs a graphics card with 6 GB or more, and this one has { $gb } GB ({ $card }). Choose Higgs.
drama-needs-ram = On a card under 24 GB, Drama 3 keeps its experts (18 GB) in RAM: it needs 32 GB of RAM, and this computer has { $gb } GB. Choose Higgs.
render-drama-missing = Drama 3 is chosen but not fully downloaded: download it in Models (voice), or choose Higgs.
render-drama-load-failed = loading Drama 3: { $error }
render-drama-oom = Drama 3 ran out of video memory ({ $error }): close other programs using the graphics card, or set a smaller amount in «VRAM for Drama 3» in the settings
render-loading-drama = loading Drama 3
qwen-needs-cuda = The Qwen3-TTS voice needs an NVIDIA card with CUDA, and none was found ({ $card }). Choose Higgs.
cpu-floor-missing = { $engine } needs a processor with AVX2, FMA, F16C and BMI2 (Intel since 2013, AMD since 2017), and this one lacks { $missing }
stage-name-sep = Voice separation
stage-name-asr = Speech recognition (Parakeet)
stage-needs-cuda = { $stage }: the settings choose «GPU (CUDA)», but { $why }. Choose «CPU» or «Auto» for this stage in the settings.
stage-gpu-why-no-nvidia = no NVIDIA driver was found
stage-gpu-why-no-device = the NVIDIA driver is installed and sees no NVIDIA card
stage-gpu-why-cuda-init = the NVIDIA driver does not answer ({ $card })
stage-gpu-why-driver-old = it needs NVIDIA driver { $min } or newer, and an older one is installed ({ $card })
stage-gpu-why-gpu-old = the card is older than Turing, and it needs compute capability { $min } or higher ({ $card })
qwen-no-linux = The Qwen3-TTS voice has no Linux build yet. Choose Higgs or Supertonic 3.
render-qwen-missing = Qwen3-TTS is chosen but not fully installed: download it in Models (voice), or choose Higgs.
render-qwen-load-failed = loading Qwen3-TTS: { $error }
render-loading-qwen = loading Qwen3-TTS
render-qwen-voice-other-engine = The voice { $voice } belongs to Qwen3-TTS, and the voice engine chosen is another one: choose Qwen3-TTS in Models (voice), or give this character another voice.
render-supertonic-missing = Supertonic 3 is chosen but not installed: download it in Models (voicing) or choose another voice.
render-loading-supertonic = Loading Supertonic 3…
render-supertonic-load-failed = Supertonic 3 did not load: { $error }
render-supertonic-voice-other-engine = The voice { $voice } belongs to Supertonic 3, and another voice engine is chosen: pick a voice of that engine or choose Supertonic 3.
render-supertonic-no-voice = Supertonic 3 does not clone, and { $voice } is not one of its voices.
render-supertonic-voices = Supertonic 3 voices by the speakers' gender: { $voices }
render-stress-failed = Russian stress marks were not placed: { $error }
setup-comp-supertonic3-name = Supertonic 3 (int8)
render-drama-no-ref-text = the voice reference has no transcript, and Drama 3 cannot clone a voice without one
setup-comp-drama3-purpose = Voices the translation in the original actors' voices, a second engine for graphics cards from 6 GB; on cards under 24 GB it needs 32 GB of RAM
setup-comp-qwen3-tts-purpose = Voices the translation in the original actors' voices or in nine ready voices of its own; a light third engine, about 3 GB of video memory on an NVIDIA card
setup-comp-qwen3-tts-engine-name = Qwen3-TTS engine
setup-comp-qwen3-tts-engine-purpose = The library and the 12 Hz codec Qwen3-TTS speaks through, shared by both quantisations
setup-comp-qwen3-tts-ru-stress-name = Russian stress for Qwen3-TTS
setup-comp-qwen3-tts-ru-stress-purpose = A small add-on for a Russian dub cloned with Qwen3-TTS: a plus before a vowel in a line (z+amok) puts the stress exactly there; the model's own voices are not affected
setup-comp-supertonic3-purpose = Supertonic 3: fast voicing in ten ready voices and 31 languages, no cloning
setup-comp-ruaccent-purpose = Puts stress marks and the letter ё into Russian text and resolves homographs by context
setup-comp-ruaccent-omograph-big-name = Large RUAccent homograph model
setup-comp-ruaccent-omograph-big-purpose = RUAccent's big_poetry model in int8 for reading homographs by meaning (замо́к/за́мок, мука́/му́ка): more right answers than the built-in tiny2 (29 of 33 against 24 on the test sentences); once downloaded it is used instead
render-drama-kept-original = ⚠ segment { $line }: the voice reference has no transcript (no speech was recognised in it), Drama 3 cannot clone without one — the original is kept
acpp-no-server = The audio.cpp engine is not installed ({ $path }): download it in Settings → Models
acpp-no-libraries = The audio.cpp engine is missing CUDA libraries ({ $files }): download the CUDA runtime in Settings → Models
acpp-no-model = The model «{ $model }» of the audio.cpp engine is not downloaded: download it in Settings → Models
acpp-error = audio.cpp engine: { $error }
acpp-no-cuda-build = The graphics card (CUDA) is chosen, but this computer has no NVIDIA card the audio.cpp engine runs on: it needs Maxwell or newer with driver 525 or later. Choose Vulkan or the processor in Settings → Models
setup-comp-audiocpp-build-name = audio.cpp engine: { $build } build
setup-comp-audiocpp-build-purpose = The engine built for another device: downloaded when voicing or recognition is set to it in Settings → Models
update-why-audiocpp-server = Higgs voices and CrisperWhisper run in the audio.cpp server instead of the old library: a stop ends the phrase at once, and the weights leave the card when voicing is done
update-why-transcribe-avx2 = Built for every processor with AVX2: speech recognition no longer closes the studio on processors without AVX-512
update-why-crisperwhisper = Recognition of other languages in place of Faster-Whisper: 99 languages with word timestamps
update-why-langid = Hears the language of a recording for CrisperWhisper when the source language is left on Auto
update-why-cuda12 = CUDA 12.9 libraries for the audio.cpp server on older cards and drivers
update-why-llama = A newer llama.cpp for translation
update-why-bsroformer-studio = The studios' own build of voice separation: ready for every NVIDIA card it supports without compiling on the first run, and on Linux it runs on Ubuntu 22.04 and newer
update-why-ffmpeg = A newer FFmpeg for reading and assembling video and sound
update-why-ytdlp = A newer yt-dlp with deno: sites change, and links download with the fresh version
update-why-faster-whisper-gone = Faster-Whisper is replaced by CrisperWhisper
update-why-parakeet-onnx-gone = Parakeet runs on the transcribe.cpp engine; these ONNX models are no longer read
update-why-higgs-library-gone = The audio.cpp server takes the place of the Higgs library
update-why-ort-gone = Replaced by ONNX Runtime 1.28.2
update-why-newer = This release uses another build of it
update-why-unused = No longer used
