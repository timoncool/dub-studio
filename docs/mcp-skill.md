---
name: dub-studio
description: Drive Dub Studio on this computer through its MCP server - dub a video into another language with the speakers' own cloned voices, make a voice-over, translated or original-language subtitles, or a transcript; get a file's transcript, translated subtitles, dubbed video, voice and background apart or on-screen text in one call; fix the translation, the timing and the speakers line by line; cut, join and move lines on the timeline; restyle the subtitles, add titles and blur boxes; cast the characters of a series and give them library voices; export the same video in several languages; export SRT or TXT; save the result to a folder - either at once with a result back, or in the studio's window while the user watches. Use whenever the user asks for anything the studio does.
---

# Dub Studio through MCP

Dub Studio serves MCP at `http://127.0.0.1:8793/mcp` while it is open (Streamable HTTP,
stateless JSON-RPC). Every tool runs the same code as a button of the studio, through the
routes its window calls.

## If the studio is not running yet

1. It is a Windows desktop application. If it is not installed, download the installer or
   the portable archive from https://github.com/timoncool/dub-studio/releases/latest. It is
   made for an NVIDIA card; its first start offers to download the models.
2. Start it. The MCP server is up as soon as its window is: `http://127.0.0.1:8793/mcp`.
   Nothing else to install - no npx, no bridge.
3. Connect (below), then call `studio_status`. If models are missing, `models_status`
   names them and `models_download` fetches them.

## Connect

```bash
claude mcp add --transport http dub-studio http://127.0.0.1:8793/mcp
```

Other clients: `{ "mcpServers": { "dub-studio": { "type": "streamable-http", "url": "http://127.0.0.1:8793/mcp" } } }`.

The server also serves this skill (resource `studio://skill`, prompt `studio`), the
language codes (`studio://languages`), every edit of a project with its fields
(`studio://patch-ops`) and a prompt `dub_video` (a file, a language, a mode). It speaks MCP
`2026-07-28` (stateless: every request carries its version in `_meta`, `server/discover`
describes the server) and the handshake revisions `2025-11-25`, `2025-06-18` and
`2025-03-26` through `initialize`. Only agents on this computer and the studio's own window
may connect.

The user sees it in the studio too: Settings, **Agent (MCP)** shows whether an agent is
connected and the address to paste.

## Ground rules

- **Start with `studio_status`.** It tells what runs now, what finished last, and whether
  the required models are there.
