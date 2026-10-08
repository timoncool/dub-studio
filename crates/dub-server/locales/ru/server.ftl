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
