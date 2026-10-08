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

## Casting, recording, voice library

casting-relabel = reetiquetado por voz: { $count } { $count ->
    [one] personaje
   *[other] personajes
} por voz (la diarización encontró { $found })
casting-skipped-no-speech = casting omitido: no hay segmentos de habla
casting-skipped-no-speakers = casting omitido: no hay hablantes
casting-speakers-ranked = personajes que hablan: { $count } (ordenados por tiempo de habla)
casting-faces-bound = caras vinculadas a hablantes: { $count } de { $total }
casting-bit-part-skipped = { $character }: el hablante { $speaker } tiene 1 frase y ni cara ni voz -> omitido (papel menor sin casting)
casting-character-face = { $character }: hablante { $speaker }, { $lines } { $lines ->
    [one] frase
   *[other] frases
}, { $seconds } s de habla, cara: sí
casting-character-no-face = { $character }: hablante { $speaker }, { $lines } { $lines ->
    [one] frase
   *[other] frases
}, { $seconds } s de habla, cara: no
casting-profile-other-type = el perfil es de otro tipo ({ $previous } ≠ { $current }); se omitió el emparejamiento cruzado
casting-cross-episode = entre episodios: nombres/voces trasladados: { $count }
casting-saved = casting.json listo: { $count } { $count ->
    [one] personaje
   *[other] personajes
}
casting-save-failed = no se pudo escribir casting.json: { $error }
casting-faces-collected = caras reunidas: { $faces } ({ $samples } muestras)
casting-face-clusters = identidades faciales (grupos por vector): { $count }
casting-anime-detector-missing = no se encontró el detector de anime ({ $path }); sin avatares
casting-anime-detector-failed = el detector de anime no se cargó: { $error }; sin avatares
casting-scrfd-missing = no se encontró SCRFD; sin avatares (casting por voz)
casting-scrfd-failed = SCRFD no se cargó: { $error }; sin avatares
casting-embedder-missing = no se encontró { $model } ({ $path }); avatares sin embedding
casting-embedder-failed = { $model } no se cargó: { $error }; avatares sin embedding
casting-no-frame = ffmpeg no extrajo el fotograma
casting-voice-no-vocals = embedding de voz omitido: no hay voz limpia
casting-voice-no-model = voz omitida: no está el modelo WeSpeaker ({ $path })
casting-voice-model-failed = WeSpeaker no se cargó: { $error }; voz omitida
casting-voice-trim-failed = muestra de voz { $speaker }: el recorte falló: { $error }
casting-voice-embedding-failed = voz { $speaker }: el embedding falló: { $error }
casting-voice-embeddings = embeddings de voz: { $count }
casting-applying-profile = aplicando el perfil de la biblioteca: { $slug }
casting-library-profile-missing = no se encontró el perfil de la biblioteca «{ $slug }»; no se aplicó nada
record-busy = Ya se está grabando
record-mic-not-found = No se encontró el micrófono «{ $name }»: está desconectado o se renombró; elige otro
record-no-mic = No se encontró ningún micrófono
record-mic-config = configuración del micrófono: { $error }
record-create-wav = creando el wav: { $error }
record-format-unsupported = el formato { $format } no es compatible
record-mic-open = abriendo el micrófono: { $error }
record-mic-start = iniciando el micrófono: { $error }
record-not-recording = No se está grabando
voices-not-file-list = { $page }: no es una lista de archivos: { $error }
voices-list-endless = { $page }: la lista de archivos del dataset no termina
voices-too-large = { $url }: más de los { $size } bytes fijados
voices-size-mismatch = { $url }: llegaron { $got } bytes, se fijaron { $size }
voices-sha-mismatch = { $url }: el SHA-256 { $got } no coincide con el fijado { $want }
voices-bad-name = nombre no válido
voices-not-in-dataset = la voz { $name } no está en el dataset { $dataset }
voices-bad-size = { $name }: el tamaño del catálogo no es válido
voices-no-sha = { $name }: no hay SHA-256 en el catálogo
voices-pack-downloading = descargando el paquete de voces
voices-pack-unpacking = descomprimiendo
voices-open-zip = abriendo el zip: { $error }
voices-create-file = creando { $name }: { $error }
voices-unpack-file = descomprimiendo { $name }: { $error }
voices-pack-done = listo: { $count } { $count ->
    [one] archivo
   *[other] archivos
}

