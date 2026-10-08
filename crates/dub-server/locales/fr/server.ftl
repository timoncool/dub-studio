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

## Glossary, hardware, job records, LLM providers

glossary-bad-format = format « { $format } » : json ou tsv
glossary-series-unreadable = le glossaire de la série est illisible : { $error }
glossary-no-text = le projet n’a pas de texte : le glossaire se construit à partir de la parole reconnue, lancez d’abord l’analyse
glossary-lines = glossaire : { $count } { $count ->
    [one] ligne
   *[other] lignes
} de texte
glossary-no-llm = glossaire : le LLM est indisponible : { $error }
glossary-failed = glossaire : { $error }
glossary-proposed = glossaire : { $count } { $count ->
    [one] entrée proposée
   *[other] entrées proposées
}
glossary-series-profile-missing = profil de série « { $slug } » introuvable ; son glossaire n’est pas appliqué
glossary-series-slug-unreadable = le glossaire de la série « { $slug } » est illisible : { $error }
glossary-series-applied = glossaire de la série « { $slug } » : { $count } { $count ->
    [one] entrée
   *[other] entrées
}, { $added } ajoutées au projet
hw-no-nvidia = pas de GPU NVIDIA
jobs-serialize-record = sérialisation de job.json : { $error }
jobs-record-missing = { $file } est absent de { $dir }
llm-llama-server-missing = llama-server introuvable ({ $path })
llm-gemma-missing = le GGUF de Gemma est introuvable ({ $path })
llm-mmproj-missing = le projecteur de vision de Gemma (mmproj) est introuvable ({ $path })
llm-chat-client = client de chat : { $error }
llm-local-no-text-model = le serveur local ({ $url }) est choisi pour la traduction, mais aucun modèle n’est sélectionné
llm-local-no-vision-model = le serveur local ({ $url }) est choisi pour la vision, mais aucun modèle n’est sélectionné
llm-local-client = client du serveur local : { $error }
llm-local-label = serveur local { $url } · { $model }
llm-openrouter-no-key = OpenRouter est choisi, mais aucune clé n’est définie
llm-openrouter-no-text-model = OpenRouter est choisi pour la traduction, mais aucun modèle n’est sélectionné
llm-openrouter-no-vision-model = OpenRouter est choisi pour la vision, mais aucun modèle n’est sélectionné
llm-openrouter-not-text = le modèle OpenRouter { $model } ne répond pas en texte ; choisissez-en un autre pour la traduction
llm-openrouter-not-vision = le modèle OpenRouter { $model } n’accepte pas les images ; choisissez un modèle de vision
llm-vision-missing = aucun ({ $reason })
llm-pair = traduction : { $text } ; vision : { $vision }

## Media tools and OCR

common-ffmpeg-exit = ffmpeg a quitté avec le code { $code } :
    { $tail }
media-ffprobe-start = ffprobe n’a pas pu démarrer : { $error }
media-ffprobe-exit = ffprobe a quitté avec le code { $code } : { $stderr }
media-ffprobe-no-streams = ffprobe : aucun flux
media-no-streams = l’entrée n’a ni flux vidéo ni flux audio
media-no-duration = impossible de déterminer la durée
media-no-wav = ffmpeg n’a pas créé le wav
media-ffmpeg-hung = ffmpeg ne s’est pas terminé en { $seconds } s et a été arrêté (bloqué)
media-ffprobe-duration-exit = ffprobe duration a quitté avec le code { $code }
media-env-filter-script = script de filtre d’enveloppe : { $error }
media-no-sample-rate = { $path } : la fréquence d’échantillonnage n’a pas été lue ({ $stderr })
ocr-no-blur = { $error } ; sans flou
ocr-models-missing = les modèles OCR sont introuvables
ocr-detection-failed = la détection OCR a échoué ({ $error })
ocr-summary = OCR : { $regions } { $regions ->
    [one] région
   *[other] régions
}, { $localize } à localiser, { $bands } { $bands ->
    [one] bande
   *[other] bandes
}, sub_y={ $sub_y }

