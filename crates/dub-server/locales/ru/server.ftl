asr-windowed = транскрипция по окнам на GPU ({ $speakers } { $speakers ->
    [one] спикер
    [few] спикера
   *[many] спикеров
})

## Separation, OCR, benchmark, casting library, cloud ASR and TTS

atomic-extract-separation-audio = извлечение аудио 44.1k для сепарации
atomic-separating = сепарация ({ $model })
atomic-separation-failed = сепарация: { $error }
ocr-detecting-burned-text = детекция вшитого текста ({ $model })
bench-stage = ⏱ { $name }: { $seconds }с | GPU ~{ $gpu_avg }% (пик { $gpu_max }%) | CPU ~{ $cpu_avg }% | VRAM { $vram }МБ | узко: { $bound }
bench-total = ⏱ { $label } ИТОГО: { $seconds }с
casting-no-casting-json = в проекте нет casting.json (кастинг не запускался)
casting-profile-delete-failed = удалить профиль { $slug }: { $error }
casting-profile-not-found = профиль «{ $slug }» не найден
cloud-asr-no-key = облачный ASR включён, но ключ OpenRouter не задан
cloud-asr-no-model = STT-модель OpenRouter не выбрана в настройках
cloud-asr-failed = облачный ASR: { $error }
cloud-asr-empty = облачный STT вернул пустой транскрипт
cloud-tts-no-key = облачный TTS включён, но ключ OpenRouter не задан
cloud-tts-no-model = TTS-модель не выбрана в настройках (Облачные модели · OpenRouter)
cloud-tts-no-voice = голос TTS не задан в настройках (у каждой модели свои голоса)
cloud-tts-failed = облачный TTS: { $error }
cloud-tts-too-short = облачный TTS: слишком короткое аудио ({ $bytes } { $bytes ->
    [one] байт
    [few] байта
   *[many] байт
})
cloud-tts-read-wav = чтение облачного wav: { $error }

## Shared messages, compositing, downloads, dub timing, endpoints, preview frame

common-read = чтение { $path }: { $error }
common-parse = разбор { $path }: { $error }
common-ffmpeg-start = ffmpeg запуск: { $error }
compose-summary = композит: титров={ $titles } (bbox), блюр { $boxes } { $boxes ->
    [one] бокса
    [few] боксов
   *[many] боксов
}, sub_px={ $sub_px }
compose-taglines-no-mt = таглайны: MT недоступен ({ $error }) -> только блюр
compose-taglines-translating = таглайны: перевод надписей титр-карты
compose-taglines-failed = таглайны: перевод не удался; только блюр
downloads-interrupted = закачка оборвалась вместе с приложением; скачанное сохранено в .part и докачается с места
downloads-nothing-to-download = нет скачиваемых компонентов среди выбранных id
downloads-busy = закачка уже идёт
downloads-thread-failed = поток закачки не запустился: { $error }
timing-words-not-recognized = пословные тайминги дубляжа не распознаны для { $count } { $count ->
    [one] фразы
    [few] фраз
   *[many] фраз
} ({ $examples }) — у них подсветка слов по длине
openrouter-catalog-failed = каталог OpenRouter: { $error }
openrouter-empty-key = пустой ключ
llm-server-no-address = адрес сервера не задан
llm-server-no-answer = сервер { $base } не ответил: { $reason }
llm-server-status = { $endpoint } ответил { $status }
llm-server-not-json = { $endpoint } вернул не JSON: { $error }
llm-server-not-model-list = { $endpoint } вернул не список моделей OpenAI (нет поля data)
remix-start = ремикс { $count } { $count ->
    [one] строки
    [few] строк
   *[many] строк
} → { $instruction }
remix-no-llm = ремикс: LLM недоступен — { $error }
remix-lines-changed = ремикс: реплики изменились, пока шёл ремикс — проект не менялся, запустите ремикс ещё раз
frame-empty-ass = пустой ASS: { $error }
frame-read-preview = чтение превью-кадра: { $error }
frame-read-source = чтение кадра оригинала: { $error }

## Glossary, hardware, job records, LLM providers

glossary-bad-format = format «{ $format }»: json или tsv
glossary-series-unreadable = глоссарий сериала не читается: { $error }
glossary-no-text = в проекте нет текста: глоссарий собирается из распознанной речи — сначала анализ
glossary-lines = глоссарий: { $count } { $count ->
    [one] строка
    [few] строки
   *[many] строк
} текста
glossary-no-llm = глоссарий: LLM недоступен — { $error }
glossary-failed = глоссарий: { $error }
glossary-proposed = глоссарий: предложено записей — { $count }
glossary-series-profile-missing = профиль сериала «{ $slug }» не найден — его глоссарий не применён
glossary-series-slug-unreadable = глоссарий сериала «{ $slug }» не читается: { $error }
glossary-series-applied = глоссарий сериала «{ $slug }»: { $count } { $count ->
    [one] запись
    [few] записи
   *[many] записей
}, добавлено в проект { $added }
hw-no-nvidia = нет NVIDIA GPU
jobs-serialize-record = сериализация job.json: { $error }
jobs-record-missing = { $file } нет в { $dir }
llm-llama-server-missing = llama-server не найден ({ $path })
llm-gemma-missing = GGUF Gemma не найден ({ $path })
llm-mmproj-missing = vision-проектор Gemma (mmproj) не найден ({ $path })
llm-chat-client = клиент чата: { $error }
llm-local-no-text-model = локальный сервер ({ $url }) выбран для перевода, но модель не выбрана
llm-local-no-vision-model = локальный сервер ({ $url }) выбран для vision, но модель не выбрана
llm-local-client = клиент локального сервера: { $error }
llm-local-label = локальный сервер { $url } · { $model }
llm-openrouter-no-key = OpenRouter выбран, но ключ не задан
llm-openrouter-no-text-model = OpenRouter выбран для перевода, но модель не выбрана
llm-openrouter-no-vision-model = OpenRouter выбран для vision, но модель не выбрана
llm-openrouter-not-text = модель OpenRouter { $model } не отвечает текстом — выберите другую для перевода
llm-openrouter-not-vision = модель OpenRouter { $model } не принимает картинки — выберите vision-модель
llm-vision-missing = нет ({ $reason })
llm-pair = перевод: { $text }; vision: { $vision }

## Media tools and OCR

common-ffmpeg-exit = ffmpeg код { $code }:
    { $tail }
media-ffprobe-start = ffprobe запуск не удался: { $error }
media-ffprobe-exit = ffprobe вернул код { $code }: { $stderr }
media-ffprobe-no-streams = ffprobe: нет streams
media-no-streams = во входе нет ни видео-, ни аудиопотока
media-no-duration = не удалось определить длительность
media-no-wav = ffmpeg не создал wav
media-ffmpeg-hung = ffmpeg не завершился за { $seconds }с — убит (зависание)
media-ffprobe-duration-exit = ffprobe duration код { $code }
media-env-filter-script = env filter-скрипт: { $error }
media-no-sample-rate = { $path }: частота звука не прочитана ({ $stderr })
ocr-no-blur = { $error }; без блюра
ocr-models-missing = модели OCR не найдены
ocr-detection-failed = OCR-детекция не удалась ({ $error })
ocr-summary = OCR: { $regions } { $regions ->
    [one] регион
    [few] региона
   *[many] регионов
}, { $localize } localize, { $bands } band-{ $bands ->
    [one] спан
    [few] спана
   *[many] спанов
}, sub_y={ $sub_y }

## Hardware presets

