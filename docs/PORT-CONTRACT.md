# Dub Studio — карта REST/SSE контракта (порт Python → Rust)

Источник истины: `backend/app.py` (FastAPI, один GPU-воркер + asyncio-очередь). Rust-сервер
(`crates/dub-server`, axum) обязан отдавать **тот же** контракт, чтобы SPA (`frontend/dist`) работал
без правок. Формат ответов совпадает; порт Rust-сервера — 8793 (`DUB_STUDIO_PORT`), у питона был 8765.

Легенда статуса: **done** — реализовано в раунде 1; **todo** — каркас/следующие раунды (нужен движок).

## Инфраструктура запросов

- Один GPU-воркер, одна очередь джоб. Тяжёлые (GPU) операции ставятся в очередь и возвращают
  `{"job_id": "..."}`; прогресс — по SSE `GET /jobs/{id}/events`. Синхронные (не-GPU) правки Project
  отвечают сразу телом Project.
- Снапшот `OPTS` берётся в момент постановки джобы (иммунитет к конкурентному `PATCH /engine/opts`).
- В Rust: `crates/dub-server/src/jobs.rs` — единый воркер (`spawn_blocking`), broadcast-канал на SSE,
  oneshot для синхронного ожидания (preview/original). У джобы есть вид (analyze|retranslate|remix|
  dub_audio|render|export_lang|download|voices_pack|frame|align|separate|detect_text|shorten|glossary), проект, время и последнее событие.
  Завершённые джобы живут в истории 10 мин (не больше 50), читать результат можно сколько угодно раз.
- Джобы проекта (analyze/retranslate/remix/dub_audio/render/export_lang) пишут `workspace/<pid>/job.json`
  ДО постановки: {kind, args, state queued|running|failed|interrupted|done|cancelled, stage,
  error{code,text}, job_id, resumes, started_at, updated_at}. При старте сервиса running/queued →
  interrupted.
- Повторная постановка джобы того же класса по тому же проекту, пока первая не завершена → 409
  `{"error":"job_conflict","job_id":..,"kind":..}`. Классы: text = analyze/retranslate/remix,
  audio = dub_audio/render/export_lang.
- Автор и ревизия (mcp/window.rs::track, слой на весь `api`): запрос окна несёт `x-dub-window: <метка окна>`,
  вызов MCP-инструмента — `x-dub-agent`; остальное — `api`. Каждая запись project.json (`save_project_atomic`)
  поднимает ревизию проекта и оповещает окна `{changed:"project", pid, rev, by, job}`; запись джобы приписывается
  автору запроса, поставившего её в очередь (`job: true`; `JobQueue::enqueue` оборачивает каждую джобу в `mcp::carry_job`), а не тому, кто
  поставил следующую джобу проекта. Ответы, которые и есть проект (GET/PATCH/PUT
  `/projects/{pid}`), несут `x-project-rev`. PUT `/projects/{pid}` несёт в
  `x-project-rev` ревизию, на которой снят записываемый снимок (undo/redo окна; без своей — последнюю известную окну);
  если после неё проект сохранял кто-то, кроме автора PUT (агент, API, другое окно, джоба не этого окна), или такой
  ревизии сервер не достигал (рестарт) → 409 `{error:"project_changed", detail}`: undo окна не затирает правки агента
  или другого окна, даже когда своя более поздняя правка окна уже принесла ему их. Создание/удаление проектов, настройки, голоса, кастинг и старт джоб (`{changed:"jobs", job_id,
  kind, pid}`) тоже оповещают окна.

## SSE — формат событий (`GET /jobs/{id}/events`)  — **done**

```
data: {"type":"queued", "position": n}
data: {"type":"running"}
data: {"type":"progress", "stage": "...", "msg": "...", "resumed": true?, ...}
data: {"type":"done", "result": <json|null>}
data: {"type":"error", "error": "..."}
data: {"type":"cancelled"}
```
Первым приходит терминал (если джоба уже завершена) или последнее известное событие. Поток завершается
на `done`/`error`/`cancelled`, keep-alive комментариями. 404 если job_id неизвестен или вытеснен из
истории. `resumed:true` — стадия/сегменты взяты из кэша прошлого прогона.

## Джобы: статус, отмена, продолжение — **done**

| Метод | Путь | Назначение |
|-------|------|-----------|
| GET | `/jobs?pid=` | {jobs:[снапшот...], project_job: job.json|null}: активные и недавние (без frame), новые сверху; с pid — только проекта |
| GET | `/jobs/{id}?wait=N` | Снапшот {id, kind, pid, status queued|running|done|error|cancelled, stage, msg, pct, position?, created_at, started_at, finished_at, result|error}; wait (сек, ≤55) — long-poll до завершения |
| POST | `/jobs/{id}/cancel` | В очереди → cancelled сразу; выполняется → {"status":"cancelling"}: кооперативная отмена между стадиями/сегментами, engine.cancel() Higgs, kill учтённых дочерних процессов (ffmpeg, BSRoformer, llama-server, whisper); завершённая → 409 |
| POST | `/projects/{pid}/resume` | Тот же kind с теми же args из job.json на ТОМ ЖЕ проекте → {job_id, kind, project_id}. Нет job.json → 404; done → 409 `{"error":"nothing_to_resume"}`; активная джоба → 409 `{"error":"job_conflict","job_id","kind"}` |
| DELETE | `/projects/{pid}` | При незавершённой джобе проекта → 409 `{"error":"project_busy","job_id","kind"}` |

Продолжение дешёвое: analyze пропускает стадии с совпавшим ключом параметров (diar.json, transcript.json,
translated.json, ocr.json, casting.json в cache.json), render синтезирует только сегменты без файла или
с изменившимся ключом синтеза (seg_ckpt.json, затем Segment.ckpt). Все артефакты пишутся tmp+rename.

## Эндпоинты

