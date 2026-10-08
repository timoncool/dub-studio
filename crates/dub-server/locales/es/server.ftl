asr-windowed = transcripción por ventanas en la GPU ({ $speakers } { $speakers ->
    [one] hablante
   *[other] hablantes
})

## Separation, OCR, benchmark, casting library, cloud ASR and TTS

atomic-extract-separation-audio = extrayendo audio a 44,1k para la separación
atomic-separating = separando ({ $model })
atomic-separation-failed = separación: { $error }
ocr-detecting-burned-text = detectando texto incrustado ({ $model })
bench-stage = ⏱ { $name }: { $seconds } s | GPU ~{ $gpu_avg }% (pico { $gpu_max }%) | CPU ~{ $cpu_avg }% | VRAM { $vram } MB | límite: { $bound }
bench-total = ⏱ { $label } TOTAL: { $seconds } s
casting-no-casting-json = el proyecto no tiene casting.json (el casting no se ha ejecutado)
casting-profile-delete-failed = eliminando el perfil { $slug }: { $error }
casting-profile-not-found = no se encontró el perfil «{ $slug }»
cloud-asr-no-key = el ASR en la nube está activado, pero no hay clave de OpenRouter
cloud-asr-no-model = no hay ningún modelo STT de OpenRouter seleccionado en los ajustes
cloud-asr-failed = ASR en la nube: { $error }
cloud-asr-empty = el STT en la nube devolvió una transcripción vacía
cloud-tts-no-key = el TTS en la nube está activado, pero no hay clave de OpenRouter
cloud-tts-no-model = no hay ningún modelo TTS seleccionado en los ajustes (Modelos en la nube · OpenRouter)
cloud-tts-no-voice = no hay voz TTS en los ajustes (cada modelo tiene sus propias voces)
cloud-tts-failed = TTS en la nube: { $error }
cloud-tts-too-short = TTS en la nube: el audio es demasiado corto ({ $bytes } { $bytes ->
    [one] byte
   *[other] bytes
})
cloud-tts-read-wav = leyendo el wav de la nube: { $error }