common-write = запись { $what }: { $error }
preset-rtx5090-title = RTX 5090 (32 ГБ)
preset-rtx4090-title = RTX 4090 (24 ГБ)
preset-top-subtitle = Максимальное качество — топовые кванты локально
preset-gpu16-title = GPU 16 ГБ
preset-gpu16-subtitle = Высокое качество (4080/4070 Ti и подобные)
preset-gpu12-title = GPU 12 ГБ
preset-gpu12-subtitle = Сбалансированно (3060/4070 и подобные)
preset-gpu8-title = GPU 8 ГБ
preset-gpu8-subtitle = Экономный — лёгкие кванты (3060 Ti/4060)
preset-weak-nvidia-cloud-title = Слабая NVIDIA + облако
preset-weak-nvidia-cloud-subtitle = Тяжёлое (перевод/vision/озвучка) в OpenRouter, сепарация и ASR на вашей GPU
preset-cloud-title = CPU + облако (без NVIDIA)
preset-cloud-subtitle = Тяжёлое в OpenRouter, локальное на процессоре — запускается без видеокарты (нужен ключ)
preset-custom-title = Пользовательский
preset-custom-subtitle = Настрою каждый параметр вручную
preset-reason-no-gpu = NVIDIA GPU не найдена — режим «CPU + облако»: тяжёлое в OpenRouter, локальное на процессоре (медленнее, но работает)
preset-reason-top-card = Обнаружена { $gpu } — максимальные кванты
preset-reason-by-vram = { $gpu } · { $vram } ГБ VRAM — { $preset }
preset-reason-low-vram = { $gpu } · { $vram } ГБ VRAM маловато для локали — облако надёжнее
preset-unknown = неизвестный пресет: { $id }

## Service port, shortening, frontend, launch defaults

common-corrupt = { $path } повреждён: { $error }
common-create-dir = каталог { $path }: { $error }
service-bad-port = { $value }: не номер порта (ожидается 1..65535)
service-exe-path = путь к exe сервиса: { $error }
service-request-not-sent = соединение принято, но запрос не ушёл: { $error }
service-no-health-answer = соединение принято, ответа на /health нет: { $error }
service-not-http = ответ не по HTTP
service-health-status = HTTP-сервер, /health ответил { $code }
service-health-not-dub-studio = HTTP-сервер, /health ответил не телом Dub Studio
service-health-not-json = HTTP-сервер, /health ответил не JSON
service-health-no-fields = /health называет себя { $app }, но без полей сервиса: { $error }
service-other-app = другое приложение ({ $app })
service-health-no-app = HTTP-сервер, /health без имени приложения
service-port-reserved = система не отдаёт порт, хотя соединений на нём никто не принимает (возможно, он в зарезервированном диапазоне Windows: { $command })
service-port-dub-studio = на нём Dub Studio { $version } ({ $executable })
service-port-other = его занял другой процесс: { $what }
service-port-busy = Порт 127.0.0.1:{ $port } занят уже { $seconds } с: { $who }. Ошибка: { $error }.

    Закройте программу, которая его держит, или задайте другой порт переменной окружения { $env } (например { $env }={ $other_port }).
shorten-line-done = сокращение { $n }/{ $total }: { $from } -> { $to } символов
shorten-line-rejected = сокращение { $n }/{ $total }: ответ не принят ({ $reason })
shorten-line-no-answer = сокращение { $n }/{ $total }: LLM не ответил — { $error }
shorten-auto-start = не влезли в слот { $count } { $count ->
    [one] фраза
    [few] фразы
   *[many] фраз
} — сокращаю перевод и озвучиваю только их
shorten-higgs-unloaded = Higgs выгружен на время сокращения перевода
shorten-auto-no-llm = сокращение перевода пропущено: LLM недоступен — { $error }
shorten-none-shortened = сокращение: ни одна из { $count } фраз не сократилась — остаются как озвучены
shorten-done = сокращено { $count } { $count ->
    [one] фраза
    [few] фразы
   *[many] фраз
} из { $total }
shorten-nothing = нечего сокращать: все фразы влезают в свои слоты
shorten-no-llm = сокращение: LLM недоступен — { $error }
shorten-start = сокращение { $count } { $count ->
    [one] фразы
    [few] фраз
   *[many] фраз
}: { $provider }
shorten-all-failed = сокращение не выполнено: LLM не ответил ни на одну фразу ({ $id }: { $error })
spa-not-built = frontend не собран
settings-bad-speaker-count = speaker_count: ожидается целое число от 0 до { $max } (0 — автоматически)
settings-bad-vo-gain = vo_gain_db={ $value }: ожидается число от { $min } до { $max } дБ
settings-bad-src-lang = src_lang={ $value }: не код языка и не "auto"
settings-bad-tgt-lang = tgt_lang={ $value }: не код языка
settings-bad-casting-ref = casting_ref={ $value }: не slug профиля кастинга
settings-style-too-long = tr_style_custom длиннее { $max } символов
settings-too-many-slots = { $name }: больше { $max } слотов
settings-unknown-field = незнакомое поле дефолтов запуска: { $key }
settings-read-failed = чтение дефолтов запуска: { $error }
settings-patch-not-object = тело PATCH /settings/launch — объект полей
settings-write-failed = запись дефолтов запуска: { $error }

## Takes, translation, TTS text, MCP arguments

takes-delete-old = удаление старого дубля { $path }: { $error }
takes-missing = дубля { $take } нет в истории фразы { $line }
translate-no-vision = тип контента не определён: vision недоступен ({ $error })
translate-subs-already-translated = субтитры уже на языке перевода -> без MT (только озвучка)
translate-transcribe-only = transcribe: tgt=исходный текст, без перевода
translate-same-language = same-lang -> без MT (tgt=исходник)
translate-no-llm = перевод не выполнен: LLM недоступен: { $error }
translate-ctx-pass = ctx-проход: vision layout/scene + перевод транскрипта
translate-failed = перевод не выполнен: { $error }
translate-untranslated = перевод не выполнен: { $left } из { $total } { $total ->
    [one] строки
    [few] строк
   *[many] строк
} остались на исходном языке (подробности — в журнале и logs/llama-server.log)
translate-untranslated-auto = перевод не выполнен: { $left } из { $total } { $total ->
    [one] строки
    [few] строк
   *[many] строк
} остались на исходном языке; если речь в ролике уже на языке перевода, укажите язык оригинала — тогда перевод не нужен (подробности — в журнале и logs/llama-server.log)
translate-done = перевод готов: { $done }/{ $total } строк, тайтлов={ $titles }
translate-left-untranslated = { $left } из { $total } { $total ->
    [one] строки
    [few] строк
   *[many] строк
} остались на исходном языке (подробности — в logs/llama-server.log)
translate-coverage-retry = покрытие перевода: { $count } { $count ->
    [one] строка
    [few] строки
   *[many] строк
} без перевода — доперевожу
translate-coverage-failed = покрытие перевода: доперевод не удался ({ $error })
translate-coverage-left = покрытие перевода: осталось { $count } без перевода
tts-silent-after-cleanup = без речи после чистки текста — тишина: { $count } { $count ->
    [one] фраза
    [few] фразы
   *[many] фраз
} ({ $lines })
mcp-bad-speaker-count = speaker_count: ожидается целое число от 0 до { $max }
mcp-speakers-without-diarize = speaker_count больше 1 несовместим с diarize=false

## Routes: projects, voices, setup, translation, export, jobs, alignment

