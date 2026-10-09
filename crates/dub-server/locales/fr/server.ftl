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

## Routes: projects, voices, setup, translation, export, jobs, alignment

project-serialize = sérialisation de project.json : { $error }
project-delete-failed = suppression du projet { $path } : { $error }
setup-no-ids = ids est vide
setup-fetching-missing = Téléchargement des modèles qui manquent pour cette fonction…
setup-diarization-missing = Le modèle de diarisation ne s’est pas téléchargé ({ $error }) ; l’analyse se fera sans distinguer les locuteurs
setup-pick-model-files = Fichier(s) du modèle
setup-pick-models-folder = Dossier des modèles prêts
setup-no-folder = le dossier { $path } n’existe pas
voices-not-found = la voix { $name } est introuvable dans voices/
voices-no-vocals = pas de voix pour mesurer F0 ; lancez d’abord l’analyse
voices-speakers-changed = voix par emplacements : les locuteurs ont changé pendant la mesure de la voix ; le projet n’a pas été modifié, relancez
analyze-args-not-object = arguments d’analyze : un objet était attendu
cost-analyze = OpenRouter : ${ $spent } dépensés pour l’analyse (${ $total } utilisés au total)
cost-run = OpenRouter : ${ $spent } dépensés pour l’exécution (${ $total } utilisés au total)
translate-lines-to = Traduction de { $count } { $count ->
    [one] ligne
   *[other] lignes
} → { $lang }
translate-note = traduction : { $note }
translate-titles-failed = les titres n’ont pas été traduits ({ $error }) ; ils resteront dans la langue source dans la vidéo
translate-titles-error = traduction des titres : { $error }
translate-lines-changed = traduction : les répliques ou les titres ont changé pendant la traduction ; le projet n’a pas été modifié, relancez la traduction
export-pick-folder = Où enregistrer les résultats
export-copy-failed = copie vers { $path } : { $error }
jobs-wait-not-number = wait : un nombre de secondes était attendu
jobs-unknown-kind = type de tâche inconnu dans job.json : { $kind }
jobs-not-resumable = une tâche { $kind } ne peut pas reprendre
align-no-source-text = alignement sur la parole : les répliques n’ont pas de texte dans la langue d’origine (les sous-titres ont été importés dans la langue cible)
align-no-vocals = alignement sur la parole : le projet n’a pas de piste vocale ; lancez d’abord l’analyse
align-recognizing = alignement sur la parole : reconnaissance des mots
align-recognition-failed = alignement sur la parole : reconnaissance : { $error }
align-no-speech = alignement sur la parole : aucune parole reconnue ; les temps n’ont pas changé
align-mismatch = alignement sur la parole : les répliques ne correspondent pas à la parole ({ $share } % appariés) ; les temps n’ont pas changé
align-lines-changed = alignement sur la parole : les répliques ont changé pendant la reconnaissance ; les temps n’ont pas changé, relancez l’alignement
align-done = aligné sur la parole : { $share } % des répliques par mots, temps modifiés pour { $changed } ; décalage { $offset } s

## Analysis

