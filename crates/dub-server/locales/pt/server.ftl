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