| Метод | Путь | Статус | Назначение |
|-------|------|--------|-----------|
| GET | `/health` | **done** | {status:"ok", app:"dub-studio", version, service_executable, repo_root, port}: чей сервис на порту; по нему оболочка переиспользует уже запущенную Dub Studio (service.rs) |
| GET | `/engine/capabilities` | **done** | JSON: device, tts_quant, asr_model, models{asr,llm,vision,tts}, ffmpeg(bool), languages[], voice_modes[] |
| PATCH | `/engine/opts` | **done** | Валидирует непустые строки (иначе 400), возвращает {models:{...}}. Рантайм-свап слота вне scope: OPTS иммутабелен (Arc<EngineOpts>), стек задаётся при старте — форма ответа соблюдена (endpoints.rs::set_opts) |
| — | | | **Раунд 2:** analyze покрывает ТОЛЬКО транскрипт-стадию (ASR+диаризация); перевод/vision/OCR/captions — раунд 3 |
| GET | `/fonts` | **done** | {fonts: family→описание} из captions.FONTS (порядок питона, endpoints.rs::fonts) |
| GET | `/voices` | **done** | {voices: []} — у порта нет именованного voice-пака (Higgs клонирует из ref-аудио), отдаём пусто как питон при отсутствии пака (endpoints.rs::voices) |
| GET | `/presets` | **done** | {presets: TEMPLATES{reveal,plate,font,base,plate_c?,accent?}, reveals: [...]} (fresh-семейство без plate_c, endpoints.rs::presets) |
| POST | `/projects` | **done** | multipart-загрузка видео → `workspace/<pid>/source.<ext>` + `source.txt`; вернуть {project_id, filename} |
| GET | `/projects` | **done** | Сводка проектов для «Недавних» + job_kind/job_state/job_stage/job_error из job.json |
| POST | `/projects/from-path` | **done** | {path, key?, tool?} — проект из файла на диске БЕЗ копии и без чтения в память: полный путь, существующий файл с расширением видео/аудио окна, ffprobe читает (иначе 400 с причиной); `source.txt` = путь, начальный project.json, `agent.json` {source: agent, project_id, tool, key, file: {path, identity, size, modified}, created_at}. С key — найти проект агента того же файла (путь, размер, время изменения) с тем же key и вернуть его. Вернуть {project_id, reused, filename} (atomic.rs::from_path) |
| POST | `/projects/{pid}/analyze` | **part** | Query: tgt_lang, mode, src_lang, subs, rewrite, speaker_count (целое 0–8; 0 — автоматически, 1 — один голос без диаризации, 2–8 — ожидаемое число участников на всю запись с сопоставлением голосов через WeSpeaker между часовыми фрагментами). Неверное число → 400 до постановки задания. Ошибка голосового сопоставления при заданном числе → явная ошибка задания; автоматическая подмена запрещена. Найдено меньше голосов — прогресс сообщает фактическое число. speaker_count и отпечаток WeSpeaker входят в ключ кэша диаризации; project.meta.speaker_count сохраняет выбор. Настройки стартового флоу, которые сервер кладёт на проект в конце анализа (и повторяет при resume): vo_gain (дБ, op voiceover_gain), sub_blur (0/1, op sub_blur), keep_original=1 + container mp4/mkv (op keep_original), voice_slots (JSON {male:[…], female:[…]}, как POST /voice-slots); кривое значение → 400. Результат джобы: {project_id, output, post:{voice_slots?: {assigned, speakers} \| {error:"missing_voices", names} \| {error:"no_vocals"}}}. Джоба: analyze()→project.json. Вернуть {job_id}. **Раунд 2: ASR+диаризация (транскрипт-стадия): ffmpeg extract 16k mono → Nemotron 3 Diarization turns (при <2 спикеров штатная single-speaker ветка) → TDT int8 словные таймстемпы → Project (src_text, words в extra, speaker, mode/subs-дефолты). Раунд 3: стадии translate+vision через сайдкар Gemma (llama-server + mmproj) — ctx-проход vision layout/scene + audio-контекст + перевод всего транскрипта → tgt_text, captions.titles(+tgt)/sub_style/sub_y/brands, raw_ctx. Перевод нужен, но не выполнен (LLM недоступен, сбой, ≥50 % строк остались на исходном языке) — джоба падает с причиной. OCR/captions/render — раунд 4.** SSE-фазы: probe/extract_audio/diarize/asr/vision/translate |
| POST | `/projects/{pid}/remix` | **done** | Query: instruction. Джоба: Gemma (flat_rewrite dub-translate) переписывает весь транскрипт, все dirty, audio.rewrite=instr. Вернуть {job_id} (endpoints.rs::remix_project) |
| GET | `/projects/{pid}` | **done** | Тело Project (JSON) или 404/409. У озвучиваемых реплик дубляжа/закадра вычисленное `fit` {est, slot, ratio, verdict fits\|tight\|impossible, calibrated, cps, cap, eff_cap, over, rendered: {needed, cap, eff_cap, raw, slot, dur, over}\|null} — прогноз укладки (слот и кап — dub_core::fit, как у рендера; темп — медиана клипов голоса из dub_timing.json от 3 клипов, иначе таблица языка) и отчёт последнего рендера этого текста. Критерий: needed = длительность / слот; `cap` — штатный предел слота (max_stretch, на слоте < 1,5 с не ниже 1,3), `eff_cap` — предел, с которым рендер реально ускоряет (при speech_rate_on до 4,0, при отставании дубля > 0,6 с до 2,0, иначе cap); fits — est ≤ slot, tight — est ≤ slot · eff_cap (прогноз без отставания), impossible — больше; `rendered.over` = needed > eff_cap того прохода (та же граница у телеметрии рендера «выше капа», у all_over сокращения и у замкнутого цикла auto_shorten); `over` — rendered.over, без отчёта — verdict impossible; у реплик с историей дублей — `takes` {count, active, pinned}. Те же поля в ответах PATCH/PUT; PUT их отбрасывает (fitplan.rs) |
| PATCH | `/projects/{pid}` | **part** | Синхронная правка Project (без GPU), `{op: ...}` — см. таблицу ниже. **Раунд 2: segment/subpos/mode (атомарно tmp+rename). Раунд 3: translate (смена tgt_lang + subs=translate, funny→rewrite, все dirty), rewrite (инструкция ре-дубляжа, mode=dub, все dirty). Прочие op → 400.** |
| PUT | `/projects/{pid}` | **done** | Полная замена Project (undo/redo снапшот); валидирует тело как Project, сохраняет атомарно (endpoints.rs::put_project) |
| POST | `/projects/{pid}/shorten` | **done** | {ids: [..]} \| {all_over: true} → джоба shorten (класс text): LLM перевода (llm_provider, temperature 0.15) переписывает каждую реплику короче — исходник, текущий перевод, по две соседние реплики, лимит floor(слот · темп · 0.95), не длиннее самой строки; ответ принимается, если он письмом целевого языка, строго короче и не эхо исходника. Принятое — новый tgt_text (dirty, extra.shortened {from, to}) только в реплики, чей текст не меняли за время джобы; закрепление дубля с прежним текстом снимается. Результат {shortened: [{id, from, to}], rejected: [{id, reason}], failed, unpinned}. Нет ids и all_over → 400, незнакомые id → 404, проект не дубляж/закадр → 409 `{"error":"not_dubbed"}` (shorten.rs) |
| GET | `/projects/{pid}/segments/{id}/takes` | **done** | История дублей реплики (до 5): {id, active, pinned, takes: [{n, text, text_matches, dur, qc, source synth\|multitake\|qc\|shorten, voice, reference, params, created, file}]}, новые сверху (takes.rs) |
| GET | `/projects/{pid}/segments/{id}/takes/{n}/audio` | **done** | WAV дубля (Range) для прослушивания |
| GET | `/projects/{pid}/waveform?n=600` | **done** | Пики аудио (ffmpeg → s16le 8kHz, np.array_split-эквивалент), кэш waveform.json, CPU вне GPU-воркера (endpoints.rs::waveform + wavio::waveform_peaks) |
| GET | `/projects/{pid}/preview?t=&rev=` | **done** | Джоба preview_frame → PNG через GPU-воркер (build_ass из ТЕКУЩЕГО Project + burn_frame). Синхронное ожидание (timeout 300с), abandoned при таймауте (endpoints.rs::preview + frame.rs::preview_frame) |
| POST | `/projects/{pid}/render` | **done** | Джоба render()→output.mp4 (раунд 4): probe→extract 44.1k→separate(dub-sep)→Higgs clone TTS per-seg (кэш seg_XXX.wav, ре-TTS ТОЛЬКО dirty)→fit_to_slot(atempo)→timeline→mix(instr+dub)→build ASS(dub-captions)→burn(blur из blur_boxes)→mux. regen_dub если dirty; после — сбросить dirty. Синтез сегмента — только если нет seg-файла или ключ синтеза (текст/спикер/слот/голос/реф/движок/нонс regen) не совпал с seg_ckpt.json. SSE-фазы probe/extract_audio/separate/tts/mix/build/burn/mux. Вернуть {job_id} |
| GET | `/projects/{pid}/output?dl=` | **done** | Отдать output.mp4 (Range через tower-http ServeFile → 206); dl=1 → Content-Disposition attachment |
| GET | `/projects/{pid}/original?t=` | **done** | ОДИН PNG-кадр оригинала на t (порт app.py.original → source_frame; ComparePane вставляет как `<img src>`). Джоба source_frame → PNG, timeout 60с. **ИСПРАВЛЕНО (раунд 5): раньше отдавал Range-видео — это ломало ComparePane (broken img). Сырое видео для плеера — /dub.** (endpoints.rs::original_frame) |
| GET | `/projects/{pid}/dub` | **done** | Проигрываемое видео: output.mp4, иначе analyzed.mp4 (Range) |
| GET | `/projects/{pid}/files` | **done** | Пути файлов проекта на диске: папка, исходник, output (mkv раньше mp4) и проигрываемый output, dub_audio.m4a, project.json, casting.json, голос и фон сепарации (vocals, background — stems/vocals.wav, stems/instrumental.wav), записанные SRT/VTT/ASS/TXT и JSON-строки transcript.lines.json/translation.lines.json (стадии analyze — diar.json, transcript.json, translated.json, ocr.json — не в списке); null — ещё не сделан (project_files.rs::files) |
| POST | `/projects/{pid}/export-text` | **done** | {format: srt\|vtt\|ass\|txt\|json, text?: tgt\|src\|both, order?: translation_top\|original_top, dir?, name?, speaker_label?, content?} — строки проекта файлом, как кнопки окна (перевод: строки как вжигание — без скрытых и оставленных оригиналом, свой текст субтитра вместо перевода, tgt иначе src; транскрипт: строки с src; both — двуязычные SRT/VTT/ASS: перевод и оригинал двумя строками одного субтитра в порядке order, без order — порядок проекта subs.bilingual.order; для txt и json — 400). vtt — WebVTT, при >1 спикере голосовые теги `<v Speaker N>`; json — [{id, start, end, speaker, text, original? (исходник под переводом), words? (словные тайминги транскрипта)}]; ass — стиль вжигания build_ass (титры — только у перевода; у транскрипта исходный текст без титров), аудио-проект — 400. Без dir — в папку проекта под фиксированным именем вида (subtitles.srt/.vtt/.ass, transcript.srt/.vtt/.ass/.txt, bilingual.srt/.vtt/.ass, translation.txt, transcript.lines.json, translation.lines.json — ни одно не совпадает с файлами стадий analyze и прочими служебными) с перезаписью, своё name без dir — 400 (служебные source.txt, name.txt, import_subs.* не перезаписать); в dir — занятое имя получает (2), (3); Проводник не открывает. Вернуть {ok, path, lines, content? (текст файла при content: true)} (project_files.rs::export_text) |
| POST | `/projects/{pid}/separate` | **done** | Голос и фон проекта (stems/vocals.wav, stems/instrumental.wav — кэш, общий с analyze/render): есть — {cached: true, vocals, background} без джобы; нет модели сепаратора — 409; иначе джоба вида separate (audio_hq.wav 44.1k → dub_sep::separate) с результатом {vocals, background} → {job_id}; незавершённая separate проекта → 409 `{"error":"job_conflict","job_id","kind"}`; джоба, поставленная второй после конца первой, отдаёт её stems без пересчёта (atomic.rs::separate) |
| POST | `/projects/{pid}/detect-text` | **done** | Вшитый текст кадра с параметрами детекции analyze (caption_fps, min_dur .3, iou .3, pad 8, jitter 20, score .4): text_regions.json есть — {cached: true, ...}; аудио-проект или нет моделей OCR — 409; иначе джоба вида detect_text → {job_id}; незавершённая detect_text проекта → 409 job_conflict; поставленная второй после конца первой отдаёт прочитанное. Результат {file, width, height, fps, count, regions: [{text, x, y, w, h, t0, t1}]}, пишется tmp+rename в text_regions.json (atomic.rs::detect_text) |
| GET | `/projects/{pid}/glossary` | **done** | {entries, tgt_lang, casting_ref, stale}; `?format=tsv` — TSV (term, translation, keep, pronunciation). stale — перевод сделан с другими переводами терминов или отметками keep (glossary_api.rs::project_get) |
| PUT | `/projects/{pid}/glossary` | **done** | {entries \| tsv, merge?, lang?} → как GET. merge — влить в имеющиеся (присланное главнее; из TSV только его колонки); запись без lang с переводом или произношением берёт lang (нет — язык цели). Отказ проверки — 400 {error: glossary_over_limit\|glossary_empty_term\|glossary_field_too_long\|glossary_duplicate\|glossary_tsv_keep\|glossary_tsv_empty_term\|glossary_one_of\|glossary_nothing, detail, args}. Перевод не трогает. Джобы analyze, retranslate и export-lang перед записью проекта берут глоссарий с диска — правка во время джобы не откатывается (glossary_api.rs::project_put, on_disk) |
| POST | `/projects/{pid}/glossary/extract` | **done** | Кандидаты из текста проекта: джоба вида glossary → {job_id}; итог {entries} с source auto, уже записанные термины выкинуты; ничего не сохраняет. Нет project.json — 409 (glossary_api.rs::extract) |
| GET | `/casting/library/{slug}/glossary` | **done** | Глоссарий профиля сериала {slug, entries} или TSV; нет профиля — 404. Анализ с casting_ref кладёт его под записи проекта с source series; повторный анализ берёт такие записи из профиля заново (glossary_api.rs::series_get, for_analyze) |
| PUT | `/casting/library/{slug}/glossary` | **done** | {entries \| tsv, merge?, lang?} → {slug, entries}; отказы — как у PUT глоссария проекта (glossary_api.rs::series_put) |
| POST | `/mcp` | **done** | MCP-сервер (Streamable HTTP, stateless JSON-RPC): каждый tool зовёт маршрут этой таблицы внутри процесса; skill — docs/mcp-skill.md (mcp.rs::handle). Порядок в build_router как в YuE2: `mcp::install(api.clone())` до гарда Origin/Host, гард — снаружи `api` вместе с `/mcp` и `/mcp/status`; внутренние запросы инструментов несут `Host: 127.0.0.1` (mcp.rs::call_route_raw) и проходят гард, где бы он ни стоял |
| GET | `/mcp/status` | **done** | {agent_connected, agent_last_call, agent_seconds_ago, agent_calls, window_open} для раздела настроек «Агент (MCP)» (mcp.rs::status) |
| GET | `/mcp/window` | **done** | SSE окна: первое событие `{window: n}`, дальше команды `{id, window, command, args}` для окна, к которому пользователь повернулся последним, и оповещения `{changed, pid?, rev?, by, job?, job_id?, kind?, project_id?}` всем окнам (mcp/window.rs::window_events). Отставшее окно получает `{changed: "everything"}` |
| POST | `/mcp/window/result` | **done** | ответ окна на команду: `{id, result}` или `{id, error}` → 204; неизвестный id → 404 (window_result). Инструменты ui_* / editor_* ждут его 15–30 с; окна нет — сразу понятная ошибка инструмента |
| POST | `/mcp/window/focus` | **done** | `{window}` — окно, к которому повернулся пользователь, получает следующие команды (window_focus) |
| GET | `/projects` | **done** | {projects: [{pid, video, tgt_lang, mode, width, height, duration, segments, created, audio_only, mtime, done, source}]}, новые правки сверху. `source` — agent (проект из файла от инструмента агента, agent.json с project_id этого проекта) или window. `created` — рождение каталога проекта (сек. эпохи; null, если ФС его не хранит), `mtime` — последняя правка project.json (lib.rs::list_projects) |
| GET | `/settings/launch` | **done** | Дефолты запуска дубляжа (форма стартового экрана): {defaults: {audio, subs, burn, detect_text, src_lang, speaker_count (целое 0–8; 0 — автоматически), tgt_lang\|null, casting, casting_ref, content_type, vo_gain_db, tr_style, tr_style_custom, sub_blur, keep_orig, container, voice_src, voice_slots_m, voice_slots_f}, saved}. Файл `models/launch_defaults.json`; нет файла — встроенные дефолты и saved=false (studio_settings.rs) |
| PATCH | `/settings/launch` | **done** | Частичная правка тех же полей; незнакомое поле или неверное значение — 400 целиком, файл не меняется; запись атомарная. Ответ как у GET, saved=true |
| GET | `/app/paths` | **done** | {data_dir, projects_dir, models_dir} — где лежат данные студии (раздел «О программе») |
| — (fallback) | `/{spa_path}` | **done** | SPA: реальный статик-файл (с защитой от path-traversal), иначе index.html |

