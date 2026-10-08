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

## Hardware presets

common-write = escribiendo { $what }: { $error }
preset-rtx5090-title = RTX 5090 (32 GB)
preset-rtx4090-title = RTX 4090 (24 GB)
preset-top-subtitle = Máxima calidad: los mejores cuantizados en local
preset-gpu16-title = GPU de 16 GB
preset-gpu16-subtitle = Alta calidad (4080/4070 Ti y similares)
preset-gpu12-title = GPU de 12 GB
preset-gpu12-subtitle = Equilibrado (3060/4070 y similares)
preset-gpu8-title = GPU de 8 GB
preset-gpu8-subtitle = Económico: cuantizados ligeros (3060 Ti/4060)
preset-weak-nvidia-cloud-title = NVIDIA modesta + nube
preset-weak-nvidia-cloud-subtitle = Lo pesado (traducción/visión/voz) en OpenRouter, separación y ASR en tu GPU
preset-cloud-title = CPU + nube (sin NVIDIA)
preset-cloud-subtitle = Lo pesado en OpenRouter, lo local en el procesador; funciona sin tarjeta gráfica (se necesita clave)
preset-custom-title = Personalizado
preset-custom-subtitle = Ajustaré cada parámetro a mano
preset-reason-no-gpu = No se encontró GPU NVIDIA: modo «CPU + nube», lo pesado en OpenRouter y lo local en el procesador (más lento, pero funciona)
preset-reason-top-card = Se detectó { $gpu }: cuantizados máximos
preset-reason-by-vram = { $gpu } · { $vram } GB de VRAM: { $preset }
preset-reason-low-vram = { $gpu } · { $vram } GB de VRAM son pocos para modelos locales; la nube es más fiable
preset-unknown = preajuste desconocido: { $id }

## Service port, shortening, frontend, launch defaults

common-corrupt = { $path } está dañado: { $error }
common-create-dir = carpeta { $path }: { $error }
service-bad-port = { $value }: no es un número de puerto (se espera 1..65535)
service-exe-path = ruta del exe del servicio: { $error }
service-request-not-sent = la conexión se aceptó, pero la petición no se envió: { $error }
service-no-health-answer = la conexión se aceptó, pero /health no respondió: { $error }
service-not-http = la respuesta no es HTTP
service-health-status = un servidor HTTP; /health respondió { $code }
service-health-not-dub-studio = un servidor HTTP; /health no respondió con un cuerpo de Dub Studio
service-health-not-json = un servidor HTTP; /health no respondió con JSON
service-health-no-fields = /health se presenta como { $app }, pero sin los campos del servicio: { $error }
service-other-app = otra aplicación ({ $app })
service-health-no-app = un servidor HTTP; /health sin nombre de aplicación
service-port-reserved = el sistema no cede el puerto, aunque nadie acepta conexiones en él (puede estar en un rango reservado por Windows: { $command })
service-port-dub-studio = en él está Dub Studio { $version } ({ $executable })
service-port-other = otro proceso lo ocupa: { $what }
service-port-busy = El puerto 127.0.0.1:{ $port } lleva { $seconds } s ocupado: { $who }. Error: { $error }.

    Cierra el programa que lo ocupa o indica otro puerto con la variable de entorno { $env } (por ejemplo { $env }={ $other_port }).
shorten-line-done = acortando { $n }/{ $total }: { $from } -> { $to } caracteres
shorten-line-rejected = acortando { $n }/{ $total }: la respuesta no se aceptó ({ $reason })
shorten-line-no-answer = acortando { $n }/{ $total }: el LLM no respondió: { $error }
shorten-auto-start = { $count } { $count ->
    [one] frase no cabe
   *[other] frases no caben
} en su espacio; acorto la traducción y doblo solo esas
shorten-higgs-unloaded = Higgs se descargó mientras se acorta la traducción
shorten-auto-no-llm = se omitió acortar la traducción: el LLM no está disponible: { $error }
shorten-none-shortened = acortado: ninguna de las { $count } frases se acortó; quedan como se doblaron
shorten-done = se acortaron { $count } de { $total } { $total ->
    [one] frase
   *[other] frases
}
shorten-nothing = no hay nada que acortar: todas las frases caben en su espacio
shorten-no-llm = acortado: el LLM no está disponible: { $error }
shorten-start = acortando { $count } { $count ->
    [one] frase
   *[other] frases
}: { $provider }
shorten-all-failed = no se pudo acortar: el LLM no respondió a ninguna frase ({ $id }: { $error })
spa-not-built = el frontend no está compilado
settings-bad-speaker-count = speaker_count: se espera un número entero de 0 a { $max } (0 es automático)
settings-bad-vo-gain = vo_gain_db={ $value }: se espera un número de { $min } a { $max } dB
settings-bad-src-lang = src_lang={ $value }: no es un código de idioma ni "auto"
settings-bad-tgt-lang = tgt_lang={ $value }: no es un código de idioma
settings-bad-casting-ref = casting_ref={ $value }: no es el slug de un perfil de casting
settings-style-too-long = tr_style_custom tiene más de { $max } caracteres
settings-too-many-slots = { $name }: más de { $max } espacios
settings-unknown-field = campo de valores de inicio desconocido: { $key }
settings-read-failed = leyendo los valores de inicio: { $error }
settings-patch-not-object = el cuerpo de PATCH /settings/launch es un objeto de campos
settings-write-failed = guardando los valores de inicio: { $error }
