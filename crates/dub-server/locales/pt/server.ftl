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

## Downloads by link

url-no-fetch = não existe o download { $id }
url-stopped = interrompido
url-already-downloading = este link já está sendo baixado: { $id }
url-probe-not-json = a resposta do yt-dlp -J não é JSON: { $error }; { $stderr }
url-interrupted = o download parou junto com o estúdio; o que foi baixado está na pasta dele e continuará dali
url-not-a-link = “{ $url }” não é um link: { $error }
url-not-http = “{ $url }” não é um link http(s)
url-bad-subs-lang = “{ $lang }” não é um código de idioma de legendas
url-cookies-not-file = { $path } não é um arquivo cookies.txt de até 1 MB
url-cookies-empty-or-large = cookies.txt está vazio ou passa de 1 MB
url-thread-failed = a thread de download não iniciou: { $error }
url-cookies-gone = o cookies.txt deste download sumiu da pasta; comece o download de novo com cookies
url-disk-space = são necessários cerca de { $need } MB, há { $free } MB livres
url-no-subs = o vídeo não tem legendas “{ $lang }” feitas por pessoas (há: { $have })
url-no-subs-none = o vídeo não tem legendas “{ $lang }” feitas por pessoas (não há nenhuma)
url-no-media-file = o yt-dlp terminou, mas não há arquivo de vídeo em { $path }: { $stderr }
url-subs-failed = as legendas “{ $lang }” não foram baixadas ({ $code }): { $error }
url-ytdlp-start = iniciando o yt-dlp: { $error }
url-ytdlp-wait = aguardando o yt-dlp: { $error }
url-still-downloading = { $id } ainda está sendo baixado
url-cancelled-removed = { $id } foi cancelado e o que foi baixado foi removido: comece um novo download
url-still-stopping = { $id } ainda está parando; tente de novo em alguns segundos
url-cancel-first = { $id } ainda está sendo baixado; cancele primeiro
url-no-subs-file = o yt-dlp terminou sem arquivo de legendas em { $path }: { $stderr }
url-no-parent = { $path } não tem pasta pai
url-subs-empty = as legendas { $path } não têm nenhuma fala
url-ytdlp-missing = o componente ytdlp não foi baixado
url-bad-quality = qualidade “{ $quality }”: best, 1080, 720, 480 ou audio

## yt-dlp

ytdlp-update-missing = a atualização do yt-dlp { $version } ({ $path }) não está no lugar; roda a fixada { $pinned }
ytdlp-http-client = cliente HTTP: { $error }
ytdlp-tag-not-version = o lançamento do yt-dlp com a tag “{ $tag }” não é uma versão
ytdlp-no-checksum = { $url } não contém { $file }
ytdlp-new-says = o novo yt-dlp { $tag } diz ser “{ $said }”; continua o { $current }
ytdlp-new-failed = o novo yt-dlp { $tag } não iniciou: { $error }; continua o { $current }
ytdlp-too-large = { $url }: mais de { $limit } bytes
ytdlp-sha-mismatch = { $url }: o SHA-256 { $got } não coincide com SHA2-256SUMS { $want }
ytdlp-exit-code = código de saída { $code }: { $stderr }
ytdlp-component-missing = o componente ytdlp (yt-dlp e deno) não foi baixado
ytdlp-no-ffmpeg = sem ffmpeg: nem o componente ffmpeg nem ffmpeg.exe no PATH
ytdlp-start = iniciando { $program }: { $error }
ytdlp-timeout = o yt-dlp não respondeu em { $seconds } s: { $stderr }
ytdlp-playlist = o link é uma playlist ou um canal ({ $count } { $count ->
    [one] vídeo
   *[other] vídeos
}), não um único vídeo
ytdlp-redirect-only = o link não tem o vídeo em si, só outro link
ytdlp-live = uma transmissão ao vivo ({ $status })
ytdlp-thumbnail-not-image = a miniatura não é uma imagem ({ $mime })
ytdlp-thumbnail-too-large = a miniatura passa de 4 MB
ytdlp-failed-silently = o yt-dlp falhou sem mensagem

## Render: voicing, QC, mix, mux

