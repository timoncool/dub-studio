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