## Downloads by link

url-no-fetch = no existe la descarga { $id }
url-stopped = detenido
url-already-downloading = este enlace ya se está descargando: { $id }
url-probe-not-json = la respuesta de yt-dlp -J no es JSON: { $error }; { $stderr }
url-interrupted = la descarga se interrumpió al cerrarse el estudio; lo descargado está en su carpeta y se reanudará desde ahí
url-not-a-link = «{ $url }» no es un enlace: { $error }
url-not-http = «{ $url }» no es un enlace http(s)
url-bad-subs-lang = «{ $lang }» no es un código de idioma de subtítulos
url-cookies-not-file = { $path } no es un archivo cookies.txt de hasta 1 MB
url-cookies-empty-or-large = cookies.txt está vacío o supera 1 MB
url-thread-failed = el hilo de descarga no se inició: { $error }
url-cookies-gone = el cookies.txt de esta descarga desapareció de su carpeta; vuelve a iniciar la descarga con cookies
url-disk-space = se necesitan unos { $need } MB, hay { $free } MB libres
url-no-subs = el vídeo no tiene subtítulos «{ $lang }» subidos por personas (hay: { $have })
url-no-subs-none = el vídeo no tiene subtítulos «{ $lang }» subidos por personas (no hay ninguno)
url-no-media-file = yt-dlp terminó, pero no hay archivo de vídeo en { $path }: { $stderr }
url-subs-failed = los subtítulos «{ $lang }» no se descargaron ({ $code }): { $error }
url-ytdlp-start = iniciando yt-dlp: { $error }
url-ytdlp-wait = esperando a yt-dlp: { $error }
url-still-downloading = { $id } aún se está descargando
url-cancelled-removed = { $id } se canceló y lo descargado se eliminó: inicia una descarga nueva
url-still-stopping = { $id } aún se está deteniendo; vuelve a intentarlo en un par de segundos
url-cancel-first = { $id } aún se está descargando; cancélalo primero
url-no-subs-file = yt-dlp terminó sin archivo de subtítulos en { $path }: { $stderr }
url-no-parent = { $path } no tiene carpeta superior
url-subs-empty = los subtítulos { $path } no tienen ninguna frase
url-ytdlp-missing = el componente ytdlp no está descargado
url-bad-quality = calidad «{ $quality }»: best, 1080, 720, 480 o audio

## yt-dlp

ytdlp-update-missing = falta la actualización de yt-dlp { $version } ({ $path }); se usa la fijada { $pinned }
ytdlp-http-client = cliente HTTP: { $error }
ytdlp-tag-not-version = la versión de yt-dlp con la etiqueta «{ $tag }» no es una versión
ytdlp-no-checksum = { $url } no incluye { $file }
ytdlp-new-says = el nuevo yt-dlp { $tag } se presenta como «{ $said }»; se mantiene { $current }
ytdlp-new-failed = el nuevo yt-dlp { $tag } no se inició: { $error }; se mantiene { $current }
ytdlp-too-large = { $url }: más de { $limit } bytes
ytdlp-sha-mismatch = { $url }: el SHA-256 { $got } no coincide con SHA2-256SUMS { $want }
ytdlp-exit-code = código de salida { $code }: { $stderr }
ytdlp-component-missing = el componente ytdlp (yt-dlp y deno) no está descargado
ytdlp-no-ffmpeg = no hay ffmpeg: ni el componente ffmpeg ni ffmpeg.exe en el PATH
ytdlp-start = iniciando { $program }: { $error }
ytdlp-timeout = yt-dlp no respondió en { $seconds } s: { $stderr }
ytdlp-playlist = el enlace es una lista o un canal ({ $count } { $count ->
    [one] vídeo
   *[other] vídeos
}), no un solo vídeo
ytdlp-redirect-only = el enlace no tiene el vídeo en sí, solo otro enlace
ytdlp-live = una emisión en directo ({ $status })
ytdlp-thumbnail-not-image = la miniatura no es una imagen ({ $mime })
ytdlp-thumbnail-too-large = la miniatura supera los 4 MB
ytdlp-failed-silently = yt-dlp falló sin mensaje

