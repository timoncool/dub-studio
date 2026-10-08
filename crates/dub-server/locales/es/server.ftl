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

## Takes, translation, TTS text, MCP arguments

takes-delete-old = eliminando la toma anterior { $path }: { $error }
takes-missing = la toma { $take } no está en el historial de la frase { $line }
translate-no-vision = no se determinó el tipo de contenido: la visión no está disponible ({ $error })
translate-subs-already-translated = los subtítulos ya están en el idioma de destino -> sin traducción automática (solo voz)
translate-transcribe-only = transcribir: tgt=el texto original, sin traducción
translate-same-language = mismo idioma -> sin traducción automática (tgt=el original)
translate-no-llm = la traducción falló: el LLM no está disponible: { $error }
translate-ctx-pass = pasada de contexto: visión de composición/escena + traducción de la transcripción
translate-failed = la traducción falló: { $error }
translate-untranslated = la traducción falló: { $left } de { $total } { $total ->
    [one] línea
   *[other] líneas
} quedaron en el idioma original (detalles en el registro y en logs/llama-server.log)
translate-untranslated-auto = la traducción falló: { $left } de { $total } { $total ->
    [one] línea
   *[other] líneas
} quedaron en el idioma original; si el habla del vídeo ya está en el idioma de destino, indica el idioma original y no hará falta traducir (detalles en el registro y en logs/llama-server.log)
translate-done = traducción lista: { $done }/{ $total } líneas, títulos={ $titles }
translate-left-untranslated = { $left } de { $total } { $total ->
    [one] línea
   *[other] líneas
} quedaron en el idioma original (detalles en logs/llama-server.log)
translate-coverage-retry = cobertura de la traducción: { $count } { $count ->
    [one] línea
   *[other] líneas
} sin traducir; las vuelvo a traducir
translate-coverage-failed = cobertura de la traducción: la nueva traducción falló ({ $error })
translate-coverage-left = cobertura de la traducción: quedan { $count } sin traducir
tts-silent-after-cleanup = no queda habla tras limpiar el texto, así que silencio: { $count } { $count ->
    [one] frase
   *[other] frases
} ({ $lines })
mcp-bad-speaker-count = speaker_count: se espera un número entero de 0 a { $max }
mcp-speakers-without-diarize = speaker_count mayor que 1 no es compatible con diarize=false

## Routes: projects, voices, setup, translation, export, jobs, alignment

project-serialize = serializando project.json: { $error }
project-delete-failed = eliminando el proyecto { $path }: { $error }
setup-no-ids = ids está vacío
setup-fetching-missing = Descargando los modelos que faltan para esta función…
setup-diarization-missing = El modelo de diarización no se descargó ({ $error }); el análisis seguirá sin separar a los hablantes
setup-pick-model-files = Archivo(s) del modelo
setup-pick-models-folder = Carpeta con modelos listos
setup-no-folder = no existe la carpeta { $path }
voices-not-found = no se encontró la voz { $name } en voices/
voices-no-vocals = no hay voz para medir F0; primero analiza
voices-speakers-changed = voces por espacios: los hablantes cambiaron mientras se medía la voz; el proyecto no se modificó, ejecútalo de nuevo
analyze-args-not-object = argumentos de analyze: se esperaba un objeto
cost-analyze = OpenRouter: ${ $spent } gastados en el análisis (${ $total } usados en total)
cost-run = OpenRouter: ${ $spent } gastados en la ejecución (${ $total } usados en total)
translate-lines-to = Traduciendo { $count } { $count ->
    [one] línea
   *[other] líneas
} → { $lang }
translate-note = traducción: { $note }
translate-titles-failed = los títulos no se tradujeron ({ $error }); en el vídeo quedarán en el idioma original
translate-titles-error = traduciendo los títulos: { $error }
translate-lines-changed = traducción: las frases o los títulos cambiaron durante la traducción; el proyecto no se modificó, vuelve a traducir
export-pick-folder = Dónde guardar los resultados
export-copy-failed = copiando a { $path }: { $error }
jobs-wait-not-number = wait: se esperaba un número de segundos
jobs-unknown-kind = tipo de tarea desconocido en job.json: { $kind }
jobs-not-resumable = una tarea { $kind } no se puede reanudar
align-no-source-text = alineación con el habla: las frases no tienen texto en el idioma original (los subtítulos se importaron en el idioma de destino)
align-no-vocals = alineación con el habla: el proyecto no tiene pista de voz; primero analiza
align-recognizing = alineación con el habla: reconociendo las palabras
align-recognition-failed = alineación con el habla: reconocimiento: { $error }
align-no-speech = alineación con el habla: no se reconoció habla; los tiempos no se cambiaron
align-mismatch = alineación con el habla: las frases no coincidieron con el habla ({ $share }% emparejado); los tiempos no se cambiaron
align-lines-changed = alineación con el habla: las frases cambiaron durante el reconocimiento; los tiempos no se cambiaron, vuelve a alinear
align-done = alineado con el habla: { $share }% de las frases por palabras, tiempos cambiados en { $changed }; desfase { $offset } s