analyze-serialize = sérialisation de { $what } : { $error }
analyze-read-model = lecture du modèle { $path } : { $error }
analyze-stage-cache-unreadable = le cache de l’étape { $stage } est illisible ({ $error }) ; recalcul
analyze-checkpoint-not-saved = le point de reprise de l’étape { $stage } n’a pas été enregistré : { $error }
analyze-diarize-continuous = diarisation : { $speakers } { $speakers ->
    [one] locuteur attendu
   *[other] locuteurs attendus
}, une passe continue sur tout l’enregistrement sans réinitialiser les étiquettes à chaque heure
analyze-diarize-fragments = diarisation : { $speakers } { $speakers ->
    [one] locuteur attendu
   *[other] locuteurs attendus
}, appariement des voix entre les fragments
analyze-wespeaker-needed = WeSpeaker est nécessaire pour un nombre de locuteurs imposé : { $error }
analyze-align-skipped = alignement sur la parole ignoré : les sous-titres sont dans la langue cible et la parole dans la langue d’origine
analyze-align-recognizing = alignement des sous-titres sur la parole : reconnaissance des mots
analyze-align-recognition-failed = alignement des sous-titres : reconnaissance de la parole : { $error }
analyze-align-no-speech = alignement sur la parole : aucune parole reconnue ; les temps du fichier sont conservés
analyze-align-mismatch = les sous-titres ne correspondent pas à la parole ({ $share } % des répliques appariées) ; les temps du fichier sont conservés
analyze-align-done = sous-titres alignés sur la parole : { $share } % des répliques par mots, les autres décalées avec leurs voisines ; décalage du fichier { $offset } s
analyze-more = { $count } de plus
analyze-window-plan = piste longue de { $duration } s : plan de { $windows } { $windows ->
    [one] fenêtre
   *[other] fenêtres
} (la première ~{ $first } s) ; l’ASR par fenêtres n’est pas encore actif, traitement d’un bloc
analyze-audio-cached = audio depuis le cache (la source n’a pas changé) ; ffmpeg ignoré
analyze-extracting-audio = extraction de l’audio (ffmpeg -> 16k mono)
analyze-stems-stale = les pistes séparées viennent d’une extraction précédente ; nouvelle séparation
analyze-separating = séparation de la voix ({ $model }) : une voix propre pour la diarisation/l’ASR
analyze-separation-cached = séparation depuis le cache (les pistes sont déjà prêtes)
analyze-separation-failed = la séparation a échoué ({ $error }) ; diarisation/ASR sur l’audio brut
analyze-separator-missing = { $model } est introuvable ; diarisation/ASR sur l’audio brut
analyze-diarizing = diarisation ({ $model })
analyze-diarization-cached = diarisation depuis le cache
analyze-diarization-count-failed = diarisation avec un nombre de locuteurs imposé : { $error }
analyze-diarization-failed = la diarisation a échoué ({ $error }) ; on continue avec un seul locuteur
analyze-diarization-model-missing-count = le modèle { $model } est introuvable : le nombre de locuteurs imposé ne peut pas être appliqué
analyze-subs-no-diarization = sous-titres : sans diarisation (tout le clip d’un bloc)
analyze-diarization-model-missing = le modèle de diarisation est introuvable ; on continue avec un seul locuteur
analyze-fewer-speakers = { $expected } { $expected ->
    [one] locuteur attendu
   *[other] locuteurs attendus
}, { $found } distingués : les voix manquantes n’ont pas été ajoutées
analyze-speakers-matched = voix appariées entre les fragments : { $speakers } { $speakers ->
    [one] locuteur
   *[other] locuteurs
}
analyze-transcript-cached = transcription depuis le cache : { $segments } { $segments ->
    [one] segment
   *[other] segments
}
analyze-read-subs = lecture des sous-titres { $path } : { $error }
analyze-subs-empty = les sous-titres sont illisibles ou vides : { $path }
analyze-subs-imported = sous-titres importés : { $lines } { $lines ->
    [one] réplique
   *[other] répliques
}, { $speakers } { $speakers ->
    [one] locuteur
   *[other] locuteurs
}
analyze-cloud-asr = transcription dans le cloud (OpenRouter STT)
analyze-cloud-stt-failed = STT cloud : { $error }
analyze-cloud-done = cloud : { $lines } { $lines ->
    [one] réplique
   *[other] répliques
}, { $speakers } { $speakers ->
    [one] locuteur
   *[other] locuteurs
}
analyze-hallucination-filter = filtre d’hallucinations de l’ASR : { $error }
analyze-asr-language = Parakeet ne transcrit pas « { $lang } » : il connaît 25 langues européennes. Choisissez Whisper dans Réglages → Modèles (99 langues) ; choisir la langue source dans la fenêtre le bascule pour vous.
analyze-asr-no-words = La vidéo contient { $speech } s de parole et Parakeet n’y a entendu que { $words } mots : il ne connaît que 25 langues européennes. Choisissez la langue source dans la fenêtre au lieu d’Auto — pour une langue hors d’Europe, le studio bascule la reconnaissance sur Whisper.
analyze-hidden-hallucinations = répliques hallucinées de l’ASR masquées (pas de voix) : { $count } : { $lines }
analyze-hidden-by-text = titres et sons de l’ASR masqués d’après leur texte (il y a du son sur l’intervalle, la voix n’est pas séparée de la musique) : { $count } : { $lines }
analyze-voiced-suspects = ressemblent à des hallucinations, mais il y a une voix ; gardées avec une marque : { $count } : { $lines }
analyze-merged-fragments = fusion des fragments : { $before } -> { $after } segments
analyze-characters-by-voice = personnages par voix : { $count }
analyze-segments-speakers = { $segments } { $segments ->
    [one] segment
   *[other] segments
}, { $speakers } { $speakers ->
    [one] locuteur
   *[other] locuteurs
}
analyze-project-unparsable = project.json est illisible ({ $error }) : il contient le glossaire du projet, l’analyse s’est donc arrêtée pour ne pas le perdre
analyze-glossary-fixed = glossaire : { $count } { $count ->
    [one] erreur corrigée
   *[other] erreurs corrigées
} dans la reconnaissance des termes
analyze-no-speech-nodub = aucun segment de parole ; je garde la piste d’origine (nodub)
analyze-auto-nodub = auto : pas de parole à doubler -> NODUB (l’original + texte à l’écran localisé)
analyze-casting-style = descriptions des personnages du profil de casting -> style de traduction ({ $chars } car.)
analyze-empty-removed = segments sans mots supprimés : { $count }
analyze-translation-cached = traduction depuis le cache
analyze-ocr-cached = détection du texte à l’écran depuis le cache
analyze-audio-no-ocr = mode audio : pas de vidéo, la détection du texte à l’écran n’est pas nécessaire
analyze-ocr-off = la détection du texte incrusté est désactivée (la case)
analyze-content-type = type de contenu (auto) : { $kind }
analyze-casting-cached = casting depuis le cache
analyze-casting-checkpoint = le point de reprise du casting n’a pas été enregistré : { $error }
analyze-profile-voices-missing = les voix du profil sont introuvables dans voices/ -> clone : { $voices }
analyze-profile-voices-applied = les voix reprises du profil sont appliquées au doublage ({ $count } { $count ->
    [one] personnage
   *[other] personnages
})
analyze-audio-no-casting = mode audio : pas de vidéo, le casting des personnages n’est pas nécessaire
analyze-cache-not-saved = impossible d’enregistrer cache.json : { $error } (sans gravité)

## Casting, recording, voice library