project-serialize = сериализация project.json: { $error }
project-delete-failed = удаление проекта { $path }: { $error }
setup-no-ids = ids пуст
setup-fetching-missing = Догружаю недостающие модели для этой функции…
setup-diarization-missing = Модель диаризации не скачалась ({ $error }) — анализ пойдёт без разделения спикеров
setup-pick-model-files = Файл(ы) модели
setup-pick-models-folder = Папка с готовыми моделями
setup-no-folder = нет папки { $path }
voices-not-found = голос { $name } не найден в voices/
voices-no-vocals = нет вокала для замера F0 — сначала analyze
voices-speakers-changed = голоса по слотам: спикеры изменились, пока мерился голос — проект не менялся, запустите ещё раз
analyze-args-not-object = analyze args: ожидался объект
cost-analyze = OpenRouter: потрачено ${ $spent } за анализ (всего использовано ${ $total })
cost-run = OpenRouter: потрачено ${ $spent } за прогон (всего использовано ${ $total })
translate-lines-to = Перевод { $count } { $count ->
    [one] строки
    [few] строк
   *[many] строк
} → { $lang }
translate-note = перевод: { $note }
translate-titles-failed = титры не переведены ({ $error }) — в видео они останутся на исходном языке
translate-titles-error = перевод титров: { $error }
translate-lines-changed = перевод: реплики или титры изменились, пока шёл перевод — проект не менялся, запустите перевод ещё раз
export-pick-folder = Куда сохранить результаты
export-copy-failed = копирование в { $path }: { $error }
jobs-wait-not-number = wait: ожидалось число секунд
jobs-unknown-kind = неизвестный вид джобы в job.json: { $kind }
jobs-not-resumable = джоба { $kind } не продолжается
align-no-source-text = выравнивание по речи: у реплик нет текста на языке оригинала (субтитры импортированы на языке перевода)
align-no-vocals = выравнивание по речи: нет дорожки вокала проекта — сначала анализ
align-recognizing = выравнивание по речи: распознавание слов
align-recognition-failed = выравнивание по речи: распознавание: { $error }
align-no-speech = выравнивание по речи: речь не распознана — тайминги не менялись
align-mismatch = выравнивание по речи: реплики не совпали с речью (сопоставлено { $share }%) — тайминги не менялись
align-lines-changed = выравнивание по речи: реплики изменились, пока шло распознавание — тайминги не менялись, запустите выравнивание ещё раз
align-done = выровнено по речи: { $share }% реплик по словам, изменён тайминг у { $changed }; сдвиг { $offset } с

## Analysis

analyze-serialize = сериализация { $what }: { $error }
analyze-read-model = чтение модели { $path }: { $error }
analyze-stage-cache-unreadable = кэш стадии { $stage } не читается ({ $error }) — пересчёт
analyze-checkpoint-not-saved = чекпоинт стадии { $stage } не сохранён: { $error }
analyze-diarize-continuous = диаризация: ожидается { $speakers } { $speakers ->
    [one] спикер
    [few] спикера
   *[many] спикеров
}, непрерывный проход всей записи без сброса меток на часовых границах
analyze-diarize-fragments = диаризация: ожидается { $speakers } { $speakers ->
    [one] спикер
    [few] спикера
   *[many] спикеров
}, сопоставление голосов между фрагментами
analyze-wespeaker-needed = WeSpeaker необходим для заданного числа спикеров: { $error }
analyze-align-skipped = выравнивание по речи пропущено: субтитры на языке перевода, речь — на языке оригинала
analyze-align-recognizing = выравнивание субтитров по речи: распознавание слов
analyze-align-recognition-failed = выравнивание субтитров: распознавание речи: { $error }
analyze-align-no-speech = выравнивание по речи: речь не распознана — тайминги файла оставлены
analyze-align-mismatch = субтитры не совпали с речью (сопоставлено { $share }% реплик) — тайминги файла оставлены
analyze-align-done = субтитры выровнены по речи: { $share }% реплик по словам, остальные сдвинуты вместе с соседями; сдвиг файла { $offset } с
analyze-more = ещё { $count }
analyze-window-plan = длинная дорожка { $duration }с: план { $windows } { $windows ->
    [one] окна
    [few] окон
   *[many] окон
} (первое ~{ $first }с) — оконный ASR ещё не активен, обработка монолитная
analyze-audio-cached = аудио из кэша (источник не менялся) — пропуск ffmpeg
analyze-extracting-audio = извлечение аудио (ffmpeg -> 16k mono)
analyze-stems-stale = стемы посчитаны из звука прежнего извлечения — сепарация заново
analyze-separating = сепарация вокала ({ $model }) — чистый голос для диаризации/ASR
analyze-separation-cached = сепарация из кэша (stems уже посчитаны)
analyze-separation-failed = сепарация не удалась ({ $error }) — диаризация/ASR по сырому аудио
analyze-separator-missing = { $model } не найден — диаризация/ASR по сырому аудио
analyze-diarizing = диаризация ({ $model })
analyze-diarization-cached = диаризация из кэша
analyze-diarization-count-failed = диаризация с заданным числом спикеров: { $error }
analyze-diarization-failed = диаризация не удалась ({ $error }); single-speaker путь
analyze-diarization-model-missing-count = модель { $model } не найдена: заданное число спикеров не может быть применено
analyze-subs-no-diarization = субтитры: без диаризации (весь клип одним куском)
analyze-diarization-model-missing = модель диаризации не найдена; single-speaker путь
analyze-fewer-speakers = ожидалось { $expected } { $expected ->
    [one] спикер
    [few] спикера
   *[many] спикеров
}, различено { $found }: отсутствующие голоса не добавлены
analyze-speakers-matched = голоса сопоставлены между фрагментами: { $speakers } { $speakers ->
    [one] спикер
    [few] спикера
   *[many] спикеров
}
analyze-transcript-cached = транскрипт из кэша: { $segments } { $segments ->
    [one] сегмент
    [few] сегмента
   *[many] сегментов
}
analyze-read-subs = чтение субтитров { $path }: { $error }
analyze-subs-empty = субтитры не распознаны/пусты: { $path }
analyze-subs-imported = субтитры импортированы: { $lines } { $lines ->
    [one] реплика
    [few] реплики
   *[many] реплик
}, { $speakers } { $speakers ->
    [one] спикер
    [few] спикера
   *[many] спикеров
}
analyze-cloud-asr = транскрипция через облако (OpenRouter STT)
analyze-cloud-stt-failed = облачный STT: { $error }
analyze-cloud-done = облако: { $lines } { $lines ->
    [one] реплика
    [few] реплики
   *[many] реплик
}, { $speakers } { $speakers ->
    [one] спикер
    [few] спикера
   *[many] спикеров
}
analyze-hallucination-filter = фильтр галлюцинаций ASR: { $error }
analyze-asr-language = Parakeet не распознаёт язык «{ $lang }»: он знает 25 европейских языков. Выберите Whisper в «Настройки → Модели» (99 языков); если указать язык источника в окне, студия переключит сама.
analyze-asr-no-words = В ролике { $speech } с речи, а Parakeet услышал в ней { $words } слов: он знает только 25 европейских языков. Укажите язык источника в окне вместо «Авто» — для языка вне Европы студия переключит распознавание на Whisper.
analyze-hidden-hallucinations = скрыто фраз-галлюцинаций ASR (голоса нет): { $count } — { $lines }
analyze-hidden-by-text = скрыто титров и звуков ASR по тексту (на интервале звук, голос от музыки не отделён): { $count } — { $lines }
analyze-voiced-suspects = похожи на галлюцинацию, но голос есть — оставлены с пометкой: { $count } — { $lines }
analyze-merged-fragments = слияние огрызков: { $before } -> { $after } сегментов
analyze-characters-by-voice = персонажей по голосу: { $count }
analyze-segments-speakers = { $segments } { $segments ->
    [one] сегмент
    [few] сегмента
   *[many] сегментов
}, { $speakers } { $speakers ->
    [one] спикер
    [few] спикера
   *[many] спикеров
}
analyze-project-unparsable = project.json не разбирается ({ $error }): в нём глоссарий проекта — анализ остановлен, чтобы его не потерять
analyze-glossary-fixed = глоссарий: исправлено ошибок распознавания терминов — { $count }
analyze-no-speech-nodub = нет речевых сегментов; оставляю оригинальную дорожку (nodub)
analyze-auto-nodub = auto: нет дубляж-годной речи -> NODUB (оригинал + локализация экранного текста)
analyze-casting-style = описания персонажей из профиля кастинга -> стиль перевода ({ $chars } симв.)
analyze-empty-removed = убрано сегментов без слов: { $count }
analyze-translation-cached = перевод из кэша
analyze-ocr-cached = детекция экранного текста из кэша
analyze-audio-no-ocr = аудио-режим: без видео, детекция экранного текста не нужна
analyze-ocr-off = детекция вшитого текста отключена (галочка)
analyze-content-type = тип контента (авто): { $kind }
analyze-casting-cached = кастинг из кэша
analyze-casting-checkpoint = чекпоинт кастинга не сохранён: { $error }
analyze-profile-voices-missing = голоса профиля не найдены в voices/ -> клон: { $voices }
analyze-profile-voices-applied = перенесённые голоса профиля применены к дубляжу ({ $count } { $count ->
    [one] персонаж
    [few] персонажа
   *[many] персонажей
})
analyze-audio-no-casting = аудио-режим: без видео, кастинг персонажей не нужен
analyze-cache-not-saved = не удалось сохранить cache.json: { $error } (не критично)