## Render: voicing, QC, mix, mux

render-input = entrada { $width }x{ $height } dur={ $duration }s
render-nodub-original = nodub: la pista de audio original
render-done-audio = listo (solo audio) -> { $path }
render-building-ass = generando el ASS (títulos + subtítulos doblados)
render-burning = incrustando los subtítulos + desenfoque (ffmpeg + libass, NVENC)
render-burn-off = subtítulos/títulos desactivados (subs.burn=off)
render-muxing = multiplexando vídeo + audio
render-track-dub = { $lang } (doblaje)
render-track-original = { $lang } (original)
render-two-tracks = dos pistas: { $dub } + { $original } -> { $container }
render-multitrack-failed = el multiplexado multipista falló ({ $error }) -> una pista
render-subtitle-tracks = subtítulos como pistas mkv: { $tracks }
render-subtitle-tracks-failed = subtítulos como pistas mkv: { $error }
render-mp4-companion-failed = no se generó el mp4 complementario ({ $error }); el reproductor abrirá el mkv (VLC funciona)
render-done = listo -> { $path }
render-dub-audio-done = el audio doblado está listo
render-synth-thread-ended = el hilo de síntesis terminó sin resultado
render-synth-timeout = la síntesis superó { $seconds } s; cancelada, el motor está libre
render-engine-stuck = la síntesis no se cancela tras { $seconds } s; el render se detuvo (el motor se colgó en la DLL)
render-higgs-load-failed = cargando la DLL de Higgs: { $error }
render-defect-runaway = desbocada
render-defect-cutoff = cortada
render-defect-silence = silencio
render-defect-hum = zumbido
render-recognition-no-answer = el reconocimiento no devolvió respuesta
render-second-pass-overflow = la segunda pasada de voz pidió un acortado que en ella está desactivado
render-no-translated-lines = no hay frases traducidas -> silencio, la pista original
render-takes-of-removed-lines = historiales de tomas de frases eliminadas borrados: { $count }
render-extracting-audio = extrayendo el audio (ffmpeg 44.1k estéreo)
render-separator-missing = no se encontró el motor de separación -> sin fondo (keep_music off)
render-ref-from-mix = la referencia del clon sale de la mezcla sin separar: en ella también suena el fondo original
render-emotion-ref-failed = segmento { $segment }: no se recortó la referencia emocional ({ $error }); se usa la referencia de identidad del hablante
render-cloud-voices = voces en la nube por hablante: { $voices }
render-synth-keys-reset = { $error }; las claves de síntesis empiezan de nuevo
render-synthesizing = sintetizando { $count } de { $total } { $total ->
    [one] segmento
   *[other] segmentos
}
render-voicing-cached = voz de la caché: { $count } { $count ->
    [one] segmento
   *[other] segmentos
}
render-cloud-tts-parallel = TTS en la nube: { $count } { $count ->
    [one] segmento
   *[other] segmentos
} en { $threads } hilos paralelos
render-cloud-tts-ready = TTS en la nube: síntesis previa lista ({ $count } { $count ->
    [one] segmento
   *[other] segmentos
})
render-takes-quarantined = { $error }; el historial de tomas de la frase { $line } se apartó en { $path } y empieza de nuevo
render-pinned-take = frase { $line }: suena la toma fijada; la nueva voz no la reemplaza
render-take-unpinned = frase { $line }: la toma se desfijó porque cambió el texto de la frase
render-selected-take = frase { $line }: suena la toma elegida { $take }; la nueva voz no la reemplaza
render-write-cloud-segment = escribiendo el seg{ $line } de la nube: { $error }
render-cloud-tts-failed = ⚠ segmento { $line }: el TTS en la nube falló ({ $error }); se mantiene el original
render-loading-higgs = cargando Higgs
render-failures-kept-generated = ⚠ segmento { $line }: { $attempts } fallos de síntesis ({ $error }); se usa la voz generada (rango { $range } dB)
render-failures-kept-original = ⚠ segmento { $line }: { $attempts } fallos/tiempos agotados de síntesis ({ $error }); se mantiene la frase original
render-regenerating = segmento { $line }: { $error }; regenerando ({ $attempt }/{ $attempts })
render-defects-kept-generated = ⚠ segmento { $line }: los { $attempts } intentos tienen un defecto ({ $defect }); se usa la voz generada (rango { $range } dB)
render-silent-kept-original = ⚠ segmento { $line }: { $attempts } intentos sin sonido; se usa el original
render-retry-alt-ref = otra referencia
render-retry-temperature = más temperatura
render-defect-regenerating = segmento { $line }: defecto de síntesis ({ $defect }); regenerando ({ $via } { $attempt }/{ $attempts })
render-write-segment = escribiendo seg{ $line }: { $error }
render-too-many-artifacts = TTS: demasiados artefactos de zumbido ({ $in_a_row } seguidos, { $retries } reintentos en total); regenerar no ayuda. Probablemente el problema es el equipo (modelo/VRAM) o los clips de referencia de voz. Detenido en el segmento { $line }.
render-multi-take = segmento { $line }: varias tomas; se eligió la más cercana al espacio (desviación de { $deviation } s)
render-stretch-over-cap = segmento { $line }: necesita estirarse x{ $needed } (espacio { $slot } s), tope x{ $cap }; el texto va más rápido de lo normal
render-silence-trimmed = recorte de silencio del TTS: { $seconds } s quitados de { $lines } { $lines ->
    [one] frase
   *[other] frases
} ({ $pauses } s de pausas); gracias al recorte la aceleración quedó dentro del tope en { $into_cap } { $into_cap ->
    [one] frase
   *[other] frases
}
render-fit-summary = ajuste: { $over }/{ $total } segmentos por encima del tope ({ $share }%)
render-fit-summary-drift = ajuste: { $over }/{ $total } segmentos por encima del tope ({ $share }%), sincronía recuperada en { $drift }
render-qc-start = QC: comprobando { $count } { $count ->
    [one] frase
   *[other] frases
} por transcripción
render-qc-unheard = QC: { $count } de { $total } frases no se comprobaron; el reconocimiento falló: { $reason }
render-qc-mismatch = QC: { $count } { $count ->
    [one] frase no coincide
   *[other] frases no coinciden
} con la traducción; se vuelve a sintetizar
render-qc-resynthesized = QC: segmento { $line } sintetizado de nuevo (intento { $attempt })
render-qc-unconfirmed = ⚠ QC: no se pudo confirmar el segmento { $line } («{ $text }»); revisa la frase a mano
render-qc-kept-mismatch = ⚠ QC: el segmento { $line } no coincide con el texto traducido; se mantiene la voz generada
render-qc-resynth-unheard = QC: { $count } { $count ->
    [one] frase sintetizada de nuevo no se comprobó
   *[other] frases sintetizadas de nuevo no se comprobaron
}; el reconocimiento falló: { $reason }
render-qc-summary = resultado de QC: { $fixed }/{ $total } corregidas, { $flagged } siguen marcadas, { $unheard } sin comprobar
render-qc-all-confirmed = QC: todas las frases se confirmaron por transcripción ✓
render-qc-rest-confirmed = QC: el resto de frases se confirmaron por transcripción
render-laying-out = colocando el doblaje en la línea de tiempo
render-peak-limiter = limitador de picos: { $lines } { $lines ->
    [one] frase
   *[other] frases
}, { $samples } muestras por encima del techo { $ceiling } bajadas sin recortar
render-tempo-fit = ajuste de tempo de toda la pista x{ $factor }
render-voiceover-envelope = voz superpuesta: el original a { $db } dB BAJO la traducción, completo en las pausas (envolvente dinámica, { $blocks } bloques)
render-voiceover-flat = voz superpuesta: la envolvente no está disponible -> atenuación plana
render-mix-no-ducking = mezcla: instrumental + voz doblada (ducking DESACTIVADO, fondo completo)
render-mix-ducking = mezcla: instrumental + voz doblada (ducking ACTIVADO, envolvente, { $blocks } bloques)
render-mix-sidechain = la envolvente no está disponible -> ducking por sidechain
render-mix-plain = el sidechain no está disponible -> mezcla directa
render-loudness-off = la igualación de volumen está desactivada: la mezcla tal cual
render-loudness-normalizing = normalizando el volumen (EBU R128, true peak)
render-loudnorm-skipped = loudnorm omitido ({ $error })
render-track-gain = ganancia de la pista { $db } dB
render-dub-timings-not-written = no se escribieron los tiempos del doblaje para los subtítulos: la colocación ({ $lines } frases, { $spans } tramos) no coincidió con los segmentos ({ $segments }); los subtítulos siguen los tiempos originales
render-dub-timing-mismatch = tiempos del doblaje: la frase { $line } de la vista de síntesis no coincide con el segmento n.º { $index } del proyecto
render-word-timings = tiempos por palabra de los subtítulos: reconociendo { $count } { $count ->
    [one] frase
   *[other] frases
} del doblaje
render-refs-unchecked = comprobación de referencias: { $count } { $count ->
    [one] candidata aceptada
   *[other] candidatas aceptadas
} sin comprobar; el reconocimiento falló: { $error }
render-refs-all-failed = ⚠ hablante { $speaker }: ninguna referencia candidata pasó la comprobación (se oye: «{ $heard }»); se toma la mejor por puntuación
render-speaker-ref-failed = referencia del hablante { $speaker }: { $error }
render-speaker-ref = referencia del hablante { $speaker }: «{ $text }» ({ $seconds } s, { $candidates } { $candidates ->
    [one] candidata
   *[other] candidatas
}, comprobación ok)
render-speaker-ref-unchecked = referencia del hablante { $speaker }: «{ $text }» ({ $seconds } s, { $candidates } { $candidates ->
    [one] candidata
   *[other] candidatas
}, comprobación ⚠ fallida)