casting-relabel = réétiquetage par la voix : { $count } { $count ->
    [one] personnage
   *[other] personnages
} par la voix (la diarisation en a trouvé { $found })
casting-skipped-no-speech = casting ignoré : aucun segment de parole
casting-skipped-no-speakers = casting ignoré : aucun locuteur
casting-speakers-ranked = personnages parlants : { $count } (classés par temps de parole)
casting-faces-bound = visages liés aux locuteurs : { $count } sur { $total }
casting-bit-part-skipped = { $character } : le locuteur { $speaker } n’a qu’1 réplique, ni visage ni voix -> ignoré (petit rôle sans casting)
casting-character-face = { $character } : locuteur { $speaker }, { $lines } { $lines ->
    [one] réplique
   *[other] répliques
}, { $seconds } s de parole, visage : oui
casting-character-no-face = { $character } : locuteur { $speaker }, { $lines } { $lines ->
    [one] réplique
   *[other] répliques
}, { $seconds } s de parole, visage : non
casting-profile-other-type = le profil est d’un autre type ({ $previous } ≠ { $current }) ; appariement croisé ignoré
casting-cross-episode = entre épisodes : noms/voix repris : { $count }
casting-saved = casting.json est prêt : { $count } { $count ->
    [one] personnage
   *[other] personnages
}
casting-save-failed = impossible d’écrire casting.json : { $error }
casting-faces-collected = visages collectés : { $faces } ({ $samples } échantillons)
casting-face-clusters = identités de visage (groupes par vecteur) : { $count }
casting-anime-detector-missing = le détecteur d’anime est introuvable ({ $path }) ; sans avatars
casting-anime-detector-failed = le détecteur d’anime ne s’est pas chargé : { $error } ; sans avatars
casting-scrfd-missing = SCRFD est introuvable ; sans avatars (casting par la voix)
casting-scrfd-failed = SCRFD ne s’est pas chargé : { $error } ; sans avatars
casting-embedder-missing = { $model } est introuvable ({ $path }) ; avatars sans embedding
casting-embedder-failed = { $model } ne s’est pas chargé : { $error } ; avatars sans embedding
casting-no-frame = ffmpeg n’a pas extrait l’image
casting-voice-no-vocals = embedding vocal ignoré : pas de voix propre
casting-voice-no-model = voix ignorée : pas de modèle WeSpeaker ({ $path })
casting-voice-model-failed = WeSpeaker ne s’est pas chargé : { $error } ; voix ignorée
casting-voice-trim-failed = échantillon de voix { $speaker } : le découpage a échoué : { $error }
casting-voice-embedding-failed = voix { $speaker } : l’embedding a échoué : { $error }
casting-voice-embeddings = embeddings vocaux : { $count }
casting-applying-profile = application du profil de la bibliothèque : { $slug }
casting-library-profile-missing = le profil de la bibliothèque « { $slug } » est introuvable ; rien n’est appliqué
record-busy = Un enregistrement est déjà en cours
record-mic-not-found = Le micro « { $name } » est introuvable : il est débranché ou renommé ; choisissez-en un autre
record-no-mic = Aucun micro trouvé
record-mic-config = configuration du micro : { $error }
record-create-wav = création du wav : { $error }
record-format-unsupported = le format { $format } n’est pas pris en charge
record-mic-open = ouverture du micro : { $error }
record-mic-start = démarrage du micro : { $error }
record-not-recording = Aucun enregistrement en cours
voices-not-file-list = { $page } : ce n’est pas une liste de fichiers : { $error }
voices-list-endless = { $page } : la liste des fichiers du jeu de données ne finit pas
voices-too-large = { $url } : plus que les { $size } octets fixés
voices-size-mismatch = { $url } : { $got } octets reçus, { $size } fixés
voices-sha-mismatch = { $url } : le SHA-256 { $got } ne correspond pas au { $want } fixé
voices-bad-name = nom invalide
voices-not-in-dataset = la voix { $name } n’est pas dans le jeu de données { $dataset }
voices-bad-size = { $name } : la taille dans le catalogue n’est pas valide
voices-no-sha = { $name } : pas de SHA-256 dans le catalogue
voices-pack-downloading = téléchargement du pack de voix
voices-pack-unpacking = décompression
voices-open-zip = ouverture du zip : { $error }
voices-create-file = création de { $name } : { $error }
voices-unpack-file = décompression de { $name } : { $error }
voices-pack-done = terminé : { $count } { $count ->
    [one] fichier
   *[other] fichiers
}

## Downloads by link

url-no-fetch = le téléchargement { $id } n’existe pas
url-stopped = arrêté
url-already-downloading = ce lien est déjà en téléchargement : { $id }
url-probe-not-json = la réponse de yt-dlp -J n’est pas du JSON : { $error } ; { $stderr }
url-interrupted = le téléchargement s’est arrêté avec le studio ; ce qui a été reçu est dans son dossier et reprendra de là
url-not-a-link = « { $url } » n’est pas un lien : { $error }
url-not-http = « { $url } » n’est pas un lien http(s)
url-bad-subs-lang = « { $lang } » n’est pas un code de langue de sous-titres
url-cookies-not-file = { $path } n’est pas un fichier cookies.txt de 1 Mo au plus
url-cookies-empty-or-large = cookies.txt est vide ou dépasse 1 Mo
url-thread-failed = le fil de téléchargement n’a pas démarré : { $error }
url-cookies-gone = le cookies.txt de ce téléchargement a disparu de son dossier ; relancez le téléchargement avec les cookies
url-disk-space = il faut environ { $need } Mo, { $free } Mo sont libres
url-no-subs = la vidéo n’a pas de sous-titres « { $lang } » faits par des humains (il y a : { $have })
url-no-subs-none = la vidéo n’a pas de sous-titres « { $lang } » faits par des humains (il n’y en a aucun)
url-no-media-file = yt-dlp a terminé, mais il n’y a pas de fichier vidéo dans { $path } : { $stderr }
url-subs-failed = les sous-titres « { $lang } » ne se sont pas téléchargés ({ $code }) : { $error }
url-ytdlp-start = lancement de yt-dlp : { $error }
url-ytdlp-wait = attente de yt-dlp : { $error }
url-still-downloading = { $id } est encore en téléchargement
url-cancelled-removed = { $id } a été annulé et ce qui a été reçu est supprimé : lancez un nouveau téléchargement
url-still-stopping = { $id } est encore en train de s’arrêter ; réessayez dans quelques secondes
url-cancel-first = { $id } est encore en téléchargement ; annulez-le d’abord
url-no-subs-file = yt-dlp a terminé sans fichier de sous-titres dans { $path } : { $stderr }
url-no-parent = { $path } n’a pas de dossier parent
url-subs-empty = les sous-titres { $path } n’ont aucune réplique
url-ytdlp-missing = le composant ytdlp n’est pas téléchargé
url-bad-quality = qualité « { $quality } » : best, 1080, 720, 480 ou audio

## yt-dlp