render-input = entrada { $width }x{ $height } dur={ $duration }s
render-nodub-original = nodub: a faixa de áudio original
render-done-audio = pronto (só áudio) -> { $path }
render-building-ass = montando o ASS (títulos + legendas dubladas)
render-burning = gravando as legendas + desfoque (ffmpeg + libass, NVENC)
render-burn-off = legendas/títulos desativados (subs.burn=off)
render-muxing = multiplexando vídeo + áudio
render-track-dub = { $lang } (dublagem)
render-track-original = { $lang } (original)
render-two-tracks = duas faixas: { $dub } + { $original } -> { $container }
render-multitrack-failed = a multiplexação multifaixa falhou ({ $error }) -> uma faixa
render-subtitle-tracks = legendas como faixas mkv: { $tracks }
render-subtitle-tracks-failed = legendas como faixas mkv: { $error }
render-mp4-companion-failed = o mp4 complementar não foi montado ({ $error }); o player abrirá o mkv (o VLC funciona)
render-done = pronto -> { $path }
render-dub-audio-done = o áudio dublado está pronto
render-synth-thread-ended = a thread de síntese terminou sem resultado
render-synth-timeout = a síntese passou de { $seconds } s; cancelada, o motor está livre
render-engine-stuck = a síntese não cancela após { $seconds } s; a renderização parou (o motor travou na DLL)
render-higgs-load-failed = carregando a DLL do Higgs: { $error }
render-defect-runaway = descontrolada
render-defect-cutoff = cortada
render-defect-silence = silêncio
render-defect-hum = zumbido
render-recognition-no-answer = o reconhecimento não retornou resposta
render-second-pass-overflow = a segunda passada de voz pediu um encurtamento, que nela está desativado
render-no-translated-lines = não há falas traduzidas -> silêncio, a faixa original
render-takes-of-removed-lines = históricos de tomadas de falas removidas apagados: { $count }
render-extracting-audio = extraindo o áudio (ffmpeg 44,1k estéreo)
render-separator-missing = o motor de separação não foi encontrado -> sem fundo (keep_music off)
render-ref-from-mix = a referência do clone vem da mixagem sem separação: o fundo original também soa nela
render-emotion-ref-failed = segmento { $segment }: a referência de emoção não foi cortada ({ $error }); usando a referência de identidade do falante
render-cloud-voices = vozes na nuvem por falante: { $voices }
render-synth-keys-reset = { $error }; as chaves de síntese recomeçam
render-synthesizing = sintetizando { $count } de { $total } { $total ->
    [one] segmento
   *[other] segmentos
}
render-voicing-cached = voz do cache: { $count } { $count ->
    [one] segmento
   *[other] segmentos
}
render-cloud-tts-parallel = TTS na nuvem: { $count } { $count ->
    [one] segmento
   *[other] segmentos
} em { $threads } threads paralelas
render-cloud-tts-ready = TTS na nuvem: pré-síntese pronta ({ $count } { $count ->
    [one] segmento
   *[other] segmentos
})
render-takes-quarantined = { $error }; o histórico de tomadas da fala { $line } foi separado em { $path } e recomeçou
render-pinned-take = fala { $line }: toca a tomada fixada; a nova voz não a substitui
render-take-unpinned = fala { $line }: a tomada foi desafixada porque o texto da fala mudou
render-selected-take = fala { $line }: toca a tomada escolhida { $take }; a nova voz não a substitui
render-write-cloud-segment = gravando o seg{ $line } da nuvem: { $error }
render-cloud-tts-failed = ⚠ segmento { $line }: o TTS na nuvem falhou ({ $error }); o original foi mantido
render-loading-higgs = carregando o Higgs
render-failures-kept-generated = ⚠ segmento { $line }: { $attempts } falhas de síntese ({ $error }); usada a voz gerada (amplitude { $range } dB)
render-failures-kept-original = ⚠ segmento { $line }: { $attempts } falhas/timeouts de síntese ({ $error }); a fala original foi mantida
render-regenerating = segmento { $line }: { $error }; regenerando ({ $attempt }/{ $attempts })
render-defects-kept-generated = ⚠ segmento { $line }: todas as { $attempts } tentativas têm defeito ({ $defect }); usada a voz gerada (amplitude { $range } dB)
render-silent-kept-original = ⚠ segmento { $line }: { $attempts } tentativas sem som; usado o original
render-retry-alt-ref = outra referência
render-retry-temperature = temperatura maior
render-defect-regenerating = segmento { $line }: defeito de síntese ({ $defect }); regenerando ({ $via } { $attempt }/{ $attempts })
render-write-segment = gravando seg{ $line }: { $error }
render-too-many-artifacts = TTS: artefatos de zumbido demais ({ $in_a_row } seguidos, { $retries } novas tentativas no total); regenerar não ajuda. O problema provavelmente é a máquina (modelo/VRAM) ou os clipes de referência de voz. Parado no segmento { $line }.
render-multi-take = segmento { $line }: várias tomadas; escolhida a mais próxima do espaço (desvio de { $deviation } s)
render-stretch-over-cap = segmento { $line }: precisa esticar x{ $needed } (espaço { $slot } s), limite x{ $cap }; o texto está mais rápido que o normal
render-silence-trimmed = corte de silêncio do TTS: { $seconds } s removidos de { $lines } { $lines ->
    [one] fala
   *[other] falas
} ({ $pauses } s de pausas); graças ao corte a aceleração ficou dentro do limite em { $into_cap } { $into_cap ->
    [one] fala
   *[other] falas
}
render-fit-summary = ajuste: { $over }/{ $total } segmentos acima do limite ({ $share }%)
render-fit-summary-drift = ajuste: { $over }/{ $total } segmentos acima do limite ({ $share }%), sincronia recuperada em { $drift }
render-qc-start = QC: conferindo { $count } { $count ->
    [one] fala
   *[other] falas
} por transcrição
render-qc-unheard = QC: { $count } de { $total } falas não foram conferidas; o reconhecimento falhou: { $reason }
render-qc-mismatch = QC: { $count } { $count ->
    [one] fala não coincide
   *[other] falas não coincidem
} com a tradução; ressintetizando
render-qc-resynthesized = QC: segmento { $line } ressintetizado (tentativa { $attempt })
render-qc-unconfirmed = ⚠ QC: não foi possível confirmar o segmento { $line } (“{ $text }”); confira a fala manualmente
render-qc-kept-mismatch = ⚠ QC: o segmento { $line } não coincide com o texto traduzido; mantida a voz gerada
render-qc-resynth-unheard = QC: { $count } { $count ->
    [one] fala ressintetizada não foi conferida
   *[other] falas ressintetizadas não foram conferidas
}; o reconhecimento falhou: { $reason }
render-qc-summary = resultado do QC: { $fixed }/{ $total } corrigidas, { $flagged } ainda marcadas, { $unheard } não conferidas
render-qc-all-confirmed = QC: todas as falas foram confirmadas por transcrição ✓
render-qc-rest-confirmed = QC: as demais falas foram confirmadas por transcrição
render-laying-out = posicionando a dublagem na linha do tempo
render-peak-limiter = limitador de picos: { $lines } { $lines ->
    [one] fala
   *[other] falas
}, { $samples } amostras acima do teto { $ceiling } reduzidas sem clipping
render-tempo-fit = ajuste de andamento da faixa inteira x{ $factor }
render-voiceover-envelope = voice-over: o original a { $db } dB SOB a tradução, cheio nas pausas (envelope dinâmico, { $blocks } blocos)
render-voiceover-flat = voice-over: o envelope está indisponível -> atenuação plana
render-mix-no-ducking = mixagem: instrumental + voz dublada (ducking DESLIGADO, fundo cheio)
render-mix-ducking = mixagem: instrumental + voz dublada (ducking LIGADO, envelope, { $blocks } blocos)
render-mix-sidechain = o envelope está indisponível -> ducking por sidechain
render-mix-plain = o sidechain está indisponível -> mixagem direta
render-loudness-off = o nivelamento de volume está desligado: a mixagem como está
render-loudness-normalizing = normalizando o volume (EBU R128, true peak)
render-loudnorm-skipped = loudnorm pulado ({ $error })
render-track-gain = ganho da faixa { $db } dB
render-dub-timings-not-written = os tempos da dublagem para as legendas não foram gravados: o posicionamento ({ $lines } falas, { $spans } trechos) não coincidiu com os segmentos ({ $segments }); as legendas seguem os tempos originais
render-dub-timing-mismatch = tempos da dublagem: a fala { $line } da visão de síntese não coincide com o segmento nº { $index } do projeto
render-word-timings = tempos por palavra das legendas: reconhecendo { $count } { $count ->
    [one] fala
   *[other] falas
} da dublagem
render-refs-unchecked = conferência de referências: { $count } { $count ->
    [one] candidata aceita
   *[other] candidatas aceitas
} sem conferência; o reconhecimento falhou: { $error }
render-refs-all-failed = ⚠ falante { $speaker }: nenhuma referência candidata passou na conferência (ouvido: “{ $heard }”); usando a melhor pela pontuação
render-speaker-ref-failed = referência do falante { $speaker }: { $error }
render-speaker-ref = referência do falante { $speaker }: “{ $text }” ({ $seconds } s, { $candidates } { $candidates ->
    [one] candidata
   *[other] candidatas
}, conferência ok)
render-speaker-ref-unchecked = referência do falante { $speaker }: “{ $text }” ({ $seconds } s, { $candidates } { $candidates ->
    [one] candidata
   *[other] candidatas
}, conferência ⚠ falhou)

