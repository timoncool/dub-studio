asr-windowed = transcrição por janelas na GPU ({ $speakers } { $speakers ->
    [one] falante
   *[other] falantes
})

## Separation, OCR, benchmark, casting library, cloud ASR and TTS

atomic-extract-separation-audio = extraindo áudio 44,1k para a separação
atomic-separating = separando ({ $model })
atomic-separation-failed = separação: { $error }
ocr-detecting-burned-text = detectando texto embutido ({ $model })
bench-stage = ⏱ { $name }: { $seconds } s | GPU ~{ $gpu_avg }% (pico { $gpu_max }%) | CPU ~{ $cpu_avg }% | VRAM { $vram } MB | limite: { $bound }
bench-total = ⏱ { $label } TOTAL: { $seconds } s
casting-no-casting-json = o projeto não tem casting.json (o casting não foi executado)
casting-profile-delete-failed = excluindo o perfil { $slug }: { $error }
casting-profile-not-found = perfil “{ $slug }” não encontrado
cloud-asr-no-key = o ASR na nuvem está ativado, mas a chave do OpenRouter não foi definida
cloud-asr-no-model = nenhum modelo STT do OpenRouter foi escolhido nas configurações
cloud-asr-failed = ASR na nuvem: { $error }
cloud-asr-empty = o STT na nuvem retornou uma transcrição vazia
cloud-tts-no-key = o TTS na nuvem está ativado, mas a chave do OpenRouter não foi definida
cloud-tts-no-model = nenhum modelo TTS foi escolhido nas configurações (Modelos na nuvem · OpenRouter)
cloud-tts-no-voice = nenhuma voz TTS foi definida nas configurações (cada modelo tem suas próprias vozes)
cloud-tts-failed = TTS na nuvem: { $error }
cloud-tts-too-short = TTS na nuvem: o áudio é curto demais ({ $bytes } { $bytes ->
    [one] byte
   *[other] bytes
})
cloud-tts-read-wav = lendo o wav da nuvem: { $error }

## Shared messages, compositing, downloads, dub timing, endpoints, preview frame

common-read = lendo { $path }: { $error }
common-parse = analisando { $path }: { $error }
common-ffmpeg-start = iniciando o ffmpeg: { $error }
compose-summary = composição: títulos={ $titles } (bbox), desfoque em { $boxes } { $boxes ->
    [one] caixa
   *[other] caixas
}, sub_px={ $sub_px }
compose-taglines-no-mt = slogans: a tradução automática está indisponível ({ $error }) -> só desfoque
compose-taglines-translating = slogans: traduzindo os textos da cartela
compose-taglines-failed = slogans: a tradução falhou; só desfoque
downloads-interrupted = o download parou junto com o aplicativo; o que foi baixado está salvo em .part e continuará dali
downloads-nothing-to-download = nenhum dos ids escolhidos é um componente baixável
downloads-busy = já há um download em andamento
downloads-thread-failed = a thread de download não iniciou: { $error }
timing-words-not-recognized = os tempos por palavra da dublagem não foram reconhecidos em { $count } { $count ->
    [one] fala
   *[other] falas
} ({ $examples }); as palavras delas são destacadas pelo comprimento
openrouter-catalog-failed = catálogo do OpenRouter: { $error }
openrouter-empty-key = a chave está vazia
llm-server-no-address = o endereço do servidor não foi definido
llm-server-no-answer = o servidor { $base } não respondeu: { $reason }
llm-server-status = { $endpoint } respondeu { $status }
llm-server-not-json = { $endpoint } não retornou JSON: { $error }
llm-server-not-model-list = { $endpoint } não retornou uma lista de modelos OpenAI (sem o campo data)
remix-start = remixando { $count } { $count ->
    [one] linha
   *[other] linhas
} → { $instruction }
remix-no-llm = remix: o LLM está indisponível: { $error }
remix-lines-changed = remix: as falas mudaram durante o remix; o projeto não foi alterado, execute o remix de novo
frame-empty-ass = ASS vazio: { $error }
frame-read-preview = lendo o quadro de prévia: { $error }
frame-read-source = lendo o quadro original: { $error }