## Casting, recording, voice library

casting-relabel = голосовая переразметка: { $count } { $count ->
    [one] персонаж
    [few] персонажа
   *[many] персонажей
} по голосу (диаризация нашла { $found })
casting-skipped-no-speech = кастинг пропущен: нет речевых сегментов
casting-skipped-no-speakers = кастинг пропущен: нет спикеров
casting-speakers-ranked = персонажей-спикеров: { $count } (ранжированы по времени речи)
casting-faces-bound = лиц привязано к спикерам: { $count } из { $total }
casting-bit-part-skipped = { $character }: спикер { $speaker } — 1 реплика, ни лица ни голоса -> пропуск (не кастуемый бит-парт)
casting-character-face = { $character }: спикер { $speaker }, реплик { $lines }, речь { $seconds }с, лицо: да
casting-character-no-face = { $character }: спикер { $speaker }, реплик { $lines }, речь { $seconds }с, лицо: нет
casting-profile-other-type = профиль другого типа ({ $previous } ≠ { $current }) — кросс-матч пропущен
casting-cross-episode = cross-episode: перенесено имён/голосов: { $count }
casting-saved = casting.json готов: { $count } { $count ->
    [one] персонаж
    [few] персонажа
   *[many] персонажей
}
casting-save-failed = не удалось записать casting.json: { $error }
casting-faces-collected = лиц собрано: { $faces } (сэмплов { $samples })
casting-face-clusters = лиц-персон (кластеров по вектору): { $count }
casting-anime-detector-missing = аниме-детектор не найден ({ $path }) — без аватаров
casting-anime-detector-failed = аниме-детектор не загрузился: { $error } — без аватаров
casting-scrfd-missing = SCRFD не найден — без аватаров (кастинг по голосу)
casting-scrfd-failed = SCRFD не загрузился: { $error } — без аватаров
casting-embedder-missing = { $model } не найден ({ $path }) — аватар без эмбеддинга
casting-embedder-failed = { $model } не загрузился: { $error } — аватар без эмбеддинга
casting-no-frame = ffmpeg не извлёк кадр
casting-voice-no-vocals = голосовой эмбеддинг пропущен: нет чистого вокала
casting-voice-no-model = голос пропущен: нет модели WeSpeaker ({ $path })
casting-voice-model-failed = WeSpeaker не загрузился: { $error }; голос пропущен
casting-voice-trim-failed = образец голоса { $speaker }: обрезка не удалась: { $error }
casting-voice-embedding-failed = голос { $speaker }: эмбеддинг не удался: { $error }
casting-voice-embeddings = голосовых эмбеддингов: { $count }
casting-applying-profile = применяю профиль библиотеки: { $slug }
casting-library-profile-missing = профиль библиотеки «{ $slug }» не найден — без применения
record-busy = Уже идёт запись
record-mic-not-found = Микрофон «{ $name }» не найден — он отключён или переименован; выберите другой
record-no-mic = Микрофон не найден
record-mic-config = конфиг микрофона: { $error }
record-create-wav = создать wav: { $error }
record-format-unsupported = формат { $format } не поддержан
record-mic-open = открыть микрофон: { $error }
record-mic-start = старт микрофона: { $error }
record-not-recording = Запись не идёт
voices-not-file-list = { $page }: не список файлов: { $error }
voices-list-endless = { $page }: список файлов датасета не кончается
voices-too-large = { $url }: больше закреплённых { $size } байт
voices-size-mismatch = { $url }: пришло { $got } байт, закреплено { $size }
voices-sha-mismatch = { $url }: SHA-256 { $got } не совпал с закреплённым { $want }
voices-bad-name = плохое имя
voices-not-in-dataset = голоса { $name } нет в датасете { $dataset }
voices-bad-size = { $name }: размер в каталоге не годится
voices-no-sha = { $name }: нет SHA-256 в каталоге
voices-pack-downloading = скачивание пака голосов
voices-pack-unpacking = распаковка
voices-open-zip = открыть zip: { $error }
voices-create-file = создать { $name }: { $error }
voices-unpack-file = распаковка { $name }: { $error }
voices-pack-done = готово: { $count } { $count ->
    [one] файл
    [few] файла
   *[many] файлов
}

## Downloads by link

url-no-fetch = нет загрузки { $id }
url-stopped = остановлено
url-already-downloading = эта ссылка уже качается: { $id }
url-probe-not-json = ответ yt-dlp -J не JSON: { $error }; { $stderr }
url-interrupted = загрузка оборвалась вместе со студией; скачанное лежит в её папке и докачается с места
url-not-a-link = «{ $url }» не ссылка: { $error }
url-not-http = «{ $url }» — не http(s)-ссылка
url-bad-subs-lang = «{ $lang }» — не код языка субтитров
url-cookies-not-file = { $path } — не файл cookies.txt до 1 МБ
url-cookies-empty-or-large = cookies.txt пуст или больше 1 МБ
url-thread-failed = поток загрузки не запустился: { $error }
url-cookies-gone = cookies.txt этой загрузки пропал из её папки — начните загрузку заново с cookies
url-disk-space = нужно около { $need } МБ, свободно { $free } МБ
url-no-subs = у видео нет субтитров «{ $lang }», загруженных людьми (есть: { $have })
url-no-subs-none = у видео нет субтитров «{ $lang }», загруженных людьми (есть: никаких)
url-no-media-file = yt-dlp закончил, но файла видео нет в { $path }: { $stderr }
url-subs-failed = субтитры «{ $lang }» не скачались ({ $code }): { $error }
url-ytdlp-start = запуск yt-dlp: { $error }
url-ytdlp-wait = ожидание yt-dlp: { $error }
url-still-downloading = { $id } ещё качается
url-cancelled-removed = { $id } отменена, скачанное удалено: начните новую загрузку
url-still-stopping = { $id } ещё останавливается — повторите через пару секунд
url-cancel-first = { $id } ещё качается — сначала отмените
url-no-subs-file = yt-dlp закончил без файла субтитров в { $path }: { $stderr }
url-no-parent = { $path } без родителя
url-subs-empty = в субтитрах { $path } нет ни одной реплики
url-ytdlp-missing = компонент ytdlp не скачан
url-bad-quality = качество «{ $quality }»: best, 1080, 720, 480 или audio