## Setup: components

setup-comp-higgs-purpose = Síntese da dublagem e clonagem de voz (TTS)
setup-comp-higgs-engine-name = Motor Higgs (audiocpp_engine.dll)
setup-comp-higgs-engine-purpose = Motor TTS nativo do Higgs (ABI C)
setup-comp-gemma-purpose = Tradução e orquestrador de visão de legendas/títulos
setup-comp-gemma-q5-0-purpose = Tradução e visão, mais precisa que q4_0
setup-comp-gemma-q6-k-purpose = Tradução e visão, ainda mais precisa
setup-comp-gemma-q8-0-purpose = Tradução e visão, precisão máxima
setup-comp-parakeet-purpose = Reconhecimento de fala com marcas de tempo por palavra (ASR)
setup-comp-higgs-q6-k-purpose = Síntese da dublagem e clonagem de voz (TTS), mais leve que Q8_0
setup-comp-higgs-q4-k-m-purpose = Síntese da dublagem e clonagem de voz (TTS), a variante mais leve
setup-comp-parakeet-fp32-purpose = Reconhecimento de fala (ASR), precisão fp32 completa
setup-comp-parakeet-ultra-purpose = Reconhecimento de fala (ASR), a versão ajustada da Moondream com menos erros
setup-comp-whisper-engine-name = Whisper-Faster (motor ASR)
setup-comp-whisper-engine-purpose = Um motor alternativo de reconhecimento de fala (faster-whisper) no lugar do Parakeet
setup-comp-whisper-cuda-name = Aceleração CUDA do Whisper (cuBLAS + cuDNN)
setup-comp-whisper-cuda-purpose = Inferência do Whisper na GPU (senão o reconhecimento roda na CPU, várias vezes mais lento)
setup-comp-whisper-tiny-name = Whisper tiny (modelo ASR)
setup-comp-whisper-tiny-purpose = ASR Whisper, o modelo mais leve e rápido
setup-comp-whisper-base-name = Whisper base (modelo ASR)
setup-comp-whisper-base-purpose = ASR Whisper, um modelo leve, mais preciso que o tiny
setup-comp-whisper-small-name = Whisper small (modelo ASR)
setup-comp-whisper-small-purpose = ASR Whisper, um modelo equilibrado
setup-comp-whisper-medium-name = Whisper medium (modelo ASR)
setup-comp-whisper-medium-purpose = ASR Whisper, alta precisão
setup-comp-whisper-large-v3-name = Whisper large-v3 (modelo ASR)
setup-comp-whisper-large-v3-purpose = ASR Whisper, precisão máxima (large-v3)
setup-comp-whisper-large-v3-turbo-name = Whisper large-v3-turbo (modelo ASR)
setup-comp-whisper-large-v3-turbo-purpose = ASR Whisper, quase large-v3, mas bem mais rápido (turbo)
setup-comp-sortformer-name = Nemotron 3 Diarization (até 8 falantes)
setup-comp-sortformer-purpose = Separação de falantes (quem fala quando), até 8 vozes
setup-comp-roformer-purpose = Modelo de separação voz/instrumental
setup-comp-roformer-q5-purpose = Separação, mais leve que Q8_0
setup-comp-roformer-q4-purpose = Separação, a variante mais leve
setup-comp-casting-name = Modelos de casting de personagens (rostos + voz)
setup-comp-casting-purpose = Detecção/embedding de rostos (reais + anime) + embedding de voz para o casting
setup-comp-bsroformer-engine-name = Motor BSRoformer.cpp (CUDA)
setup-comp-bsroformer-engine-purpose = Motor nativo de separação (bs_roformer-cli + ggml-CUDA)
setup-comp-bsroformer-engine-cpu-name = Motor BSRoformer.cpp (CPU)
setup-comp-bsroformer-engine-cpu-purpose = Separação no processador, o modo sem NVIDIA (mais lenta, função completa)
setup-comp-llama-name = Servidor llama.cpp (CUDA 13.4)
setup-comp-llama-purpose = Servidor auxiliar para o Gemma (tradução/visão)
setup-comp-onnxruntime-purpose = Runtime de ASR/OCR/diarização (estritamente 1.28.x)
setup-comp-onnxruntime-gpu-purpose = Provedor CUDA para diarização/Parakeet na GPU (modo local_backend=gpu)
setup-comp-ffmpeg-purpose = Decodificação/codificação de vídeo e áudio (NVENC)
setup-comp-ytdlp-name = Download por link (yt-dlp + deno)
setup-comp-ytdlp-purpose = Baixar um vídeo por link (YouTube e outros sites do yt-dlp) em um novo projeto
setup-comp-cuda-runtime-purpose = DLLs CUDA redistribuíveis para os motores e o CUDA EP do onnxruntime (sem CUDA Toolkit)
setup-comp-cudnn-purpose = Necessário ao provedor CUDA do onnxruntime para diarização/Parakeet na GPU
setup-comp-vcruntime-purpose = DLLs de sistema dos motores (incluídas)
setup-comp-ocr-name = Modelos de OCR (PP-OCR ONNX)
setup-comp-ocr-purpose = Detecção de texto embutido → desfoque (incluídos)
setup-comp-nvidia-driver-name = Driver NVIDIA
setup-comp-nvidia-driver-purpose = Aceleração por GPU (instalado à parte, não pelo aplicativo)

