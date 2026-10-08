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

## Shared messages, compositing, downloads, dub timing, endpoints, preview frame

common-read = lecture de { $path } : { $error }
common-parse = analyse de { $path } : { $error }
common-ffmpeg-start = lancement de ffmpeg : { $error }
compose-summary = composite : titres={ $titles } (bbox), flou sur { $boxes } { $boxes ->
    [one] zone
   *[other] zones
}, sub_px={ $sub_px }
compose-taglines-no-mt = slogans : la traduction automatique est indisponible ({ $error }) -> flou seulement
compose-taglines-translating = slogans : traduction des textes du carton
compose-taglines-failed = slogans : la traduction a échoué ; flou seulement
downloads-interrupted = le téléchargement s’est arrêté avec l’application ; ce qui a été reçu est gardé dans .part et reprendra de là
downloads-nothing-to-download = aucun des id choisis n’est un composant téléchargeable
downloads-busy = un téléchargement est déjà en cours
downloads-thread-failed = le fil de téléchargement n’a pas démarré : { $error }
timing-words-not-recognized = les temps par mot du doublage n’ont pas été reconnus pour { $count } { $count ->
    [one] réplique
   *[other] répliques
} ({ $examples }) ; leurs mots sont surlignés selon leur longueur
openrouter-catalog-failed = catalogue OpenRouter : { $error }
openrouter-empty-key = la clé est vide
llm-server-no-address = aucune adresse de serveur n’est définie
llm-server-no-answer = le serveur { $base } n’a pas répondu : { $reason }
llm-server-status = { $endpoint } a répondu { $status }
llm-server-not-json = { $endpoint } n’a pas renvoyé de JSON : { $error }
llm-server-not-model-list = { $endpoint } n’a pas renvoyé de liste de modèles OpenAI (pas de champ data)
remix-start = remix de { $count } { $count ->
    [one] ligne
   *[other] lignes
} → { $instruction }
remix-no-llm = remix : le LLM est indisponible : { $error }
remix-lines-changed = remix : les répliques ont changé pendant le remix ; le projet n’a pas été modifié, relancez le remix
frame-empty-ass = ASS vide : { $error }
frame-read-preview = lecture de l’image d’aperçu : { $error }
frame-read-source = lecture de l’image d’origine : { $error }