## yt-dlp

ytdlp-update-missing = обновление yt-dlp { $version } ({ $path }) не на месте — работает закреплённая { $pinned }
ytdlp-http-client = http-клиент: { $error }
ytdlp-tag-not-version = релиз yt-dlp с тегом «{ $tag }» — не версия
ytdlp-no-checksum = в { $url } нет { $file }
ytdlp-new-says = новый yt-dlp { $tag } называет себя «{ $said }» — остаётся { $current }
ytdlp-new-failed = новый yt-dlp { $tag } не запустился: { $error } — остаётся { $current }
ytdlp-too-large = { $url }: больше { $limit } байт
ytdlp-sha-mismatch = { $url }: SHA-256 { $got } не совпал с SHA2-256SUMS { $want }
ytdlp-exit-code = код выхода { $code }: { $stderr }
ytdlp-component-missing = компонент ytdlp (yt-dlp и deno) не скачан
ytdlp-no-ffmpeg = нет ffmpeg: ни компонента ffmpeg, ни ffmpeg.exe в PATH
ytdlp-start = запуск { $program }: { $error }
ytdlp-timeout = yt-dlp не ответил за { $seconds } с: { $stderr }
ytdlp-playlist = по ссылке плейлист или канал ({ $count } видео), а не одно видео
ytdlp-redirect-only = по ссылке нет самого видео, только ссылка дальше
ytdlp-live = эфир ({ $status })
ytdlp-thumbnail-not-image = превью не картинка ({ $mime })
ytdlp-thumbnail-too-large = превью больше 4 МБ
ytdlp-failed-silently = yt-dlp завершился с ошибкой без сообщения

## Render: voicing, QC, mix, mux

render-input = вход { $width }x{ $height } dur={ $duration }s
render-nodub-original = nodub: оригинальная аудиодорожка
render-done-audio = готово (только аудио) -> { $path }
render-building-ass = сборка ASS (титры + дублированные субтитры)
render-burning = вжигание субтитров + блюр (ffmpeg + libass, NVENC)
render-burn-off = субтитры/титры отключены (subs.burn=off)
render-muxing = муксирование видео + аудио
render-track-dub = { $lang } (дубляж)
render-track-original = { $lang } (оригинал)
render-two-tracks = две дорожки: { $dub } + { $original } -> { $container }
render-multitrack-failed = мультитрек-mux не удался ({ $error }) -> одна дорожка
render-subtitle-tracks = субтитры дорожками mkv: { $tracks }
render-subtitle-tracks-failed = субтитры дорожками mkv: { $error }
render-mp4-companion-failed = mp4-компаньон не собран ({ $error }) — плеер откроет mkv (VLC ок)
render-done = готово -> { $path }
render-dub-audio-done = дуб-аудио готово
render-synth-thread-ended = поток синтеза завершился без результата
render-synth-timeout = таймаут синтеза >{ $seconds }с — отменён, движок свободен
render-engine-stuck = синтез не отменяется >{ $seconds }с — рендер прерван (движок завис в DLL)
render-higgs-load-failed = загрузка Higgs DLL: { $error }
render-defect-runaway = затянулась
render-defect-cutoff = обрыв
render-defect-silence = тишина
render-defect-hum = гул
render-recognition-no-answer = распознавание не вернуло ответ
render-second-pass-overflow = второй проход озвучки запросил сокращение, которое в нём выключено
render-no-translated-lines = нет строк с переводом -> тишина, оригинальная дорожка
render-takes-of-removed-lines = истории дублей удалённых фраз убраны: { $count }
render-extracting-audio = извлечение аудио (ffmpeg 44.1k stereo)
render-separator-missing = движок сепарации не найден -> без фона (keep_music off)
render-ref-from-mix = реф клона из микса без сепарации: в нём звучит и фон оригинала
render-emotion-ref-failed = сегмент { $segment }: эмоц-реф не вырезан ({ $error }) — identity-реф спикера
render-cloud-voices = облачные голоса по спикерам: { $voices }
render-synth-keys-reset = { $error } — ключи синтеза начаты заново
render-synthesizing = синтез { $count } из { $total } { $total ->
    [one] сегмента
    [few] сегментов
   *[many] сегментов
}
render-voicing-cached = озвучка из кэша: { $count } { $count ->
    [one] сегмент
    [few] сегмента
   *[many] сегментов
}
render-cloud-tts-parallel = облачный TTS: { $count } { $count ->
    [one] сегмент
    [few] сегмента
   *[many] сегментов
} в { $threads } параллельных потоков
render-cloud-tts-ready = облачный TTS: пре-синтез готов ({ $count } { $count ->
    [one] сегмент
    [few] сегмента
   *[many] сегментов
})
render-takes-quarantined = { $error } — история дублей фразы { $line } отложена в { $path } и начата заново
render-pinned-take = фраза { $line }: звучит закреплённый дубль — новая озвучка его не заменяет
render-take-unpinned = фраза { $line }: закрепление дубля снято — текст реплики изменён
render-selected-take = фраза { $line }: звучит выбранный дубль { $take } — новая озвучка его не заменяет
render-write-cloud-segment = запись облачного seg{ $line }: { $error }
render-cloud-tts-failed = ⚠ сегмент { $line }: облачный TTS не удался ({ $error }) — оригинал
render-loading-higgs = загрузка Higgs
render-failures-kept-generated = ⚠ сегмент { $line }: { $attempts } сбоев синтеза ({ $error }) — взята сгенерированная озвучка (размах { $range } дБ)
render-failures-kept-original = ⚠ сегмент { $line }: { $attempts } сбоев/таймаутов синтеза ({ $error }) — оставлена оригинальная реплика
render-regenerating = сегмент { $line }: { $error } — регенерация ({ $attempt }/{ $attempts })
render-defects-kept-generated = ⚠ сегмент { $line }: все { $attempts } попыток с дефектом ({ $defect }) — взята сгенерированная озвучка (размах { $range } дБ)
render-silent-kept-original = ⚠ сегмент { $line }: { $attempts } попыток без звука — подставлен оригинал
render-retry-alt-ref = альт-реф
render-retry-temperature = temp-бамп
render-defect-regenerating = сегмент { $line }: дефект синтеза ({ $defect }), регенерация ({ $via } { $attempt }/{ $attempts })
render-write-segment = запись seg{ $line }: { $error }
render-too-many-artifacts = TTS: слишком много артефактов-гудения (подряд { $in_a_row }, всего ретраев { $retries }) — регенерация не помогает. Вероятно проблема со стендом (модель/VRAM) или с реф-клипами голосов. Остановлено на сегменте { $line }.
render-multi-take = сегмент { $line }: multi-take — выбран дубль ближе к слоту ({ $deviation }с отклонение)
render-stretch-over-cap = сегмент { $line }: нужно растянуть x{ $needed } (слот { $slot }с), кап x{ $cap } — текст быстрее нормы
render-silence-trimmed = обрезка тишины TTS: снято { $seconds } с у { $lines } фраз (из них паузы { $pauses } с); ускорение ушло в кап благодаря обрезке у { $into_cap } фраз
render-fit-summary = укладка: { $over }/{ $total } сегментов выше капа ({ $share }%)
render-fit-summary-drift = укладка: { $over }/{ $total } сегментов выше капа ({ $share }%), догон синка на { $drift }
render-qc-start = QC: сверка { $count } { $count ->
    [one] фразы
    [few] фраз
   *[many] фраз
} транскрипцией
render-qc-unheard = QC: { $count } из { $total } фраз не сверены — распознавание не удалось: { $reason }
render-qc-mismatch = QC: { $count } { $count ->
    [one] фраза не совпала
    [few] фразы не совпали
   *[many] фраз не совпали
} с переводом — пересинтез
render-qc-resynthesized = QC: сегмент { $line } пересинтезирован (попытка { $attempt })
render-qc-unconfirmed = ⚠ QC: сегмент { $line } («{ $text }») не удалось подтвердить — проверь фразу вручную
render-qc-kept-mismatch = ⚠ QC: сегмент { $line } не совпадает с текстом перевода — оставлена сгенерированная озвучка
render-qc-resynth-unheard = QC: { $count } пересинтезированных фраз не сверены — распознавание не удалось: { $reason }
render-qc-summary = QC итог: исправлено { $fixed }/{ $total }, осталось помеченных { $flagged }, не сверено { $unheard }
render-qc-all-confirmed = QC: все фразы подтверждены транскрипцией ✓
render-qc-rest-confirmed = QC: остальные фразы подтверждены транскрипцией
render-laying-out = укладка дубляжа на таймлайн
render-peak-limiter = лимитер пиков: { $lines } фраз, { $samples } сэмплов выше полки { $ceiling } опущены без клипа
render-tempo-fit = tempo-fit всей дорожки x{ $factor }
render-voiceover-envelope = voiceover: оригинал { $db } dB ПОД переводом, полный в паузах (динам. огибающая, { $blocks } блоков)
render-voiceover-flat = voiceover: огибающая недоступна -> плоское приглушение
render-mix-no-ducking = сведение: инструментал + дубль-вокал (дакинг ВЫКЛ — фон полный)
render-mix-ducking = сведение: инструментал + дубль-вокал (дакинг ВКЛ, огибающая, { $blocks } блоков)
render-mix-sidechain = огибающая недоступна -> сайдчейн-дакинг
render-mix-plain = sidechain недоступен -> прямой mix
render-loudness-off = выравнивание громкости выключено: микс как есть
render-loudness-normalizing = нормализация громкости (EBU R128, true-peak)
render-loudnorm-skipped = loudnorm пропущен ({ $error })
render-track-gain = гейн дорожки { $db } dB
render-dub-timings-not-written = тайминги дубляжа для субтитров не записаны: укладка ({ $lines } фраз, { $spans } спанов) не сопоставилась с сегментами ({ $segments }) — субтитры по таймингам оригинала
render-dub-timing-mismatch = тайминги дубляжа: фраза { $line } вида для синтеза не совпала с сегментом проекта №{ $index }
render-word-timings = пословные тайминги субтитров: распознавание { $count } фраз дубляжа
render-refs-unchecked = сверка рефов: { $count } кандидатов приняты без сверки — распознавание не удалось: { $error }
render-refs-all-failed = ⚠ спикер { $speaker }: все реф-кандидаты не прошли сверку (слышно: «{ $heard }») — беру лучший по скору
render-speaker-ref-failed = реф спикера { $speaker }: { $error }
render-speaker-ref = реф спикера { $speaker }: «{ $text }» ({ $seconds }с, { $candidates } { $candidates ->
    [one] кандидат
    [few] кандидата
   *[many] кандидатов
}, сверка ok)
render-speaker-ref-unchecked = реф спикера { $speaker }: «{ $text }» ({ $seconds }с, { $candidates } { $candidates ->
    [one] кандидат
    [few] кандидата
   *[many] кандидатов
}, сверка ⚠ не пройдена)

