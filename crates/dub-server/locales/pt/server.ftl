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

## Hardware presets

common-write = gravando { $what }: { $error }
preset-rtx5090-title = RTX 5090 (32 GB)
preset-rtx4090-title = RTX 4090 (24 GB)
preset-top-subtitle = Qualidade máxima: os melhores quants localmente
preset-gpu16-title = GPU de 16 GB
preset-gpu16-subtitle = Alta qualidade (4080/4070 Ti e similares)
preset-gpu12-title = GPU de 12 GB
preset-gpu12-subtitle = Equilibrado (3060/4070 e similares)
preset-gpu8-title = GPU de 8 GB
preset-gpu8-subtitle = Econômico: quants leves (3060 Ti/4060)
preset-weak-nvidia-cloud-title = NVIDIA fraca + nuvem
preset-weak-nvidia-cloud-subtitle = O pesado (tradução/visão/voz) no OpenRouter, separação e ASR na sua GPU
preset-cloud-title = CPU + nuvem (sem NVIDIA)
preset-cloud-subtitle = O pesado no OpenRouter, o local no processador; roda sem placa de vídeo (é preciso uma chave)
preset-custom-title = Personalizado
preset-custom-subtitle = Vou ajustar cada parâmetro manualmente
preset-reason-no-gpu = Nenhuma GPU NVIDIA encontrada: modo “CPU + nuvem”, o pesado no OpenRouter e o local no processador (mais lento, mas funciona)
preset-reason-top-card = { $gpu } detectada: quants máximos
preset-reason-by-vram = { $gpu } · { $vram } GB de VRAM: { $preset }
preset-reason-low-vram = { $gpu } · { $vram } GB de VRAM é pouco para modelos locais; a nuvem é mais confiável
preset-unknown = predefinição desconhecida: { $id }

## Service port, shortening, frontend, launch defaults

common-corrupt = { $path } está corrompido: { $error }
common-create-dir = pasta { $path }: { $error }
service-bad-port = { $value }: não é um número de porta (esperado 1..65535)
service-exe-path = caminho do exe do serviço: { $error }
service-request-not-sent = a conexão foi aceita, mas a requisição não foi enviada: { $error }
service-no-health-answer = a conexão foi aceita, mas /health não respondeu: { $error }
service-not-http = a resposta não é HTTP
service-health-status = um servidor HTTP; /health respondeu { $code }
service-health-not-dub-studio = um servidor HTTP; /health não respondeu com um corpo do Dub Studio
service-health-not-json = um servidor HTTP; /health não respondeu com JSON
service-health-no-fields = /health diz ser { $app }, mas sem os campos do serviço: { $error }
service-other-app = outro aplicativo ({ $app })
service-health-no-app = um servidor HTTP; /health sem nome de aplicativo
service-port-reserved = o sistema não libera a porta, embora ninguém aceite conexões nela (talvez esteja numa faixa reservada pelo Windows: { $command })
service-port-dub-studio = o Dub Studio { $version } está nela ({ $executable })
service-port-other = outro processo a ocupa: { $what }
service-port-busy = A porta 127.0.0.1:{ $port } está ocupada há { $seconds } s: { $who }. Erro: { $error }.

    Feche o programa que a ocupa ou defina outra porta com a variável de ambiente { $env } (por exemplo { $env }={ $other_port }).
