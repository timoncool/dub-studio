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
