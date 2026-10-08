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

## Shared messages, compositing, downloads, dub timing, endpoints, preview frame

common-read = leyendo { $path }: { $error }
common-parse = analizando { $path }: { $error }
common-ffmpeg-start = iniciando ffmpeg: { $error }
compose-summary = composición: títulos={ $titles } (bbox), desenfoque de { $boxes } { $boxes ->
    [one] caja
   *[other] cajas
}, sub_px={ $sub_px }
compose-taglines-no-mt = lemas: la traducción automática no está disponible ({ $error }) -> solo desenfoque
compose-taglines-translating = lemas: traduciendo los textos del rótulo
compose-taglines-failed = lemas: la traducción falló; solo desenfoque
downloads-interrupted = la descarga se interrumpió al cerrarse la aplicación; lo descargado se guardó en .part y se reanudará desde ahí
downloads-nothing-to-download = ninguno de los id seleccionados es un componente descargable
downloads-busy = ya hay una descarga en curso
downloads-thread-failed = el hilo de descarga no se inició: { $error }
timing-words-not-recognized = no se reconocieron los tiempos por palabra del doblaje en { $count } { $count ->
    [one] frase
   *[other] frases
} ({ $examples }); sus palabras se resaltan por longitud
openrouter-catalog-failed = catálogo de OpenRouter: { $error }
openrouter-empty-key = la clave está vacía
llm-server-no-address = no se ha indicado la dirección del servidor
llm-server-no-answer = el servidor { $base } no respondió: { $reason }
llm-server-status = { $endpoint } respondió { $status }
llm-server-not-json = { $endpoint } no devolvió JSON: { $error }
llm-server-not-model-list = { $endpoint } no devolvió una lista de modelos de OpenAI (falta el campo data)
remix-start = remezclando { $count } { $count ->
    [one] línea
   *[other] líneas
} → { $instruction }
remix-no-llm = remezcla: el LLM no está disponible: { $error }
remix-lines-changed = remezcla: las frases cambiaron mientras se hacía la remezcla; el proyecto no se modificó, vuelve a ejecutarla
frame-empty-ass = ASS vacío: { $error }
frame-read-preview = leyendo el fotograma de vista previa: { $error }
frame-read-source = leyendo el fotograma original: { $error }

## Glossary, hardware, job records, LLM providers

glossary-bad-format = format «{ $format }»: json o tsv
glossary-series-unreadable = no se puede leer el glosario de la serie: { $error }
glossary-no-text = el proyecto no tiene texto: el glosario se crea a partir del habla reconocida, primero analiza
glossary-lines = glosario: { $count } { $count ->
    [one] línea
   *[other] líneas
} de texto
glossary-no-llm = glosario: el LLM no está disponible: { $error }
glossary-failed = glosario: { $error }
glossary-proposed = glosario: { $count } { $count ->
    [one] entrada propuesta
   *[other] entradas propuestas
}
glossary-series-profile-missing = no se encontró el perfil de serie «{ $slug }»; su glosario no se aplicó
glossary-series-slug-unreadable = no se puede leer el glosario de la serie «{ $slug }»: { $error }
glossary-series-applied = glosario de la serie «{ $slug }»: { $count } { $count ->
    [one] entrada
   *[other] entradas
}, { $added } añadidas al proyecto
hw-no-nvidia = no hay GPU NVIDIA
jobs-serialize-record = serializando job.json: { $error }
jobs-record-missing = { $file } no está en { $dir }
llm-llama-server-missing = no se encontró llama-server ({ $path })
llm-gemma-missing = no se encontró el GGUF de Gemma ({ $path })
llm-mmproj-missing = no se encontró el proyector de visión de Gemma (mmproj) ({ $path })
llm-chat-client = cliente de chat: { $error }
llm-local-no-text-model = el servidor local ({ $url }) está elegido para la traducción, pero no hay modelo seleccionado
llm-local-no-vision-model = el servidor local ({ $url }) está elegido para visión, pero no hay modelo seleccionado
llm-local-client = cliente del servidor local: { $error }
llm-local-label = servidor local { $url } · { $model }
llm-openrouter-no-key = OpenRouter está elegido, pero no hay clave
llm-openrouter-no-text-model = OpenRouter está elegido para la traducción, pero no hay modelo seleccionado
llm-openrouter-no-vision-model = OpenRouter está elegido para visión, pero no hay modelo seleccionado
llm-openrouter-not-text = el modelo de OpenRouter { $model } no responde con texto; elige otro para la traducción
llm-openrouter-not-vision = el modelo de OpenRouter { $model } no acepta imágenes; elige un modelo de visión
llm-vision-missing = ninguno ({ $reason })
llm-pair = traducción: { $text }; visión: { $vision }

## Media tools and OCR

common-ffmpeg-exit = ffmpeg terminó con el código { $code }:
    { $tail }
media-ffprobe-start = ffprobe no se pudo iniciar: { $error }
media-ffprobe-exit = ffprobe terminó con el código { $code }: { $stderr }
media-ffprobe-no-streams = ffprobe: no hay streams
media-no-streams = la entrada no tiene ni pista de vídeo ni de audio
media-no-duration = no se pudo determinar la duración
media-no-wav = ffmpeg no creó el wav
media-ffmpeg-hung = ffmpeg no terminó en { $seconds } s y se detuvo (se colgó)
media-ffprobe-duration-exit = ffprobe duration terminó con el código { $code }
media-env-filter-script = script de filtro de envolvente: { $error }
media-no-sample-rate = { $path }: no se leyó la frecuencia de muestreo ({ $stderr })
ocr-no-blur = { $error }; sin desenfoque
ocr-models-missing = no se encontraron los modelos de OCR
ocr-detection-failed = la detección OCR falló ({ $error })
ocr-summary = OCR: { $regions } { $regions ->
    [one] región
   *[other] regiones
}, { $localize } para localizar, { $bands } { $bands ->
    [one] franja
   *[other] franjas
}, sub_y={ $sub_y }