## Setup: downloads and installation

setup-http-status = { $url }: status { $status }
setup-write = gravando: { $error }
setup-not-zip = não é um zip: { $error }
setup-zip-entry = entrada do zip: { $error }
setup-open = abrindo { $path }: { $error }
setup-verifying = Conferindo o SHA-256 de { $file }…
setup-install-record = o registro de instalação: { $error }
setup-disk-space = espaço insuficiente: são necessários { $need } GB, há { $free } GB livres ({ $path })
setup-unknown-component = não existe o componente { $id }
setup-not-removable = { $id } não é instalado pelo aplicativo
setup-component-busy = o componente está sendo baixado; pause o download
setup-paused = o download foi pausado
setup-rate-limited = { $url }: o servidor responde { $status } há { $minutes } min
setup-proxy-scheme = proxy { $proxy }: o esquema { $scheme } não serve para downloads (http, https, socks4, socks5)
setup-proxy-no-host = proxy { $proxy }: sem host
setup-proxy-no-port = proxy { $proxy }: sem porta
setup-proxy-credentials = proxy { $proxy }: o download de modelos (ureq) não consegue passar ao proxy esse login ou senha: há / ? #, espaço, caracteres não ASCII ou (no SOCKS5) dois-pontos na senha; as requisições à nuvem por esse proxy funcionam, para baixar é preciso uma senha sem esses caracteres
setup-start-failed = { $url }: não foi possível começar em { $retries } tentativas: { $error }
setup-chunk-manifest = o manifesto de blocos: { $error }
setup-range-incomplete = intervalo incompleto: { $got }/{ $want } bytes
setup-range-failed = intervalo { $start }-{ $end } após { $retries } tentativas: { $error }
setup-range-status = intervalo { $start }-{ $end }: status { $status } (esperado 206)
setup-range-read = lendo o intervalo: { $error }
setup-create = criando { $path }: { $error }
setup-read = lendo: { $error }
setup-download-failed = { $url } após { $retries } tentativas: { $error }
setup-size-mismatch = { $file }: baixados { $got } bytes, fixados { $want }; o arquivo foi removido e a próxima tentativa recomeça
setup-hash-mismatch = { $file }: o SHA-256 { $got } não coincide com o fixado { $want }; o arquivo foi removido e a próxima tentativa recomeça
setup-unpacking = Descompactando { $file }…
setup-rename = renomeando { $path }: { $error }
setup-waiting-other = Aguardando outro download dos mesmos componentes…
setup-delete = excluindo { $path }: { $error }
setup-downloading = Baixando os modelos…
setup-source-changed = { $url }: o servidor entrega { $got } bytes, fixados { $want }; a fonte mudou
setup-manifest-write = manifesto { $path }: { $error }
setup-archive-no-files = o arquivo { $path } não contém os arquivos necessários
setup-wheel-not-zip = o wheel não é um zip: { $error }
setup-wheel-entry = entrada do wheel: { $error }
setup-archive-no-dll = o arquivo { $path } não contém DLL
setup-unpack = descompactando { $path }: { $error }
setup-finalize = finalizando { $path }: { $error }