- **Long work is a job**: `project_analyze`, `project_dub_audio`, `project_render`,
  `project_export_lang`, `project_retranslate`, `project_remix`, `segment_shorten`,
  `project_resume`, `voices_download_pack`, `glossary_extract`, and a download by link
  (`project_create_from_url`, whose `fetch.id` is waited for the same way). Each answers a
  `job_id`; then `studio_wait` with it (or `until: analyze | dub_audio | render |
  export_lang | retranslate | remix | align | shorten | download | voices_pack | separate |
  detect_text | glossary | idle`) instead of polling. It returns within a minute (30 s by default, 55
  at most) with how far the work got; call it again. `job_get` and `jobs_list`
  read jobs (`jobs_list` with a `pid` also shows the project's last stored job),
  `job_cancel` stops one, `project_resume` starts a project's interrupted or failed job
  again where it stopped.
- **One job holds the graphics card at a time**, and a preview frame is made there too:
  `project_frame` answers "busy" while a job runs. `studio_wait until: idle`, then take
  the frame.
- **Edits are instant and saved.** A line whose words, timing, speaker or voice changed is
  *dirty*; `project_dub_audio` and `project_render` voice only the dirty lines again, the
  rest comes from the cache. `segment_regen` marks one line, `segments_regen_all` every
  line. Changing the mode, the target language, the translation's tone or the voices makes
  every line dirty.
- **How the dub sounds**: a voiced line loses the silence around it before it is fitted to its
  slot (long pauses inside shrink only when it would not fit), and a cloned voice's reference is
  cut from the separated vocals in full band, at word boundaries - so is a `voice_from_speaker`
  voice, which is refused with `no_separation` when the project has no separated vocals and no
  separation engine is installed, and fails with the reason when its line cannot be separated.
- **Answers are short by default**: `project_get` leaves out word timings and the vision
  context, an edit answers what it changed and how many lines are dirty. Pass
  `response_format: detailed` for everything - and only take a project for `project_put`
  from a detailed `project_get`, or the fields left out are lost.
- **Look ids up, never guess them**: `projects_list`, `project_get` (line ids, title and
  blur box idx), `voices_list`, `casting_get`, `casting_library_list`, `models_status`,
  `engine_presets_get`.
- **Files on this computer are passed by path**: `project_create` (a video, and subtitles
  to import), the one-call tools (below), `models_import`, `project_save_output`,
  `project_export_text` and `export_subtitles` (a folder). `project_files` names the
  project's own files.
- **Destructive tools say so**: deleting, cancelling, replacing a project, analyzing again
  (it replaces the lines), retranslating, remixing, and edits that overwrite text are marked
  destructive, so your client asks the user first.
- **The OpenRouter key never comes back**: `settings_get` and `studio_capabilities` show
  only `or_key_set`, `openrouter_status` where the key comes from.
- **Two ways to do a thing.** The tools above do it at once and answer the result - a
  transcript (`project_transcript`), a fixed line, a render - with or without the window.
  The `editor_*` and `ui_*` tools do the same in the studio's window while the user watches
  (below). Use the window when the user wants to see it done or to learn how; use the atomic
  tools for batches and for work nobody watches.
- **The window keeps up with you.** Whatever you change with any tool, the open window reads
  again at once: the lines, the lists, the settings, a job you started (its progress shows in
  the window, a render in its Files panel). The window's undo never erases your edits or what
  your jobs saved, even when the user started a job of their own meanwhile: after your change
  its history starts again, and an undo made on an older state is refused.

## One call on a file, or work in the studio

### Direct Google TTS and Batch

`google_status` reports whether a key is configured; `google_set_key` verifies and stores it,
`google_delete_key` removes it. The key is never returned. `google_models` lists available TTS
model IDs and generation methods. Set `tts_provider=google`, `google_tts_model` to an ID without
`models/`, and `google_tts_mode=standard|batch` using `settings_set`. Voices and autocasting use
`or_tts_voice` and `or_tts_autocast`; standard concurrency uses `or_concurrency`.

Run `project_dub_audio` or `project_render`, then `studio_wait`. Batch uses Google's discounted
asynchronous service and may wait in its queue for up to the documented 24-hour target.
`project_google_tts_report` returns persisted Google Batch names/states and the latest usage,
audio duration, elapsed time and tariff-based estimated USD cost, not an invoice.
Cancelling the local job pauses polling; it does not cancel the remote Google batch. Use
`project_resume` to retrieve that same paid batch. An uncertain submission is blocked from
automatic resubmission; inspect the recorded Google displayName before retrying.
`tts_style` on each segment is voice direction metadata, never part of the spoken text.
The renderer keeps existing takes; pinned takes remain protected until explicitly unpinned.

The same results come two ways.

- **One call on a file** - when the user wants a result of a file and nothing more: "the
  transcript of this", "Spanish subtitles for it", "dub it into Russian", "take the voice
  off the music", "what does the sign say". `transcribe_file` (text, SRT, VTT or JSON with
  speakers and word timings), `translate_file` (SRT, VTT or JSON), `dub_file` (the finished
  video, copied into `out_dir` when given), `separate_file` (the voice and the background as
  WAV) and `detect_text_file` (the text in the picture, with its boxes and times) take the
  file's `path` and answer the result itself. The file is read where it lies, never copied.
  `export_subtitles` writes a project's subtitles as SRT, VTT, ASS or TXT, in its target
  language or the original.
