asr-windowed = transcripción por ventanas ({ $speakers } { $speakers ->
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
downloads-panicked = la descarga se detuvo por un error interno: { $error }
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
llm-agent-label = Agente (MCP)
llm-agent-no-port = el puerto del servidor aún no se conoce; inténtelo de nuevo en un momento
llm-agent-absent = el modelo de lenguaje es el agente conectado, pero ahora no hay ninguno: conecte uno (Ajustes, Agente) o elija otro proveedor de LLM
llm-agent-no-answer = el agente no respondió en { $minutes } min
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
media-unreadable = { $file } no se puede leer como vídeo ni como audio: el archivo está dañado o no se descargó por completo (ffprobe: { $stderr })
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
tts-markup-ignored = el motor de voz no lee esta marca, así que se quitó: { $count } { $count ->
    [one] frase
   *[other] frases
} ({ $lines })
mcp-bad-speaker-count = speaker_count: se espera un número entero de 0 a { $max }
mcp-speakers-without-diarize = speaker_count mayor que 1 no es compatible con diarize=false

## Routes: projects, voices, setup, translation, export, jobs, alignment

project-serialize = serializando project.json: { $error }
project-delete-failed = eliminando el proyecto { $path }: { $error }
project-delete-in-use = un archivo del proyecto está en uso (lo lee la ventana u otro programa): { $error }
project-restore-exists = El proyecto ya está en su sitio.
project-restore-gone = El proyecto no está en la papelera: se vació o no se borró desde aquí.
project-restore-finder = En macOS el proyecto borrado se recupera desde la Papelera en Finder: selecciónalo allí y elige «Devolver».
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
analyze-asr-to-crisperwhisper = idioma «{ $lang }»: Parakeet no lo conoce, reconozco con CrisperWhisper
analyze-asr-no-words = El vídeo tiene { $speech } s de voz y Parakeet oyó { $words } palabras: solo conoce 25 idiomas europeos. Elige el idioma de origen en la ventana en lugar de Auto: para un idioma fuera de Europa el estudio cambia el reconocimiento a Whisper.
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
voices-pack-folder = el paquete de voces va a la carpeta voices del programa, y las voces están ahora en { $path } (DUBENGINE_VOICES)
voices-pack-taken = tomado de { $app }: { $count } { $count ->
    [one] archivo
   *[other] archivos
}
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
render-encode-cpu-mode = el estudio funciona en el procesador, así que el vídeo se codifica en el procesador (x264), más lento que en la tarjeta gráfica
render-encode-cpu-driver = el controlador NVIDIA { $have } es anterior a { $need }, que hace falta para codificar en la tarjeta gráfica, así que el vídeo se codifica en el procesador (x264), más lento. Actualiza el controlador desde el sitio de NVIDIA para codificar en la tarjeta
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
render-higgs-gpu-old = La voz local de Higgs no puede funcionar en { $card }: necesita una NVIDIA desde Maxwell (compute capability { $min }, GTX 900 o más nuevas). Elige la voz «Google · directo» o «Mediante OpenRouter».
render-higgs-driver-old = La voz local de Higgs necesita el controlador NVIDIA { $min } o más nuevo, y este es anterior: { $card }. Actualiza el controlador o elige la voz «Google · directo» o «Mediante OpenRouter».
render-higgs-engine-failed = Higgs no dijo ninguna frase: el motor falló antes de cualquier sonido ({ $error })
render-higgs-no-gpu = La voz local de Higgs necesita una tarjeta NVIDIA con CUDA y no se encontró ninguna ({ $card }). Elige la voz «Google · directo» o «Mediante OpenRouter».
render-defect-runaway = desbocada
render-defect-cutoff = cortada
render-defect-silence = silencio
render-defect-hum = zumbido
render-defect-repeat = un trozo repetido
render-defect-end-burst = un chasquido o una palabra de más al final
render-defect-padding = casi todo silencio
render-defect-whistle = un silbido
render-defect-hiss = ruido en las pausas
render-cloud-defect = ⚠ segmento { $line }: la voz en la nube tiene un defecto ({ $defect }); regenera la frase en el editor
render-voice-gender = ⚠ hablante { $speaker }: su voz es { $want }, pero la del doblaje es { $got }; elige otra voz
render-gender-male = masculina
render-gender-female = femenina
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
render-too-many-artifacts = TTS: demasiados intentos de voz fallidos ({ $in_a_row } frases seguidas, { $retries } reintentos en total); regenerar no ayuda. Causas: { $causes }; la última: { $last }. Detenido en el segmento { $line }.
render-cause-engine = error del motor
render-multi-take = segmento { $line }: varias tomas; se eligió la más cercana al espacio (desviación de { $deviation } s)
render-stretch-over-cap = segmento { $line }: necesita estirarse x{ $needed } (espacio { $slot } s), tope x{ $cap }; el texto va más rápido de lo normal
render-lead-into-silence = empezar en el silencio: { $lines } frases empiezan antes, { $seconds } s en total, en lugar de acelerarse
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

setup-comp-higgs-purpose = Dobla la traducción con las voces de los actores originales
setup-comp-higgs-engine-name = Motor audio.cpp
setup-comp-higgs-engine-purpose = El programa que ejecuta la voz Higgs y el reconocimiento CrisperWhisper
setup-comp-gemma-purpose = Traduce el habla y lee el texto en pantalla
setup-comp-gemma-q5-0-purpose = Traducción y texto en pantalla: más preciso que q4_0
setup-comp-gemma-q6-k-purpose = Traducción y texto en pantalla: aún más preciso
setup-comp-gemma-q8-0-purpose = Traducción y texto en pantalla: el más preciso y el más pesado
setup-comp-parakeet-purpose = Reconoce el habla del vídeo con el tiempo exacto de cada palabra
setup-comp-transcribe-engine-purpose = El motor de reconocimiento transcribe.cpp en la GPU; en él funciona Parakeet
setup-comp-transcribe-engine-cpu-purpose = El motor de reconocimiento transcribe.cpp en el procesador, para un equipo sin NVIDIA; en él funciona Parakeet, más lento
setup-comp-transcribe-engine-cuda12-purpose = El motor de reconocimiento transcribe.cpp en una tarjeta o un controlador NVIDIA antiguos (CUDA 12.9); en él funciona Parakeet
setup-comp-higgs-q6-k-purpose = Voz: variante más ligera que Q8_0
setup-comp-higgs-q4-k-m-purpose = Voz: la variante más ligera
setup-comp-parakeet-ultra-purpose = Reconocimiento de voz: versión afinada, con menos errores
setup-comp-crisperwhisper-purpose = Reconocimiento de voz en 99 idiomas con marcas de tiempo, más allá de los 25 europeos de Parakeet. Licencia: nyra health Non-Commercial (LICENSE.md junto a los pesos)
setup-comp-langid-purpose = Detecta el idioma de la grabación cuando el idioma de origen queda en Automático, para CrisperWhisper
setup-comp-sortformer-name = Quién habla (hasta 8 voces)
setup-comp-sortformer-purpose = Separa las frases por hablante, para que cada uno tenga su voz
setup-comp-roformer-purpose = Separa la voz de la música y el ruido, para conservar el fondo
setup-comp-roformer-q5-purpose = Separación de voz: variante más ligera que Q8_0
setup-comp-roformer-q4-purpose = Separación de voz: la variante más ligera
setup-comp-casting-name = Casting de personajes (caras y voces)
setup-comp-casting-purpose = Reconoce a los personajes por cara y voz, para que cada uno conserve su voz
setup-comp-bsroformer-engine-name = Motor de separación de voz (NVIDIA)
setup-comp-bsroformer-engine-purpose = El programa que separa la voz en la tarjeta gráfica
setup-comp-bsroformer-engine-cpu-name = Motor de separación de voz (procesador)
setup-comp-bsroformer-engine-cpu-purpose = Para un equipo sin NVIDIA: funciona en el procesador, más lento
setup-comp-llama-name = Servidor de traducción (llama.cpp)
setup-comp-llama-purpose = El programa que ejecuta Gemma para traducir y leer el texto en pantalla
setup-comp-onnxruntime-purpose = Necesario para reconocer el habla, el texto en pantalla y a los hablantes
setup-comp-onnxruntime-gpu-purpose = Para que el reconocimiento de voz y de hablantes use la tarjeta gráfica y no el procesador
setup-comp-ffmpeg-purpose = Lee y monta el vídeo y el audio
setup-comp-ytdlp-name = Descarga por enlace (yt-dlp + deno)
setup-comp-ytdlp-purpose = Descargar un vídeo por enlace (YouTube y otros sitios de yt-dlp) en un proyecto nuevo
setup-comp-cuda-runtime-purpose = Bibliotecas de NVIDIA que usan los motores en la tarjeta gráfica; no hace falta instalar CUDA aparte
setup-comp-cudnn-purpose = Necesario para el reconocimiento de voz y de hablantes en la tarjeta gráfica
setup-comp-vcruntime-purpose = Bibliotecas del sistema de Windows para los motores (incluidas)
setup-comp-ocr-name = Texto en pantalla (OCR)
setup-comp-ocr-purpose = Encuentra el texto incrustado en el vídeo para cubrirlo con la traducción (incluido)
setup-comp-nvidia-driver-name = Controlador NVIDIA
setup-comp-nvidia-driver-purpose = Necesario para usar la tarjeta gráfica; se instala desde el sitio de NVIDIA, no desde la app

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
setup-archive-no-dll = el archivo { $path } no contiene bibliotecas
setup-unpack = descomprimiendo { $path }: { $error }
setup-finalize = finalizando { $path }: { $error }

## Settings applied after analysis

post-analyze-bad-vo-gain = vo_gain: se esperaba un número de dB, llegó { $value }
post-analyze-bad-flag = { $name }: se esperaba 0 o 1, llegó { $value }
post-analyze-bad-container = container: se esperaba mp4 o mkv, llegó { $value }
post-analyze-bad-voice-slots = voice_slots: se esperaba un objeto {"{"}male:[…], female:[…]{"}"}
post-analyze-edit-failed = el ajuste tras el análisis { $edit }: { $error }

## Messages of the engines and libraries: glossary, LLM, ASR, translation, separation, faces, TTS, captions, OCR

glossary-over-limit = el glosario tiene { $total } entradas, más de { $max }
glossary-empty-term = la entrada { $entry } no tiene término
glossary-field-too-long = entrada { $entry } («{ $term }»): un campo supera los { $max } caracteres
glossary-duplicate = el término «{ $term }» aparece dos veces (entradas { $first } y { $second })
glossary-tsv-keep = línea { $line }: keep vale «{ $value }»; se esperaba 1 o 0
glossary-tsv-empty-term = línea { $line }: falta el término
glossary-one-of = envía entradas o TSV, no ambos
glossary-nothing = nada que guardar: no hay entradas ni TSV
llm-spawn-failed = llama-server no se inició: { $error }
llm-gguf-missing = no se encontró el modelo GGUF ({ $path })
llm-log-file = registro de llama-server { $path }: { $error }
llm-exited-early = llama-server terminó antes de estar listo ({ $status }); stderr: { $stderr }
llm-not-ready = llama-server no arrancó en { $secs } s (puerto { $port }); stderr: { $stderr }
llm-no-gpu-device = la Gemma local necesita una tarjeta gráfica y la versión de llama.cpp no ve ninguna ({ $detail }). Elige OpenRouter o tu propio servidor para la traducción y la visión, o revisa el controlador de la tarjeta.
llm-http = la solicitud al modelo falló: { $error }
llm-api = API del modelo: { $error }
llm-rejected = la API del modelo rechazó la solicitud ({ $status }): { $body }
llm-empty-answer = el modelo { $model } devolvió una respuesta vacía (sin motivo)
llm-empty-answer-reason = el modelo { $model } devolvió una respuesta vacía (finish_reason={ $reason })
llm-cut-short = el modelo { $model } llegó al límite de { $max_tokens } { $max_tokens ->
    [one] token
   *[other] tokens
} (finish_reason=length): la respuesta está incompleta; probablemente gasta el presupuesto en razonar — elige un modelo sin razonamiento obligatorio
llm-prompt-cut = el servidor leyó solo { $read } { $read ->
    [one] token
   *[other] tokens
} de una solicitud de { $chars } { $chars ->
    [one] carácter
   *[other] caracteres
} y descartó el principio — su contexto es pequeño: aumenta num_ctx en Ollama o el Context Length del modelo en LM Studio
asr-engine = reconocimiento de voz: { $error }
asr-wav-read = no se pudo leer el wav { $path }: { $error }
asr-resample = remuestreo: { $error }
asr-speaker-count = el número de hablantes debe ser de 1 a { $max }
speakers-embedding = voz del hablante { $speaker }: { $error }
speakers-no-sample = hablante { $speaker }: no hay un fragmento de voz de al menos 0,3 segundos para comparar la voz
speakers-too-many-voices = número de hablantes indicado: { $given }, pero el fragmento tiene { $found } { $found ->
    [one] voz distinta
   *[other] voces distintas
}: no se pudieron unir con seguridad por la voz
speakers-bad-embedding = el modelo de voz devolvió rasgos de voz vacíos o no válidos
speakers-dimension-changed = cambió el tamaño de los rasgos de voz
speakers-more-voices = el fragmento tiene más voces que el número de hablantes indicado
speakers-unmatched = no se pudieron emparejar con fiabilidad las voces del fragmento con el número de hablantes indicado
translate-format-json = traducción: el modelo responde con un objeto JSON según el esquema ({ $model })
translate-format-json-probe = traducción: probando respuestas con esquema JSON ({ $model }); si lo rechaza, líneas numeradas
translate-format-numbered = traducción: líneas numeradas; el modelo { $model } no declara structured_outputs en el catálogo de OpenRouter
translate-schema-ignored = traducción: { $model } aceptó el esquema JSON pero respondió con líneas numeradas; a partir de ahora, líneas numeradas
translate-schema-ignored-server = traducción: el servidor aceptó el esquema JSON pero respondió con líneas numeradas; a partir de ahora, líneas numeradas
translate-schema-refused = traducción: { $model } rechazó las respuestas con esquema JSON ({ $status }: { $body }); a partir de ahora, líneas numeradas
translate-schema-refused-server = traducción: el servidor rechazó las respuestas con esquema JSON ({ $status }: { $body }); a partir de ahora, líneas numeradas
translate-line-flawed = traducción: la línea { $line } conserva una traducción con una observación: { $reason }
translate-line-failed = traducción: la línea { $line } no se tradujo: { $reason }
translate-line-reason = { $line }: { $reason }
translate-lines-rejected = traducción: { $bad } de { $total } { $total ->
    [one] línea
   *[other] líneas
} no pasaron la verificación ({ $reasons })
translate-batch-stopped = traducción: falló el bloque de líneas { $first }..{ $last } ({ $error }); la traducción se detuvo
translate-batch-failed = traducción: falló el bloque de líneas { $first }..{ $last } ({ $error })
translate-layout-no-vision = composición del fotograma: omitida (no hay un modelo de visión elegido o disponible)
translate-layout-not-needed = composición del fotograma: omitida (los subtítulos no se incrustan, no hace falta)
translate-layout = composición del fotograma: sub_style={ $sub_style } titles={ $titles } brands={ $brands }
translate-layout-failed = composición del fotograma: omitida ({ $error })
translate-scene-failed = contexto de la escena: omitido ({ $error })
translate-scene-no-vision = contexto de la escena: omitido (no hay un modelo de visión elegido o disponible)
translate-audio-failed = contexto de audio: omitido ({ $error })
translate-context-trimmed = traducción: el bloque de contexto de { $chars } { $chars ->
    [one] carácter
   *[other] caracteres
} se recorta a { $budget } (protección de n_ctx)
translate-names-skipped = traducción: se omite el glosario automático de nombres ({ $error })
translate-chunks = traducción: { $lines } { $lines ->
    [one] línea
   *[other] líneas
} -> { $chunks } { $chunks ->
    [one] bloque
   *[other] bloques
} (glosario: { $terms } { $terms ->
    [one] término
   *[other] términos
}, { $names } { $names ->
    [one] nombre
   *[other] nombres
})
translate-pass-done = traducción: listo, { $translated } traducidas ({ $flawed } con observación), { $untranslated } en el idioma original
glossary-pass = glosario: pasada del modelo { $pass }/{ $passes }
glossary-schema-refused = glosario: el servidor rechazó las respuestas con esquema JSON ({ $status }); se pide JSON como texto
content-type-decided = tipo de contenido: { $decided ->
    [anime] animación
   *[other] imagen real
} ({ $votes } { $votes ->
    [one] respuesta clara
   *[other] respuestas claras
} de { $frames } fotogramas)
line-missing = falta en la respuesta
line-cut = la respuesta se cortó por el límite de tokens
line-untranslated = no está en el idioma de destino
line-echo = repite el original
line-too-short = demasiado corta ({ $got } < { $min })
line-too-long = demasiado larga ({ $got } > { $max })
line-loop = se repite en bucle «{ $gram }»
line-term-missing = falta el término del glosario «{ $term }»
line-gender = el hablante habla de sí mismo en el otro género: «{ $form }»
line-latin = la palabra original «{ $word }» quedó sin traducir
translate-frame = extracción de un fotograma: { $error }
translate-audio = contexto de audio: { $error }
translate-empty = el modelo no tradujo ninguna de { $lines } { $lines ->
    [one] línea
   *[other] líneas
}; la última causa: { $reason }
translate-empty-no-reason = el modelo no tradujo ninguna de { $lines } { $lines ->
    [one] línea
   *[other] líneas
}
translate-contract = la respuesta del modelo: { $problem }
translate-contract-answer = la respuesta del modelo: { $problem }; la respuesta: { $answer }
answer-no-json-object = no contiene un objeto JSON
answer-not-json = no es un JSON válido ({ $error })
answer-no-terms = no contiene la lista terms
remix-failed = remix: { $error }
sep-engine-missing = no se encontró el motor de separación ({ $path })
sep-model-missing = no se encontró el modelo de separación ({ $path })
sep-spawn = inicio del motor de separación: { $error }
sep-engine-failed = el motor de separación falló (código { $code }): { $tail }
sep-engine-killed = el motor de separación se detuvo sin código de salida: { $tail }
sep-no-output = el motor de separación no creó la pista de voz ({ $path })
sep-audio-io = audio de la separación: { $error }
sep-fell-back-to-cpu = la versión de la separación de voz para la tarjeta gráfica no encontró la tarjeta y calculó en el procesador ({ $why }). Elige «CPU» o «Auto» para la separación de voz en los ajustes o revisa el controlador de la tarjeta.
faces-model-missing = no se encontró el modelo ({ $path })
ffmpeg-exit = ffmpeg terminó con el código { $code }: { $tail }
ffmpeg-killed = ffmpeg se detuvo sin código de salida: { $tail }
onnx-runtime = el entorno ONNX: { $error }
files-error = archivos: { $error }
model-no-outputs = el modelo no devolvió salidas
faces-output-shape = { $model }: la salida del modelo tiene una forma inesperada { $shape }
faces-output-count = { $model }: se esperaban { $expected } salidas del modelo, llegaron { $got }
faces-crop-size = la incrustación facial espera un recorte alineado de 112x112, llegó { $width }x{ $height }
faces-sample-rate = el clip de voz debe ser de { $expected } Hz, es de { $got } Hz
faces-clip-too-short = el clip de voz es demasiado corto para analizarlo
tts-library-load = no se pudo cargar el motor de voz: { $error }
tts-cancelled = se canceló la generación
tts-generation = la generación falló: { $error }
tts-invalid-param = parámetro de voz no válido: { $error }
tts-streaming-unsupported = esta DLL del motor no admite streaming
render-higgs-model-failed = carga del modelo Higgs: { $error }
captions-write-ass = escritura del archivo de subtítulos: { $error }
captions-filter-script = escritura del script de filtros de ffmpeg: { $error }
captions-ffmpeg-wait = espera de ffmpeg: { $error }
captions-ffmpeg-timeout = ffmpeg no terminó en { $secs } s y se detuvo (se colgó): { $tail }
captions-burn-failed = falló la incrustación de los subtítulos: { $tail }
captions-nvenc-driver-old = el controlador de NVIDIA es demasiado antiguo para codificar vídeo en la tarjeta gráfica (NVENC): hace falta la versión { $need } o posterior. Actualiza el controlador desde el sitio de NVIDIA y vuelve a renderizar
captions-frame-failed = falló el fotograma de vista previa: { $tail }
ocr-no-dictionary = el modelo de reconocimiento de texto no tiene diccionario de caracteres
ocr-internal = reconocimiento de texto: error interno ({ $error })
update-title = Actualización de Dub Studio
quit-title = ¿Cerrar Dub Studio?
quit-running = El estudio sigue trabajando: { $n } tarea(s) en cola o en curso. Si cierra ahora, se detienen; lo ya hecho queda en el proyecto y el doblaje sigue desde ahí con Continuar.
quit-button-close = Cerrar
quit-button-stay = Mantener abierto
update-open-page = Ya está disponible la versión { $version }. ¿Abrir la página de descarga?
update-install-now = Ya está disponible la versión { $version }. ¿Actualizar ahora? La aplicación se reiniciará.
update-button-open = Abrir
update-button-install = Actualizar
update-button-later = Más tarde
update-open-failed = No se pudo abrir { $url }: { $error }
update-failed = La actualización no se instaló: { $error }
drama-needs-cuda = La voz Drama 3 necesita una tarjeta NVIDIA con CUDA y no se encontró ninguna ({ $card }). Elige Higgs.
drama-needs-vram = Drama 3 necesita una tarjeta gráfica de 6 GB o más, y esta tiene { $gb } GB ({ $card }). Elige Higgs.
drama-needs-ram = En una tarjeta de menos de 24 GB, Drama 3 guarda sus expertos (18 GB) en la RAM: necesita 32 GB de RAM, y este equipo tiene { $gb } GB. Elige Higgs.
render-drama-missing = Drama 3 está elegida pero no descargada por completo: descárgala en Modelos (voz) o elige Higgs.
render-drama-load-failed = cargando Drama 3: { $error }
render-drama-oom = Drama 3 se quedó sin memoria de vídeo ({ $error }): cierra otros programas que usan la tarjeta gráfica o pon una cantidad menor en «VRAM para Drama 3» en los ajustes
render-loading-drama = cargando Drama 3
qwen-needs-cuda = La voz Qwen3-TTS necesita una tarjeta NVIDIA con CUDA y no se encontró ninguna ({ $card }). Elige Higgs.
cpu-floor-missing = { $engine } necesita un procesador con AVX2, FMA, F16C y BMI2 (Intel desde 2013, AMD desde 2017), y a este le falta { $missing }
stage-name-sep = Separación de la voz
stage-name-asr = Reconocimiento de voz (Parakeet)
stage-needs-cuda = { $stage }: los ajustes eligen «GPU (CUDA)», pero { $why }. Elige «CPU» o «Auto» para esta etapa en los ajustes.
stage-gpu-why-no-nvidia = no se encontró el controlador de NVIDIA
stage-gpu-why-no-device = el controlador de NVIDIA está instalado y no ve ninguna tarjeta NVIDIA
stage-gpu-why-cuda-init = el controlador de NVIDIA no responde ({ $card })
stage-gpu-why-driver-old = necesita el controlador de NVIDIA { $min } o más reciente, y hay uno más antiguo ({ $card })
stage-gpu-why-gpu-old = la tarjeta es anterior a Turing, y necesita compute capability { $min } o superior ({ $card })
qwen-no-linux = La voz Qwen3-TTS aún no tiene versión para Linux. Elige Higgs o Supertonic 3.
render-qwen-missing = Qwen3-TTS está elegido pero no instalado por completo: descárgalo en Modelos (voz) o elige Higgs.
render-qwen-load-failed = cargando Qwen3-TTS: { $error }
render-loading-qwen = cargando Qwen3-TTS
render-qwen-voice-other-engine = La voz { $voice } es de Qwen3-TTS y el motor de voz elegido es otro: elige Qwen3-TTS en Modelos (voz) o da otra voz a este personaje.
render-supertonic-missing = Supertonic 3 está elegido pero no instalado: descárgalo en Modelos (locución) o elige otra voz.
render-loading-supertonic = Cargando Supertonic 3…
render-supertonic-load-failed = Supertonic 3 no se cargó: { $error }
render-supertonic-voice-other-engine = La voz { $voice } es de Supertonic 3 y está elegido otro motor de voz: asigna una voz de ese motor o elige Supertonic 3.
render-supertonic-no-voice = Supertonic 3 no clona y { $voice } no es una de sus voces.
render-supertonic-voices = Voces de Supertonic 3 según el género de los hablantes: { $voices }
render-stress-failed = No se marcaron los acentos del texto ruso: { $error }
setup-comp-supertonic3-name = Supertonic 3 (int8)
render-drama-no-ref-text = la referencia de voz no tiene transcripción, y Drama 3 no clona una voz sin ella
setup-comp-drama3-purpose = Dobla la traducción con las voces de los actores originales; segundo motor, para tarjetas gráficas desde 6 GB; con menos de 24 GB necesita 32 GB de RAM
setup-comp-qwen3-tts-purpose = Dobla la traducción con las voces de los actores originales o con nueve voces propias; tercer motor ligero, unos 3 GB de memoria de vídeo en una tarjeta NVIDIA
setup-comp-qwen3-tts-engine-name = Motor de Qwen3-TTS
setup-comp-qwen3-tts-engine-purpose = La biblioteca y el códec de 12 Hz con los que habla Qwen3-TTS, comunes a las dos cuantizaciones
setup-comp-qwen3-tts-ru-stress-name = Acento ruso para Qwen3-TTS
setup-comp-qwen3-tts-ru-stress-purpose = Un pequeño complemento para el doblaje al ruso clonado con Qwen3-TTS: un más antes de una vocal en la frase (z+amok) pone el acento justo ahí; no afecta a las voces propias del modelo
setup-comp-supertonic3-purpose = Supertonic 3: locución rápida con diez voces listas y 31 idiomas, sin clonación
setup-comp-ruaccent-purpose = Marca el acento y la letra ё en textos rusos y distingue homógrafos por el contexto
setup-comp-ruaccent-omograph-big-name = Modelo grande de homógrafos de RUAccent
setup-comp-ruaccent-omograph-big-purpose = El modelo big_poetry de RUAccent en int8 para leer los homógrafos por su sentido (замо́к/за́мок, мука́/му́ка): acierta más que el tiny2 incluido (29 de 33 frente a 24 en las frases de prueba); una vez descargado se usa en su lugar
render-drama-kept-original = ⚠ frase { $line }: la referencia de voz no tiene transcripción (no se reconoció habla en ella), Drama 3 no clona sin ella — se conserva el original
acpp-no-server = El motor audio.cpp no está instalado ({ $path }): descárgalo en Ajustes → Modelos
acpp-no-libraries = Al motor audio.cpp le faltan bibliotecas de CUDA ({ $files }): descarga el entorno de CUDA en Ajustes → Modelos
acpp-no-model = El modelo «{ $model }» del motor audio.cpp no está descargado: descárgalo en Ajustes → Modelos
acpp-error = motor audio.cpp: { $error }
update-why-audiocpp-server = Las voces de Higgs y CrisperWhisper funcionan en el servidor audio.cpp en lugar de la antigua biblioteca: detener corta la frase al instante y los pesos dejan la tarjeta al terminar el doblaje
update-why-transcribe-avx2 = Compilado para cualquier procesador con AVX2: el reconocimiento de voz ya no cierra el estudio en procesadores sin AVX-512
update-why-crisperwhisper = Reconocimiento de otros idiomas en lugar de Faster-Whisper: 99 idiomas con marcas de tiempo por palabra
update-why-langid = Detecta el idioma de la grabación para CrisperWhisper cuando el idioma de origen se deja en «auto»
update-why-cuda12 = Bibliotecas CUDA 12.9 para el servidor audio.cpp en tarjetas y controladores antiguos
update-why-llama = Una versión más reciente de llama.cpp para la traducción
update-why-bsroformer-studio = La compilación propia de los estudios para separar la voz: lista para cada tarjeta NVIDIA compatible sin compilar en el primer inicio, y en Linux funciona en Ubuntu 22.04 y posteriores
update-why-ffmpeg = Una versión más reciente de FFmpeg para leer y montar vídeo y sonido
update-why-ytdlp = Un yt-dlp más reciente con deno: los sitios cambian y los enlaces se descargan con la versión nueva
update-why-faster-whisper-gone = Faster-Whisper se sustituye por CrisperWhisper
update-why-parakeet-onnx-gone = Parakeet funciona con el motor transcribe.cpp; estos modelos ONNX ya no se leen
update-why-higgs-library-gone = El servidor audio.cpp sustituye a la biblioteca de Higgs
update-why-ort-gone = Sustituido por ONNX Runtime 1.28.2
update-why-newer = Esta versión usa otra compilación
update-why-unused = Ya no se usa
