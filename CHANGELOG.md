# Changelog

What changed, newest first. Dates are release dates; the app is versioned by its Windows
build. Every change a user can see is written here in the commit that makes it, and the
release notes on GitHub are taken from the release's section.

## 2026-10-10 — 4.1.1

### Fixed

- On the first-run screen the statistics checkbox, its label and What is sent stand on one line in one
  colour; the link sat lower than the label and in another colour.

## 2026-10-09 — 4.1.0

### Added

- **Everything the studio says is in the window's language.** Progress, errors, setup and download messages, casting, the voice library and the reports an agent reads come from the server in the six languages of the window, not in Russian or English whatever the window shows (#7).
- **Even out loudness** is a switch beside the track gain, on by default as before: every phrase is brought to one level and the dub to -14 LUFS with a -1 dBTP ceiling, as streaming platforms play it; off, the mix stays as it came. Agents use `loudness_set`.
- **Anonymous statistics and news from the hub.** The first-run screen and Settings - Anonymous statistics have a checkbox, on by default, with which the app sends once a day how many tasks finished, failed or were cancelled, its version, the OS and the class of the graphics card - never videos, transcripts, translations, voices or file names; What is sent shows the report, and DO_NOT_TRACK=1 or STUDIO_TELEMETRY=0 turns it off entirely. News from the author arrive without an update, on top of What's new.
- **Parakeet Ultra int8.** The Moondream fine-tune of Parakeet, quantized to int8 (about 0.7 GB instead of 2.6 GB), is a recogniser of its own in the variant picker and in the model list. The base int8 model stays the default and the fp32 Ultra stays where it is; the importer no longer takes the base int8 files for the Ultra ones or the other way round, though their sizes are nearly the same.
- **Direct Google Gemini TTS and Batch.** Choose Google alongside local voices and OpenRouter, with a separate API key and a live TTS model list. Batch runs use the `rust-genai` SDK and persist their Google job name; continuing a stopped render retrieves the same paid batch. MCP exposes the key settings, model list and project usage report with audio duration, wall time and tariff-based cost estimates.
- **An experimental Linux build** (x86-64, .deb and AppImage), made by hand by the `Linux build (experimental)` workflow. "First run" downloads the Linux builds of the same engines and CUDA libraries (llama.cpp, ONNX Runtime with CUDA, BSRoformer.cpp, ffmpeg, the Higgs engine for Linux, yt-dlp with deno, faster-whisper), unpacks tar.gz and tar.xz with their library links, and keeps everything in `~/.local/share/dub-studio`. The studio checks the card through `libcuda.so.1`; local voice on Linux needs an RTX 30 or newer, since the Higgs engine for Linux is built for those cards only. Engines started by the studio end with it. The build is not tested on a real Linux desktop: fixes from people who use Linux are very welcome.
- **The number of speakers in a recording.** The launch form leaves it automatic or takes 1 to 8 people. With a number given, voices are matched across the parts of a long recording with WeSpeaker; the choice is saved and available through MCP. From [pull request #13](https://github.com/timoncool/dub-studio/pull/13) by lostintired.

### Fixed

- **Per-line Gemini voice direction.** The renderer forwards `tts_style` as speech metadata and includes it, the provider and request mode in take cache keys. Style instructions are never prepended to spoken dialogue.
- **A video in a language Parakeet does not know** no longer comes out undubbed. Parakeet transcribes 25 European languages; on Chinese it heard one word in minutes of speech. With Auto as the source, a run where the voice is heard but almost no words stops and says to pick the source language (picking one outside Europe switches recognition to Whisper); with such a language set while Parakeet is selected, for example from an agent, the analysis says to choose Whisper.
- **Recognition on parakeet-rs 0.4.0:** repeated words keep their timestamps, and diarization no longer fails on some recording lengths.
- **Long recordings are transcribed in windows** of about 90 seconds cut at pauses; one pass over more than about ten minutes failed with a broadcast error or ran out of memory (#4, #9, #11).
- **Speaker labels of a long recording.** With a number of voices given, continuous diarization can be turned on with `DUB_STUDIO_DIAR_CONTINUOUS=1`: the model keeps its state across the whole track, and the results of this mode have a cache of their own. It needs memory for the whole recording. From pull request #13 by lostintired.

## 2026-10-01 — 4.0.0

### Added

- **An MCP server inside the studio.** An AI agent (Claude Code, Cursor, Codex and others) connects to
  `http://127.0.0.1:8793/mcp` and drives the whole studio: makes projects of your videos, analyzes them,
  edits the transcript and the translation line by line, casts the voices, renders, exports more
  languages and saves the result. The Settings section **Agent (MCP)** shows whether an agent is
  connected and gives the command for Claude Code and the config for other clients.
- **One call for one result.** An agent gets a file's transcript, translated subtitles, a dubbed video,
  the voice and the background apart or the text in the picture with a single tool; a second call for
  the same file answers from the finished project. Subtitles are written as SRT, WebVTT, JSON with word
  timings or ASS styled as the render burns them.
- **The agent works in the studio's window while you watch.** It opens a project in the editor, selects
  and edits lines, cuts and joins them, moves them on the timeline, restyles the subtitles and switches
  their languages, shortens lines to fit, picks and pins takes, edits the glossary, plays the dub and
  starts the export; every step is highlighted and signed «Agent: …». It can also record a voice from
  your microphone, open the models folder and save text files into a project. It sees the window as a
  picture and as a list of controls; a key or a password field reads only as filled or empty. What an
  agent or another window saves appears at once, and undo never writes over it.
- **Cut and join lines** in the editor: scissors at the playhead and join with the next line; the
  words, the texts and a line's own subtitle are divided or joined with them.
- **Jobs you can stop and continue.** Every long task shows its step and has **Cancel**; a task stopped
  by an error, a crash or closing the app is continued with **Continue** from where it stopped: the
  finished stages and the already voiced lines are taken from the project, not made again (issue #2).
- **Parakeet Ultra**, a fine-tuned speech recogniser with fewer errors (a download in the model
  settings), and **Nemotron 3 Diarization**, which tells apart up to 8 speakers.
- **A model manager.** Downloads run beside the jobs and continue after a restart; every file is checked
  against its pinned SHA-256; the disk space is checked first; downloaded models can be removed; the
  settings show whether the NVIDIA driver can run CUDA 13.
- **Translation and reading the frames each by its own provider**: the studio's Gemma, a local
  OpenAI-compatible server (Ollama, LM Studio, vLLM, llama-server) or OpenRouter with the price and the
  context of every model. The OpenRouter key and the proxy password are kept out of the settings files.
- **Proxy in three modes** (as in Windows, your own, none) for model downloads, the cloud and updates;
  HTTP, HTTPS, SOCKS5 and SOCKS4, a seller's `host:port:login:password` works as it is.
- **Settings in sections** with **About**, a screen of **all projects** with search, sorting and
  filters, and the start form's choices kept by the studio for every window.
- **Align imported subtitles to the speech**: a switch for SRT/ASS in the video's language; it holds a
  frame rate drift and cut pieces.
- **What's new** window with the release notes, and a README on six languages with every download,
  what runs where, troubleshooting and the models' licenses.
- **A video by its link.** Paste a link on the start screen (YouTube and the other sites
  yt-dlp knows), see the title, length and preview, pick the quality (best, 1080p, 720p,
  480p or audio only) and, if the site has them, its subtitles made by people: they come
  into the project as imported subtitles; when they cannot be downloaded, the video still
  comes, without them. The download runs in the background beside the dubbing jobs, survives
  a restart of the app and continues where it stopped; the downloaded video then waits on the
  start screen like a chosen file: Start processing dubs it with the settings there, Manual
  mode opens it in the editor. A video that wants a signed-in browser (age check, members, bot
  check) takes a cookies.txt; a blocked, private or DRM video says why and what to do. The
  downloader is an optional component in Settings → Models (yt-dlp with deno, pinned and
  checked by SHA-256); a newer yt-dlp is checked for once a day and used only after it
  passes its checks, and the version with an Update button is shown there. The same through
  MCP: `url_probe`, `project_create_from_url` and the `url_*` tools.
- **Subtitles in two languages.** The subtitle language is chosen apart from the dub: none, the
  original, the translation or both. With both, the translation is the main line, lit word by word
  with the dub, and the original is a second line above or below it, smaller, in its own color and
  opacity — in the preview, the burned video, SRT and WebVTT (two lines in one subtitle) and as
  separate MKV tracks with their language. In a dub, «original» now shows the original words, not
  the dubbed translation. Lines follow Netflix's rules: at most 42 characters, and a subtitle longer
  than 7 seconds is split at a word. The same through MCP: `subtitles_content_set`.
- **Will the line fit?** Every line of the editor and the transcript shows whether its translation
  fits its slot, is tight or does not fit, judged by the voice's own pace; a counter and a filter
  find the lines that do not fit. **Shorten to fit** rewrites one line or all of them with the
  translation model by the real length of the voiced phrase, and the render can do it by itself
  (Settings: shorten the translation when a phrase does not fit). The same through MCP:
  `segment_shorten`.
- **A line's takes.** The last five voicings of every line are kept: listen to them, pick another one
  without voicing again, go back to an earlier one with its text, or pin one so that a new render
  keeps it. The same through MCP: `takes_list`, `take_select`, `take_pin`.
- **A glossary for the project and the series.** Names, terms and brands with their translation or
  «do not translate», how the voice says them, and how speech recognition misspells them (the
  analysis corrects it). **Collect from text** proposes candidates for you to confirm; the glossary
  goes to and from TSV, is saved to a series' casting profile and comes into the next episode's
  analysis. When the glossary changes after the translation, the window says the translation is out
  of date and makes it again. Replacing a word of a recognised line in the editor offers to add the
  term. The same through MCP: `glossary_get`, `glossary_set`, `glossary_extract` and
  `series_glossary_*`.

### Changed

- The studio answers on the fixed port **8793**, and a second launch brings the open window forward
  instead of starting another studio.
- Nothing the studio starts outlives it: separation, recognition, translation and ffmpeg end with the
  app, however it is closed.
- Subtitle words light up by the dub's own speech in the word-by-word presets, and the dub's phrases are
  placed where they really sound.
- Peaks of a phrase are lowered by a look-ahead limiter instead of being clipped, and the mix is kept
  uncompressed until the one final encoding.
- Phrases that speech recognition invents over music and silence ("Thanks for watching", subtitle
  credits) are hidden from the dub and the subtitles and can be brought back in the editor.
- Punctuation stays with its word in the transcript and the subtitles ("uniform?", not "uniform ?").
- The OpenRouter helper program is gone: the studio talks to OpenRouter itself.
- **Less speeding up of dubbed lines.** The silence the voice leaves around a line is cut before
  the line is fitted to its slot, and long pauses inside it shrink when it would not fit
  otherwise. A line that fitted with its silence is placed shorter than its slot, not slowed down.
  The render log says how much was cut and how many lines stayed within the speed-up limit thanks
  to it. Projects dubbed earlier with the studio's own voice engine voice their lines once more on
  the next render, with the new references.
- **Cloned voices keep their high frequencies.** A cloned voice's reference is cut from the
  separated vocals in full band instead of 16 kHz, its edges fall on pauses between words and its
  transcript is exactly the words it contains. Only when neither the analysis nor the render
  separated the voice does it come from the original mix, and the render log says so.
- **Voices made from a speaker are full band too.** They are cut from the project's separated vocals
  the same way; when the line cannot be separated from the music, making the voice fails instead of
  saving it with the music, and the status line says why: that the vocal separation engine is not
  installed and where to install it, or what the engine answered.
- **Translation is checked line by line.** Where the server holds it — the studio's Gemma, OpenRouter
  models with structured outputs, a local server that accepts it — the model answers in a strict JSON
  format, and every line is checked: its language, a copy of the source, its length against the
  slot, loops, a cut-off answer and the glossary's terms. A line that fails is asked again in a
  smaller batch; a failure a smaller batch cannot cure (the network, a refused key, an empty answer)
  stops the translation with its reason.
- **The voice says only what is meant to be heard.** Sound tags ([music], (laughs), \*sigh\*), speaker
  labels, markup and repeated loops are taken out of the text before it is voiced, and the glossary's
  pronunciations are applied; a line left without words stays silent, and the editor says why.
- A dot after an abbreviation, an initial or in a decimal number no longer ends a transcript line, and
  a line where the speaker changes is cut at that word.

### Fixed

- A translation that did not happen (no model, the local server down, most lines left as they were)
  now stops the analysis with the reason instead of dubbing the video in its own language.
- Translation on the new llama.cpp: a whole frame fits the micro-batch, and the server's log is kept in
  `logs/llama-server.log`.
- A short clip with two people is no longer merged into one speaker, and casting no longer splits two or
  three people into five characters.
- The header no longer stays on "Downloading models…" after the download finished.
- Saved subtitles leave out hidden lines and the lines that keep the original speech, and use a line's
  own subtitle text, as the burned subtitles do.
- Deleting a casting profile asks in the studio's own dialog and shows why it failed instead of
  silently keeping the profile.
- **Sound no longer drifts from the picture on clips with broken timestamps.** Audio is read by
  its timestamps, with gaps filled by silence, so on screen recordings, phone clips and remuxed
  files the recognition, the dub and the subtitles stay in sync to the end.
- **The extra voices list shows the whole catalog.** It stopped at the first 500 voices of
  the dataset.
- **Voice downloads are checked.** The voice pack and the extra voices come from a pinned
  revision; the pack and every voice are checked against their SHA-256 before they are kept,
  and a broken download no longer leaves a half file in the library.

## 2026-08-06 — 3.1.1

### Fixed

- **Export no longer fails on ffmpeg 8.** Burning subtitles stopped with
  `Unrecognized option 'filter_complex_script'` on ffmpeg 8.x when on-screen text was
  translated in a clip of several minutes; the option the app used was removed in ffmpeg 8.0.
  The right option is now picked for the installed ffmpeg version, and a test guards future
  ffmpeg updates. Thanks to @nevoin for the detailed log in #1.

## 2026-08-06 — 3.1.0

The whole release is the work of Serega (SilentBob), who took the sources, improved dubbing
quality, fixed bugs and sent the code. Thank you!

### Added

- **Multi-take selection.** Three takes of every phrase with different variability are
  synthesized and the one whose natural rhythm fits the slot best is kept: less artificial
  speed-up. A switch in Settings, off by default (slower, better).
- **Emotional reference of the scene.** A micro-reference of the original voice is taken from
  the current second of the clip, so the dub keeps the actor's delivery (shout, whisper,
  laughter, anger, irony) without losing the timbre; it is skipped for a voice from the
  library. On by default.
- **Dynamic speech rate.** The text density (characters per second) sets the pace so the
  phrase fits its subtitle window. On by default.
- **Two-way time stretching.** Phrases are also softly stretched (up to +15 %) without a change
  of pitch, which removes dead silence after short lines.
- **Phrase duration control (Stretch QC)** for the timing limits and the maximum stretch, with
  drift compensation on dense dialogue. On by default.
- **A 10 ms crossfade** at the edges of every segment removes clicks between phrases.
- **Breaths between phrases**: a quiet natural breath is inserted into pauses of 0.4-1.8 s.
  Off by default.
- **Text check through ASR**: what was actually spoken is compared with the translation using
  the local Whisper. Off by default, because turning it off makes synthesis noticeably faster.
- **A full subtitle editor**: translated text, original text and speaker assignment;
  drag-and-drop of the start and end of a phrase on the timeline with magnetic snapping to the
  peaks of the waveform and the neighbours' edges; import of `.srt` / `.ass` with their timing;
  saving subtitles with a "Save as" dialog; a manual project mode for subtitle work.

### Changed

- **The dub is never replaced by the original.** Short lines, shouts and chorus phrases used to
  fall back to the original track after a bad synthesis; the generated dub now always stays on
  the timeline and a defective phrase is re-synthesized down a ladder of variability.
- Deleted and hidden phrases are no longer re-synthesized, which saves GPU time on export.
- Regenerating one phrase resets the cache of that phrase only.
- The log shows how many phrases actually go to synthesis.

### Fixed

- **ffmpeg timeout on long videos**: a hard-coded limit replaced the calculated one.
- **Chinese, Japanese and Korean speech** was detected wrongly: the detector required at least
  four words separated by spaces, which CJK text does not have.
- A phrase with no overlap with any speaker goes to the nearest speaker in time, not always to
  the first.
- The emotional reference overwrote files in multi-take and did not turn off for a voice from
  the library.
- The action log froze during long synthesis and rendering.
- The settings list scrolls as a whole under a fixed header.

## 2026-07-28 — 3.0.4

### Added

- **Cloud voices in casting.** With cloud voice-over (OpenRouter) on, casting and auto-casting
  work with cloud voices: each character gets a cloud voice, picked by gender, instead of
  a forced local clone.

### Changed

- **Audio quality no longer degrades.** In the subtitles, transcript and multi-track exports
  the original audio track is kept as it is (channels including 5.1, sample rate and bitrate
  untouched; it used to be mixed down to stereo and recompressed); a dub is assembled at a
  high bitrate.

### Fixed

- **Whisper on the GPU really runs on the GPU.** It silently ran on the CPU for lack of CUDA
  libraries; the app now downloads cuBLAS and cuDNN for it.

## 2026-07-23 — 3.0.3

### Fixed

- **SOCKS5 proxy.** The Proxy tab of 3.0.2 failed with "Enable feature socks-proxy" on SOCKS
  proxies; SOCKS4 and SOCKS5 work now. A SOCKS proxy is entered with its scheme:
  `socks5://host:port`.

## 2026-07-22 — 3.0.2

### Added

- **A proxy server.** A Proxy tab in Settings sends all of the app's outgoing traffic (model
  downloads and OpenRouter requests) through an HTTP, HTTPS or SOCKS5 proxy, with a Check
  button that tells whether Hugging Face and OpenRouter answer through it.

### Changed

- **Switching from Transcript to Dub, Subtitles or Voice-over is instant**: the finished
  transcript is reused and only the translation runs, where the whole analysis used to start
  again.

## 2026-07-22 — 3.0.1

### Fixed

- **Character casting works again.** The face and voice recognition models were missing from
  the installer of 3.0.0, so casting saw no faces and put every line on one speaker. They are
  downloaded now.
- **GPU acceleration no longer fails.** Without cuFFT, diarization and speech recognition on
  the GPU stopped with "CUDA execution provider is not enabled in this build"; cuFFT now comes
  with the CUDA runtime and the portable build.
- **Missing models are fetched on demand**: switch a feature on, and its models are downloaded
  before the dub starts.

## 2026-07-21 — 3.0.0

### Added

- **Character casting (beta).** A character is a face plus a voice: faces are gathered across
  the video, the same face is recognized and bound to a speaker by co-occurrence of lines, and
  the clearest frame becomes the avatar. The casting profile is saved for the whole series, so
  the next episode applies the same voices to the same characters. A Real faces / Cartoon and
  anime switch selects the detection.
- **OpenRouter as a cloud provider**, chosen per stage next to the local engine: translation
  and vision (Gemma or OpenRouter), voice-over (Higgs or OpenRouter), speech recognition
  (Parakeet, Whisper or OpenRouter). Models and voices come from the live OpenRouter catalogue,
  the key is stored locally and checked with one button, the cost of a run is shown, and
  requests run in 1-16 parallel streams. Everything cloud is off by default.
- **Any engine on any device.** Separation, diarization and recognition each run on the GPU or
  on the CPU on their own, in any combination; a CPU build of the separation engine is
  downloaded for that.
- **Auto-casting of cloud voices (beta)** by the speaker's gender and the dub language, from
  a built-in reference of 271 voices of 12 providers.
- **Hardware presets**: the GPU and VRAM are detected and a preset is recommended (RTX 5090,
  RTX 4090, 16, 12 and 8 GB, Custom, cloud). With cloud selected on the first run the heavy
  local models stop being required and are not downloaded.
- **Background ducking** under the voice and the **blur plate under subtitles** became options.

## 2026-07-18 — 2.7.0

### Added

- **Library voices at the start**: lists of male and female voices taken from a folder; after
  the analysis each speaker's gender is detected and voices are handed out by priority, an
  unset gender is cloned.
- **Translation style**: Normal, Technical, Literary and Colloquial presets plus an
  instruction of your own.
- **Keeping the original audio track** on export as a second track next to the dub, in MP4 or
  MKV, with language and title metadata.
- **Keyboard shortcuts** for playback, seeking, volume and full screen, a seek slider with a
  timecode under the editor preview, and a Back button from the transcript window.
- The original's volume slider under a voice-over is on the start screen and in batch mode;
  the new default is -12 dB.

### Changed

- **Ducking without seesaw**: the background is lowered along a deterministic envelope built
  from the exact phrase timing, and pauses shorter than 1.6 s do not raise the music.
- **Steadier speech rate**: the speed-up cap went from x2.0 to x1.25, and translation gets
  a length budget so it does not swell.
- The dub is 8-10 LU above the background, the broadcast norm.
- A clean start screen: only the file, languages, voice-over mode and Start are visible, the
  rest is in collapsible sections.

### Fixed

- A hard timeout on mixing with ducking prevents a hung worker on a many-hour file.
- Speaker gender is detected by a normalized cross-correlation without octave errors.

## 2026-07-17 — 2.6.0

### Added

- **Automatic quality control of the voice-over.** Voice references are chosen by speech
  density and each is checked by transcription, so shouts, noise and empty clips do not get
  into references; every synthesized phrase is recognized and, if it does not match the
  translation, re-synthesized with other parameters and an alternative reference.
- **Long videos**: a 90-minute film is analyzed in about five minutes, three hours in about
  eighteen; separation and diarization switch to windowed processing and their memory does not
  grow with the length.
- An optional per-stage **benchmark** (time, GPU and VRAM) in Settings, off by default.

### Changed

- **Sound as in a real dub**: the voice is always audible over the background (the music
  ducks only under phrases and lives at 100 % in pauses) and the dialogue is level across the
  whole film (per-phrase EBU R128 normalization to -14 LUFS).
- The pipeline is separation, diarization, recognition, so the transcript and references are
  built on the clean vocal (+4 % recognition accuracy).
- Steps that really run are the only ones shown, and the status line shows the recognition
  engine actually in use.

### Fixed

- A video build that hung forever on hundreds of subtitle plates (the Windows command-line
  length limit) and hard timeouts on every ffmpeg call.
- A cap on the length of TTS generation: a synthesis that ran away for tens of seconds is
  excluded, and the VRAM peak fell from 24 GB to 6.5 GB.

## 2026-07-16 — 2.5.1

### Changed

- More subtitle fonts.
- The top bar holds the mode buttons and Export, the action log sits in the centre, and the
  status line shows the whole engine stack (ASR, translation and vision, TTS, separation,
  diarization, OCR).
- The header of the phrase list (tabs and selection bar) stays in place while scrolling, and
  the speaker picker is shown only on the active phrase.
- The window title carries the version.

## 2026-07-16 — 2.5.0

### Added

- **Import of ready-made subtitles** (`.srt`, `.ass`, `.ssa`): text and timing come from the
  file instead of automatic recognition, and speakers are assigned by diarization. Tick
  "subtitles already in the target language" and translation is skipped too.
- **Search in long lists**: voices and the 100+ languages are filterable inputs, languages
  also by their name in the interface language and by code.

## 2026-07-16 — 2.4.1

### Fixed

- A desync of the dub after a manual edit of a phrase's timing.
- A preview deadlock when the video or project changed.
- A hang of the progress of a multi-language export.
- A title no longer stays in a foreign language when translation fails.
- Hiding the console no longer touches a shared terminal.

## 2026-07-16 — 2.4.0

### Added

- **Multi-language export.** The arrow next to Export sends one video into several languages
  at once; every language keeps the subtitle layout, styles, blur boxes and the cloned voice,
  and only the text is translated and re-voiced.
- **Subtitle timing edit**: start and end fields for the active phrase.
- **Add a speaker** in the Voice panel or right in a phrase row, for voices recognition did not
  find.
- **Recent projects** on the start screen with preview frames.
- Hints next to options and a window icon in Alt+Tab and the taskbar.

## 2026-07-16 — 2.3.6

### Changed

- **A fast live preview.** The preview follows the sound at the real speed of the server, with
  frames dropped but smooth and in sync: a new frame is requested when the previous one has
  loaded, frames are JPEG, and big videos are rendered smaller while playing. Export is
  unchanged.

## 2026-07-16 — 2.3.5

### Added

- **Audio-only mode.** An audio file (WAV, mp3, flac, m4a, ogg) is voiced into a WAV, singly or
  as a batch.

### Fixed

- The preview played the old voice-over after "regenerate all"; it now plays the newest.

## 2026-07-15 — 2.3.4

### Changed

- Detection of on-screen text (OCR) is off by default, so a dub without subtitles does not
  spend minutes scanning frames.

### Fixed

- "Download all" no longer loops at 100 %: the check of installed weights tolerates a small
  drift of file sizes on Hugging Face.
- OCR cleans its temporary frames folder.
- The version in the header is right after an overlay update of the portable build.

## 2026-07-15 — 2.3.3

### Fixed

- Export no longer re-voices a finished dub after a single phrase was edited.

## 2026-07-15 — 2.3.2

### Added

- **Export of SRT and TXT** written directly to a file, and every save or export opens the
  file manager with the file selected.
- The app version in the header, an updated help and donation text.

### Changed

- Editing one phrase (regenerate, delete, hide, gain) rebuilds only what it touches, and the
  separated vocal is cached.
- Detection of on-screen text is a checkbox.

## 2026-07-15 — 2.3.1

### Added

- **100+ dub languages** (the full Whisper set, up from 30): Whisper recognizes the source in
  99 languages, Gemma translates, Higgs Audio voices with the cloned voice. A source outside
  the 25 European languages of Parakeet switches ASR to Whisper with a notice.
- **Your own phrases**: a + phrase button in the transcript adds a line voiced by the speaker
  and shown in the subtitles.

## 2026-07-15 — 2.3.0

### Added

- **30 languages** of dub and subtitles (up from 6) with native names in the picker; subtitles
  in any script render out of the box.

## 2026-07-15 — 2.2.5

### Fixed

- **Resumable downloads.** A dropped connection no longer restarts a 12 GB download: a manifest
  of finished chunks lives next to the file, only the missing chunks are fetched, and data is
  written to disk before it is marked. This fixes Gemma Q8 that could not be installed.

## 2026-07-15 — 2.2.4

### Fixed

- The first-run screen hung at 100 % when the NVIDIA driver check gave a false negative; the
  driver no longer blocks entering the app.
- The dub drifted out of sync after a segment was edited or deleted; the voice-over cache is
  now kept by segment id.
- The no-subtitles mode is really clean and a click on a mode no longer revives disabled
  subtitles.
- A float16 Whisper quantization on the CPU no longer breaks transcription; the engine, model
  and quantization are kept between runs.
- The portable build contains the `.exe` again.

### Added

- Visible **Performance** controls: the Gemma prefill batch and the clone reference length, for
  machines with 32 GB of RAM.

## 2026-07-15 — 2.2.3

### Added

- **Whisper as an alternative ASR engine** ([Purfview faster-whisper
  standalone](https://github.com/Purfview/whisper-standalone-win)): runs on the CPU out of the
  box, in sizes tiny to large-v3-turbo and several quantizations.
- **Composable modes**: independent switches for audio (original, dub, voice-over, transcript),
  subtitles (none, original, translated), burn-in and funny remix, in any combination, also in
  batch and in the editor.

## 2026-07-15 — 2.2.2

### Fixed

- **Switching models works**: downloaded alternative quantizations of Higgs, Roformer,
  Parakeet and Gemma are applied at every generation, without a restart.
- **Drag-and-drop of files** works again.

## 2026-07-14 — 2.2.1

### Added

- Karaoke play-along in the transcript mode.

### Fixed

- Alternative models downloaded corrupted from Hugging Face Xet storage; downloads now retry
  ranges, are validated by size and `GGUF` magic, and only then renamed.
- Processing status follows the interface language.

## 2026-07-14 — 2.2.0

### Added

- **Voice-over mode**: the translated voice over the ducked original, with the volume of the
  original adjustable in the editor.
- **Subtitles in the original language** without translation.
- All modes switch inside the editor, and the selected video has a preview on the home screen.
- Real alternative Gemma quantizations (Q5_K_M, Q6_K, Q8_0).

### Fixed

- Export no longer re-synthesizes voice-over that was already approved.

## 2026-07-14 — 2.1.0

### Added

- **Auto-update**: the app checks GitHub for a new version and installs it in one click, with
  a signature check for the installed build and a notice with a link for the portable one.
- **Drag and resize of titles right on the frame**, moving a phrase to another speaker, timing
  and alignment of titles, timing of masks.
- Models download through a shared pool of 16 connections.

## 2026-07-13 — 2.0.0

### Added

- **A fully native rewrite**: Rust and C++/CUDA engines (GGUF and ONNX) instead of Python, one
  `.exe`, every model, engine, the CUDA and VC++ runtime and ffmpeg downloaded by a button on
  the first run. Projects of the previous version open without conversion.
- **Transcript with diarization** as a screen of its own, with a one-click voice from each
  speaker.
- **Batch processing**: a queue of files run with one setup.
- **Any video format** through ffmpeg.
- Text cover-up that makes the video look as if it was made that way: a near-uniform
  background gets a solid fill of the same colour, a textured scene a blur.

### Changed

- Subtitle blur plates are exactly the size of the drawn text.
- Voices are better: a different voice lands on every speaker in pack mode, and the Higgs
  clone gets the text of its reference.

## 2026-06-23 — 1.0.0

### Added

- The first public release: a Windows installer of the Python-based app with the models
  downloaded on the first run.