## Setup: components

setup-comp-higgs-purpose = Síntesis del doblaje y clonación de voz (TTS)
setup-comp-higgs-engine-name = Motor Higgs (audiocpp_engine.dll)
setup-comp-higgs-engine-purpose = Motor TTS nativo de Higgs (C ABI)
setup-comp-gemma-purpose = Traducción y orquestador de visión de subtítulos/títulos
setup-comp-gemma-q5-0-purpose = Traducción y visión, más precisa que q4_0
setup-comp-gemma-q6-k-purpose = Traducción y visión, aún más precisa
setup-comp-gemma-q8-0-purpose = Traducción y visión, máxima precisión
setup-comp-parakeet-purpose = Reconocimiento del habla con marcas de tiempo por palabra (ASR)
setup-comp-higgs-q6-k-purpose = Síntesis del doblaje y clonación de voz (TTS), más ligera que Q8_0
setup-comp-higgs-q4-k-m-purpose = Síntesis del doblaje y clonación de voz (TTS), la variante más ligera
setup-comp-parakeet-fp32-purpose = Reconocimiento del habla (ASR), precisión fp32 completa
setup-comp-parakeet-ultra-purpose = Reconocimiento del habla (ASR), la versión afinada de Moondream con menos errores
setup-comp-whisper-engine-name = Whisper-Faster (motor ASR)
setup-comp-whisper-engine-purpose = Un motor alternativo de reconocimiento del habla (faster-whisper) en lugar de Parakeet
setup-comp-whisper-cuda-name = Aceleración CUDA de Whisper (cuBLAS + cuDNN)
setup-comp-whisper-cuda-purpose = Inferencia de Whisper en la GPU (si no, el reconocimiento va en la CPU, varias veces más lento)
setup-comp-whisper-tiny-name = Whisper tiny (modelo ASR)
setup-comp-whisper-tiny-purpose = ASR Whisper, el modelo más ligero y rápido
setup-comp-whisper-base-name = Whisper base (modelo ASR)
setup-comp-whisper-base-purpose = ASR Whisper, un modelo ligero, más preciso que tiny
setup-comp-whisper-small-name = Whisper small (modelo ASR)
setup-comp-whisper-small-purpose = ASR Whisper, un modelo equilibrado
setup-comp-whisper-medium-name = Whisper medium (modelo ASR)
setup-comp-whisper-medium-purpose = ASR Whisper, alta precisión
setup-comp-whisper-large-v3-name = Whisper large-v3 (modelo ASR)
setup-comp-whisper-large-v3-purpose = ASR Whisper, máxima precisión (large-v3)
setup-comp-whisper-large-v3-turbo-name = Whisper large-v3-turbo (modelo ASR)
setup-comp-whisper-large-v3-turbo-purpose = ASR Whisper, casi large-v3 pero bastante más rápido (turbo)
setup-comp-sortformer-name = Nemotron 3 Diarization (hasta 8 hablantes)
setup-comp-sortformer-purpose = Separación de hablantes (quién habla y cuándo), hasta 8 voces
setup-comp-roformer-purpose = Modelo de separación de voz/instrumental
setup-comp-roformer-q5-purpose = Separación, más ligera que Q8_0
setup-comp-roformer-q4-purpose = Separación, la variante más ligera
setup-comp-casting-name = Modelos de casting de personajes (caras + voz)
setup-comp-casting-purpose = Detección/embedding de caras (reales + anime) + embedding de voz para el casting
setup-comp-bsroformer-engine-name = Motor BSRoformer.cpp (CUDA)
setup-comp-bsroformer-engine-purpose = Motor nativo de separación (bs_roformer-cli + ggml-CUDA)
setup-comp-bsroformer-engine-cpu-name = Motor BSRoformer.cpp (CPU)
setup-comp-bsroformer-engine-cpu-purpose = Separación en el procesador, el modo sin NVIDIA (más lenta, función completa)
setup-comp-llama-name = Servidor llama.cpp (CUDA 13.4)
setup-comp-llama-purpose = Servidor auxiliar para Gemma (traducción/visión)
setup-comp-onnxruntime-purpose = Entorno de ejecución de ASR/OCR/diarización (estrictamente 1.28.x)
setup-comp-onnxruntime-gpu-purpose = Proveedor CUDA para la diarización/Parakeet en la GPU (modo local_backend=gpu)
setup-comp-ffmpeg-purpose = Decodificación/codificación de vídeo y audio (NVENC)
setup-comp-ytdlp-name = Descarga por enlace (yt-dlp + deno)
setup-comp-ytdlp-purpose = Descargar un vídeo por enlace (YouTube y otros sitios de yt-dlp) en un proyecto nuevo
setup-comp-cuda-runtime-purpose = DLL de CUDA redistribuibles para los motores y el CUDA EP de onnxruntime (sin CUDA Toolkit)
setup-comp-cudnn-purpose = Lo necesita el proveedor CUDA de onnxruntime para la diarización/Parakeet en la GPU
setup-comp-vcruntime-purpose = DLL del sistema de los motores (incluidas)
setup-comp-ocr-name = Modelos OCR (PP-OCR ONNX)
setup-comp-ocr-purpose = Detección de texto incrustado → desenfoque (incluidos)
setup-comp-nvidia-driver-name = Controlador NVIDIA
setup-comp-nvidia-driver-purpose = Aceleración por GPU (se instala aparte, no la instala la aplicación)