shorten-line-done = encurtando { $n }/{ $total }: { $from } -> { $to } caracteres
shorten-line-rejected = encurtando { $n }/{ $total }: a resposta não foi aceita ({ $reason })
shorten-line-no-answer = encurtando { $n }/{ $total }: o LLM não respondeu: { $error }
shorten-auto-start = { $count } { $count ->
    [one] fala não cabe
   *[other] falas não cabem
} no espaço; encurtando a tradução e dublando só elas
shorten-higgs-unloaded = o Higgs foi descarregado enquanto a tradução é encurtada
shorten-auto-no-llm = o encurtamento da tradução foi pulado: o LLM está indisponível: { $error }
shorten-none-shortened = encurtamento: nenhuma das { $count } falas ficou mais curta; ficam como foram dubladas
shorten-done = { $count } de { $total } { $total ->
    [one] fala encurtada
   *[other] falas encurtadas
}
shorten-nothing = nada a encurtar: todas as falas cabem no seu espaço
shorten-no-llm = encurtamento: o LLM está indisponível: { $error }
shorten-start = encurtando { $count } { $count ->
    [one] fala
   *[other] falas
}: { $provider }
shorten-all-failed = o encurtamento falhou: o LLM não respondeu a nenhuma fala ({ $id }: { $error })
spa-not-built = o frontend não foi compilado
settings-bad-speaker-count = speaker_count: espera-se um número inteiro de 0 a { $max } (0 é automático)
settings-bad-vo-gain = vo_gain_db={ $value }: espera-se um número de { $min } a { $max } dB
settings-bad-src-lang = src_lang={ $value }: não é um código de idioma nem "auto"
settings-bad-tgt-lang = tgt_lang={ $value }: não é um código de idioma
settings-bad-casting-ref = casting_ref={ $value }: não é o slug de um perfil de casting
settings-style-too-long = tr_style_custom tem mais de { $max } caracteres
settings-too-many-slots = { $name }: mais de { $max } espaços
settings-unknown-field = campo de padrões de início desconhecido: { $key }
settings-read-failed = lendo os padrões de início: { $error }
settings-patch-not-object = o corpo de PATCH /settings/launch é um objeto de campos
settings-write-failed = gravando os padrões de início: { $error }

## Takes, translation, TTS text, MCP arguments

takes-delete-old = excluindo a tomada antiga { $path }: { $error }
takes-missing = a tomada { $take } não está no histórico da fala { $line }
translate-no-vision = o tipo de conteúdo não foi determinado: a visão está indisponível ({ $error })
translate-subs-already-translated = as legendas já estão no idioma de destino -> sem tradução automática (só voz)
translate-transcribe-only = transcrever: tgt=o texto original, sem tradução
translate-same-language = mesmo idioma -> sem tradução automática (tgt=o original)
translate-no-llm = a tradução falhou: o LLM está indisponível: { $error }
translate-ctx-pass = passada de contexto: visão de layout/cena + tradução da transcrição
translate-failed = a tradução falhou: { $error }
translate-untranslated = a tradução falhou: { $left } de { $total } { $total ->
    [one] linha
   *[other] linhas
} ficaram no idioma original (detalhes no registro e em logs/llama-server.log)
translate-untranslated-auto = a tradução falhou: { $left } de { $total } { $total ->
    [one] linha
   *[other] linhas
} ficaram no idioma original; se a fala do vídeo já está no idioma de destino, indique o idioma original e a tradução não será necessária (detalhes no registro e em logs/llama-server.log)
translate-done = tradução pronta: { $done }/{ $total } linhas, títulos={ $titles }
translate-left-untranslated = { $left } de { $total } { $total ->
    [one] linha
   *[other] linhas
} ficaram no idioma original (detalhes em logs/llama-server.log)
translate-coverage-retry = cobertura da tradução: { $count } { $count ->
    [one] linha
   *[other] linhas
} sem tradução; traduzindo de novo
translate-coverage-failed = cobertura da tradução: a nova tradução falhou ({ $error })
translate-coverage-left = cobertura da tradução: restam { $count } sem tradução
tts-silent-after-cleanup = nenhuma fala restou após limpar o texto, então silêncio: { $count } { $count ->
    [one] fala
   *[other] falas
} ({ $lines })
mcp-bad-speaker-count = speaker_count: espera-se um número inteiro de 0 a { $max }
mcp-speakers-without-diarize = speaker_count maior que 1 não é compatível com diarize=false

## Routes: projects, voices, setup, translation, export, jobs, alignment