- Each makes a project of the file, listed by `projects_list` with `source: agent`. The
  same tool called again with the same file and arguments finds that project and answers
  from the finished work at once; a changed file or other arguments make another project.
  `project_delete` removes it; nothing else does.
- A call waits for the studio up to `seconds` (45 by default, 55 at most). When the work
  takes longer it answers `done: false` with the `job` at work: `studio_wait` with its
  `job_id`, then call the tool again with the same arguments - the finished stages are
  kept, and `dub_file` goes on from the analysis to the render.
- `dub_file` tells what fell back in `degradations`, read off the finished dub:
  `background_not_separated` (no voice separator: no music or effects under the dub),
  `ocr_skipped` (no on-screen text reader: the text in the picture stays as it is),
  `single_speaker` (every line voiced as one speaker; its detail says whether the diarizer
  is missing), `voices_not_cast` (autocast left these `speakers` on their own cloned voice),
  `voices_not_as_asked`, `no_speech`. Tell the user; `models_status` and `models_download`
  bring a missing model. Its copy in `out_dir` is `<file>.<tgt_lang>`, or with (2), (3)
  when that name is another file's; called again, it answers the copy already there.
- **Work in the studio** - when the result needs looking at and fixing: the translation
  line by line, the speakers, the voices and the characters, the look of the subtitles,
  titles, blur, several languages: `project_create`, `project_analyze`, the edits and
  `project_render`, as the recipes below go.
- A one-call project is an ordinary project: its `project_id` goes to `project_get`, the
  edits and `project_render` like any other. Called again after its settings were changed
  in the studio (mode, languages, subtitles), a one-call tool analyzes it anew as it asks.

Take one call when the user asks for a result; take the studio when they want to see,
choose or correct. They combine: `transcribe_file`, then `project_get` with its
`project_id`, `segment_update` where a line is wrong, and `export_subtitles`.

## Recipes

**Dub a video**

1. `studio_status`; if models are missing, `models_download` and `studio_wait`.
2. `project_create` with the video's `path`; keep its `project_id`.
3. `project_analyze` with `tgt_lang` (a code of `studio://languages`) and `mode: dub`
   (`voiceover` for a voice-over, `nodub` for subtitles only, `transcribe` for a
   transcript); `studio_wait` with its `job_id`.
4. `project_get`: read the translation line by line and fix it with `segment_update`.
5. `project_render`; `studio_wait` with its `job_id`.
6. `project_frame` at a moment with speech to see the burned-in subtitles.
7. `project_save_output` into the folder the user wants; tell them the path.

**Dub a video from a link**

1. `url_tool_status`: the component `ytdlp` must be installed (`models_download` with
   `ids: ["ytdlp"]` otherwise).
2. `url_probe` with the `url`: the title, the length, the qualities and the site's
   subtitles made by people (`subtitles`, apart from the languages of the automatic ones in
   `auto_subtitles`); the preview is its link, `thumbnail`. A refusal names its
   code and a `hint`: a proxy for a geo block, `cookies` (the path of a cookies.txt) for an
   age check, a members' video or a bot check.
3. `project_create_from_url` with `url`, `quality` and, to take the site's subtitles as the
   project's imported ones, `subs_lang`. It downloads in the background beside the jobs:
   `studio_wait` with its `fetch.id` as `job_id`; the result names the `project_id`.
   `url_fetch_cancel` stops it, `url_fetch_resume` continues an interrupted or failed one.
4. Go on as in *Dub a video* from `project_analyze`. The user answers for their right to
   the content.

**Fix the translation and render again**

1. `project_get` with `from`/`to` around the moment, or `ids`.
2. `segment_update` with `tgt_text` (and `start`/`end` or `speaker` when they are wrong);
   `segments_hide` for a line that should be neither voiced nor subtitled,
   `segments_keep_original` where the original voice should stay.
3. `project_dub_audio` to hear it quickly, or `project_render` for the video; either voices
   only the dirty lines. `studio_wait`, then `project_frame` to check.

**Make the translation fit the timing**