## Hardware presets

common-write = écriture de { $what } : { $error }
preset-rtx5090-title = RTX 5090 (32 Go)
preset-rtx4090-title = RTX 4090 (24 Go)
preset-top-subtitle = Qualité maximale : les meilleurs quants en local
preset-gpu16-title = GPU 16 Go
preset-gpu16-subtitle = Haute qualité (4080/4070 Ti et équivalentes)
preset-gpu12-title = GPU 12 Go
preset-gpu12-subtitle = Équilibré (3060/4070 et équivalentes)
preset-gpu8-title = GPU 8 Go
preset-gpu8-subtitle = Économe : quants légers (3060 Ti/4060)
preset-weak-nvidia-cloud-title = NVIDIA modeste + cloud
preset-weak-nvidia-cloud-subtitle = Le lourd (traduction/vision/voix) sur OpenRouter, séparation et ASR sur votre GPU
preset-cloud-title = CPU + cloud (sans NVIDIA)
preset-cloud-subtitle = Le lourd sur OpenRouter, le local sur le processeur ; fonctionne sans carte graphique (clé requise)
preset-custom-title = Personnalisé
preset-custom-subtitle = Je règle chaque paramètre moi-même
preset-reason-no-gpu = Aucun GPU NVIDIA trouvé : mode « CPU + cloud », le lourd sur OpenRouter, le local sur le processeur (plus lent, mais ça marche)
preset-reason-top-card = { $gpu } détectée : quants maximaux
preset-reason-by-vram = { $gpu } · { $vram } Go de VRAM : { $preset }
preset-reason-low-vram = { $gpu } · { $vram } Go de VRAM, c’est peu pour le local ; le cloud est plus fiable
preset-unknown = préréglage inconnu : { $id }

## Service port, shortening, frontend, launch defaults

common-corrupt = { $path } est endommagé : { $error }
common-create-dir = dossier { $path } : { $error }
service-bad-port = { $value } : ce n’est pas un numéro de port (1..65535 attendu)
service-exe-path = chemin de l’exe du service : { $error }
service-request-not-sent = la connexion a été acceptée, mais la requête n’est pas partie : { $error }
service-no-health-answer = la connexion a été acceptée, mais /health n’a pas répondu : { $error }
service-not-http = la réponse n’est pas en HTTP
service-health-status = un serveur HTTP ; /health a répondu { $code }
service-health-not-dub-studio = un serveur HTTP ; /health n’a pas répondu avec un corps Dub Studio
service-health-not-json = un serveur HTTP ; /health n’a pas répondu en JSON
service-health-no-fields = /health se dit { $app }, mais sans les champs du service : { $error }
service-other-app = une autre application ({ $app })
service-health-no-app = un serveur HTTP ; /health sans nom d’application
service-port-reserved = le système ne libère pas le port, alors que personne n’y accepte de connexions (il est peut-être dans une plage réservée par Windows : { $command })
service-port-dub-studio = Dub Studio { $version } l’occupe ({ $executable })
service-port-other = un autre processus l’occupe : { $what }
service-port-busy = Le port 127.0.0.1:{ $port } est occupé depuis { $seconds } s : { $who }. Erreur : { $error }.

    Fermez le programme qui l’occupe, ou indiquez un autre port avec la variable d’environnement { $env } (par exemple { $env }={ $other_port }).