project-serialize = serializando project.json: { $error }
project-delete-failed = excluindo o projeto { $path }: { $error }
setup-no-ids = ids está vazio
setup-fetching-missing = Baixando os modelos que faltam para esta função…
setup-diarization-missing = O modelo de diarização não foi baixado ({ $error }); a análise seguirá sem separar os falantes
setup-pick-model-files = Arquivo(s) do modelo
setup-pick-models-folder = Pasta com modelos prontos
setup-no-folder = a pasta { $path } não existe
voices-not-found = a voz { $name } não foi encontrada em voices/
voices-no-vocals = não há voz para medir o F0; analise primeiro
voices-speakers-changed = vozes por espaços: os falantes mudaram enquanto a voz era medida; o projeto não foi alterado, execute de novo
analyze-args-not-object = argumentos de analyze: esperava-se um objeto
cost-analyze = OpenRouter: US${ $spent } gastos na análise (US${ $total } usados no total)
cost-run = OpenRouter: US${ $spent } gastos na execução (US${ $total } usados no total)
translate-lines-to = Traduzindo { $count } { $count ->
    [one] linha
   *[other] linhas
} → { $lang }
translate-note = tradução: { $note }
translate-titles-failed = os títulos não foram traduzidos ({ $error }); no vídeo eles ficarão no idioma original
translate-titles-error = traduzindo os títulos: { $error }
translate-lines-changed = tradução: as falas ou os títulos mudaram durante a tradução; o projeto não foi alterado, execute a tradução de novo
export-pick-folder = Onde salvar os resultados
export-copy-failed = copiando para { $path }: { $error }
jobs-wait-not-number = wait: esperava-se um número de segundos
jobs-unknown-kind = tipo de tarefa desconhecido em job.json: { $kind }
jobs-not-resumable = uma tarefa { $kind } não pode ser retomada
align-no-source-text = alinhamento com a fala: as falas não têm texto no idioma original (as legendas foram importadas no idioma de destino)
align-no-vocals = alinhamento com a fala: o projeto não tem faixa de voz; analise primeiro
align-recognizing = alinhamento com a fala: reconhecendo as palavras
align-recognition-failed = alinhamento com a fala: reconhecimento: { $error }
align-no-speech = alinhamento com a fala: nenhuma fala reconhecida; os tempos não foram alterados
align-mismatch = alinhamento com a fala: as falas não coincidiram com a fala ({ $share }% correspondido); os tempos não foram alterados
align-lines-changed = alinhamento com a fala: as falas mudaram durante o reconhecimento; os tempos não foram alterados, execute o alinhamento de novo
align-done = alinhado com a fala: { $share }% das falas por palavras, tempo alterado em { $changed }; deslocamento { $offset } s

## Analysis