## Analysis

analyze-serialize = serializando { $what }: { $error }
analyze-read-model = leyendo el modelo { $path }: { $error }
analyze-stage-cache-unreadable = no se puede leer la caché de la etapa { $stage } ({ $error }); se recalcula
analyze-checkpoint-not-saved = no se guardó el punto de control de la etapa { $stage }: { $error }
analyze-diarize-continuous = diarización: se esperan { $speakers } { $speakers ->
    [one] hablante
   *[other] hablantes
}, una pasada continua por toda la grabación sin reiniciar etiquetas en los límites de cada hora
analyze-diarize-fragments = diarización: se esperan { $speakers } { $speakers ->
    [one] hablante
   *[other] hablantes
}, emparejando voces entre fragmentos
analyze-wespeaker-needed = WeSpeaker es necesario para un número fijo de hablantes: { $error }
analyze-align-skipped = alineación con el habla omitida: los subtítulos están en el idioma de destino y el habla en el original
analyze-align-recognizing = alineando los subtítulos con el habla: reconociendo las palabras
analyze-align-recognition-failed = alineando los subtítulos: reconocimiento del habla: { $error }
analyze-align-no-speech = alineación con el habla: no se reconoció habla; se mantienen los tiempos del archivo
analyze-align-mismatch = los subtítulos no coincidieron con el habla ({ $share }% de las frases emparejadas); se mantienen los tiempos del archivo
analyze-align-done = subtítulos alineados con el habla: { $share }% de las frases por palabras, el resto desplazado con sus vecinas; desfase del archivo { $offset } s
analyze-more = { $count } más
analyze-window-plan = pista larga de { $duration } s: plan de { $windows } { $windows ->
    [one] ventana
   *[other] ventanas
} (la primera ~{ $first } s); el ASR por ventanas aún no está activo, se procesa de una vez
analyze-audio-cached = audio de la caché (la fuente no cambió); se omite ffmpeg
analyze-extracting-audio = extrayendo el audio (ffmpeg -> 16k mono)
analyze-stems-stale = los stems se hicieron con una extracción anterior; se separa de nuevo
analyze-separating = separando la voz ({ $model }): voz limpia para la diarización/ASR
analyze-separation-cached = separación de la caché (los stems ya están hechos)
analyze-separation-failed = la separación falló ({ $error }); diarización/ASR sobre el audio original
analyze-separator-missing = no se encontró { $model }; diarización/ASR sobre el audio original
analyze-diarizing = diarización ({ $model })
analyze-diarization-cached = diarización de la caché
analyze-diarization-count-failed = diarización con un número fijo de hablantes: { $error }
analyze-diarization-failed = la diarización falló ({ $error }); se sigue con un solo hablante
analyze-diarization-model-missing-count = no se encontró el modelo { $model }: no se puede aplicar el número fijo de hablantes
analyze-subs-no-diarization = subtítulos: sin diarización (todo el clip de una vez)
analyze-diarization-model-missing = no se encontró el modelo de diarización; se sigue con un solo hablante
analyze-fewer-speakers = se esperaban { $expected } { $expected ->
    [one] hablante
   *[other] hablantes
}, se distinguieron { $found }: no se añadieron las voces que faltan
analyze-speakers-matched = voces emparejadas entre fragmentos: { $speakers } { $speakers ->
    [one] hablante
   *[other] hablantes
}
analyze-transcript-cached = transcripción de la caché: { $segments } { $segments ->
    [one] segmento
   *[other] segmentos
}
analyze-read-subs = leyendo los subtítulos { $path }: { $error }
analyze-subs-empty = los subtítulos no se reconocieron o están vacíos: { $path }
analyze-subs-imported = subtítulos importados: { $lines } { $lines ->
    [one] frase
   *[other] frases
}, { $speakers } { $speakers ->
    [one] hablante
   *[other] hablantes
}
analyze-cloud-asr = transcribiendo en la nube (OpenRouter STT)
analyze-cloud-stt-failed = STT en la nube: { $error }
analyze-cloud-done = nube: { $lines } { $lines ->
    [one] frase
   *[other] frases
}, { $speakers } { $speakers ->
    [one] hablante
   *[other] hablantes
}
analyze-hallucination-filter = filtro de alucinaciones del ASR: { $error }
analyze-hidden-hallucinations = frases alucinadas del ASR ocultas (sin voz): { $count }: { $lines }
analyze-hidden-by-text = títulos y sonidos del ASR ocultos por su texto (hay sonido en el intervalo, la voz no está separada de la música): { $count }: { $lines }
analyze-voiced-suspects = parecen alucinaciones, pero hay voz; se mantienen marcadas: { $count }: { $lines }
analyze-merged-fragments = fusionando fragmentos: { $before } -> { $after } segmentos
analyze-characters-by-voice = personajes por voz: { $count }
analyze-segments-speakers = { $segments } { $segments ->
    [one] segmento
   *[other] segmentos
}, { $speakers } { $speakers ->
    [one] hablante
   *[other] hablantes
}
analyze-project-unparsable = no se puede analizar project.json ({ $error }): contiene el glosario del proyecto, así que el análisis se detuvo para no perderlo
analyze-glossary-fixed = glosario: { $count } { $count ->
    [one] error corregido
   *[other] errores corregidos
} en el reconocimiento de términos
analyze-no-speech-nodub = no hay segmentos de habla; se mantiene la pista original (nodub)
analyze-auto-nodub = auto: no hay habla apta para doblaje -> NODUB (el original + texto en pantalla localizado)
analyze-casting-style = descripciones de personajes del perfil de casting -> estilo de traducción ({ $chars } caract.)
analyze-empty-removed = segmentos sin palabras eliminados: { $count }
analyze-translation-cached = traducción de la caché
analyze-ocr-cached = detección de texto en pantalla de la caché
analyze-audio-no-ocr = modo audio: sin vídeo, no hace falta detectar texto en pantalla
analyze-ocr-off = la detección de texto incrustado está desactivada (la casilla)
analyze-content-type = tipo de contenido (auto): { $kind }
analyze-casting-cached = casting de la caché
analyze-casting-checkpoint = no se guardó el punto de control del casting: { $error }
analyze-profile-voices-missing = no se encontraron las voces del perfil en voices/ -> clon: { $voices }
analyze-profile-voices-applied = las voces del perfil se aplicaron al doblaje ({ $count } { $count ->
    [one] personaje
   *[other] personajes
})
analyze-audio-no-casting = modo audio: sin vídeo, no hace falta el casting de personajes
analyze-cache-not-saved = no se pudo guardar cache.json: { $error } (no es grave)
