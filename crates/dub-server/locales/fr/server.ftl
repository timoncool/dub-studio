asr-windowed = transcription par fenêtres sur le GPU ({ $speakers } { $speakers ->
    [one] locuteur
   *[other] locuteurs
})

## Separation, OCR, benchmark, casting library, cloud ASR and TTS

atomic-extract-separation-audio = extraction de l’audio 44,1k pour la séparation
atomic-separating = séparation ({ $model })
atomic-separation-failed = séparation : { $error }
ocr-detecting-burned-text = détection du texte incrusté ({ $model })
bench-stage = ⏱ { $name } : { $seconds } s | GPU ~{ $gpu_avg } % (pic { $gpu_max } %) | CPU ~{ $cpu_avg } % | VRAM { $vram } Mo | limité par : { $bound }
bench-total = ⏱ { $label } TOTAL : { $seconds } s
casting-no-casting-json = le projet n’a pas de casting.json (le casting n’a pas été lancé)
casting-profile-delete-failed = suppression du profil { $slug } : { $error }
casting-profile-not-found = profil « { $slug } » introuvable
cloud-asr-no-key = l’ASR cloud est activé, mais aucune clé OpenRouter n’est définie
cloud-asr-no-model = aucun modèle STT OpenRouter n’est choisi dans les réglages
cloud-asr-failed = ASR cloud : { $error }
cloud-asr-empty = le STT cloud a renvoyé une transcription vide
cloud-tts-no-key = le TTS cloud est activé, mais aucune clé OpenRouter n’est définie
cloud-tts-no-model = aucun modèle TTS n’est choisi dans les réglages (Modèles cloud · OpenRouter)
cloud-tts-no-voice = aucune voix TTS n’est définie dans les réglages (chaque modèle a ses propres voix)
cloud-tts-failed = TTS cloud : { $error }
cloud-tts-too-short = TTS cloud : l’audio est trop court ({ $bytes } { $bytes ->
    [one] octet
   *[other] octets
})
cloud-tts-read-wav = lecture du wav cloud : { $error }