## Setup: components

setup-comp-higgs-purpose = Синтез дубляжа и клон голоса (TTS)
setup-comp-higgs-engine-name = Higgs движок (audiocpp_engine.dll)
setup-comp-higgs-engine-purpose = Нативный TTS-движок Higgs (C-ABI)
setup-comp-gemma-purpose = Перевод и vision-оркестратор субтитров/титров
setup-comp-gemma-q5-0-purpose = Перевод и vision — точнее q4_0
setup-comp-gemma-q6-k-purpose = Перевод и vision — ещё точнее
setup-comp-gemma-q8-0-purpose = Перевод и vision — максимальная точность
setup-comp-parakeet-purpose = Распознавание речи со словными таймстемпами (ASR)
setup-comp-higgs-q6-k-purpose = Синтез дубляжа и клон голоса (TTS) — вариант полегче Q8_0
setup-comp-higgs-q4-k-m-purpose = Синтез дубляжа и клон голоса (TTS) — самый лёгкий вариант
setup-comp-parakeet-fp32-purpose = Распознавание речи (ASR) — полная точность fp32
setup-comp-parakeet-ultra-purpose = Распознавание речи (ASR) — дообученная Moondream версия, меньше ошибок
setup-comp-whisper-engine-name = Whisper-Faster (движок ASR)
setup-comp-whisper-engine-purpose = Альтернативный движок распознавания речи (faster-whisper) вместо Parakeet
setup-comp-whisper-cuda-name = CUDA-ускорение Whisper (cuBLAS + cuDNN)
setup-comp-whisper-cuda-purpose = GPU-инференс Whisper (иначе распознавание идёт на CPU, в разы медленнее)
setup-comp-whisper-tiny-name = Whisper tiny (модель ASR)
setup-comp-whisper-tiny-purpose = ASR Whisper — самая лёгкая и быстрая модель
setup-comp-whisper-base-name = Whisper base (модель ASR)
setup-comp-whisper-base-purpose = ASR Whisper — лёгкая модель, точнее tiny
setup-comp-whisper-small-name = Whisper small (модель ASR)
setup-comp-whisper-small-purpose = ASR Whisper — сбалансированная модель
setup-comp-whisper-medium-name = Whisper medium (модель ASR)
setup-comp-whisper-medium-purpose = ASR Whisper — высокая точность
setup-comp-whisper-large-v3-name = Whisper large-v3 (модель ASR)
setup-comp-whisper-large-v3-purpose = ASR Whisper — максимальная точность (large-v3)
setup-comp-whisper-large-v3-turbo-name = Whisper large-v3-turbo (модель ASR)
setup-comp-whisper-large-v3-turbo-purpose = ASR Whisper — почти large-v3, но заметно быстрее (turbo)
setup-comp-sortformer-name = Nemotron 3 Diarization (до 8 спикеров)
setup-comp-sortformer-purpose = Разделение спикеров (кто когда говорит), до 8 голосов
setup-comp-roformer-purpose = Модель вокал/инструментал сепарации
setup-comp-roformer-q5-purpose = Сепарация — вариант полегче Q8_0
setup-comp-roformer-q4-purpose = Сепарация — самый лёгкий вариант
setup-comp-casting-name = Модели кастинга персонажей (лица + голос)
setup-comp-casting-purpose = Детект/эмбеддинг лиц (реальные + аниме) + голосовой эмбеддинг для кастинга
setup-comp-bsroformer-engine-name = BSRoformer.cpp движок (CUDA)
setup-comp-bsroformer-engine-purpose = Нативный движок сепарации (bs_roformer-cli + ggml-CUDA)
setup-comp-bsroformer-engine-cpu-name = BSRoformer.cpp движок (CPU)
setup-comp-bsroformer-engine-cpu-purpose = Сепарация на процессоре — режим без NVIDIA (медленнее, полная функция)
setup-comp-llama-name = llama.cpp сервер (CUDA 13.4)
setup-comp-llama-purpose = Сайдкар-сервер для Gemma (перевод/vision)
setup-comp-onnxruntime-purpose = Рантайм ASR/OCR/диаризации (строго 1.28.x)
setup-comp-onnxruntime-gpu-purpose = CUDA-провайдер для диаризации/Parakeet на GPU (режим local_backend=gpu)
setup-comp-ffmpeg-purpose = Декод/энкод видео и аудио (NVENC)
setup-comp-ytdlp-name = Загрузка по ссылке (yt-dlp + deno)
setup-comp-ytdlp-purpose = Скачать видео по ссылке (YouTube и другие сайты yt-dlp) в новый проект
setup-comp-cuda-runtime-purpose = Редистрибутивные CUDA-DLL для движков и CUDA-EP onnxruntime (без CUDA Toolkit)
setup-comp-cudnn-purpose = Нужен CUDA-провайдеру onnxruntime для диаризации/Parakeet на GPU
setup-comp-vcruntime-purpose = Системные DLL движков (идут в комплекте)
setup-comp-ocr-name = OCR-модели (PP-OCR ONNX)
setup-comp-ocr-purpose = Детекция вшитого текста → блюр (идут в комплекте)
setup-comp-nvidia-driver-name = Драйвер NVIDIA
setup-comp-nvidia-driver-purpose = GPU-ускорение (ставится отдельно, не приложением)