## Setup: downloads and installation

setup-http-status = { $url }: estado { $status }
setup-write = escribiendo: { $error }
setup-not-zip = no es un zip: { $error }
setup-zip-entry = entrada del zip: { $error }
setup-open = abriendo { $path }: { $error }
setup-verifying = Comprobando el SHA-256 de { $file }…
setup-install-record = el registro de instalación: { $error }
setup-disk-space = no hay espacio suficiente: se necesitan { $need } GB, hay { $free } GB libres ({ $path })
setup-unknown-component = no existe el componente { $id }
setup-not-removable = { $id } no lo instala la aplicación
setup-component-busy = el componente se está descargando; pausa la descarga
setup-paused = la descarga está en pausa
setup-rate-limited = { $url }: el servidor lleva { $minutes } min respondiendo { $status }
setup-proxy-scheme = proxy { $proxy }: el esquema { $scheme } no sirve para descargas (http, https, socks4, socks5)
setup-proxy-no-host = proxy { $proxy }: falta el host
setup-proxy-no-port = proxy { $proxy }: falta el puerto
setup-proxy-credentials = proxy { $proxy }: las descargas de modelos (ureq) no pueden pasar al proxy ese usuario o contraseña: contiene / ? #, un espacio, caracteres no ASCII o (en SOCKS5) dos puntos en la contraseña; las peticiones a la nube por este proxy funcionan, para descargar hace falta una contraseña sin esos caracteres
setup-start-failed = { $url }: no se pudo empezar en { $retries } intentos: { $error }
setup-chunk-manifest = el manifiesto de fragmentos: { $error }
setup-range-incomplete = rango incompleto: { $got }/{ $want } bytes
setup-range-failed = rango { $start }-{ $end } tras { $retries } intentos: { $error }
setup-range-status = rango { $start }-{ $end }: estado { $status } (se esperaba 206)
setup-range-read = leyendo el rango: { $error }
setup-create = creando { $path }: { $error }
setup-read = leyendo: { $error }
setup-download-failed = { $url } tras { $retries } intentos: { $error }
setup-size-mismatch = { $file }: se descargaron { $got } bytes, se fijaron { $want }; el archivo se eliminó y el próximo intento empezará de cero
setup-hash-mismatch = { $file }: el SHA-256 { $got } no coincide con el fijado { $want }; el archivo se eliminó y el próximo intento empezará de cero
setup-unpacking = Descomprimiendo { $file }…
setup-rename = renombrando { $path }: { $error }
setup-waiting-other = Esperando otra descarga de los mismos componentes…
setup-delete = eliminando { $path }: { $error }
setup-downloading = Descargando los modelos…
setup-source-changed = { $url }: el servidor entrega { $got } bytes, se fijaron { $want }; la fuente cambió
setup-manifest-write = manifiesto { $path }: { $error }
setup-archive-no-files = el archivo { $path } no contiene los archivos necesarios
setup-wheel-not-zip = el wheel no es un zip: { $error }
setup-wheel-entry = entrada del wheel: { $error }
setup-archive-no-dll = el archivo { $path } no contiene DLL
setup-unpack = descomprimiendo { $path }: { $error }
setup-finalize = finalizando { $path }: { $error }

## Settings applied after analysis

post-analyze-bad-vo-gain = vo_gain: se esperaba un número de dB, llegó { $value }
post-analyze-bad-flag = { $name }: se esperaba 0 o 1, llegó { $value }
post-analyze-bad-container = container: se esperaba mp4 o mkv, llegó { $value }
post-analyze-bad-voice-slots = voice_slots: se esperaba un objeto {"{"}male:[…], female:[…]{"}"}
post-analyze-edit-failed = el ajuste tras el análisis { $edit }: { $error }