ytdlp-update-missing = la mise à jour de yt-dlp { $version } ({ $path }) est absente ; la version fixée { $pinned } fonctionne
ytdlp-http-client = client HTTP : { $error }
ytdlp-tag-not-version = la publication de yt-dlp étiquetée « { $tag } » n’est pas une version
ytdlp-no-checksum = { $url } ne contient pas { $file }
ytdlp-new-says = le nouveau yt-dlp { $tag } se présente comme « { $said } » ; { $current } reste
ytdlp-new-failed = le nouveau yt-dlp { $tag } n’a pas démarré : { $error } ; { $current } reste
ytdlp-too-large = { $url } : plus de { $limit } octets
ytdlp-sha-mismatch = { $url } : le SHA-256 { $got } ne correspond pas à SHA2-256SUMS { $want }
ytdlp-exit-code = code de sortie { $code } : { $stderr }
ytdlp-component-missing = le composant ytdlp (yt-dlp et deno) n’est pas téléchargé
ytdlp-no-ffmpeg = pas de ffmpeg : ni le composant ffmpeg, ni ffmpeg.exe dans le PATH
ytdlp-start = lancement de { $program } : { $error }
ytdlp-timeout = yt-dlp n’a pas répondu en { $seconds } s : { $stderr }
ytdlp-playlist = le lien est une playlist ou une chaîne ({ $count } { $count ->
    [one] vidéo
   *[other] vidéos
}), pas une seule vidéo
ytdlp-redirect-only = le lien ne contient pas la vidéo elle-même, seulement un autre lien
ytdlp-live = un direct ({ $status })
ytdlp-thumbnail-not-image = la miniature n’est pas une image ({ $mime })
ytdlp-thumbnail-too-large = la miniature dépasse 4 Mo
ytdlp-failed-silently = yt-dlp a échoué sans message

## Render: voicing, QC, mix, mux

render-input = entrée { $width }x{ $height } durée={ $duration }s
render-nodub-original = nodub : la piste audio d’origine
render-done-audio = terminé (audio seulement) -> { $path }
render-building-ass = construction de l’ASS (titres + sous-titres doublés)
render-burning = incrustation des sous-titres + flou (ffmpeg + libass, NVENC)
render-burn-off = sous-titres/titres désactivés (subs.burn=off)
render-muxing = multiplexage vidéo + audio
render-track-dub = { $lang } (doublage)
render-track-original = { $lang } (original)
render-two-tracks = deux pistes : { $dub } + { $original } -> { $container }
render-multitrack-failed = le multiplexage multipiste a échoué ({ $error }) -> une piste
render-subtitle-tracks = sous-titres en pistes mkv : { $tracks }
render-subtitle-tracks-failed = sous-titres en pistes mkv : { $error }
render-mp4-companion-failed = le mp4 compagnon n’a pas été construit ({ $error }) ; le lecteur ouvrira le mkv (VLC fonctionne)
render-done = terminé -> { $path }
render-dub-audio-done = l’audio doublé est prêt
render-synth-thread-ended = le fil de synthèse s’est terminé sans résultat
render-synth-timeout = la synthèse a dépassé { $seconds } s ; annulée, le moteur est libre
render-engine-stuck = la synthèse ne s’annule pas après { $seconds } s ; le rendu est interrompu (le moteur est bloqué dans la DLL)
render-higgs-load-failed = chargement de la DLL Higgs : { $error }
render-defect-runaway = emballement
render-defect-cutoff = coupure
render-defect-silence = silence
render-defect-hum = bourdonnement
render-recognition-no-answer = la reconnaissance n’a renvoyé aucune réponse
render-second-pass-overflow = la seconde passe de voix a demandé un raccourcissement, désactivé dans celle-ci
render-no-translated-lines = aucune réplique traduite -> silence, la piste d’origine
render-takes-of-removed-lines = historiques de prises des répliques supprimées effacés : { $count }
render-extracting-audio = extraction de l’audio (ffmpeg 44,1k stéréo)
render-separator-missing = le moteur de séparation est introuvable -> sans fond (keep_music off)
render-ref-from-mix = la référence du clone vient du mix non séparé : le fond d’origine s’y entend aussi
render-emotion-ref-failed = segment { $segment } : la référence d’émotion n’a pas été découpée ({ $error }) ; référence d’identité du locuteur
render-cloud-voices = voix cloud par locuteur : { $voices }
render-synth-keys-reset = { $error } ; les clés de synthèse repartent de zéro
render-synthesizing = synthèse de { $count } { $total ->
    [one] segment
   *[other] segments
} sur { $total }
render-voicing-cached = voix depuis le cache : { $count } { $count ->
    [one] segment
   *[other] segments
}
render-cloud-tts-parallel = TTS cloud : { $count } { $count ->
    [one] segment
   *[other] segments
} sur { $threads } fils parallèles
render-cloud-tts-ready = TTS cloud : pré-synthèse prête ({ $count } { $count ->
    [one] segment
   *[other] segments
})
render-takes-quarantined = { $error } ; l’historique des prises de la réplique { $line } est mis de côté dans { $path } et repart de zéro
render-pinned-take = réplique { $line } : la prise épinglée est jouée ; la nouvelle voix ne la remplace pas
render-take-unpinned = réplique { $line } : la prise est désépinglée car le texte a changé
render-selected-take = réplique { $line } : la prise choisie { $take } est jouée ; la nouvelle voix ne la remplace pas
render-write-cloud-segment = écriture du seg{ $line } cloud : { $error }
render-cloud-tts-failed = ⚠ segment { $line } : le TTS cloud a échoué ({ $error }) ; l’original est gardé
render-loading-higgs = chargement de Higgs
render-failures-kept-generated = ⚠ segment { $line } : { $attempts } échecs de synthèse ({ $error }) ; la voix générée est utilisée (amplitude { $range } dB)
render-failures-kept-original = ⚠ segment { $line } : { $attempts } échecs/délais de synthèse ({ $error }) ; la réplique d’origine est gardée
render-regenerating = segment { $line } : { $error } ; régénération ({ $attempt }/{ $attempts })
render-defects-kept-generated = ⚠ segment { $line } : les { $attempts } essais ont un défaut ({ $defect }) ; la voix générée est utilisée (amplitude { $range } dB)
render-silent-kept-original = ⚠ segment { $line } : { $attempts } essais sans son ; l’original est utilisé
render-retry-alt-ref = autre référence
render-retry-temperature = température plus haute
render-defect-regenerating = segment { $line } : défaut de synthèse ({ $defect }) ; régénération ({ $via } { $attempt }/{ $attempts })
render-write-segment = écriture de seg{ $line } : { $error }
render-too-many-artifacts = TTS : trop d’artefacts de bourdonnement ({ $in_a_row } d’affilée, { $retries } nouvelles tentatives au total) ; régénérer n’aide pas. Le problème vient sans doute de la configuration (modèle/VRAM) ou des extraits de référence des voix. Arrêté au segment { $line }.
render-multi-take = segment { $line } : plusieurs prises ; la plus proche du créneau est choisie (écart de { $deviation } s)
render-stretch-over-cap = segment { $line } : il faut étirer x{ $needed } (créneau { $slot } s), plafond x{ $cap } ; le texte est plus rapide que la normale
render-silence-trimmed = découpe des silences du TTS : { $seconds } s retirées sur { $lines } { $lines ->
    [one] réplique
   *[other] répliques
} (dont { $pauses } s de pauses) ; grâce à la découpe l’accélération reste sous le plafond pour { $into_cap } { $into_cap ->
    [one] réplique
   *[other] répliques
}
render-fit-summary = ajustement : { $over }/{ $total } segments au-dessus du plafond ({ $share } %)
render-fit-summary-drift = ajustement : { $over }/{ $total } segments au-dessus du plafond ({ $share } %), synchro rattrapée sur { $drift }
render-qc-start = QC : vérification de { $count } { $count ->
    [one] réplique
   *[other] répliques
} par transcription
render-qc-unheard = QC : { $count } répliques sur { $total } non vérifiées ; la reconnaissance a échoué : { $reason }
render-qc-mismatch = QC : { $count } { $count ->
    [one] réplique ne correspond
   *[other] répliques ne correspondent
} pas à la traduction ; nouvelle synthèse
render-qc-resynthesized = QC : segment { $line } resynthétisé (essai { $attempt })
render-qc-unconfirmed = ⚠ QC : le segment { $line } (« { $text } ») n’a pas pu être confirmé ; vérifiez la réplique à la main
render-qc-kept-mismatch = ⚠ QC : le segment { $line } ne correspond pas au texte traduit ; la voix générée est gardée
render-qc-resynth-unheard = QC : { $count } { $count ->
    [one] réplique resynthétisée non vérifiée
   *[other] répliques resynthétisées non vérifiées
} ; la reconnaissance a échoué : { $reason }
render-qc-summary = bilan QC : { $fixed }/{ $total } corrigées, { $flagged } encore signalées, { $unheard } non vérifiées
render-qc-all-confirmed = QC : toutes les répliques sont confirmées par transcription ✓
render-qc-rest-confirmed = QC : les autres répliques sont confirmées par transcription
render-laying-out = placement du doublage sur la timeline
render-peak-limiter = limiteur de crêtes : { $lines } { $lines ->
    [one] réplique
   *[other] répliques
}, { $samples } échantillons au-dessus du plafond { $ceiling } abaissés sans écrêtage
render-tempo-fit = ajustement du tempo de toute la piste x{ $factor }
render-voiceover-envelope = voix off : l’original à { $db } dB SOUS la traduction, plein dans les pauses (enveloppe dynamique, { $blocks } blocs)
render-voiceover-flat = voix off : l’enveloppe est indisponible -> atténuation uniforme
render-mix-no-ducking = mixage : instrumental + voix doublée (ducking DÉSACTIVÉ, fond complet)
render-mix-ducking = mixage : instrumental + voix doublée (ducking ACTIVÉ, enveloppe, { $blocks } blocs)
render-mix-sidechain = l’enveloppe est indisponible -> ducking en sidechain
render-mix-plain = le sidechain est indisponible -> mixage direct
render-loudness-off = l’égalisation du volume est désactivée : le mix tel quel
render-loudness-normalizing = normalisation du volume (EBU R128, true peak)
render-loudnorm-skipped = loudnorm ignoré ({ $error })
render-track-gain = gain de la piste { $db } dB
render-dub-timings-not-written = les temps du doublage pour les sous-titres n’ont pas été écrits : le placement ({ $lines } répliques, { $spans } plages) ne correspond pas aux segments ({ $segments }) ; les sous-titres suivent les temps d’origine
render-dub-timing-mismatch = temps du doublage : la réplique { $line } de la vue de synthèse ne correspond pas au segment n° { $index } du projet
render-word-timings = temps par mot des sous-titres : reconnaissance de { $count } { $count ->
    [one] réplique
   *[other] répliques
} du doublage
render-refs-unchecked = vérification des références : { $count } { $count ->
    [one] candidat accepté
   *[other] candidats acceptés
} sans vérification ; la reconnaissance a échoué : { $error }
render-refs-all-failed = ⚠ locuteur { $speaker } : aucune référence candidate n’a passé la vérification (entendu : « { $heard } ») ; je prends la meilleure au score
render-speaker-ref-failed = référence du locuteur { $speaker } : { $error }
render-speaker-ref = référence du locuteur { $speaker } : « { $text } » ({ $seconds } s, { $candidates } { $candidates ->
    [one] candidat
   *[other] candidats
}, vérification ok)
render-speaker-ref-unchecked = référence du locuteur { $speaker } : « { $text } » ({ $seconds } s, { $candidates } { $candidates ->
    [one] candidat
   *[other] candidats
}, vérification ⚠ échouée)