## Setup: downloads and installation

setup-http-status = { $url }: статус { $status }
setup-write = запись: { $error }
setup-not-zip = не zip: { $error }
setup-zip-entry = запись zip: { $error }
setup-open = открыть { $path }: { $error }
setup-verifying = Проверяю SHA-256 { $file }…
setup-install-record = запись об установке: { $error }
setup-disk-space = не хватает места: нужно { $need } ГБ, свободно { $free } ГБ ({ $path })
setup-unknown-component = нет компонента { $id }
setup-not-removable = { $id } ставится не приложением
setup-component-busy = компонент сейчас качается — поставьте закачку на паузу
setup-paused = закачка поставлена на паузу
setup-rate-limited = { $url }: сервер отвечает { $status } уже { $minutes } мин
setup-proxy-scheme = прокси { $proxy }: схема { $scheme } закачке не подходит (http, https, socks4, socks5)
setup-proxy-no-host = прокси { $proxy }: нет хоста
setup-proxy-no-port = прокси { $proxy }: нет порта
setup-proxy-credentials = прокси { $proxy }: закачка моделей (ureq) не может передать прокси такой логин или пароль — в нём / ? #, пробел, не-ASCII или (для SOCKS5) двоеточие в пароле; облачные запросы через этот прокси работают, для закачки нужен пароль без этих символов
setup-start-failed = { $url }: не удалось начать за { $retries } попыток: { $error }
setup-chunk-manifest = манифест чанков: { $error }
setup-range-incomplete = неполный range: { $got }/{ $want } байт
setup-range-failed = range { $start }-{ $end } после { $retries } попыток: { $error }
setup-range-status = range { $start }-{ $end }: статус { $status } (ждали 206)
setup-range-read = чтение range: { $error }
setup-create = создать { $path }: { $error }
setup-read = чтение: { $error }
setup-download-failed = { $url } после { $retries } попыток: { $error }
setup-size-mismatch = { $file }: скачано { $got } байт, закреплено { $want } — файл удалён, следующая попытка начнёт заново
setup-hash-mismatch = { $file }: SHA-256 { $got } не совпал с закреплённым { $want } — файл удалён, следующая попытка начнёт заново
setup-unpacking = Распаковываю { $file }…
setup-rename = переименовать { $path }: { $error }
setup-waiting-other = Жду другую закачку этих же компонентов…
setup-delete = удалить { $path }: { $error }
setup-downloading = Скачиваю модели…
setup-source-changed = { $url }: сервер отдаёт { $got } байт, закреплено { $want } — источник изменился
setup-manifest-write = манифест { $path }: { $error }
setup-archive-no-files = в архиве { $path } не найдено нужных файлов
setup-wheel-not-zip = wheel не zip: { $error }
setup-wheel-entry = запись wheel: { $error }
setup-archive-no-dll = в архиве { $path } нет DLL
setup-unpack = распаковка { $path }: { $error }
setup-finalize = финализация { $path }: { $error }

## Settings applied after analysis

post-analyze-bad-vo-gain = vo_gain: ожидалось число дБ, пришло { $value }
post-analyze-bad-flag = { $name }: ожидалось 0 или 1, пришло { $value }
post-analyze-bad-container = container: ожидалось mp4 или mkv, пришло { $value }
post-analyze-bad-voice-slots = voice_slots: ожидался объект {"{"}male:[…], female:[…]{"}"}
post-analyze-edit-failed = настройка после анализа { $edit }: { $error }

## Messages of the engines and libraries: glossary, LLM, ASR, translation, separation, faces, TTS, captions, OCR