## Glossary, hardware, job records, LLM providers

glossary-bad-format = format “{ $format }”: json ou tsv
glossary-series-unreadable = o glossário da série não pode ser lido: { $error }
glossary-no-text = o projeto não tem texto: o glossário é montado a partir da fala reconhecida, analise primeiro
glossary-lines = glossário: { $count } { $count ->
    [one] linha
   *[other] linhas
} de texto
glossary-no-llm = glossário: o LLM está indisponível: { $error }
glossary-failed = glossário: { $error }
glossary-proposed = glossário: { $count } { $count ->
    [one] entrada proposta
   *[other] entradas propostas
}
glossary-series-profile-missing = perfil da série “{ $slug }” não encontrado; o glossário dele não foi aplicado
glossary-series-slug-unreadable = o glossário da série “{ $slug }” não pode ser lido: { $error }
glossary-series-applied = glossário da série “{ $slug }”: { $count } { $count ->
    [one] entrada
   *[other] entradas
}, { $added } adicionadas ao projeto
hw-no-nvidia = sem GPU NVIDIA
jobs-serialize-record = serializando job.json: { $error }
jobs-record-missing = { $file } não está em { $dir }
llm-llama-server-missing = llama-server não encontrado ({ $path })
llm-gemma-missing = o GGUF do Gemma não foi encontrado ({ $path })
llm-mmproj-missing = o projetor de visão do Gemma (mmproj) não foi encontrado ({ $path })
llm-chat-client = cliente de chat: { $error }
llm-local-no-text-model = o servidor local ({ $url }) foi escolhido para a tradução, mas nenhum modelo foi selecionado
llm-local-no-vision-model = o servidor local ({ $url }) foi escolhido para visão, mas nenhum modelo foi selecionado
llm-local-client = cliente do servidor local: { $error }
llm-local-label = servidor local { $url } · { $model }
llm-openrouter-no-key = o OpenRouter foi escolhido, mas a chave não foi definida
llm-openrouter-no-text-model = o OpenRouter foi escolhido para a tradução, mas nenhum modelo foi selecionado
llm-openrouter-no-vision-model = o OpenRouter foi escolhido para visão, mas nenhum modelo foi selecionado
llm-openrouter-not-text = o modelo do OpenRouter { $model } não responde em texto; escolha outro para a tradução
llm-openrouter-not-vision = o modelo do OpenRouter { $model } não aceita imagens; escolha um modelo de visão
llm-vision-missing = nenhum ({ $reason })
llm-pair = tradução: { $text }; visão: { $vision }

## Media tools and OCR

common-ffmpeg-exit = o ffmpeg saiu com o código { $code }:
    { $tail }
media-ffprobe-start = o ffprobe não iniciou: { $error }
media-ffprobe-exit = o ffprobe saiu com o código { $code }: { $stderr }
media-ffprobe-no-streams = ffprobe: sem streams
media-no-streams = a entrada não tem fluxo de vídeo nem de áudio
media-no-duration = não foi possível determinar a duração
media-no-wav = o ffmpeg não criou o wav
media-ffmpeg-hung = o ffmpeg não terminou em { $seconds } s e foi encerrado (travou)
media-ffprobe-duration-exit = ffprobe duration saiu com o código { $code }
media-env-filter-script = script de filtro de envelope: { $error }
media-no-sample-rate = { $path }: a taxa de amostragem não foi lida ({ $stderr })
ocr-no-blur = { $error }; sem desfoque
ocr-models-missing = os modelos de OCR não foram encontrados
ocr-detection-failed = a detecção OCR falhou ({ $error })
ocr-summary = OCR: { $regions } { $regions ->
    [one] região
   *[other] regiões
}, { $localize } para localizar, { $bands } { $bands ->
    [one] faixa
   *[other] faixas
}, sub_y={ $sub_y }