## Setup: components

setup-comp-higgs-purpose = Synthèse du doublage et clonage de voix (TTS)
setup-comp-higgs-engine-name = Moteur Higgs (audiocpp_engine)
setup-comp-higgs-engine-purpose = Moteur TTS natif de Higgs (ABI C)
setup-comp-gemma-purpose = Traduction et orchestrateur de vision des sous-titres/titres
setup-comp-gemma-q5-0-purpose = Traduction et vision, plus précise que q4_0
setup-comp-gemma-q6-k-purpose = Traduction et vision, encore plus précise
setup-comp-gemma-q8-0-purpose = Traduction et vision, précision maximale
setup-comp-parakeet-purpose = Reconnaissance de la parole avec horodatage des mots (ASR)
setup-comp-higgs-q6-k-purpose = Synthèse du doublage et clonage de voix (TTS), plus léger que Q8_0
setup-comp-higgs-q4-k-m-purpose = Synthèse du doublage et clonage de voix (TTS), la variante la plus légère
setup-comp-parakeet-fp32-purpose = Reconnaissance de la parole (ASR), pleine précision fp32
setup-comp-parakeet-ultra-purpose = Reconnaissance de la parole (ASR), la version affinée de Moondream, moins d’erreurs
setup-comp-whisper-engine-name = Whisper-Faster (moteur ASR)
setup-comp-whisper-engine-purpose = Un autre moteur de reconnaissance de la parole (faster-whisper) à la place de Parakeet
setup-comp-whisper-cuda-name = Accélération CUDA de Whisper (cuBLAS + cuDNN)
setup-comp-whisper-cuda-purpose = Inférence de Whisper sur le GPU (sinon la reconnaissance tourne sur le CPU, plusieurs fois plus lentement)
setup-comp-whisper-tiny-name = Whisper tiny (modèle ASR)
setup-comp-whisper-tiny-purpose = ASR Whisper, le modèle le plus léger et le plus rapide
setup-comp-whisper-base-name = Whisper base (modèle ASR)
setup-comp-whisper-base-purpose = ASR Whisper, un modèle léger, plus précis que tiny
setup-comp-whisper-small-name = Whisper small (modèle ASR)
setup-comp-whisper-small-purpose = ASR Whisper, un modèle équilibré
setup-comp-whisper-medium-name = Whisper medium (modèle ASR)
setup-comp-whisper-medium-purpose = ASR Whisper, haute précision
setup-comp-whisper-large-v3-name = Whisper large-v3 (modèle ASR)
setup-comp-whisper-large-v3-purpose = ASR Whisper, précision maximale (large-v3)
setup-comp-whisper-large-v3-turbo-name = Whisper large-v3-turbo (modèle ASR)
setup-comp-whisper-large-v3-turbo-purpose = ASR Whisper, presque large-v3 mais nettement plus rapide (turbo)
setup-comp-sortformer-name = Nemotron 3 Diarization (jusqu’à 8 locuteurs)
setup-comp-sortformer-purpose = Distinction des locuteurs (qui parle quand), jusqu’à 8 voix
setup-comp-roformer-purpose = Modèle de séparation voix/instrumental
setup-comp-roformer-q5-purpose = Séparation, plus légère que Q8_0
setup-comp-roformer-q4-purpose = Séparation, la variante la plus légère
setup-comp-casting-name = Modèles de casting des personnages (visages + voix)
setup-comp-casting-purpose = Détection/embedding des visages (réels + anime) + embedding vocal pour le casting
setup-comp-bsroformer-engine-name = Moteur BSRoformer.cpp (CUDA)
setup-comp-bsroformer-engine-purpose = Moteur natif de séparation (bs_roformer-cli + ggml-CUDA)
setup-comp-bsroformer-engine-cpu-name = Moteur BSRoformer.cpp (CPU)
setup-comp-bsroformer-engine-cpu-purpose = Séparation sur le processeur, le mode sans NVIDIA (plus lent, fonction complète)
setup-comp-llama-name = Serveur llama.cpp (CUDA 13.4)
setup-comp-llama-purpose = Serveur annexe pour Gemma (traduction/vision)
setup-comp-onnxruntime-purpose = Environnement d’exécution ASR/OCR/diarisation (strictement 1.28.x)
setup-comp-onnxruntime-gpu-purpose = Fournisseur CUDA pour la diarisation/Parakeet sur le GPU (mode local_backend=gpu)
setup-comp-ffmpeg-purpose = Décodage/encodage vidéo et audio (NVENC)
setup-comp-ytdlp-name = Téléchargement par lien (yt-dlp + deno)
setup-comp-ytdlp-purpose = Télécharger une vidéo par lien (YouTube et les autres sites de yt-dlp) dans un nouveau projet
setup-comp-cuda-runtime-purpose = Bibliothèques CUDA redistribuables pour les moteurs et le CUDA EP d’onnxruntime (sans CUDA Toolkit)
setup-comp-cudnn-purpose = Nécessaire au fournisseur CUDA d’onnxruntime pour la diarisation/Parakeet sur le GPU
setup-comp-vcruntime-purpose = DLL système des moteurs (incluses)
setup-comp-ocr-name = Modèles OCR (PP-OCR ONNX)
setup-comp-ocr-purpose = Détection du texte incrusté → flou (inclus)
setup-comp-nvidia-driver-name = Pilote NVIDIA
setup-comp-nvidia-driver-purpose = Accélération GPU (installé à part, pas par l’application)