1. `project_get`: every voiced line of a dub or voice-over has `fit` - `verdict` `fits`
   (spoken at its own pace), `tight` (the render speeds it up within `eff_cap`: up to 4x
   with `speech_rate_on`, else the natural `cap`) or `impossible`, `over` when it does not
   fit, and after a render `rendered.needed` against `rendered.eff_cap` for the text the
   render voiced. `calibrated: true` once three clips of that voice were
   measured; before that the language's usual pace is used.
2. `segment_shorten` with the `ids` of the lines, or `all_over: true`; `studio_wait`. The
   translation model rewrites each shorter within the slot's character limit; the result
   lists `shortened`, `rejected` (other script, not shorter, echo of the source).
3. `project_dub_audio` voices the new text. With `settings_set` `auto_shorten` `"1"` (the
   default) a render does this by itself once for lines that did not fit.

**Pick the better take of a line**

1. `takes_list` with the line `id`: the last five voicings with their text, duration and
   QC similarity; `active` is what the mix plays.
2. `take_select` with `take` (its `n`) - a take of other text brings that text back - then
   `project_dub_audio` mixes again without voicing. While another take is pinned it is
   refused: `take_pin` with `pinned: false` first.
3. `take_pin` with `pinned: true` keeps the active take through regenerations and QC;
   editing the line's text unpins it.

**Only subtitles, or a transcript**

1. `project_analyze` with `mode: nodub` (translated subtitles over the original audio) or
   `mode: transcribe` (the transcript in the original language).