analyze-serialize = serializando { $what }: { $error }
analyze-read-model = lendo o modelo { $path }: { $error }
analyze-stage-cache-unreadable = o cache da etapa { $stage } não pode ser lido ({ $error }); recalculando
analyze-checkpoint-not-saved = o ponto de controle da etapa { $stage } não foi salvo: { $error }
analyze-diarize-continuous = diarização: { $speakers } { $speakers ->
    [one] falante esperado
   *[other] falantes esperados
}, uma passada contínua pela gravação inteira sem reiniciar rótulos a cada hora
analyze-diarize-fragments = diarização: { $speakers } { $speakers ->
    [one] falante esperado
   *[other] falantes esperados
}, associando vozes entre fragmentos
analyze-wespeaker-needed = o WeSpeaker é necessário para um número definido de falantes: { $error }
analyze-align-skipped = alinhamento com a fala pulado: as legendas estão no idioma de destino e a fala no original
analyze-align-recognizing = alinhando as legendas com a fala: reconhecendo as palavras
analyze-align-recognition-failed = alinhando as legendas: reconhecimento da fala: { $error }
analyze-align-no-speech = alinhamento com a fala: nenhuma fala reconhecida; os tempos do arquivo foram mantidos
analyze-align-mismatch = as legendas não coincidiram com a fala ({ $share }% das falas correspondidas); os tempos do arquivo foram mantidos
analyze-align-done = legendas alinhadas com a fala: { $share }% das falas por palavras, o resto deslocado junto com as vizinhas; deslocamento do arquivo { $offset } s
analyze-more = mais { $count }
analyze-window-plan = faixa longa de { $duration } s: plano de { $windows } { $windows ->
    [one] janela
   *[other] janelas
} (a primeira ~{ $first } s); o ASR por janelas ainda não está ativo, processamento em um bloco
analyze-audio-cached = áudio do cache (a fonte não mudou); pulando o ffmpeg
analyze-extracting-audio = extraindo o áudio (ffmpeg -> 16k mono)
analyze-stems-stale = os stems vieram de uma extração anterior; separando de novo
analyze-separating = separando a voz ({ $model }): voz limpa para a diarização/ASR
analyze-separation-cached = separação do cache (os stems já estão prontos)
analyze-separation-failed = a separação falhou ({ $error }); diarização/ASR no áudio bruto
analyze-separator-missing = { $model } não foi encontrado; diarização/ASR no áudio bruto
analyze-diarizing = diarização ({ $model })
analyze-diarization-cached = diarização do cache
analyze-diarization-count-failed = diarização com um número definido de falantes: { $error }
analyze-diarization-failed = a diarização falhou ({ $error }); seguindo com um único falante
analyze-diarization-model-missing-count = o modelo { $model } não foi encontrado: o número definido de falantes não pode ser aplicado
analyze-subs-no-diarization = legendas: sem diarização (o clipe inteiro de uma vez)
analyze-diarization-model-missing = o modelo de diarização não foi encontrado; seguindo com um único falante
analyze-fewer-speakers = eram esperados { $expected } { $expected ->
    [one] falante
   *[other] falantes
}, { $found } distinguidos: as vozes que faltam não foram adicionadas
analyze-speakers-matched = vozes associadas entre fragmentos: { $speakers } { $speakers ->
    [one] falante
   *[other] falantes
}
analyze-transcript-cached = transcrição do cache: { $segments } { $segments ->
    [one] segmento
   *[other] segmentos
}
analyze-read-subs = lendo as legendas { $path }: { $error }
analyze-subs-empty = as legendas não foram reconhecidas ou estão vazias: { $path }
analyze-subs-imported = legendas importadas: { $lines } { $lines ->
    [one] fala
   *[other] falas
}, { $speakers } { $speakers ->
    [one] falante
   *[other] falantes
}
analyze-cloud-asr = transcrevendo na nuvem (OpenRouter STT)
analyze-cloud-stt-failed = STT na nuvem: { $error }
analyze-cloud-done = nuvem: { $lines } { $lines ->
    [one] fala
   *[other] falas
}, { $speakers } { $speakers ->
    [one] falante
   *[other] falantes
}
analyze-hallucination-filter = filtro de alucinações do ASR: { $error }
analyze-hidden-hallucinations = falas alucinadas do ASR ocultadas (sem voz): { $count }: { $lines }
analyze-hidden-by-text = títulos e sons do ASR ocultados pelo texto (há som no intervalo, a voz não está separada da música): { $count }: { $lines }
analyze-voiced-suspects = parecem alucinações, mas há voz; mantidas com uma marca: { $count }: { $lines }
analyze-merged-fragments = mesclando fragmentos: { $before } -> { $after } segmentos
analyze-characters-by-voice = personagens por voz: { $count }
analyze-segments-speakers = { $segments } { $segments ->
    [one] segmento
   *[other] segmentos
}, { $speakers } { $speakers ->
    [one] falante
   *[other] falantes
}
analyze-project-unparsable = project.json não pode ser lido ({ $error }): ele contém o glossário do projeto, então a análise parou para não perdê-lo
analyze-glossary-fixed = glossário: { $count } { $count ->
    [one] erro corrigido
   *[other] erros corrigidos
} no reconhecimento de termos
analyze-no-speech-nodub = não há segmentos de fala; mantendo a faixa original (nodub)
analyze-auto-nodub = auto: não há fala adequada para dublagem -> NODUB (o original + texto na tela localizado)
analyze-casting-style = descrições de personagens do perfil de casting -> estilo de tradução ({ $chars } caract.)
analyze-empty-removed = segmentos sem palavras removidos: { $count }
analyze-translation-cached = tradução do cache
analyze-ocr-cached = detecção de texto na tela do cache
analyze-audio-no-ocr = modo áudio: sem vídeo, a detecção de texto na tela não é necessária
analyze-ocr-off = a detecção de texto embutido está desativada (a caixa)
analyze-content-type = tipo de conteúdo (auto): { $kind }
analyze-casting-cached = casting do cache
analyze-casting-checkpoint = o ponto de controle do casting não foi salvo: { $error }
analyze-profile-voices-missing = as vozes do perfil não foram encontradas em voices/ -> clone: { $voices }
analyze-profile-voices-applied = as vozes do perfil foram aplicadas à dublagem ({ $count } { $count ->
    [one] personagem
   *[other] personagens
})
analyze-audio-no-casting = modo áudio: sem vídeo, o casting de personagens não é necessário
analyze-cache-not-saved = não foi possível salvar cache.json: { $error } (não é grave)

## Casting, recording, voice library