## Setup: downloads and installation

setup-http-status = { $url } : statut { $status }
setup-write = écriture : { $error }
setup-not-zip = ce n’est pas un zip : { $error }
setup-zip-entry = entrée du zip : { $error }
setup-open = ouverture de { $path } : { $error }
setup-verifying = Vérification du SHA-256 de { $file }…
setup-install-record = l’enregistrement d’installation : { $error }
setup-disk-space = espace insuffisant : { $need } Go nécessaires, { $free } Go libres ({ $path })
setup-unknown-component = le composant { $id } n’existe pas
setup-not-removable = { $id } n’est pas installé par l’application
setup-component-busy = le composant est en cours de téléchargement ; mettez le téléchargement en pause
setup-paused = le téléchargement est en pause
setup-rate-limited = { $url } : le serveur répond { $status } depuis { $minutes } min
setup-proxy-scheme = proxy { $proxy } : le schéma { $scheme } ne convient pas aux téléchargements (http, https, socks4, socks5)
setup-proxy-no-host = proxy { $proxy } : pas d’hôte
setup-proxy-no-port = proxy { $proxy } : pas de port
setup-proxy-credentials = proxy { $proxy } : le téléchargement des modèles (ureq) ne peut pas transmettre au proxy cet identifiant ou ce mot de passe : il contient / ? #, une espace, du non-ASCII ou (pour SOCKS5) deux-points dans le mot de passe ; les requêtes cloud via ce proxy fonctionnent, le téléchargement demande un mot de passe sans ces caractères
setup-start-failed = { $url } : impossible de démarrer en { $retries } essais : { $error }
setup-chunk-manifest = le manifeste des blocs : { $error }
setup-range-incomplete = plage incomplète : { $got }/{ $want } octets
setup-range-failed = plage { $start }-{ $end } après { $retries } essais : { $error }
setup-range-status = plage { $start }-{ $end } : statut { $status } (206 attendu)
setup-range-read = lecture de la plage : { $error }
setup-create = création de { $path } : { $error }
setup-read = lecture : { $error }
setup-download-failed = { $url } après { $retries } essais : { $error }
setup-size-mismatch = { $file } : { $got } octets téléchargés, { $want } fixés ; le fichier est supprimé, le prochain essai repartira de zéro
setup-hash-mismatch = { $file } : le SHA-256 { $got } ne correspond pas au { $want } fixé ; le fichier est supprimé, le prochain essai repartira de zéro
setup-unpacking = Décompression de { $file }…
setup-rename = renommage de { $path } : { $error }
setup-waiting-other = En attente d’un autre téléchargement des mêmes composants…
setup-delete = suppression de { $path } : { $error }
setup-downloading = Téléchargement des modèles…
setup-source-changed = { $url } : le serveur fournit { $got } octets, { $want } fixés ; la source a changé
setup-manifest-write = manifeste { $path } : { $error }
setup-archive-no-files = l’archive { $path } ne contient aucun des fichiers nécessaires
setup-wheel-not-zip = le wheel n’est pas un zip : { $error }
setup-wheel-entry = entrée du wheel : { $error }
setup-archive-no-dll = l’archive { $path } ne contient pas de bibliothèques
setup-unpack = décompression de { $path } : { $error }
setup-finalize = finalisation de { $path } : { $error }