## Settings applied after analysis

post-analyze-bad-vo-gain = vo_gain: esperava-se um número de dB, chegou { $value }
post-analyze-bad-flag = { $name }: esperava-se 0 ou 1, chegou { $value }
post-analyze-bad-container = container: esperava-se mp4 ou mkv, chegou { $value }
post-analyze-bad-voice-slots = voice_slots: esperava-se um objeto {"{"}male:[…], female:[…]{"}"}
post-analyze-edit-failed = o ajuste após a análise { $edit }: { $error }

## Messages of the engines and libraries: glossary, LLM, ASR, translation, separation, faces, TTS, captions, OCR

glossary-over-limit = o glossário tem { $total } entradas, mais que { $max }
glossary-empty-term = a entrada { $entry } não tem termo
glossary-field-too-long = entrada { $entry } («{ $term }»): um campo passa de { $max } caracteres
glossary-duplicate = o termo «{ $term }» aparece duas vezes (entradas { $first } e { $second })
glossary-tsv-keep = linha { $line }: keep é «{ $value }»; esperado 1 ou 0
glossary-tsv-empty-term = linha { $line }: falta o termo
glossary-one-of = envie entradas ou TSV, não ambos
glossary-nothing = nada para salvar: não há entradas nem TSV
llm-spawn-failed = o llama-server não iniciou: { $error }
llm-gguf-missing = o modelo GGUF não foi encontrado ({ $path })
llm-log-file = log do llama-server { $path }: { $error }
llm-exited-early = o llama-server terminou antes de ficar pronto ({ $status }); stderr: { $stderr }
llm-not-ready = o llama-server não subiu em { $secs } s (porta { $port }); stderr: { $stderr }
llm-http = a solicitação ao modelo falhou: { $error }
llm-api = API do modelo: { $error }
llm-rejected = a API do modelo recusou a solicitação ({ $status }): { $body }
llm-empty-answer = o modelo { $model } devolveu uma resposta vazia (sem motivo)
llm-empty-answer-reason = o modelo { $model } devolveu uma resposta vazia (finish_reason={ $reason })
llm-cut-short = o modelo { $model } atingiu o limite de { $max_tokens } { $max_tokens ->
    [one] token
   *[other] tokens
} (finish_reason=length): a resposta está incompleta; provavelmente gasta o orçamento raciocinando — escolha um modelo sem raciocínio obrigatório
llm-prompt-cut = o servidor leu só { $read } { $read ->
    [one] token
   *[other] tokens
} de uma solicitação de { $chars } { $chars ->
    [one] caractere
   *[other] caracteres
} e descartou o início — o contexto dele é pequeno: aumente num_ctx no Ollama ou o Context Length do modelo no LM Studio
asr-engine = reconhecimento de fala: { $error }
asr-wav-read = não foi possível ler o wav { $path }: { $error }
asr-resample = reamostragem: { $error }
asr-speaker-count = o número de locutores deve ser de 1 a { $max }
speakers-embedding = voz do locutor { $speaker }: { $error }
speakers-no-sample = locutor { $speaker }: não há trecho de fala de pelo menos 0,3 segundo para comparar a voz
speakers-too-many-voices = número de locutores indicado: { $given }, mas o trecho tem { $found } { $found ->
    [one] voz diferente
   *[other] vozes diferentes
}: não foi possível juntá-las com segurança pela voz
speakers-bad-embedding = o modelo de voz devolveu características de voz vazias ou inválidas
speakers-dimension-changed = o tamanho das características de voz mudou
speakers-more-voices = o trecho tem mais vozes que o número de locutores indicado
speakers-unmatched = não foi possível associar com segurança as vozes do trecho ao número de locutores indicado
translate-format-json = tradução: o modelo responde com um objeto JSON pelo esquema ({ $model })
translate-format-json-probe = tradução: tentando respostas por esquema JSON ({ $model }); se recusar, linhas numeradas
translate-format-numbered = tradução: linhas numeradas; o modelo { $model } não declara structured_outputs no catálogo do OpenRouter
translate-schema-ignored = tradução: { $model } aceitou o esquema JSON, mas respondeu com linhas numeradas; daqui em diante, linhas numeradas
translate-schema-ignored-server = tradução: o servidor aceitou o esquema JSON, mas respondeu com linhas numeradas; daqui em diante, linhas numeradas
translate-schema-refused = tradução: { $model } recusou respostas por esquema JSON ({ $status }: { $body }); daqui em diante, linhas numeradas
translate-schema-refused-server = tradução: o servidor recusou respostas por esquema JSON ({ $status }: { $body }); daqui em diante, linhas numeradas
translate-line-flawed = tradução: a linha { $line } mantém uma tradução com uma observação: { $reason }
translate-line-failed = tradução: a linha { $line } não foi traduzida: { $reason }
translate-line-reason = { $line }: { $reason }
translate-lines-rejected = tradução: { $bad } de { $total } { $total ->
    [one] linha
   *[other] linhas
} não passaram na verificação ({ $reasons })
translate-batch-stopped = tradução: o bloco de linhas { $first }..{ $last } falhou ({ $error }); a tradução parou
translate-batch-failed = tradução: o bloco de linhas { $first }..{ $last } falhou ({ $error })
translate-layout-no-vision = layout do quadro: ignorado (nenhum modelo de visão escolhido ou disponível)
translate-layout-not-needed = layout do quadro: ignorado (as legendas não são gravadas no vídeo, não é necessário)
translate-layout = layout do quadro: sub_style={ $sub_style } titles={ $titles } brands={ $brands }
translate-layout-failed = layout do quadro: ignorado ({ $error })
translate-scene-failed = contexto da cena: ignorado ({ $error })
translate-scene-no-vision = contexto da cena: ignorado (nenhum modelo de visão escolhido ou disponível)
translate-audio-failed = contexto de áudio: ignorado ({ $error })
translate-context-trimmed = tradução: o bloco de contexto de { $chars } { $chars ->
    [one] caractere
   *[other] caracteres
} foi cortado para { $budget } (proteção de n_ctx)
translate-names-skipped = tradução: o glossário automático de nomes foi ignorado ({ $error })
translate-chunks = tradução: { $lines } { $lines ->
    [one] linha
   *[other] linhas
} -> { $chunks } { $chunks ->
    [one] bloco
   *[other] blocos
} (glossário: { $terms } { $terms ->
    [one] termo
   *[other] termos
}, { $names } { $names ->
    [one] nome
   *[other] nomes
})
translate-pass-done = tradução: pronto, { $translated } traduzidas ({ $flawed } com observação), { $untranslated } no idioma original
glossary-pass = glossário: passagem do modelo { $pass }/{ $passes }
glossary-schema-refused = glossário: o servidor recusou respostas por esquema JSON ({ $status }); pedindo JSON como texto
content-type-decided = tipo de conteúdo: { $decided ->
    [anime] animação
   *[other] imagem real
} ({ $votes } { $votes ->
    [one] resposta clara
   *[other] respostas claras
} de { $frames } quadros)
line-missing = ausente da resposta
line-cut = a resposta foi cortada pelo limite de tokens
line-untranslated = não está no idioma de destino
line-echo = repete o original
line-too-short = curta demais ({ $got } < { $min })
line-too-long = longa demais ({ $got } > { $max })
line-loop = repete em loop «{ $gram }»
line-term-missing = falta o termo do glossário «{ $term }»
translate-frame = extração de um quadro: { $error }
translate-audio = contexto de áudio: { $error }
translate-empty = o modelo não traduziu nenhuma de { $lines } { $lines ->
    [one] linha
   *[other] linhas
}; a última causa: { $reason }
translate-empty-no-reason = o modelo não traduziu nenhuma de { $lines } { $lines ->
    [one] linha
   *[other] linhas
}
translate-contract = a resposta do modelo: { $problem }
translate-contract-answer = a resposta do modelo: { $problem }; a resposta: { $answer }
answer-no-json-object = não contém um objeto JSON
answer-not-json = não é um JSON válido ({ $error })
answer-no-terms = não contém a lista terms
remix-failed = remix: { $error }