Окна об атомарных операциях агента пока не извещаются: канал «changed» для окон делает W3-bridge
(`mcp/window.rs::announce_route`, мидлвар `track`). При слиянии с ним: `("POST", ["projects", "from-path"])` →
`changed: "projects"` с `pid` = `project_id` из ответа (сегмент пути `from-path` за pid не берётся, `project_of` его
не пропускает); `"separate"` и `"detect-text"` — в `JOB_ROUTES` (kind из маршрута: separate, detect_text); их
ответ `{cached: true, ...}` без `job_id` штатный (готовое, ничего не поставлено) — не ошибка «answered without a job_id».

## PATCH `/projects/{pid}` — операции `op` (все синхронные, без GPU) — **todo**

Правки применяются к Project, затем `project.json` перезаписывается и возвращается Project. Ошибки:
неизвестная/битая правка → 400; неизвестный seg/idx → 404.

| op | поля | действие |
|----|------|---------|
| `caption` | seg_id, + поля стиля | edit_caption; TypeError/ValueError/KeyError → 400 |
| `segment` ✅ | id, tgt_text?, src_text?, voice?, hidden?, keep_original? | edit_segment; неизвестный id → 404 (раунд 2) |
| `del_segment` | id | удалить строку (уходит и субтитр, и дубляж) |
| `hide_segment` | id, hidden? | тоггл/установка скрытия строки |
| `del_segments` | ids[] | массовое удаление |
| `hide_segments` | ids[], hidden | массовое скрытие (явный флаг) |
| `del_titles` | idxs[] | массовое удаление титров (high→low index) |
| `del_blurs` | idxs[] | массовое удаление blur-боксов (high→low index) |
| `keep_segment` | id, keep? | тоггл «keep original audio» (без дубляжа/перевода) |
| `keep_segments` | ids[], keep | массовый keep-original |
| `blur` | idx, + поля | edit_blur; IndexError/KeyError → 404 |
| `blur_add` | x,y,w,h,t0?,t1? | add_blur |
| `blur_del` | idx | del_blur |
| `blur_enable` | on? | глобальный тоггл блюра (render.blur) |
| `preset` | name? | имя TEMPLATE-пресета (None/"match" = как оригинал); только re-burn |
| `title` | idx, + поля | edit_title |
| `title_del` | idx | del_title |
| `title_add` | text,x,y,w,h,t0?,t1?,italic?,font?,color? | add_title |
| `subpos` ✅ | sub_y | перетащить полосу субтитров; ставит sub_y_locked=true (раунд 2) |
| `mode` ✅ | value | set_mode (subtitles/dub/funny → nodub/dub/dub+rewrite); неизвестное → 400 (раунд 2) |
| `subs_content` ✅ | value?, order?, secondary? | язык субтитров отдельно от аудио: none \| transcribe (оригинал; в дубляже — src_text) \| translate \| bilingual (перевод + оригинал второй строкой); order translation_top\|original_top; secondary {size_pct 40..100, color #RRGGBB\|null, opacity 10..100\|null} → subs.bilingual; неверное → 400 без изменений |
| `translate` ✅ | lang?, mode? | translate: tgt_lang=lang, subs=translate, funny→rewrite; все dirty (раунд 3) |
| `rewrite` ✅ | instruction | rewrite: audio.rewrite=instruction, mode=dub; пустая→400; все dirty (раунд 3) |
| `recast` | voice_mode?, voice_name? | recast (сменить режим/голос дубляжа) |
| `regen` | id | пометить сегмент dirty → ре-TTS только его на /render |
| `regen_all` | — | пометить все dirty → ре-TTS всего дубляжа |
| `split_segment` ✅ | id, at, tgt_text?, tgt_text_2?, new_id? | разрезать фразу в момент at: слова ASR и исходный текст — по времени, перевод — из полей или в той же доле; вторая часть `<id>_2`; обе dirty; at не внутри фразы → 400, занятый new_id (в том числе с тем же именем файла фразы — буквы, цифры и `_`) → 409 |
| `merge_segments` ✅ | ids[] | склеить соседние по списку фразы: id, спикер и голос первой, время от раннего начала до позднего конца, тексты и слова подряд; не соседи → 400 |
| `take_select` | id, take | активный дубль реплики из истории (takes/<sid>/<n>.wav → seg-файл, его ключ → seg_ckpt.json): микс берёт его без нового синтеза; другой текст дубля возвращает и tgt_text, нонс regen и ckpt — как при его синтезе; нет дубля → 404; закреплён другой дубль → 409 (сначала take_pin pinned false) |
| `take_pin` | id, pinned | закрепить активный дубль (рендер не заменяет его ни перегенерацией — ни локальной, ни облачным пре-синтезом, — ни QC, ни сокращением, и не пишет его в историю новым дублем) или снять; активный дубль другого текста → 409. Правка текста реплики (op segment, сокращение) снимает закрепление, рендер — с записью в журнал |

## Защита от path-traversal (SPA)  — **done**

app.py контейнит отдачу web-root: `f.is_file() and (f == WEB_R or WEB_R in f.parents)` на
канонизированном пути — `..%2f`-сегменты не должны вычитывать произвольные файлы. В Rust
(`spa.rs::serve_spa`) то же: `canonicalize()` запрошенного пути обязан начинаться с канон. web-root,
иначе → index.html. pid дополнительно ограничен `[A-Za-z0-9]` до касания ФС.

## Что реализуют движки-крейты (для analyze/render/preview следующих раундов)

- `crates/dub-asr` — Parakeet-TDT-v3 (словные таймстемпы, `TimestampMode::Words`) + диаризация Nemotron 3 Diarization (Streaming Sortformer v3, до 8 спикеров).
  Порт `dubengine/asr.py` (`_segment`, `transcribe`, `transcribe_turns`) и `diarize.py` (turns/assign).
- `crates/audiocpp` — Higgs Audio v3 TTS/клон голоса (FFI над audiocpp_engine.dll). Порт `tts.py`/`voices.py`.
- `crates/dub-core` — типы Project (serde, extra="allow" round-trip) и EngineOpts. Порт `project.py`/`opts.py`.
- `crates/dub-sep` — вокал/инструментал сепарация. Порт `dubengine/separate.py`, но ДВИЖОК ЗАМЕНЁН
  приказом юзера (2026-07-11): вместо UVR-MDX (audio-separator) — **Mel-Band Roformer voc_fv6-Q8_0**
  через нативный сайдкар **BSRoformer.cpp** (C++/ggml, CUDA). CLI отдаёт ТОЛЬКО вокал-стем (num_stems=1);
  инструментал = `mix − vocals` во временной области (выход выровнен по семплам, реконструкция точная).
  Движок — `tools/bsroformer/` (bs_roformer-cli.exe + 4 ggml-DLL, CUDA), модель — `models/bsroformer/`
  (в .gitignore, копируются с диска, не качаются). Вход движка 44.1кГц.
  **ВРЕЗКА (раунд 4):** analyze audio-context (Gemma «слышит» вокал) сейчас получает vocals-стем
  сепарации, а не полный микс (в раунде 3 подавался `vocals16` из полного микса — теперь перед
  извлечением 16k mono микс прогоняется через dub-sep). Рендер использует instrumental как фон.
- `crates/dub-captions` — CapCut-субтитры через ffmpeg+libass (ASS). Порт `captions.py`: build() (титры
  localized-in-place + дублированные субтитры, 26 пресетов/реверсов) + burn()/burn_frame() (gblur боксов
  + оверлей ASS, NVENC). Метрики глифов — ab_glyph (замена PIL). Шрифты в `fonts/`.
- `crates/dub-ocr` — экранный OCR (PP-OCR DBNet det + CRNN rec + cls, `models/ocr/`) для блюр-боксов
  вшитого текста. Порт `text_detect.py` (detect_regions: семплинг→det+(cls)+rec→merge→IoU-трекинг) +
  `compose.py` (analyze_layout: субтитр-полоса vs титры). Свой ort-пайплайн (rc.13 load-dynamic api-28,
  как dub-asr — один OrtApi; БЕЗ download-binaries, ndarray 0.17 — единый набор фич по воркспейсу).
  Детекция — полный DBPostProcess: connected-components → min-area-rect → box_score_fast → **unclip
  истинным edge-normal offset** (равномерно растит тонкие широкие боксы сабов по высоте; радиальный
  сдвиг от центроида резал глифы по высоте → rec шумел). Словарь rec — из **метадаты ONNX** (ключ
  `character`, как RapidOCR v3; blank префиксуется, токены КАК ЕСТЬ — лишний пробел сдвигал индексы
  CTC). Живой прогон example_original.mp4: rec читает вшитые сабы чисто («КОРОЧЕ ОН» 0.81, «ПОДКЛЮЧЕН»
  0.86–0.90, «У МЕНЯ CLAUDE» 0.90, «сигнал» 0.98, «получается» 0.94 — score>0.8).
  **spoken-гейт analyze_layout — питон-смысл, устойчивый к rec:** геометрия дословно питоновская
  (`nt>=3` distinct-строк в нижней полосе; A-шный доп. ratio `nt>=0.3*len` снят — питон убрал его как
  fps-хрупкий). Питоновский порог `spoken_frac>=0.5` структурно недостижим на СКЛЕЕННЫХ read'ах
  PP-OCR-CRNN (строка выходит без пробелов: «онотправляеТ») — проверено запуском самого питоновского
  `compose.analyze_layout` на наших raw: он тоже отдаёт 0 боксов (frac полосы ~0.11). Поэтому spoken
  служит РАЗЛИЧИТЕЛЕМ сцен-графики (снек-паки/вывески НЕ говорят сказанного → frac==0) от субтитр-полосы
  (говорит → frac>0); измерение усилено фолдингом гомоглифов (латиница↔кириллица, К↔K и т.п. — модель
  их путает) и матчем по подстроке. Полосу отбрасываем лишь если spoken есть И frac==0 — это верный
  питоновский смысл (блюрить только реальные субтитры). Живой analyze: sub_y=640, полоса сабов накрыта
  (fps=2 → 89 caption_boxes; fps=4 → 184), сцен-графика (cy≈464) исключена в localize.
- Рендер-ядро — `crates/dub-server/src/render.rs`: порт render-половины `pipeline.run` + `_build_dub`
  TTS-ветки + `assemble.py` + `compose.py`(mix)/`media.mix`. OCR-стадия analyze — `ocr.rs`.
- Остаётся (раунд 5): preview?t&rev, waveform, undo/Cmd-K, remix, прочие PATCH (caption/blur*/title*/
  preset), портативная упаковка. (Rec-качество OCR доведено: edge-normal unclip + словарь-из-меты —
  вшитые сабы читаются чисто, score>0.8; поглощена ветка r4-ocr.)

## Раунд 5 — caption-композит pipeline.run:388-643 (parity-аудит закрыт)

Аудит (задача #26) нашёл, что гигантская «склейка» pipeline.run (388-643) в порту отсутствовала:
титры не рисовались (bbox=None -> emit_title скипал), утекал EN-оригинал, не было cover-plate,
band-коалесценции, per-segment y-райдинга, auto-nodub гейта. Порт живёт в
`crates/dub-server/src/compose.rs` (модуль `compose`), вызывается из `ocr.rs::stage` ПОСЛЕ
translate-стадии (raw_ctx готов) и OCR-детекции (localize/caption_boxes готовы) — точное место в питоне.

| # | Расхождение | Статус | Где | Доказательство |
|---|-------------|--------|-----|----------------|
| 1 | bbox титров (y_frac -> OCR-box match) | **done** | compose.rs `run` (порт 497-543) | юнит `ctitle_bbox_matches_localize_boxes` + `_fallback_center_band` сверены со standalone-прогоном питон-блока; E2E: `verify_project.json` титр получил bbox (был None) |
| 2 | блюр титров/таглайнов/leftover/group/cap + band-коалесценция | **done** | compose.rs `run` (443-452, 579-589) + ocr.rs `stage` (416-442: `blur::straddles_center`+`blur::band_blur`, раньше мёртвый код) | E2E: title-регион в blur_boxes (EN не ликует) |
| 3 | sub_px (OCR-размер оригинала) | **done** | compose.rs (608-610) -> `raw_plan[sub_px]`; render build берёт | юнит `sub_px_median_on_band` (медиана высот на sub_y±7%vh) |
| 4 | sub_style mirror-from-captions + cap_px | **done** | compose.rs `ensure_sub_style_mirror` (466-484) | код-путь; fires при пустом sub_style + captions из raw_ctx |
| 5 | white-card fallback (scene_color/scene_flat) | **done** | compose.rs `white_card_fallback` (490-493) | юнит `white_card_activates_cover` |
| 6 | auto->nodub гейт (_has_speech) | **done** | analyze.rs `has_speech` + флип mode (66-78,124-129) | uniq>=0.35, coverage>=0.10; пустой транскрипт -> nodub |
| 7 | next.start слот по индексу i+1 полного списка | **done** | render.rs `build_dub` (питон 207) | seg-файл и слот по fi полного списка, не по времени |
| — | per-segment y-райдинг субтитр-полосы | **done (сосед 61b6158)** | render.rs `seg_y` по band-боксам blur_boxes | эквивалентный источник (band = покадровые caption_boxes) |

Таглайн-MT (перевод оставшихся надписей титр-карты, питон 564-578) в композите опционален и
**fail-safe**: поднимает text-only Gemma лишь при непустых tcard_rows; без весов/сбоя — регион всё
равно блюрится (EN не ликует), просто без перевода. Не-ctx ветка (ctx off / нет ctx_extra) не
портирует plain-MT title-fallback (loc_blocks там пусты) — на живом стеке ctx_translate ВКЛ, это
мёртвый путь; блюр (таглайны+group_blur+band) оригинал накрывает. Осознанное упрощение.

E2E-харнесс: `verify_captions_e2e` (пример `cargo run -p dub-server --example verify_captions`) —
берёт кэш project.json (transcript+raw_ctx), заново гонит OCR+compose+build_ass+burn БЕЗ ASR/Gemma/TTS,
даёт captioned.mp4 для покадрового сравнения порт-vs-эталон (docs/example_dub.mp4).

## Раунд 5 — ПОДЛОЖКА САБОВ ДЕФОЛТОМ (продуктовое отклонение, приказ юзера 2026-07-12)

**ОСОЗНАННОЕ ОТКЛОНЕНИЕ ОТ ТЕКУЩЕГО ПИТОНА** — прямой приказ юзера (2026-07-12). Продукт по
умолчанию рисует дублированные субтитры **на СПЛОШНОЙ ПЛАШКЕ** (визуально как эталонная версия
продукта `docs/example_dub.mp4` — текст-хаг плита за строками), а не питоновским тонким outline.

- **Где:** `crates/dub-captions/src/lib.rs::build_s_style`. Новый флаг `SubStyle.plate: Option<bool>`
  (+ `plate_color`), приходит из `extra` через `render.rs::map_sub_style` (PATCH `caption` его кладёт).
- **Поведение:** при `bg`=none/пусто И БЕЗ явной полосы vision И `plate`≠Some(false) → светлый текст
  (lum>0.45) рисуется веткой A (`BorderStyle=3, Outline=11`) на плашке `plate_color` (**дефолт чёрный
  `#000000`**, как эталон). Это ровно та ветка, которой отрисован `example_dub.mp4`.
- **Приоритеты:** явная полоса vision (bg solid, контраст ≥0.20) — как ветка A и раньше, ГЛАВНЕЕ
  продуктовой плашки. `outline_w`-оверрайд редактора — тоже главнее. Тёмный текст (lum≤0.45) сохраняет
  питоновскую белую плиту (тоже сплошную, читаемую). `plate=Some(false)` (тумблер PATCH `caption`/`preset`
  юзера) → fall-through к питоновской outline-ветке (`BorderStyle=1`) — **отключение доступно**.
- **Приёмка (задача #33):** рендер `example_original.mp4` (workspace/ae61067), кадр t=2.5 — за текстом
  сабов «MY CHATGPT SUBSCRIPTION.» СПЛОШНАЯ ЧЁРНАЯ плашка (S-style `,3,11,0,2,` + `&H00000000` плита),
  глазами как эталон. Кадры порт|эталон рядом. S-строка ASS верифицирована: `Style: S,Oswald,44,…,3,11,0,2,…`.
- **Регресс-якоря** (`dub-captions/src/lib.rs`): `greedy_example_original_default_black_plate` (дефолт →
  `3,11` чёрная плита), `plate_disabled_falls_back_to_border1` (`plate=false` → `1,2` outline),
  `textured_scene_has_default_plate_but_no_cover_kp`.

Старый вывод раунда 5 (ниже, «расхождений НЕТ») относился к паритету с captions.py на greedy-входе —
теперь СОЗНАТЕЛЬНО перекрыт этим продуктовым дефолтом по приказу юзера. Паритетная ветка сохранена и
доступна тумблером (`plate=false`).

## Раунд 5 — РЕГРЕСС ТИТРА починен (задача #34, приказ юзера 2026-07-12)

**Корень найден:** прогон `ae61067` отрисовал 0 титр-Dialogue НЕ из-за флаки-матчинга, а потому что
его `project.json` создан **00:59**, а compose-фикс (bbox-матчинг, f6fd476) закоммичен **01:51** — тот
артефакт СТАРШЕ фикса. Титр шёл из `raw_ctx` с `bbox=null` (`start=0/end=0`), `emit_title` его скипал
(ass.rs:322 — `bbox.len()<4 → return`). Текущий `compose::run` матчит `ctitles→localize` и ВСЕГДА даёт
валидный bbox: матч OCR-боксов ИЛИ **fallback центр-полоса** (`near_idx.is_empty()` → центр 8%..92%).
Пути к «bbox=null из run» нет — каждый loc_block имеет конкретный bbox.

- **Доказательство:** свежий прогон OCR+compose на том же `example_original.mp4` (verify_captions) даёт
  `композит: титров=1 (bbox)` и `KT`-Dialogue «THAT VERY "PROGRESSIVE" ACQUAINTANCE» в ASS. Стабильно
  на **3 прогонах подряд** (все: титров=1, блюр 33, sub_px=24 — детерминизм). Титр ВИДЕН на кадре t=2.5.
- **Регресс-тест:** `compose.rs::ae61067_title_always_drawn_with_bbox` — ТОЧНЫЙ титр ae61067 в ДВУХ
  входах (с localize-боксами и с ПУСТЫМ localize) → оба дают ровно 1 нарисованный титр с непустым tgt и
  `bbox.len()==4` (никогда 0). Гварда против «титр молча пропал».

## PATCH-хвост (раунд 5, задача #35) — все op портированы 1:1 с app.py

`caption` (глобальный sub_style ИЛИ per-seg override; тумблер `plate`), `del_segment`, `hide_segment`,
`del_segments`, `hide_segments`, `del_titles`, `del_blurs`, `keep_segment`, `keep_segments`, `blur`,
`blur_add`, `blur_del`, `blur_enable`, `preset`, `title`, `title_del`, `title_add` — `crates/dub-server/
src/patch.rs`. Коды ошибок как питон: битый op/поле → 400; неизвестный idx/seg → 404. Юнит-тесты на
caption(global+per-seg), del_segment(404), blur-цикл, title-цикл, preset, del_titles/del_blurs.

**CaptionOverride per-seg в build_ass:** `render.rs::build_ass` теперь читает `overrides[seg_id].text`
и рисует ЕГО вместо `tgt_text` (продуктовое требование «научить build_ass читать»). Стилевые per-seg
поля (`style/x/y/w/fs`) сохраняются round-trip, но в ASS-строку пока не вплетаются — это совпадает с
питоном (`write_artifacts` overrides в план НЕ прокидывает; dub-captions строит субтитр из общего
sub_style). Осознанное ограничение: текст-оверрайд (главный кейс редактора) работает.

## Раунд 5 — паритет плашек: семплинг vision + заливка полос (задача #28)

**Итог: расхождений НЕТ. Порт уже дословно совпадает с источником истины; «плашка эталона» —
артефакт УСТАРЕВШЕГО пайплайна.** Доказано живым прогоном Gemma + честным прогоном `captions.py`.
**ВНИМАНИЕ:** этот вывод перекрыт продуктовым дефолтом «подложка сабов» выше (приказ юзера 2026-07-12).

### 1. Семплинг LLM — сверен построчно, УЖЕ верный (не «temp 0.3 всюду» из отчёта р3)
Каждый вызов `dub-llm` шлёт РОВНО питоновские per-call параметры (проброшены через `Sampling`):

| Вызов | Питон (источник) | Порт | temp / top_p / top_k / rep_pen |
|-------|------------------|------|--------------------------------|
| ctx `ask`/`imsg` дефолт | ctx_translate.py:67-69 | vision.rs:125, ctx.rs:134 | 0.2 / 0.95 / 64 / — |
| **sub-style SP (GREEDY)** | ctx_translate.py:163 | vision.rs:242 | **0.0** / 0.95 / 64 / — |
| VP мега-промпт | ctx_translate.py:168 | vision.rs:253 | 0.2 / 0.95 / 64 / — |
| scene-контекст | ctx_translate.py:256 | vision.rs:404 | 0.2 / 0.95 / 64 / — |
| audio-контекст | ctx_translate.py:276 | ctx.rs:203 | 0.2 / 0.95 / 64 / — |
| ctx TRANSLATE (TP) | ctx_translate.py:302 | ctx.rs:134 | 0.2 / 0.95 / 64 / — |
| MT glossary `_chat` | translate.py:55-59 | translate.rs:109 | 0.2 / 0.9 / — / — |
| MT `_translate_one` fallback | translate.py:92 | translate.rs:74 | 0.7 / 0.6 / 20 / 1.05 |
| MT batch `_run_hunyuan` | translate.py:144 | translate.rs:176 | 0.3 / 0.9 / 20 / 1.05 |
| MT `rewrite` | translate.py:193 | translate.rs:248 | 0.85 / 0.95 / 40 / 1.05 |

### 2. Ветки плашек `captions.py` build() — дословный порт (`build_s_style`, lib.rs)
Заливка полосы управляется ИСКЛЮЧИТЕЛЬНО тем, что vision вернул в `sub_style.background`:
- `bg=solid hex`, контраст к тексту ≥0.20 → **BorderStyle=3, Outline=11**, плита цвета полосы
  (captions.py 507-510). Это ветка ЭТАЛОННОЙ чёрной полосы.
- `bg=none`, светлый текст (lum>0.45) → **BorderStyle=1, Outline≈2**, тонкий outline, БЕЗ плашки
  (captions.py 511-515).
- `bg=none`, тёмный текст → BorderStyle=3, Outline=10, почти-белая плита (516-518).
- «boxed»-preset с плашкой (519-521) — **МЁРТВЫЙ путь**: фолбэк `if not sub_style` (462-469) ВСЕГДА
  назначает `sub_style` (bg=none) до `if sub_style:` (488), поэтому preset-плита недостижима. Порт
  повторяет это точно (default_style → light-ветка BorderStyle=1). Проверено `sub_style=None`-прогоном
  питона: тоже BorderStyle=1,2.
`_emit_title.has_plate` (ass.rs:382) = `bg && bg!="none" && |lum(bg)-lum(txt)|>=0.20` — идентично 421.

### 3. Живой greedy-vision `example_original.mp4` (пример `vision_probe`, temp=0.0)
Все 10 кейфреймов дают согласованно: `background=""`(→none), `background_color=null`,
`scene_color=#E0E0E0/#D3D3D3`, `scene_flat=false`, титр `bg=null`. Т.е. Gemma ЧЕСТНО читает субтитр
оригинала как БЕЛЫЙ-С-ЧЁРНЫМ-OUTLINE поверх сцены — сплошной полосы В ОРИГИНАЛЕ НЕТ. Агрегация bg в
порту (vision.rs:284-286,351) дословно = питон (ctx_translate.py:197-199,228): majority-vote «none».

### 4. Приёмка пикселями (кадры 464×824, PIL Counter, доминанта зоны, квант 16)
- **Субтитр порт vs питон-источник-истины** (тот же greedy sub_style): S-строка **байт-в-байт**
  `Style: S,Oswald,66,…,1,2,0,2,…` (порт ae61067 caps.ass == pygreedy/greedy.ass). Зона субтитра
  x[120:350]y[615:665]: порт `#F0F0F0 31.1% / #000000 14.4%` ≈ питон `31.3% / 15.0%` — совпадает.
- **Эталон `example_dub.mp4`**: полосы ЕСТЬ (текст-хаг тёмные плиты) — субтитр inter-line x[180:280]
  доминанта `#000000` 23% (в оригинале там `#B06050` — сцена, плиты нет ⇒ эталон её РИСОВАЛ).
  НО эти плиты `#000000`-класс воспроизводит ТОЛЬКО ветка `bg=solid` (candidate A: py_build.py →
  A_solid_black), которая на текущем greedy-входе НЕ активируется ни в питоне, ни в порту.
- **Вывод**: эталон `example_dub.mp4` отрисован УСТАРЕВШИМ пайплайном (старый «boxed везде» до
  LLM-driven-плашек). Текущий источник истины (`ctx_translate.py`+`captions.py`), накормленный живым
  greedy-чтением этого видео, даёт outline-субтитры БЕЗ плашки — ровно как порт. **Приводить порт к
  «плашке эталона» = отклониться от источника истины** (запрещено контрактом). Порт оставлен как есть.

### Регресс-якоря (dub-captions/src/lib.rs)
- `greedy_example_original_substyle_is_border1_no_plate` — точный живой greedy sub_style → BorderStyle=1,
  без KP-плашки (паритет с captions.py на том же входе).
- `solid_band_reproduces_reference_black_plate` — `bg=#000000` → BorderStyle=3,11 чёрная плита
  (ветка, которой отрисован эталон; активна ТОЛЬКО при solid-чтении vision).
- Диагностика: `DUB_VISION_DEBUG=1` печатает per-keyframe raw sub-style read (vision.rs).

### ЗАКРЫТО (задача #34) — регресс титра «0 титр-Dialogue»
Причина: артефакт ae61067 СТАРШЕ compose-фикса (см. раздел «РЕГРЕСС ТИТРА починен» выше). Свежий
compose всегда даёт титру bbox (матч ИЛИ fallback центр-полоса); титр «THAT VERY…» ВИДЕН, стабильно
на 3 прогонах. Регресс-тест `ae61067_title_always_drawn_with_bbox` гвардит от рецидива.

## Доступ и секреты — **done**

- Весь роутер (API, SPA и всё, что появится) за middleware `guard::origin_guard`: заголовок `Origin`, если он
  есть, должен быть локальным (`localhost`, `127.0.0.1`, `[::1]`, `tauri.localhost` с любым портом или схема
  `tauri`), имя хоста в `Host` — одно из тех же (защита от DNS-rebinding); иначе 403. Агенты и curl без `Origin`
  с локальным `Host` проходят. CORS отвечает только локальным источникам (dev-фронт Vite, окно студии).
- Секреты не отдаёт ни одна ручка. `selection` в `GET /engine/capabilities` и в ответе `POST /engine/select` —
  без `or_key`, `proxy_url` без пароля, плюс флаги `or_key_set` и `proxy_password_set`. Слоты `or_key` и
  `proxy_url` через `POST /engine/select` не пишутся (400).
- Хранилище — `crates/dub-server/src/credentials.rs`: в портативной раскладке `<папка приложения>/secrets/`,
  иначе `%LOCALAPPDATA%\Dub Studio\secrets\`; `DUB_STUDIO_SECRETS_DIR` перекрывает оба. Переменная окружения
  `OPENROUTER_API_KEY` главнее сохранённого ключа. На старте сервера `or_key` и пароль из `proxy_url`
  переносятся из `models/active.json` в хранилище, active.json переписывается без них.
- Ошибки ручек ниже — JSON `{error: <код>, detail}`.

| Метод | Путь | Назначение |
|-------|------|-----------|
| GET | `/engine/openrouter/settings` | `{configured, source: environment\|local_store\|null, environment_variable}` |
| PUT | `/engine/openrouter/settings` | `{api_key}`: проверка ключа в OpenRouter (`GET /key`), затем сохранение; ответ как у GET. Коды: `empty_key`/`invalid_key` 400, `key_rejected` 400, `verify_failed` 502, `environment_key` 409, `store_failed` 500 |
| DELETE | `/engine/openrouter/settings` | Удалить сохранённый ключ; ответ как у GET. `environment_key` 409 |
| GET | `/engine/proxy/settings` | `{mode: system\|custom\|off, kind: http\|https\|socks5\|socks4, on, url, password_set, problem}` — адрес без пароля; `problem` — почему сохранённый свой адрес не читается |
| PUT | `/engine/proxy/settings` | `{mode?, kind?, url?, password?, on?}`: адрес в любой записи (`host:port:user:pass`, `user:pass@host:port`, `scheme://…`) приводится к URL со схемой по `kind`; `on` — прежняя форма (true = custom, false = off); `password` нет или `""` — оставить сохранённый, `null` — удалить, строка — заменить; пароль, вписанный в адрес, уходит в хранилище; `url: ""` — убрать адрес. Маршрут всех запросов перестраивается сразу. Коды: `proxy_password_without_user`, `invalid_proxy_url`, `invalid_proxy_mode`, `invalid_proxy_kind`, `proxy_url_required`, `invalid_proxy_password`, `invalid_proxy_on` 400, `store_failed` 500 |
| POST | `/engine/proxy/test` | `{mode?, kind?, url, password?}` (mode по умолчанию custom): HF и OpenRouter параллельно, 15 с, `{ok, hf, openrouter, hf_error, openrouter_error}`; адрес с логином без пароля проверяется с паролем из тела; сохранённый пароль подставляется, только если `url` совпадает с сохранённым адресом. Пароль хранится как есть и в адрес вставляется %-кодированным: `/ ? # @ :` в нём не меняют хост и порт |
| GET/PUT/DELETE | `/engine/server/key` | Ключ локального OpenAI-совместимого сервера. Ключ принадлежит адресу, для которого сохранён (без хвостовых `/` и `/v1`), и уходит только на него. GET `?url=` → `{configured}` для этого адреса (по умолчанию `srv_url`); PUT `{api_key, url?}` — сохранить для `url` (по умолчанию `srv_url`), `empty_key`/`invalid_key` 400; DELETE `?url=` — удалить для этого адреса (по умолчанию `srv_url`) |

## Провайдеры LLM, OpenRouter, прокси — **done**

- Перевод и vision выбираются независимо: слоты `llm_provider`/`vision_provider` = `local` (своя Gemma) \| `server`
  (локальный OpenAI-совместимый сервер: Ollama, LM Studio, vLLM, llama-server) \| `openrouter`. Пока слот не задан —
  прежние `or_llm_on`/`or_vision_on`. В `selection` ответа — всегда действующий провайдер. Сервер: `srv_url`
  (по умолчанию `http://127.0.0.1:11434`), `srv_llm`, `srv_vision`; ключ — `/engine/server/key`.
- OpenRouter — из Rust (`dub_llm::openrouter`, без сайдкара): чат, `/audio/speech` (pcm -> WAV 24 кГц моно),
  `/audio/transcriptions` (verbose_json), `GET /key`. Каталог — `GET /models?output_modalities=all`, кэш
  `models/openrouter-catalog.json`.
- Прокси — `dub_llm::net`: режимы как в Windows / свой / без прокси, маршрут спрашивается на каждый запрос;
  127.0.0.1, localhost, LAN и имена без точки — всегда напрямую.

| Метод | Путь | Назначение |
|-------|------|-----------|
| GET | `/engine/openrouter/models?kind=llm\|vision\|tts\|asr` | Модели каталога для стадии: `{models:[{id, name, context_length, pricing, input_modalities, voices}], refreshed_at}` (ключ не нужен) |
| GET | `/engine/openrouter/catalog` | `{refreshed_at, total, counts:{llm, vision, tts, asr}}` |
| POST | `/engine/openrouter/catalog/refresh` | Скачать каталог заново; ответ как у GET |
| POST | `/engine/openrouter/verify` | `{key}` -> `{ok:true, data}` \| `{ok:false, error}` без сохранения |
| GET | `/engine/server/models?url=` | Модели локального сервера (`GET <url>/v1/models` через сервер студии): `{models:[id]}`; ключ из `/engine/server/key` — только если он сохранён для этого `url`; 502 `{error: server_unreachable, detail}` |

## Видео по ссылке (yt-dlp) — **done**

- Инструмент — компонент `ytdlp` менеджера моделей (`setup.rs`): yt-dlp.exe закреплённой версии и deno (JS-рантайм,
  без которого yt-dlp не решает задачи YouTube). Раз в сутки при пробе или загрузке проверяется новый релиз yt-dlp:
  он ставится рядом (`tools/yt-dlp/update`), сверяется с SHA2-256SUMS своего релиза и отвечает на `--version`,
  только потом становится рабочим (`tools/yt-dlp/update.json`); провал оставляет прежний exe.
- Каждый вызов yt-dlp: `--ignore-config`, `--no-playlist`, свой deno (`--js-runtimes`), ffmpeg студии, `--proxy` —
  маршрут прокси студии для этой ссылки (пустой — напрямую), вывод в UTF-8.
- Загрузка идёт своим потоком мимо GPU-очереди джоб, состояние — `workspace/.fetch/fetches.json` (переживает
  перезапуск: идущая становится `interrupted`, «продолжить» докачивает `.part` из `workspace/.fetch/<id>`). Шаги:
  `-J` с выбором формата → `--load-info-json` с прогрессом (`--progress-template`) → субтитры площадки (`subs_lang`,
  только загруженные людьми, сведённые в SRT) отдельным `--load-info-json --skip-download --write-subs` после видео:
  их сбой — предупреждение `subs_failed`, а не провал загрузки → файл переезжает в `workspace/<pid>/source.<ext>`,
  имя проекта — название видео, субтитры площадки — `import_subs.srt`, как при загрузке файла. Проект создаётся без
  анализа; окно ставит его на стартовый экран как выбранный файл («Начать обработку» — analyze, «Ручной режим» —
  редактор), агент продолжает `project_analyze`.
- Качество → формат: `best` = `bv*+ba/b`, `1080|720|480` = `bv*[height<=?N]+ba/b[height<=?N]/wv*+ba/w`, видео сводится
  в mp4; `audio` = `ba/b` с `-x`.
- Ошибки — `{error, detail, hint}`: `bad_url`, `bad_quality`, `cookies_invalid` 400; `not_found` 404; `tool_missing`,
  `ffmpeg_missing`, `busy`, `running` 409; `network`, `proxy`, `rate_limited` 502; `disk_space` 507; `io` 500;
  остальное от площадки 422: `unsupported_url`, `playlist`, `live`, `geo_blocked`, `age_restricted`,
  `login_required`, `private`, `members_only`, `drm`, `unavailable`, `format_unavailable`, `no_audio`, `outdated`, `ytdlp_failed`.

| Метод | Путь | Назначение |
|-------|------|-----------|
| GET | `/url/probe?url=&cookies=` | `yt-dlp -J`: `{url, title, duration, thumbnail, thumbnail_data (data: URI через прокси студии), thumbnail_error, uploader, extractor, max_height, has_video, has_audio, qualities, subtitles:[{lang, name, formats}], auto_subtitles, expected_bytes, tool_version}`; `cookies` — путь к cookies.txt |
| POST | `/url/probe` | То же, тело `{url, cookies?, cookies_text?}` (содержимое cookies.txt — окно не знает путей файлов) |
| POST | `/projects/from_url` | `{url, quality: best\|1080\|720\|480\|audio, subs_lang?, cookies?, cookies_text?}` → `{fetch}`; готовый проект — `fetch.pid` |
| GET | `/url/fetches` | `{fetches:[{id, url, quality, subsLang, cookies, status: downloading\|completed\|failed\|cancelled\|interrupted, phase: probe\|download\|merge\|extract\|subtitles\|project\|done, title, duration, downloaded, total, speedBps, etaS, pid, subsImported, warning: subs_missing\|subs_failed\|subs_empty, warningDetail, errorCode, error, hint, toolVersion, startedAt, updatedAt}]}` новые первыми, не больше 20 |
| GET | `/url/fetches/{id}` | Одна загрузка |
| POST | `/url/fetches/{id}/cancel` | Погасить yt-dlp со всеми потомками, недокачанное удалить → `{fetch}` |
| POST | `/url/fetches/{id}/resume` | Прерванную или упавшую — заново с теми же настройками и докачкой → `{fetch}` |
| DELETE | `/url/fetches/{id}` | Убрать не идущую загрузку из списка вместе с недокачанным → `{ok}` |
| GET | `/url/tool` | `{installed, version, pinnedVersion, updated, latest, checkedAt, updateAvailable, updating, lastError}` |
| POST | `/url/tool/update` | Проверить релизы сейчас и поставить новый yt-dlp в фоне → `{started, tool}` |