## Settings applied after analysis

post-analyze-bad-vo-gain = vo_gain : un nombre de dB était attendu, { $value } est arrivé
post-analyze-bad-flag = { $name } : 0 ou 1 était attendu, { $value } est arrivé
post-analyze-bad-container = container : mp4 ou mkv était attendu, { $value } est arrivé
post-analyze-bad-voice-slots = voice_slots : un objet {"{"}male:[…], female:[…]{"}"} était attendu
post-analyze-edit-failed = le réglage après analyse { $edit } : { $error }

## Messages of the engines and libraries: glossary, LLM, ASR, translation, separation, faces, TTS, captions, OCR

glossary-over-limit = le glossaire compte { $total } entrées, plus que { $max }
glossary-empty-term = l’entrée { $entry } n’a pas de terme
glossary-field-too-long = entrée { $entry } (« { $term } ») : un champ dépasse { $max } caractères
glossary-duplicate = le terme « { $term } » figure deux fois (entrées { $first } et { $second })
glossary-tsv-keep = ligne { $line } : keep vaut « { $value } » ; 1 ou 0 attendu
glossary-tsv-empty-term = ligne { $line } : terme manquant
glossary-one-of = envoyez des entrées ou du TSV, pas les deux
glossary-nothing = rien à enregistrer : ni entrées ni TSV
llm-spawn-failed = llama-server n’a pas démarré : { $error }
llm-gguf-missing = le modèle GGUF est introuvable ({ $path })
llm-log-file = journal de llama-server { $path } : { $error }
llm-exited-early = llama-server s’est arrêté avant d’être prêt ({ $status }) ; stderr : { $stderr }
llm-not-ready = llama-server n’a pas démarré en { $secs } s (port { $port }) ; stderr : { $stderr }
llm-http = la requête au modèle a échoué : { $error }
llm-api = API du modèle : { $error }
llm-rejected = l’API du modèle a refusé la requête ({ $status }) : { $body }
llm-empty-answer = le modèle { $model } a renvoyé une réponse vide (sans raison)
llm-empty-answer-reason = le modèle { $model } a renvoyé une réponse vide (finish_reason={ $reason })
llm-cut-short = le modèle { $model } a atteint la limite de { $max_tokens } { $max_tokens ->
    [one] jeton
   *[other] jetons
} (finish_reason=length) : la réponse est incomplète ; il dépense sans doute le budget à raisonner — choisissez un modèle sans raisonnement obligatoire
llm-prompt-cut = le serveur n’a lu que { $read } { $read ->
    [one] jeton
   *[other] jetons
} d’une requête de { $chars } { $chars ->
    [one] caractère
   *[other] caractères
} et en a jeté le début — son contexte est trop petit : augmentez num_ctx dans Ollama ou le Context Length du modèle dans LM Studio
asr-engine = reconnaissance vocale : { $error }
asr-wav-read = impossible de lire le wav { $path } : { $error }
asr-resample = rééchantillonnage : { $error }
asr-speaker-count = le nombre de locuteurs doit être compris entre 1 et { $max }
speakers-embedding = voix du locuteur { $speaker } : { $error }
speakers-no-sample = locuteur { $speaker } : aucun extrait de parole d’au moins 0,3 seconde pour reconnaître la voix
speakers-too-many-voices = nombre de locuteurs indiqué : { $given }, mais l’extrait contient { $found } { $found ->
    [one] voix différente
   *[other] voix différentes
} : impossible de les fusionner sans risque par la voix
speakers-bad-embedding = le modèle de voix a renvoyé des caractéristiques vocales vides ou invalides
speakers-dimension-changed = la taille des caractéristiques vocales a changé
speakers-more-voices = l’extrait contient plus de voix que le nombre de locuteurs indiqué
speakers-unmatched = impossible d’associer de façon fiable les voix de l’extrait au nombre de locuteurs indiqué
translate-format-json = traduction : le modèle répond par un objet JSON selon le schéma ({ $model })
translate-format-json-probe = traduction : essai des réponses par schéma JSON ({ $model }) ; en cas de refus, lignes numérotées
translate-format-numbered = traduction : lignes numérotées ; le modèle { $model } ne déclare pas structured_outputs dans le catalogue OpenRouter
translate-schema-ignored = traduction : { $model } a accepté le schéma JSON mais a répondu en lignes numérotées ; lignes numérotées désormais
translate-schema-ignored-server = traduction : le serveur a accepté le schéma JSON mais a répondu en lignes numérotées ; lignes numérotées désormais
translate-schema-refused = traduction : { $model } a refusé les réponses par schéma JSON ({ $status } : { $body }) ; lignes numérotées désormais
translate-schema-refused-server = traduction : le serveur a refusé les réponses par schéma JSON ({ $status } : { $body }) ; lignes numérotées désormais
translate-line-flawed = traduction : la ligne { $line } garde une traduction avec une remarque : { $reason }
translate-line-failed = traduction : la ligne { $line } n’est pas traduite : { $reason }
translate-line-reason = { $line } : { $reason }
translate-lines-rejected = traduction : { $bad } sur { $total } { $total ->
    [one] ligne
   *[other] lignes
} n’ont pas passé la vérification ({ $reasons })
translate-batch-stopped = traduction : le lot de lignes { $first }..{ $last } a échoué ({ $error }) ; la traduction est arrêtée
translate-batch-failed = traduction : le lot de lignes { $first }..{ $last } a échoué ({ $error })
translate-layout-no-vision = mise en page de l’image : ignorée (aucun modèle de vision choisi ou disponible)
translate-layout-not-needed = mise en page de l’image : ignorée (les sous-titres ne sont pas incrustés, elle n’est pas nécessaire)
translate-layout = mise en page de l’image : sub_style={ $sub_style } titles={ $titles } brands={ $brands }
translate-layout-failed = mise en page de l’image : ignorée ({ $error })
translate-scene-failed = contexte de la scène : ignoré ({ $error })
translate-scene-no-vision = contexte de la scène : ignoré (aucun modèle de vision choisi ou disponible)
translate-audio-failed = contexte audio : ignoré ({ $error })
translate-context-trimmed = traduction : le bloc de contexte de { $chars } { $chars ->
    [one] caractère
   *[other] caractères
} est coupé à { $budget } (protection n_ctx)
translate-names-skipped = traduction : le glossaire automatique des noms est ignoré ({ $error })
translate-chunks = traduction : { $lines } { $lines ->
    [one] ligne
   *[other] lignes
} -> { $chunks } { $chunks ->
    [one] bloc
   *[other] blocs
} (glossaire : { $terms } { $terms ->
    [one] terme
   *[other] termes
}, { $names } { $names ->
    [one] nom
   *[other] noms
})
translate-pass-done = traduction : terminé, { $translated } traduites ({ $flawed } avec remarque), { $untranslated } restées dans la langue source
glossary-pass = glossaire : passage du modèle { $pass }/{ $passes }
glossary-schema-refused = glossaire : le serveur a refusé les réponses par schéma JSON ({ $status }) ; demande du JSON en texte
content-type-decided = type de contenu : { $decided ->
    [anime] animation
   *[other] prises de vues réelles
} ({ $votes } { $votes ->
    [one] réponse claire
   *[other] réponses claires
} sur { $frames } images)
line-missing = absente de la réponse
line-cut = la réponse a été coupée par la limite de jetons
line-untranslated = pas dans la langue cible
line-echo = répète la source
line-too-short = trop courte ({ $got } < { $min })
line-too-long = trop longue ({ $got } > { $max })
line-loop = boucle sur « { $gram } »
line-term-missing = le terme du glossaire « { $term } » manque
translate-frame = extraction d’une image : { $error }
translate-audio = contexte audio : { $error }
translate-empty = le modèle n’a traduit aucune de { $lines } { $lines ->
    [one] ligne
   *[other] lignes
} ; dernière cause : { $reason }
translate-empty-no-reason = le modèle n’a traduit aucune de { $lines } { $lines ->
    [one] ligne
   *[other] lignes
}
translate-contract = la réponse du modèle : { $problem }
translate-contract-answer = la réponse du modèle : { $problem } ; la réponse : { $answer }
answer-no-json-object = elle ne contient pas d’objet JSON
answer-not-json = ce n’est pas du JSON valide ({ $error })
answer-no-terms = elle ne contient pas de liste terms
remix-failed = remix : { $error }
sep-engine-missing = moteur de séparation introuvable ({ $path })
sep-model-missing = modèle de séparation introuvable ({ $path })
sep-spawn = démarrage du moteur de séparation : { $error }
sep-engine-failed = le moteur de séparation a échoué (code { $code }) : { $tail }
sep-engine-killed = le moteur de séparation s’est arrêté sans code de sortie : { $tail }
sep-no-output = le moteur de séparation n’a pas créé la piste vocale ({ $path })
sep-audio-io = audio de la séparation : { $error }
faces-model-missing = modèle introuvable ({ $path })
ffmpeg-exit = ffmpeg s’est terminé avec le code { $code } : { $tail }
ffmpeg-killed = ffmpeg s’est arrêté sans code de sortie : { $tail }
onnx-runtime = l’environnement ONNX : { $error }
files-error = fichiers : { $error }
model-no-outputs = le modèle n’a renvoyé aucune sortie
faces-output-shape = { $model } : la sortie du modèle a une forme inattendue { $shape }
faces-output-count = { $model } : { $expected } sorties du modèle attendues, { $got } reçues
faces-crop-size = l’empreinte du visage attend un recadrage aligné de 112x112, reçu { $width }x{ $height }
faces-sample-rate = l’extrait vocal doit être à { $expected } Hz, il est à { $got } Hz
faces-clip-too-short = l’extrait vocal est trop court pour être analysé
tts-library-load = le moteur de synthèse vocale ne s’est pas chargé : { $error }
tts-cancelled = la génération a été annulée
tts-generation = la génération a échoué : { $error }
tts-invalid-param = paramètre de synthèse vocale invalide : { $error }
tts-streaming-unsupported = cette DLL du moteur ne prend pas en charge le streaming
render-higgs-model-failed = chargement du modèle Higgs : { $error }
captions-write-ass = écriture du fichier de sous-titres : { $error }
captions-filter-script = écriture du script de filtres ffmpeg : { $error }
captions-ffmpeg-wait = attente de ffmpeg : { $error }
captions-ffmpeg-timeout = ffmpeg n’a pas terminé en { $secs } s et a été arrêté (blocage) : { $tail }
captions-burn-failed = l’incrustation des sous-titres a échoué : { $tail }
captions-frame-failed = l’image d’aperçu a échoué : { $tail }
ocr-no-dictionary = le modèle de reconnaissance de texte n’a pas de dictionnaire de caractères
ocr-internal = reconnaissance de texte : erreur interne ({ $error })