casting-relabel = rerrotulagem por voz: { $count } { $count ->
    [one] personagem
   *[other] personagens
} por voz (a diarização encontrou { $found })
casting-skipped-no-speech = casting pulado: não há segmentos de fala
casting-skipped-no-speakers = casting pulado: não há falantes
casting-speakers-ranked = personagens que falam: { $count } (ordenados por tempo de fala)
casting-faces-bound = rostos vinculados a falantes: { $count } de { $total }
casting-bit-part-skipped = { $character }: o falante { $speaker } tem 1 fala e nem rosto nem voz -> pulado (ponta sem casting)
casting-character-face = { $character }: falante { $speaker }, { $lines } { $lines ->
    [one] fala
   *[other] falas
}, { $seconds } s de fala, rosto: sim
casting-character-no-face = { $character }: falante { $speaker }, { $lines } { $lines ->
    [one] fala
   *[other] falas
}, { $seconds } s de fala, rosto: não
casting-profile-other-type = o perfil é de outro tipo ({ $previous } ≠ { $current }); correspondência cruzada pulada
casting-cross-episode = entre episódios: nomes/vozes transferidos: { $count }
casting-saved = casting.json pronto: { $count } { $count ->
    [one] personagem
   *[other] personagens
}
casting-save-failed = não foi possível gravar casting.json: { $error }
casting-faces-collected = rostos coletados: { $faces } ({ $samples } amostras)
casting-face-clusters = identidades de rosto (agrupamentos por vetor): { $count }
casting-anime-detector-missing = o detector de anime não foi encontrado ({ $path }); sem avatares
casting-anime-detector-failed = o detector de anime não carregou: { $error }; sem avatares
casting-scrfd-missing = o SCRFD não foi encontrado; sem avatares (casting por voz)
casting-scrfd-failed = o SCRFD não carregou: { $error }; sem avatares
casting-embedder-missing = { $model } não foi encontrado ({ $path }); avatares sem embedding
casting-embedder-failed = { $model } não carregou: { $error }; avatares sem embedding
casting-no-frame = o ffmpeg não extraiu o quadro
casting-voice-no-vocals = embedding de voz pulado: não há voz limpa
casting-voice-no-model = voz pulada: não há o modelo WeSpeaker ({ $path })
casting-voice-model-failed = o WeSpeaker não carregou: { $error }; voz pulada
casting-voice-trim-failed = amostra de voz { $speaker }: o corte falhou: { $error }
casting-voice-embedding-failed = voz { $speaker }: o embedding falhou: { $error }
casting-voice-embeddings = embeddings de voz: { $count }
casting-applying-profile = aplicando o perfil da biblioteca: { $slug }
casting-library-profile-missing = o perfil da biblioteca “{ $slug }” não foi encontrado; nada aplicado
record-busy = Já há uma gravação em andamento
record-mic-not-found = O microfone “{ $name }” não foi encontrado: está desconectado ou foi renomeado; escolha outro
record-no-mic = Nenhum microfone encontrado
record-mic-config = configuração do microfone: { $error }
record-create-wav = criando o wav: { $error }
record-format-unsupported = o formato { $format } não é suportado
record-mic-open = abrindo o microfone: { $error }
record-mic-start = iniciando o microfone: { $error }
record-not-recording = Nada está sendo gravado
voices-not-file-list = { $page }: não é uma lista de arquivos: { $error }
voices-list-endless = { $page }: a lista de arquivos do dataset não termina
voices-too-large = { $url }: mais do que os { $size } bytes fixados
voices-size-mismatch = { $url }: chegaram { $got } bytes, foram fixados { $size }
voices-sha-mismatch = { $url }: o SHA-256 { $got } não coincide com o fixado { $want }
voices-bad-name = nome inválido
voices-not-in-dataset = a voz { $name } não está no dataset { $dataset }
voices-bad-size = { $name }: o tamanho no catálogo não é válido
voices-no-sha = { $name }: não há SHA-256 no catálogo
voices-pack-downloading = baixando o pacote de vozes
voices-pack-unpacking = descompactando
voices-open-zip = abrindo o zip: { $error }
voices-create-file = criando { $name }: { $error }
voices-unpack-file = descompactando { $name }: { $error }
voices-pack-done = pronto: { $count } { $count ->
    [one] arquivo
   *[other] arquivos
}
