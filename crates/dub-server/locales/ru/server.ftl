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