glossary-over-limit = в глоссарии слишком много записей: { $total }, предел — { $max }
glossary-empty-term = запись { $entry }: пустой термин
glossary-field-too-long = запись { $entry } («{ $term }»): поле длиннее { $max } символов
glossary-duplicate = термин «{ $term }» указан дважды (записи { $first } и { $second })
glossary-tsv-keep = строка { $line }: в колонке keep «{ $value }» — нужно 1 или 0
glossary-tsv-empty-term = строка { $line }: пустой термин
glossary-one-of = нужно что-то одно: записи или TSV
glossary-nothing = нечего сохранять: нет ни записей, ни TSV
llm-spawn-failed = llama-server не запустился: { $error }
llm-gguf-missing = GGUF-модель не найдена ({ $path })
llm-log-file = лог llama-server { $path }: { $error }
llm-exited-early = llama-server завершился до готовности ({ $status }); stderr: { $stderr }
llm-not-ready = llama-server не поднялся за { $secs } с (порт { $port }); stderr: { $stderr }
llm-http = запрос к модели не прошёл: { $error }
llm-api = API модели: { $error }
llm-rejected = API модели отверг запрос ({ $status }): { $body }
llm-empty-answer = модель { $model } вернула пустой ответ (без причины)
llm-empty-answer-reason = модель { $model } вернула пустой ответ (finish_reason={ $reason })
llm-cut-short = модель { $model } упёрлась в лимит { $max_tokens } { $max_tokens ->
    [one] токен
    [few] токена
   *[many] токенов
} (finish_reason=length) — ответ неполный; вероятно, она тратит бюджет на рассуждения: выберите модель без обязательного мышления
llm-prompt-cut = сервер прочитал только { $read } { $read ->
    [one] токен
    [few] токена
   *[many] токенов
} из запроса в { $chars } { $chars ->
    [one] символ
    [few] символа
   *[many] символов
} и отбросил начало — его контекст мал: увеличьте num_ctx в Ollama или Context Length модели в LM Studio
asr-engine = распознавание речи: { $error }
asr-wav-read = не удалось прочитать wav { $path }: { $error }
asr-resample = ресемплинг: { $error }
asr-speaker-count = число спикеров должно быть от 1 до { $max }
speakers-embedding = голос спикера { $speaker }: { $error }
speakers-no-sample = спикер { $speaker }: нет речевого фрагмента длиной хотя бы 0,3 секунды для сопоставления голоса
speakers-too-many-voices = указано { $given } { $given ->
    [one] спикер
    [few] спикера
   *[many] спикеров
}, но в фрагменте найдено { $found } { $found ->
    [one] различный голос
    [few] различных голоса
   *[many] различных голосов
}: безопасно объединить их по голосу не удалось
speakers-bad-embedding = модель вернула пустые или некорректные голосовые признаки
speakers-dimension-changed = размерность голосовых признаков изменилась
speakers-more-voices = число локальных голосов превышает заданное число спикеров
speakers-unmatched = не удалось надёжно сопоставить голоса фрагмента с заданным числом участников
translate-format-json = перевод: ответ по JSON-схеме ({ $model })
translate-format-json-probe = перевод: пробую ответ по JSON-схеме ({ $model }); откажет — нумерованные строки
translate-format-numbered = перевод: нумерованные строки — модель { $model } не заявляет structured_outputs в каталоге OpenRouter
translate-schema-ignored = перевод: { $model } принял JSON-схему, но ответил нумерованными строками — дальше нумерованные строки
translate-schema-ignored-server = перевод: сервер принял JSON-схему, но ответил нумерованными строками — дальше нумерованные строки
translate-schema-refused = перевод: { $model } отверг ответ по JSON-схеме ({ $status }: { $body }) — дальше нумерованные строки
translate-schema-refused-server = перевод: сервер отверг ответ по JSON-схеме ({ $status }: { $body }) — дальше нумерованные строки
translate-line-flawed = перевод: строка { $line } — оставлен перевод с замечанием: { $reason }
translate-line-failed = перевод: строка { $line } не переведена: { $reason }
translate-line-reason = { $line }: { $reason }
translate-lines-rejected = перевод: { $bad } из { $total } { $total ->
    [one] строки
    [few] строк
   *[many] строк
} не прошли проверку ({ $reasons })
translate-batch-stopped = перевод: пакет строк { $first }..{ $last } не удался ({ $error }) — перевод остановлен
translate-batch-failed = перевод: пакет строк { $first }..{ $last } не удался ({ $error })
translate-layout-no-vision = раскладка кадра: пропущена (vision-модель не выбрана или недоступна)
translate-layout-not-needed = раскладка кадра: пропущена (субтитры не вжигаются — раскладка не нужна)
translate-layout = раскладка кадра: sub_style={ $sub_style } titles={ $titles } brands={ $brands }
translate-layout-failed = раскладка кадра: пропущена ({ $error })
translate-scene-failed = контекст сцены: пропущен ({ $error })
translate-scene-no-vision = контекст сцены: пропущен (vision-модель не выбрана или недоступна)
translate-audio-failed = аудио-контекст: пропущен ({ $error })
translate-context-trimmed = перевод: блок контекста в { $chars } { $chars ->
    [one] символ
    [few] символа
   *[many] символов
} обрезан до { $budget } (защита n_ctx)
translate-names-skipped = перевод: авто-глоссарий имён пропущен ({ $error })
translate-chunks = перевод: { $lines } { $lines ->
    [one] строка
    [few] строки
   *[many] строк
} -> { $chunks } { $chunks ->
    [one] пакет
    [few] пакета
   *[many] пакетов
} (глоссарий: { $terms } { $terms ->
    [one] термин
    [few] термина
   *[many] терминов
}, { $names } { $names ->
    [one] имя
    [few] имени
   *[many] имён
})
translate-pass-done = перевод: готово — переведено { $translated } (с замечанием { $flawed }), на исходнике { $untranslated }
glossary-pass = глоссарий: проход модели { $pass }/{ $passes }
glossary-schema-refused = глоссарий: сервер отверг ответ по JSON-схеме ({ $status }) — прошу JSON текстом
content-type-decided = тип контента: { $decided ->
    [anime] анимация
   *[other] живая съёмка
} ({ $votes } { $votes ->
    [one] внятный ответ
    [few] внятных ответа
   *[many] внятных ответов
} из { $frames } кадров)
line-missing = нет в ответе
line-cut = ответ оборван лимитом токенов
line-untranslated = не на целевом языке
line-echo = повторяет исходник
line-too-short = слишком коротко ({ $got } < { $min })
line-too-long = слишком длинно ({ $got } > { $max })
line-loop = зацикливание «{ $gram }»
line-term-missing = нет термина глоссария «{ $term }»
translate-frame = извлечение кадра: { $error }
translate-audio = аудио-контекст: { $error }
translate-empty = модель не перевела ни одной из { $lines } { $lines ->
    [one] строки
    [few] строк
   *[many] строк
}; последняя причина: { $reason }
translate-empty-no-reason = модель не перевела ни одной из { $lines } { $lines ->
    [one] строки
    [few] строк
   *[many] строк
}
translate-contract = ответ модели: { $problem }
translate-contract-answer = ответ модели: { $problem }; ответ: { $answer }
answer-no-json-object = в нём нет JSON-объекта
answer-not-json = он не разобран как JSON ({ $error })
answer-no-terms = в нём нет списка terms
remix-failed = ремикс: { $error }
sep-engine-missing = движок сепарации не найден ({ $path })
sep-model-missing = модель сепарации не найдена ({ $path })
sep-spawn = запуск движка сепарации: { $error }
sep-engine-failed = движок сепарации завершился с ошибкой (код { $code }): { $tail }
sep-engine-killed = движок сепарации остановлен без кода выхода: { $tail }
sep-no-output = движок сепарации не создал вокал-стем ({ $path })
sep-audio-io = аудио сепарации: { $error }
faces-model-missing = модель не найдена ({ $path })
ffmpeg-exit = ffmpeg завершился с кодом { $code }: { $tail }
ffmpeg-killed = ffmpeg остановлен без кода выхода: { $tail }
onnx-runtime = среда ONNX: { $error }
files-error = файлы: { $error }
model-no-outputs = модель не вернула выходов
faces-output-shape = { $model }: выход модели неожиданной формы { $shape }
faces-output-count = { $model }: ожидалось выходов модели — { $expected }, получено — { $got }
faces-crop-size = эмбеддинг лица ждёт выровненный кроп 112x112, получен { $width }x{ $height }
faces-sample-rate = голосовой клип должен быть { $expected } Гц, а он { $got } Гц
faces-clip-too-short = голосовой клип слишком короткий для анализа
tts-library-load = не удалось загрузить движок озвучки: { $error }
tts-cancelled = генерация отменена
tts-generation = генерация не удалась: { $error }
tts-invalid-param = неверный параметр озвучки: { $error }
tts-streaming-unsupported = стриминг не поддерживается этой DLL движка
render-higgs-model-failed = загрузка модели Higgs: { $error }
captions-write-ass = запись файла субтитров: { $error }
captions-filter-script = запись filter-скрипта ffmpeg: { $error }
captions-ffmpeg-wait = ожидание ffmpeg: { $error }
captions-ffmpeg-timeout = ffmpeg не завершился за { $secs } с — убит (зависание): { $tail }
captions-burn-failed = вжигание субтитров не удалось: { $tail }
captions-frame-failed = кадр превью не получен: { $tail }
ocr-no-dictionary = в модели распознавания текста нет словаря символов
ocr-internal = распознавание текста: внутренняя ошибка ({ $error })