2. `project_export_text` with `format: srt` (or `vtt`, `ass`, `txt`, `json`) and `dir` -
   `text: src` for the original words, `tgt` for the translation, `both` for bilingual
   srt, vtt or ass (the translation and the original as two lines of each subtitle,
   `order` says which is on top, the project's own order when left out); `name` goes with
   `dir` only (without `dir` the file lands in the project's folder as subtitles.srt,
   transcript.srt, bilingual.srt, the same .vtt and .ass, translation.txt or
   transcript.txt). `project_render` burns the subtitles in
   instead, `subtitles_burn_set` with `on: false` leaves the picture clean.
   `subtitles_content_set` chooses what they say apart from what is heard: `transcribe`
   (a dub with subtitles in the original language), `translate`, or `bilingual` (the
   translation with the original as a smaller second line, `order` and `secondary` style
   it); an mkv output also carries them as subtitle tracks with their languages.
3. Subtitles the user already has: `project_create` with `subtitles_path` (.srt, .ass,
   .ssa); `project_analyze` takes their text and timing instead of recognising speech, and
   `import_translated: true` when they are already in `tgt_lang`.

**The same video in several languages**

1. Finish and check the first language (the layout, the style, the titles and the blur
   carry over).
2. `project_export_lang` with `lang`, one language at a time: each answers a new
   `project_id` and a `job_id`; `studio_wait` with the job before the next.
3. `project_save_output` for each new project.

**Cast the characters**

1. `project_analyze` with `casting: true` (`content_type: real` for live action, `anime`
   for drawn characters; look at a frame with `project_frame source: original` if unsure).
2. `casting_get`: the characters with their speakers and lines; `casting_avatar` shows a
   face.
3. `casting_update` to name them, set their gender, a `dub_voice` from `voices_list`
   (null clones their own voice) and a `speech_note` for the translation's tone.
   `voice_slots_assign` deals library voices by gender instead; `voice_from_speaker` makes
   a library voice of a speaker.
4. `casting_library_save` keeps the cast; the next episode's `project_analyze` with
   `casting: true` and `casting_ref` (a slug of `casting_library_list`) applies it.

**Names and terms the same in every line and episode**

1. `glossary_get`: the project's glossary (`stale: true` - the translation was made with other term translations or keep marks; pronunciation, `asr_fix` and note do not count).
2. `glossary_extract` collects candidates from the text (a job: `studio_wait`, the entries are in its
   result); `glossary_set` with `merge: true` adds the ones to keep. An entry has `term` and either
   `translation` or `keep: true` (left as written); `pronunciation` changes only what the voice says,
   `asr_fix` lists how speech recognition misspells the term (analysis corrects it).
3. `project_retranslate` with the project's `tgt_lang` and `mode` translates again with the glossary.
4. `series_glossary_set` with `merge: true` and the entries keeps them in a saved casting; the next
   episode's `project_analyze` with that `casting_ref` adds them to its glossary. `series_glossary_get` reads it.

A line whose `tts_skip` is set in `project_get` is not voiced: nothing is left to say once sound tags
([music], (laughs), ♪…♪), speaker labels and markup are taken out; `tts_text` shows what the voice says
when it differs from the translation.

**A batch into one folder**

For each file: `project_create`, `project_analyze`, `studio_wait`, `project_render`,
`studio_wait`, `project_save_output` with the same `dir` and the file's own name - a name
already there gets (2), (3).

**The cloud instead of the graphics card**

`openrouter_set_key` stores a key (it is checked first; `openrouter_verify` only checks);
`openrouter_models` lists the models of a stage and `openrouter_voices` a speech model's
voices. `settings_set` switches the stages: `or_llm_on`/`or_llm` (translation),
`or_vision_on`/`or_vision`, `or_tts_on`/`or_tts_model`/`or_tts_voice`, `or_asr_on`/`or_asr`.
`engine_presets_get` and `engine_preset_apply` set everything for this computer's card or
for the cloud at once. The proxy has three modes: `system` (as in Windows), `custom` (its own
address, `kind` giving the scheme of an address without one) and `off`; `proxy_test` checks a mode
and address before `proxy_settings_set` stores them (`proxy_settings_get` shows them, the password hidden).

## Working in the window, in front of the user

The studio's window is the second way in: `editor_*` tools work in its editor exactly as
the user's clicks do - the list scrolls to the line, the line lights up, the frame and the
timeline change - and every step is shown to the user as "Agent: ..." at the top of the
window. They need the window open (the desktop app, or the studio's address in a browser);
without it they answer that the window is not open, and everything else still works.

- **Start**: `editor_open` with the project's `pid` (a transcript opens its transcript view,
  where only `editor_state`, `editor_seek`, `editor_select`, `editor_play`, `editor_pause`
  and `editor_mode` work). `editor_state` tells what the editor shows: the playhead, the
  line under it, the selection, undo and redo, the export's status.
- **Show**: `editor_select` a line (or a blur box, a title) - the lane switches, the
  playhead moves there and it is highlighted; `editor_seek` moves the playhead and waits for
  the frame; `editor_play` plays one line or from a moment, `editor_pause` stops;
  `editor_frame` is the frame the user sees now; `editor_lane` switches the left lane.
- **Edit**: `editor_segment_update` (text, timing, speaker, hidden, original voice),
  `editor_segment_add`, `editor_segments_delete`; the montage: `editor_segment_split` cuts a
  line at a moment (the playhead by default), `editor_segments_merge` joins neighbouring
  lines, `editor_segment_move` moves a line along the timeline keeping its length. The look:
  `editor_mode`, `editor_style`, `editor_preset`, `editor_blur_add`, `editor_blur_update`,
  `editor_title_add`, `editor_title_update`, `editor_subtitles_content` (the subtitle languages,
  both with the original's line styled). Each is one step of the window's undo:
  `editor_undo`, `editor_redo`.
- **Fit and takes**: `editor_shorten` shortens one line or all that do not fit, as the line's
  and the list's buttons do (it answers at once; `studio_wait until: shorten`); `editor_takes`
  unfolds a line's takes under it, `editor_take_select` picks one (the window mixes again:
  `studio_wait until: dub_audio`), `editor_take_pin` pins it.
- **Glossary**: `editor_glossary` opens or closes its window, `editor_glossary_set` changes it
  there as editing and Save do, `editor_glossary_extract` collects candidates there for the
  user to confirm (`studio_wait until: glossary`; keep the chosen ones with
  `editor_glossary_set` and `merge: true`). They refuse while the window holds the user's
  unsaved edits.
- **Export**: `editor_export` starts the render as the Export button does; the user watches
  it in the Files panel, and Explorer shows the file when it is done. `editor_state` shows
  its status; `studio_wait until: render` waits for it.
- **Anything else on screen**: `ui_read_page` lists the visible controls with refs (a line
  of the transcript, a blur box, a title and a recent project each read as one line with its
  controls and fields; a key or password field reads only as filled or empty, and is masked
  in `ui_screenshot` too), `ui_click`, `ui_type`, `ui_select`, `ui_press_key` (Space plays,
  Ctrl+Z undoes, Ctrl+K opens the command palette), `ui_scroll`; `ui_screenshot` is a picture
  of the whole window; `ui_navigate`, `ui_open_settings`, `ui_open_help`; `ui_notify` tells
  the user something; `ui_console` shows the page's errors. The control you click is
  highlighted for the user.
- **Confirmations are the page's own**: deleting a project or a saved cast asks in a dialog
  of the page; `ui_read_page` lists it first. Click its confirm button only when the user
  asked for the deletion.

**Show the user how to fix a translation**

1. `editor_open` with the pid; `ui_notify` what you are going to do.
2. `project_transcript` with `text: tgt` (or `editor_state`) to find the line.
3. `editor_select` it - the user sees the line and the frame; `editor_play` it.
4. `editor_segment_update` with the new `tgt_text`; `editor_frame` to see the subtitle.
5. `project_dub_audio` to voice the dirty lines (the window shows the progress), then
   `editor_play` the line again.

**Cut and join lines on the timeline**

1. `editor_select` the line; `editor_seek` to the pause where it should be cut.
2. `editor_segment_split` (the playhead is the moment; `tgt_text` and `tgt_text_2` give the
   halves' translations), then `editor_segment_update` each half's speaker when two people
   speak.
3. `editor_segments_merge` for lines broken in the middle of a sentence;
   `editor_segment_move` with `shift` for a line that starts too early or too late.
4. `editor_play` the stretch; `editor_undo` if it sounds wrong.

**Export while the user watches**

1. `editor_state`: the project is open and nothing renders.
2. `editor_export`; `ui_notify` that the render started.
3. `studio_wait until: render`; `editor_state` shows the export done; `ui_notify` the
   result, or `project_files` for the path.

## Tools by area

- **studio**: `studio_status`, `studio_wait`, `studio_system` (card, video memory, RAM),
  `studio_capabilities`.
- **jobs**: `jobs_list`, `job_get`, `job_cancel`.
- **models**: `models_status`, `models_download` (runs in the background beside the jobs: wait with
  `studio_wait` `until: download`), `models_cancel_download` (a pause: the same ids continue it),
  `models_remove` (frees disk space), `models_import` (files already on disk), `models_select`
  (a quantisation or a recogniser), `models_folder_open` (the folder in Explorer, for the user).
- **settings**: `settings_get`, `settings_set`, `engine_presets_get`,
  `engine_preset_apply`, `proxy_test`, `proxy_settings_get`, `proxy_settings_set`,
  `fonts_list`, `caption_presets_list`, `launch_defaults_get`, `launch_defaults_set` (what the start
  screen's form opens with), `studio_paths` (where the data, projects and models are).
- **openrouter**: `openrouter_status`, `openrouter_set_key`, `openrouter_delete_key`,
  `openrouter_verify`, `openrouter_models`, `openrouter_voices`, `openrouter_catalog`,
  `openrouter_catalog_refresh`.
- **local server** (Ollama, LM Studio, vLLM, llama-server for translation and vision):
  `local_server_models`, `local_server_key_status`, `local_server_key_set`, `local_server_key_delete`;
  the provider of each stage is `settings_set` `llm_provider` / `vision_provider` (local, server, openrouter).
- **project**: `projects_list` (query, since, until), `project_create`, `project_get`,
  `project_transcript` (the transcript without the window),
  `project_analyze`, `project_resume`, `project_retranslate`, `project_remix`,
  `project_align`, `project_dub_audio`, `project_render`, `project_export_lang`,
  `project_put`, `project_patch` (any edit by its op), `project_delete`,
  `project_waveform`, `project_frame`.
- **videos by link** (yt-dlp): `url_probe`, `project_create_from_url` (in the background, wait
  with `studio_wait`), `url_fetches_list`, `url_fetch_get`, `url_fetch_cancel`,
  `url_fetch_resume`, `url_fetch_delete`, `url_tool_status`, `url_tool_update` (a newer
  yt-dlp, checked against its release's SHA-256; it is checked once a day anyway).
- **one call on a file**: `transcribe_file`, `translate_file`, `dub_file`, `separate_file`,
  `detect_text_file`, and `export_subtitles` for a project's subtitles.
- **files**: `project_files`, `project_export_text` (SRT, VTT, ASS, TXT, JSON; bilingual),
  `project_save_output`, `project_open_output`, `project_reveal`, `project_save_text` (a text file
  into the project's folder, e.g. the glossary as TSV).
- **lines**: `segment_update`, `segment_add`, `segments_delete`, `segments_hide`,
  `segments_keep_original`, `segments_reorder`, `segment_regen`, `segments_regen_all`,
  `segment_split` (cut a line in two), `segments_merge` (join neighbours),
  `segment_shorten` (fit the translation to the slot).
- **takes**: `takes_list`, `take_select`, `take_pin`.
- **what the project makes**: `project_mode_set`, `audio_output_set`,
  `subtitles_content_set`, `subtitles_burn_set`, `subtitles_position_set`,
  `translation_target_set`, `translation_style_set`, `rewrite_set`, `voice_set`,
  `gain_set`, `loudness_set` (even out loudness, on by default), `voiceover_gain_set`, `original_track_set`.
- **subtitles, titles, blur**: `caption_style_set` (all lines or one), `caption_preset_set`,
  `title_add`, `title_update`, `titles_delete`, `blur_add`, `blur_update`, `blurs_delete`,
  `blur_enable`.
- **casting**: `casting_get`, `casting_update`, `casting_avatar`, `casting_library_list`,
  `casting_library_save`, `casting_library_delete`, `casting_library_avatar`.
- **glossary**: `glossary_get`, `glossary_set`, `glossary_extract`, `series_glossary_get`,
  `series_glossary_set`.
- **voices**: `voices_list`, `voices_catalog`, `voice_download`, `voices_download_pack`,
  `voice_rename`, `voice_delete`, `voice_from_speaker`, `voice_slots_assign`; a voice from the
  user's microphone: `voice_record_devices`, `voice_record_start`, `voice_record_level`,
  `voice_record_stop`.
- **the window**: `ui_screenshot`, `ui_read_page`, `ui_click`, `ui_type`, `ui_select`,
  `ui_press_key`, `ui_scroll`, `ui_navigate`, `ui_open_settings`, `ui_open_help`,
  `ui_notify`, `ui_console`.
- **the editor, in front of the user**: `editor_open`, `editor_state`, `editor_frame`,
  `editor_seek`, `editor_select`, `editor_play`, `editor_pause`, `editor_lane`,
  `editor_segment_update`, `editor_segment_add`, `editor_segments_delete`,
  `editor_segment_split`, `editor_segments_merge`, `editor_segment_move`, `editor_mode`,
  `editor_style`, `editor_preset`, `editor_blur_add`, `editor_blur_update`,
  `editor_title_add`, `editor_title_update`, `editor_undo`, `editor_redo`, `editor_export`,
  `editor_subtitles_content`, `editor_shorten`, `editor_takes`, `editor_take_select`,
  `editor_take_pin`, `editor_glossary`, `editor_glossary_set`, `editor_glossary_extract`.