shorten-line-done = raccourcissement { $n }/{ $total } : { $from } -> { $to } caractères
shorten-line-rejected = raccourcissement { $n }/{ $total } : la réponse n’a pas été acceptée ({ $reason })
shorten-line-no-answer = raccourcissement { $n }/{ $total } : le LLM n’a pas répondu : { $error }
shorten-auto-start = { $count } { $count ->
    [one] réplique ne tient
   *[other] répliques ne tiennent
} pas dans leur créneau ; je raccourcis la traduction et ne double que celles-là
shorten-higgs-unloaded = Higgs est déchargé pendant le raccourcissement de la traduction
shorten-auto-no-llm = raccourcissement de la traduction ignoré : le LLM est indisponible : { $error }
shorten-none-shortened = raccourcissement : aucune des { $count } répliques n’a raccourci ; elles restent telles que doublées
shorten-done = { $count } { $count ->
    [one] réplique raccourcie
   *[other] répliques raccourcies
} sur { $total }
shorten-nothing = rien à raccourcir : toutes les répliques tiennent dans leur créneau
shorten-no-llm = raccourcissement : le LLM est indisponible : { $error }
shorten-start = raccourcissement de { $count } { $count ->
    [one] réplique
   *[other] répliques
} : { $provider }
shorten-all-failed = raccourcissement échoué : le LLM n’a répondu à aucune réplique ({ $id } : { $error })
spa-not-built = le frontend n’est pas compilé
settings-bad-speaker-count = speaker_count : un entier de 0 à { $max } est attendu (0 = automatique)
settings-bad-vo-gain = vo_gain_db={ $value } : un nombre de { $min } à { $max } dB est attendu
settings-bad-src-lang = src_lang={ $value } : ni un code de langue ni "auto"
settings-bad-tgt-lang = tgt_lang={ $value } : ce n’est pas un code de langue
settings-bad-casting-ref = casting_ref={ $value } : ce n’est pas le slug d’un profil de casting
settings-style-too-long = tr_style_custom dépasse { $max } caractères
settings-too-many-slots = { $name } : plus de { $max } emplacements
settings-unknown-field = champ inconnu des réglages de lancement : { $key }
settings-read-failed = lecture des réglages de lancement : { $error }
settings-patch-not-object = le corps de PATCH /settings/launch est un objet de champs
settings-write-failed = écriture des réglages de lancement : { $error }

## Takes, translation, TTS text, MCP arguments

takes-delete-old = suppression de l’ancienne prise { $path } : { $error }
takes-missing = la prise { $take } n’est pas dans l’historique de la réplique { $line }
translate-no-vision = le type de contenu n’a pas été déterminé : la vision est indisponible ({ $error })
translate-subs-already-translated = les sous-titres sont déjà dans la langue cible -> pas de traduction automatique (voix seulement)
translate-transcribe-only = transcription : tgt=le texte source, sans traduction
translate-same-language = même langue -> pas de traduction automatique (tgt=la source)
translate-no-llm = la traduction a échoué : le LLM est indisponible : { $error }
translate-ctx-pass = passe de contexte : vision mise en page/scène + traduction de la transcription
translate-failed = la traduction a échoué : { $error }
translate-untranslated = la traduction a échoué : { $left } { $total ->
    [one] ligne
   *[other] lignes
} sur { $total } sont restées dans la langue source (détails dans le journal et logs/llama-server.log)
translate-untranslated-auto = la traduction a échoué : { $left } { $total ->
    [one] ligne
   *[other] lignes
} sur { $total } sont restées dans la langue source ; si la parole de la vidéo est déjà dans la langue cible, indiquez la langue d’origine et aucune traduction ne sera nécessaire (détails dans le journal et logs/llama-server.log)
translate-done = traduction prête : { $done }/{ $total } lignes, titres={ $titles }
translate-left-untranslated = { $left } { $total ->
    [one] ligne
   *[other] lignes
} sur { $total } sont restées dans la langue source (détails dans logs/llama-server.log)
translate-coverage-retry = couverture de la traduction : { $count } { $count ->
    [one] ligne
   *[other] lignes
} sans traduction ; je les retraduis
translate-coverage-failed = couverture de la traduction : la retraduction a échoué ({ $error })
translate-coverage-left = couverture de la traduction : { $count } restent sans traduction
tts-silent-after-cleanup = plus de parole après le nettoyage du texte, donc silence : { $count } { $count ->
    [one] réplique
   *[other] répliques
} ({ $lines })
mcp-bad-speaker-count = speaker_count : un entier de 0 à { $max } est attendu
mcp-speakers-without-diarize = speaker_count supérieur à 1 est incompatible avec diarize=false
