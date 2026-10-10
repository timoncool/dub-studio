<div align="center">

<img src="frontend/public/favicon.svg" width="72" alt="Dub Studio"/>

# Dub Studio

**Studio de doublage vidéo par IA, gratuit et hors ligne, pour Windows —— redouble n'importe quelle vidéo dans une autre langue avec voix clonée, sous-titres traduits et localisation du texte à l'écran. 100% local, zéro Python : un `.exe` natif (Rust + C++/CUDA) ; tous les modèles et moteurs se téléchargent d'un bouton.**

[![License](https://img.shields.io/github/license/timoncool/dub-studio?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/timoncool/dub-studio?style=flat-square)](https://github.com/timoncool/dub-studio/stargazers)
[![Latest release](https://img.shields.io/github/v/release/timoncool/dub-studio?include_prereleases&style=flat-square)](https://github.com/timoncool/dub-studio/releases)
[![Downloads](https://img.shields.io/github/downloads/timoncool/dub-studio/total?style=flat-square)](https://github.com/timoncool/dub-studio/releases)

[English](README.md) · [Русский](README.ru.md) · [中文](README.zh.md) · [Español](README.es.md) · [Português](README.pt.md) · **Français**

### [🌐 Démo en ligne et showcase avant/après →](https://timoncool.github.io/dub-studio/)



</div>

## En action

**[▶ Voir le showcase avant/après sur le site →](https://timoncool.github.io/dub-studio/#showcase)** — de vrais clips doublés de bout en bout sur un GPU local : vidéos, modes et langues différents.

| ![dub](docs/shots/mode-dub-ru.png) | ![voiceover](docs/shots/mode-voiceover-es.png) | ![dub CJK](docs/shots/mode-dub-zh.png) |
|:--:|:--:|:--:|
| 🎙️ **Doublage** · EN→RU | 🗣️ **Voix off** · EN→ES | 🈶 **Doublage** · 中文 à l'image |
| ![subtitles](docs/shots/mode-subtitles-ru.png) | ![widescreen](docs/shots/mode-dub-cinema-fr.png) | ![transcript](docs/shots/mode-transcribe-pt.png) |
| 📝 **Sous-titres** · langue d'origine | 🎬 **Doublage** · large 16:9 | 🔤 **Transcription** · diarisation |

## Qu'est-ce que c'est

**Dub Studio** transforme n'importe quelle vidéo en une version doublée dans une autre langue —— **avec le timbre du locuteur cloné, des sous-titres traduits et le texte incrusté localisé à même l'image**. Déposez un clip : une passe automatique intelligente produit le premier jet ; puis un éditeur en direct met **chaque sous-titre, voix, zone de flou, police et titre** sous votre contrôle avec un aperçu instantané.

Par défaut, tout tourne **localement sur votre machine** —— sans cloud ni abonnement : ni vos rushes ni votre voix ne quittent l'ordinateur. Et si votre PC est limité (ne fait pas tourner le Gemma/Higgs local) ou que vous voulez plus de vitesse et de qualité, les parties lourdes (traduction, vision, TTS, transcription) peuvent **en option** être déléguées au cloud via **OpenRouter** —— chaque moteur choisi séparément (local ↔ cloud), avec des voix attribuées automatiquement selon le sexe du locuteur (bêta). La clé est stockée localement ; tout est désactivé par défaut.

C'est **une réécriture entièrement native**. Pas de Python embarqué, pas de torch, pas de wheels CUDA. Tout le pipeline est en **Rust + moteurs natifs C++/CUDA (GGUF/ONNX)** : un processus, démarrage rapide, faible VRAM. Les modèles, moteurs, runtime CUDA/VC++ et ffmpeg sont **téléchargés et installés par l'application elle-même** au premier lancement. **L'application est conçue et testée pour un GPU NVIDIA** : la voix, la traduction et la vision tournent en local sur CUDA, et les sous-titres sont incrustés dans la vidéo avec NVENC. La séparation, la diarisation et la reconnaissance peuvent tourner sur le CPU et les étapes lourdes peuvent partir vers OpenRouter, mais une machine sans NVIDIA n'est pas une configuration testée : voir *Ce qui tourne où* plus bas.

## Pour les agents IA

Tant que Dub Studio est ouvert, il sert un serveur MCP à l'adresse `http://127.0.0.1:8793/mcp` : un agent comme Claude Code, Claude Desktop, Cursor ou Codex fait tout ce que fait la fenêtre, avec le même code — il crée des projets à partir de fichiers vidéo, les analyse, corrige la traduction, les timings et les locuteurs ligne par ligne, attribue les voix, met en forme les sous-titres, ajoute des titres et des zones de flou, fait le rendu, exporte la même vidéo dans d'autres langues, écrit des SRT et des TXT et enregistre les résultats dans un dossier. Dans les réglages, **Agent IA (MCP)** indique si un agent est connecté et ce qu'il faut coller dans le client.

Avec ce dépôt, un agent peut tout installer et piloter le studio lui-même :

1. Installer le studio depuis la [dernière version](https://github.com/timoncool/dub-studio/releases/latest) et le lancer.
2. Se connecter à son serveur MCP :
   ```bash
   claude mcp add --transport http dub-studio http://127.0.0.1:8793/mcp
   ```
   Autres clients : `{ "mcpServers": { "dub-studio": { "type": "streamable-http", "url": "http://127.0.0.1:8793/mcp" } } }`
3. Lire la skill que sert le serveur (ressource `studio://skill`, prompt `studio`), le même texte que [docs/mcp-skill.md](docs/mcp-skill.md) — tous les outils, les règles de base et des recettes pas à pas — puis commencer par l'outil `studio_status`.

[llms.txt](llms.txt) dit la même chose pour les outils qui le cherchent. Pour garder la skill dans Claude Code, enregistrez [docs/mcp-skill.md](docs/mcp-skill.md) sous `~/.claude/skills/dub-studio/SKILL.md`. Seuls les agents de cet ordinateur et la fenêtre du studio peuvent se connecter.

## Cinq modes, permutables à la volée

| Mode | Ce qu'il fait |
|------|---------------|
| 🎙️ **Doublage** | Re-doublage complet dans la langue cible avec le **timbre original cloné** —— distribution auto par locuteur ou voix au choix |
| 🗣️ **Voix off** | Voix traduite **par-dessus l'original atténué** —— l'original reste audible en dessous ; équilibre réglable |
| 📝 **Sous-titres** | Sous-titres dans la **langue d'origine**, audio original conservé —— sans doublage ni traduction |
| ✨ **Remix amusant** | Donnez un thème (« comme un pirate », « comme un JT ») → le modèle **réécrit tout le script** et redouble |
| 🎬 **Transcription** | **Transcription diarisée** propre avec disposition par locuteur, lecture façon karaoké, création de voix en un clic, export `.srt`/`.txt` |

Chargez un clip une fois et envoyez-le dans n'importe quel mode depuis l'éditeur.

## Fonctionnalités

- **Clonage de voix** —— le timbre original est cloné et parle la nouvelle langue (moteur natif [Higgs Audio v3](https://huggingface.co/bosonai), GGUF). Distribution auto par locuteur ou votre propre voix depuis un pack.
- **Diarisation des locuteurs** —— qui parle et quand (NVIDIA **Nemotron 3 Diarization**, jusqu'à 8 voix), une voix distincte par locuteur.
- **Casting des personnages (bêta)** —— un personnage est une paire **« visage + voix »**. L'app rassemble les visages de toute la vidéo, reconnaît la même personne et **la lie à un locuteur par cooccurrence** (celui en gros plan reçoit la voix, un auditeur en arrière-plan non) ; elle choisit automatiquement l'image-avatar la plus nette et **enregistre un profil de casting pour toute la série** —— vous attribuez voix et descriptions une fois, et l'**épisode suivant les applique tout seul**. Un interrupteur **« visages réels / dessin·anime »** adapte la détection des visages au contenu.
- **Choix du moteur ASR** — transcrivez avec **Parakeet-TDT** (GPU, par défaut) ou **Whisper** ([faster-whisper standalone de Purfview](https://github.com/Purfview/whisper-standalone-win), tourne sur CPU) — choisissez la taille du modèle (tiny … large-v3-turbo) et le quant (compute type) directement dans les réglages.
- **Importer des sous-titres prêts** — utilisez votre `.srt`/`.ass` comme transcription exacte : le texte et le timing viennent du fichier au lieu de la reconnaissance auto (les locuteurs sont quand même attribués par diarisation). Cochez **« sous-titres déjà dans la langue cible »** et la traduction est aussi ignorée — une vidéo anglaise + vos sous-titres russes → un doublage russe directement à partir d'eux.
- **Export multilingue** — la **▾** à côté d'Exporter envoie une vidéo dans plusieurs langues d'un coup ; chacune hérite de toutes vos modifications (mise en page des sous-titres, styles, zones de flou, voix clonée) — seuls la traduction et le doublage changent.
- **Sauvegarder et rouvrir des projets** — sauvegarde auto, liste des projets récents sur l'écran d'accueil, et reprenez le travail inachevé en un clic.
- **Recherche dans les listes de voix et de langues** — tapez une partie d'un nom pour filtrer des centaines de voix ou plus de 100 langues ; les langues correspondent aussi par leur nom dans la langue de l'interface.
- **Pipeline composable** — interrupteurs indépendants à l'entrée : audio (original / doublage / voix off / transcription) × sous-titres (aucun / original / traduits) × incrustation dans la vidéo oui/non × remix humoristique. N'importe quelle combinaison — doublage sans sous-titres, sous-titres traduits sans doublage, doublage humoristique avec vos propres voix — aussi en lot et dans l'éditeur.
- **Localisation du texte à l'écran** —— l'OCR détecte le texte incrusté (**PP-OCR** ONNX), **floute l'original** et imprime par-dessus un titre localisé dans un style assorti —— une fonction qu'aucun autre outil n'a.
- **Traduction + analyse visuelle du style** —— la transcription est traduite localement via **Gemma-4 12B** (GGUF, llama.cpp) ; une passe de vision lit la mise en page : style des sous-titres, titres, marques, zones de texte.
- **Séparation vocale SOTA** —— **Mel-Band Roformer** (BSRoformer.cpp natif sur CUDA) sépare la voix de la musique : la piste de fond est **préservée** et le clone s'accroche à une parole propre.
- **26 préréglages de sous-titres** —— karaoké / mot à mot / hormozi / néon et plus, rendus **sur votre image** (WYSIWYG, JASSUB sur le même `.ass` que grave ffmpeg).
- **Transcription karaoké** —— lisez la vidéo et suivez la ligne et le **mot** en cours qui s'illuminent dans la transcription.
- **Éditeur en direct** —— modifiez transcription, voix, style des sous-titres, zones de flou, titres ; **aperçu ~0,17 s/image**, chaque changement visible aussitôt.
- **Re-génération intelligente** —— à l'export, seuls les segments modifiés sont resynthétisés, pas tout le clip.
- **Vos propres répliques** —— insérez des phrases personnalisées dans la transcription ; chacune est doublée avec la voix clonée du locuteur et affichée dans les sous-titres.
- **Traitement par lot** —— file de fichiers, tous avec un même réglage, progression par fichier.
- **Comparaison avant/après** —— original et doublage côte à côte.
- **100+ langues** —— doublage vers toute langue majeure (espagnol, chinois, japonais, arabe, hindi et plus), détection auto de la langue source.
- **Tout format vidéo** —— MP4, MOV, MKV, WEBM, AVI et plus (décodé via ffmpeg).
- **Installation en un bouton + mise à jour auto** —— modèles, moteurs, runtime CUDA/VC++ et ffmpeg se téléchargent au premier lancement ; l'app se met à jour seule.
- **Téléchargements reprenables** —— les gros modèles (10 Go+) reprennent là où ils se sont arrêtés après une coupure, au lieu de tout recommencer.
- **Calculez chaque étape où vous voulez** —— la séparation, la diarisation et la reconnaissance basculent indépendamment entre **GPU et CPU**, et la reconnaissance, la traduction, la vision et la voix peuvent être déléguées à **OpenRouter**. Le tableau *Ce qui tourne où* ci-dessous indique ce que chaque étape peut réellement utiliser.
- **Adaptez à votre matériel** —— chaque moteur propose plusieurs quantifications (TTS Q8/Q6/Q4, traduction Q4…Q8, ASR int8/fp32 ou Whisper tiny…large-v3-turbo, séparation Q8/Q5/Q4) — changez-les dans les réglages ; limitez le lot de prefill et la durée de référence pour les GPU de 8–12 Go et 32 Go de RAM.
- **Toute l'app dans votre langue** — progression, erreurs, installation, casting et rapports lus par un agent arrivent dans les six langues de la fenêtre.
- **Voix Google Gemini** — Gemini TTS à côté du moteur local et d'OpenRouter, avec sa propre clé et une liste de modèles à jour ; **Batch** coûte moitié moins cher et un rendu arrêté reprend le même lot déjà payé.
- **Nombre de locuteurs** — automatique ou de 1 à 8 personnes ; les voix sont appariées sur tout un long enregistrement, transcrit par fenêtres d'environ 90 secondes.
- **Égaliser le volume** — un interrupteur activé par défaut : phrases au même niveau et doublage à -14 LUFS avec un plafond de -1 dBTP ; désactivé, le mixage reste tel quel.
- **Entièrement portable** —— rien n'est écrit dans votre profil ; supprimez le dossier, aucune trace.

## Captures

Écran d'accueil —— cinq modes, aperçu de la vidéo choisie, choix de langue, tout format :

![Écran d'accueil de Dub Studio](docs/screenshot-home.png)

Mode transcription —— transcription diarisée avec disposition par locuteur, karaoké et création de voix depuis chaque locuteur en un clic :

![Mode transcription de Dub Studio](docs/screenshot-transcribe.png)

## Prérequis

- **OS :** Windows 10 / 11 (x64) ; Linux x86-64 en version expérimentale (voir *Linux (expérimental)*)
- **GPU :** NVIDIA avec 8 Go de VRAM ou plus (des préréglages existent pour 8, 12, 16, 24 et 32 Go) et un pilote récent. La voix locale (Higgs Audio), la traduction et la vision (Gemma) tournent sur CUDA, et les sous-titres sont incrustés avec NVENC. Sans NVIDIA, seules les étapes que *Ce qui tourne où* indique pour le CPU ou le cloud peuvent fonctionner, et cette configuration n'est pas testée
- **WebView2** —— préinstallé sur Windows 11 ; sur Windows 10, l'installateur le télécharge (en cas d'échec, voir *Dépannage*)
- **Disque :** ~15 Go pour les modèles par défaut, les moteurs et le runtime (récupérés au premier lancement), plus de la place pour vos projets ; les quantifications alternatives et les modèles Whisper sont en plus

Sur une machine NVIDIA, la seule chose à installer à la main est un **[pilote NVIDIA](https://www.nvidia.com/Download/index.aspx)** récent. Tout le reste —modèles (Higgs Audio v3, Gemma-4 12B + vision, Parakeet-TDT, Nemotron 3 Diarization, Mel-Band Roformer), moteurs, runtime CUDA et ffmpeg— l'application le télécharge d'un bouton au premier lancement.

## Démarrage rapide

1. **Téléchargez** la version portable depuis [Releases](https://github.com/timoncool/dub-studio/releases) et décompressez où vous voulez (ou installez via `-setup.exe` / `.msi`).
2. **Lancez** `Dub Studio.exe`.
3. Dans le panneau **premier lancement**, cliquez **Tout télécharger** —— l'app récupère modèles, moteurs et runtime (~15 Go, une fois).
4. **Déposez une vidéo**, choisissez la langue cible → la passe auto produit le premier jet. Réglez tout dans l'éditeur puis cliquez **Exporter**.

> Tout se télécharge et vit **dans le dossier de l'app**. Modèles, caches et projets ne vont nulle part ailleurs.

Ce qui a changé et quand se trouve dans [CHANGELOG.md](CHANGELOG.md), et le bouton d'étincelles en haut de l'application l'affiche. Règles pour les contributeurs et les agents de programmation : [AGENTS.md](AGENTS.md).

## Tout ce que l'application télécharge

Le panneau de premier lancement récupère tout cela d'un seul bouton. Derrière un proxy, ou là où Hugging Face est bloqué, réglez le proxy dans Paramètres → **Réseau** : comme dans Windows, le vôtre (HTTP, HTTPS, SOCKS5 ou SOCKS4 ; la forme du vendeur `host:port:identifiant:mot de passe` marche telle quelle) ou aucun. Les téléchargements de modèles, le cloud et les mises à jour de l'application passent tous par lui. Vous pouvez aussi télécharger vous-même les fichiers directs, les placer là où l'indique la dernière colonne, à partir du dossier de l'application (celui qui contient `models\`), puis appuyer sur **Importer depuis un dossier** ; les composants en archives (`.zip`, `.whl`), l'application les télécharge elle-même.

Un fichier à l'emplacement indiqué est considéré comme installé quand sa taille est exactement celle indiquée ; chaque téléchargement est vérifié par rapport au SHA-256 fixé du fichier avant d'être utilisé, et un fichier déjà en place est vérifié de la même façon avant d'être ignoré. **Importer depuis un dossier** (dans le panneau de premier lancement et dans les paramètres des modèles) cherche les fichiers d'un composant par nom et taille exacte dans le dossier choisi et les met en place par lien physique ou copie. Les composants livrés en archives zip ou wheel (moteurs, runtimes) ne sont pas importés et ne comptent comme installés que si l'application les a téléchargés elle-même : elle garde un enregistrement de l'archive vérifiée à côté des fichiers décompressés. Les tailles et les empreintes viennent du manifeste de l'application elle-même, `crates/dub-server/src/setup.rs`, et ce tableau est vérifié par rapport à lui.

Le runtime Visual C++ et les modèles PP-OCR sont fournis dans la version et ne sont pas téléchargés ; le pilote NVIDIA s'installe à part.

<!-- downloads:start -->
| Composant | Besoin | Fichiers (liens directs) | Taille | Où le placer |
|---|---|---|---|---|
| Higgs Audio v3 Q8_0 | obligatoire | [q8_0.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/q8_0.gguf) → `models\higgs-q8_0\q8_0.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/config.json) → `models\higgs-q8_0\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/chat_template.jinja) → `models\higgs-q8_0\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer.json) → `models\higgs-q8_0\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer_config.json) → `models\higgs-q8_0\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q8_0\higgs_audio_v2_tokenizer_config.json` | 5.5 GB | tel quel, au chemin indiqué après la flèche |
| audiocpp_engine.dll (Higgs engine) | obligatoire | [audiocpp_engine.dll](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/engines/audiocpp_engine.dll) → `models\higgs-engine\audiocpp_engine.dll` | 72 MB | tel quel, au chemin indiqué après la flèche |
| Gemma-4 12B QAT q4_0 + vision | obligatoire | [gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\gemma-4-12b-it-qat-q4_0.gguf`<br>[mmproj-gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/mmproj-gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\mmproj-gemma-4-12b-it-qat-q4_0.gguf` | 7.2 GB | tel quel, au chemin indiqué après la flèche |
| Gemma-4 12B Q5_K_M + vision | facultatif | [gemma-4-12b-it-Q5_K_M.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q5_K_M.gguf) → `models\mt-q5_0\gemma-4-12b-it-Q5_K_M.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q5_0\mmproj-F16.gguf` | 8.6 GB | tel quel, au chemin indiqué après la flèche |
| Gemma-4 12B Q6_K + vision | facultatif | [gemma-4-12b-it-Q6_K.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q6_K.gguf) → `models\mt-q6_k\gemma-4-12b-it-Q6_K.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q6_k\mmproj-F16.gguf` | 10.0 GB | tel quel, au chemin indiqué après la flèche |
| Gemma-4 12B Q8_0 + vision | facultatif | [gemma-4-12b-it-Q8_0.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q8_0.gguf) → `models\mt-q8_0\gemma-4-12b-it-Q8_0.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q8_0\mmproj-F16.gguf` | 12.8 GB | tel quel, au chemin indiqué après la flèche |
| Parakeet-TDT 0.6B v3 int8 | obligatoire | [encoder-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx) → `models\tdt\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx) → `models\tdt\decoder_joint-model.int8.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt\config.json` | 671 MB | tel quel, au chemin indiqué après la flèche |
| Higgs Audio v3 Q6_K | facultatif | [q6_k.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/q6_k.gguf) → `models\higgs-q6_k\q6_k.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/config.json) → `models\higgs-q6_k\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/chat_template.jinja) → `models\higgs-q6_k\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer.json) → `models\higgs-q6_k\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer_config.json) → `models\higgs-q6_k\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q6_k\higgs_audio_v2_tokenizer_config.json` | 5.0 GB | tel quel, au chemin indiqué après la flèche |
| Higgs Audio v3 Q4_K_M | facultatif | [q4_k_m.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/q4_k_m.gguf) → `models\higgs-q4_k_m\q4_k_m.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/config.json) → `models\higgs-q4_k_m\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/chat_template.jinja) → `models\higgs-q4_k_m\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer.json) → `models\higgs-q4_k_m\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer_config.json) → `models\higgs-q4_k_m\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q4_k_m\higgs_audio_v2_tokenizer_config.json` | 4.1 GB | tel quel, au chemin indiqué après la flèche |
| Parakeet-TDT 0.6B v3 fp32 | facultatif | [encoder-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx) → `models\tdt-fp32\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx.data) → `models\tdt-fp32\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.onnx) → `models\tdt-fp32\decoder_joint-model.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-fp32\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt-fp32\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-fp32\config.json` | 2.5 GB | tel quel, au chemin indiqué après la flèche |
| Parakeet Ultra 0.6B fp32 (Moondream) | facultatif | [encoder-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx) → `models\tdt-ultra\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx.data) → `models\tdt-ultra\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/decoder_joint-model.onnx) → `models\tdt-ultra\decoder_joint-model.onnx`<br>[vocab.txt](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/vocab.txt) → `models\tdt-ultra\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/tdt/nemo128.onnx) → `models\tdt-ultra\nemo128.onnx`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-ultra\config.json` | 2.6 GB | tel quel, au chemin indiqué après la flèche |
| Parakeet Ultra 0.6B int8 (Moondream) | facultatif | [encoder-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/encoder-model.int8.onnx) → `models\tdt-ultra-int8\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/decoder_joint-model.int8.onnx) → `models\tdt-ultra-int8\decoder_joint-model.int8.onnx`<br>[vocab.txt](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/vocab.txt) → `models\tdt-ultra-int8\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-ultra-int8\nemo128.onnx`<br>[config.json](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/config.json) → `models\tdt-ultra-int8\config.json` | 0.7 GB | tel quel, au chemin indiqué après la flèche |
| Whisper-Faster (faster-whisper standalone) | facultatif | [Whisper-Faster_r192.3_windows.zip](https://github.com/Purfview/whisper-standalone-win/releases/download/faster-whisper/Whisper-Faster_r192.3_windows.zip) | 88 MB | décompresser les fichiers, sans sous-dossiers, dans `tools\whisper\` |
| Whisper CUDA (cuBLAS 11, cuDNN 8) | facultatif | [libcublas-windows-x86_64-11.11.3.6-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-11.11.3.6-archive.zip)<br>[cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip](https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip) | 1.1 GB | prendre tous les .dll de l'archive et les mettre dans `tools\whisper\` |
| Whisper tiny | facultatif | [model.bin](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/model.bin) → `models\whisper\faster-whisper-tiny\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/config.json) → `models\whisper\faster-whisper-tiny\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/tokenizer.json) → `models\whisper\faster-whisper-tiny\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/vocabulary.txt) → `models\whisper\faster-whisper-tiny\vocabulary.txt` | 78 MB | tel quel, au chemin indiqué après la flèche |
| Whisper base | facultatif | [model.bin](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/model.bin) → `models\whisper\faster-whisper-base\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/config.json) → `models\whisper\faster-whisper-base\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/tokenizer.json) → `models\whisper\faster-whisper-base\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/vocabulary.txt) → `models\whisper\faster-whisper-base\vocabulary.txt` | 148 MB | tel quel, au chemin indiqué après la flèche |
| Whisper small | facultatif | [model.bin](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/model.bin) → `models\whisper\faster-whisper-small\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/config.json) → `models\whisper\faster-whisper-small\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/tokenizer.json) → `models\whisper\faster-whisper-small\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/vocabulary.txt) → `models\whisper\faster-whisper-small\vocabulary.txt` | 486 MB | tel quel, au chemin indiqué après la flèche |
| Whisper medium | facultatif | [model.bin](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/model.bin) → `models\whisper\faster-whisper-medium\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/config.json) → `models\whisper\faster-whisper-medium\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/tokenizer.json) → `models\whisper\faster-whisper-medium\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/vocabulary.txt) → `models\whisper\faster-whisper-medium\vocabulary.txt` | 1.5 GB | tel quel, au chemin indiqué après la flèche |
| Whisper large-v3 | facultatif | [model.bin](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/model.bin) → `models\whisper\faster-whisper-large-v3\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/config.json) → `models\whisper\faster-whisper-large-v3\config.json`<br>[preprocessor_config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/tokenizer.json) → `models\whisper\faster-whisper-large-v3\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/vocabulary.json) → `models\whisper\faster-whisper-large-v3\vocabulary.json` | 3.1 GB | tel quel, au chemin indiqué après la flèche |
| Whisper large-v3-turbo | facultatif | [model.bin](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/model.bin) → `models\whisper\faster-whisper-large-v3-turbo\model.bin`<br>[config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/config.json) → `models\whisper\faster-whisper-large-v3-turbo\config.json`<br>[preprocessor_config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3-turbo\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/tokenizer.json) → `models\whisper\faster-whisper-large-v3-turbo\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/vocabulary.json) → `models\whisper\faster-whisper-large-v3-turbo\vocabulary.json` | 1.6 GB | tel quel, au chemin indiqué après la flèche |
| Nemotron 3 Diarization | recommandé | [nemotron3_diar_v3.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/nemotron3_diar_v3.onnx) → `models\nemotron-diar\nemotron3_diar_v3.onnx`<br>[LICENSE](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/LICENSE) → `models\nemotron-diar\LICENSE` | 401 MB | tel quel, au chemin indiqué après la flèche |
| Mel-Band Roformer voc_fv6 Q8_0 | recommandé | [voc_fv6-Q8_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q8_0.gguf) → `models\bsroformer\voc_fv6-Q8_0.gguf` | 252 MB | tel quel, au chemin indiqué après la flèche |
| Mel-Band Roformer voc_fv6 Q5_0 | facultatif | [voc_fv6-Q5_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q5_0.gguf) → `models\bsroformer\voc_fv6-Q5_0.gguf` | 167 MB | tel quel, au chemin indiqué après la flèche |
| Mel-Band Roformer voc_fv6 Q4_0 | facultatif | [voc_fv6-Q4_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q4_0.gguf) → `models\bsroformer\voc_fv6-Q4_0.gguf` | 139 MB | tel quel, au chemin indiqué après la flèche |
| Modèles de casting (visages et voix) | recommandé | [model.onnx](https://huggingface.co/immich-app/buffalo_l/resolve/d09715916a0778919a770c343533641e250b8699/detection/model.onnx) → `models\faces\det_10g.onnx`<br>[LVFace-L_Glint360K.onnx](https://huggingface.co/bytedance-research/LVFace/resolve/b12702ab1f5c721748e054a66dc90e1edd1f0724/LVFace-L_Glint360K/LVFace-L_Glint360K.onnx) → `models\faces\LVFace-L_Glint360K.onnx`<br>[model_feat.onnx](https://huggingface.co/deepghs/ccip_onnx/resolve/eb2acdd29af1703388d3d0c04221add322bc9110/ccip-caformer-24-randaug-pruned/model_feat.onnx) → `models\faces\ccip\model_feat.onnx`<br>[model.onnx](https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_s/model.onnx) → `models\faces\anime_face\model.onnx`<br>[xseg_1.onnx](https://huggingface.co/facefusion/models-3.1.0/resolve/c9e3a503d8e84e91c5cd89ee2d510fe5e793e570/xseg_1.onnx) → `models\faces\occluder\xseg_1.onnx`<br>[voxceleb_resnet34_LM.onnx](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/resolve/f0c48c298fd835726c27956a5d617bad7115627e/voxceleb_resnet34_LM.onnx) → `models\faces\wespeaker\voxceleb_resnet34_LM.onnx` | 1.3 GB | tel quel, au chemin indiqué après la flèche |
| BSRoformer.cpp (CUDA) | recommandé | [BSRoformer-windows-cuda-13.1.0.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-cuda-13.1.0.zip) | 165 MB | décompresser les fichiers, sans sous-dossiers, dans `tools\bsroformer\` |
| BSRoformer.cpp (CPU) | facultatif | [BSRoformer-windows-x64-msvc.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-x64-msvc.zip) | 671 KB | décompresser les fichiers, sans sous-dossiers, dans `tools\bsroformer-cpu\` |
| llama.cpp server (CUDA) | obligatoire | [llama-b11146-bin-win-cuda-13.4-x64.zip](https://github.com/ggml-org/llama.cpp/releases/download/b11146/llama-b11146-bin-win-cuda-13.4-x64.zip) | 150 MB | décompresser les fichiers, sans sous-dossiers, dans `tools\llama\` |
| ONNX Runtime | obligatoire | [onnxruntime-win-x64-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-1.28.2.zip) | 79 MB | décompresser avec son arborescence dans `models\runtime\` |
| ONNX Runtime GPU (CUDA) | recommandé | [onnxruntime-win-x64-gpu_cuda13-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-gpu_cuda13-1.28.2.zip) | 366 MB | décompresser avec son arborescence dans `models\runtime\` |
| FFmpeg (static build) | obligatoire | [ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip](https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-31-13-27/ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip) | 171 MB | prendre ffmpeg.exe et ffprobe.exe de l'archive et les mettre dans `tools\ffmpeg\` |
| yt-dlp + deno | facultatif | [yt-dlp.exe](https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.exe) → `tools\yt-dlp\yt-dlp.exe`<br>[deno-x86_64-pc-windows-msvc.zip](https://github.com/denoland/deno/releases/download/v2.9.7/deno-x86_64-pc-windows-msvc.zip) | 60 MB | tel quel, au chemin indiqué après la flèche<br>décompresser les fichiers, sans sous-dossiers, dans `tools\yt-dlp\` |
| CUDA runtime (cudart, cuBLAS, cuFFT) | obligatoire | [nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/86/00/d5436004268f049214193659ebc36550b5ef3925c3d13b4cc980e13be6f5/nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl)<br>[nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/a3/df/f1246959833e2c437db8be3e5b477f66b87f8817821ed40de6c7561c9a36/nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl)<br>[libcufft-windows-x86_64-12.4.0.43-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcufft/windows-x86_64/libcufft-windows-x86_64-12.4.0.43-archive.zip) | 586 MB | prendre tous les .dll de l'archive et les mettre dans `models\higgs-engine\` |
| cuDNN 9 | recommandé | [nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/87/6a/e55ff0ac26a5c6e2b21f41c9d04ad096b4ed6da593fba7e25845c61b0532/nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl) | 436 MB | prendre tous les .dll de l'archive et les mettre dans `models\higgs-engine\` |
<!-- downloads:end -->

## Ce qui tourne où

Chaque étape a son propre sélecteur de périphérique. *Oui* signifie que le code contient ce chemin ; le tableau ne dit rien de la vitesse ni de la couverture de tests, et les chemins CPU sont plus lents. Les chemins cloud nécessitent une clé OpenRouter (Paramètres) et sont désactivés par défaut.

| Étape | Moteur | GPU NVIDIA | CPU | OpenRouter (cloud) |
|---|---|---|---|---|
| Séparation | Mel-Band Roformer (BSRoformer.cpp) | oui (build CUDA) | oui (build CPU distinct, plus lent) | non |
| Diarisation | Nemotron 3 Diarization | oui (ONNX Runtime CUDA) | oui | non |
| Reconnaissance vocale | Parakeet-TDT ou Whisper-Faster | oui | oui | oui |
| Traduction et vision | Gemma-4 12B (llama.cpp) | oui (build CUDA) | non | oui |
| Voix et clonage | Higgs Audio v3 | oui (CUDA) | non | oui |
| Texte à l'écran | PP-OCR | non | oui | non |
| Casting (visages et voix) | SCRFD, LVFace, anime_face, CCIP, WeSpeaker | non | oui | non |
| Sous-titres incrustés dans la vidéo | ffmpeg | oui (NVENC) | non | non |

Sans NVIDIA, la voix, la traduction, la vision et la reconnaissance peuvent partir dans le cloud et la séparation et la diarisation sur le CPU, mais l'incrustation n'a pas de chemin CPU : une telle machine n'est donc pas une configuration testée.

## Statistiques anonymes et actualités

L'app demande les actualités au serveur de l'auteur au démarrage et toutes les six heures. La requête ne porte aucun id : les actualités arrivent quel que soit votre choix ci-dessous. Les nouvelles s'affichent en haut de Nouveautés et, sans connexion, l'app montre les actualités de sa version.

L'écran du premier lancement a une case **Envoyer des statistiques d'utilisation anonymes**, cochée par défaut. Le même interrupteur se trouve dans Paramètres → Statistiques anonymes, à côté de **Ce qui est envoyé** (le rapport exact du jour) et **Nouvel identifiant d'installation**. Tant qu'elle est cochée, l'app envoie une fois par jour :

- un id d'installation aléatoire créé sur cet ordinateur, lié ni au matériel ni à un compte ; décocher la case le supprime ;
- l'app et sa version, le nom et la version du système, la langue de la fenêtre ;
- la carte graphique : fabricant, tranche de mémoire vidéo (jusqu'à 8, 12, 16, 24+ Go) et si CUDA fonctionne ;
- combien de tâches (analyse, doublage, rendu, export, téléchargement et les autres) se sont terminées, ont échoué ou ont été annulées ce jour-là.
- les modèles de chaque étape (voix, reconnaissance, traduction, analyse d’images, séparation) et combien de tâches les ont utilisés ;
- la raison d’un échec, en une ligne dont les chemins, noms, liens et tout texte entre guillemets sont retirés sur cet ordinateur avant l’envoi ; le serveur garde ces raisons 30 jours.

Jamais : vidéos, transcriptions, traductions, voix, noms de fichiers ou chemins, rien de personnel. Le serveur garde le pays que Cloudflare indique pour la connexion, pas l'adresse IP. `DO_NOT_TRACK=1` ou `STUDIO_TELEMETRY=0` dans l'environnement coupent entièrement les statistiques : aucun id n'existe et rien n'est compté.

## Dépannage

**L'installateur s'arrête sur WebView2.** La fenêtre de l'application repose sur Microsoft Edge WebView2, et l'installateur le télécharge quand Windows ne l'a pas. Sur une connexion bloquée ou instable, ou sur les versions de Windows 10 qui refusent le petit programme d'amorçage de Microsoft (erreur 0x80040902), ce téléchargement échoue. Installez WebView2 avec l'installateur autonome de Microsoft, [Evergreen Standalone x64](https://go.microsoft.com/fwlink/p/?LinkId=2124701), puis relancez l'installateur de Dub Studio.

**Les téléchargements se bloquent ou échouent.** Les gros fichiers reprennent là où ils se sont arrêtés : appuyez de nouveau sur le bouton. Si Hugging Face ou GitHub sont bloqués chez vous, réglez un proxy dans Paramètres → **Réseau**, ou téléchargez à la main les fichiers directs du tableau, placez-les là où l'indique la dernière colonne et appuyez sur **Importer depuis un dossier**.

**L'export s'arrête avec `Unrecognized option 'filter_complex_script'`.** C'était un échec sur ffmpeg 8 et plus récent, corrigé dans la 3.1.1 : mettez l'application à jour. L'application utilise le ffmpeg de `tools\ffmpeg` et, s'il n'y en a pas, accepte celui trouvé dans le `PATH`. Pour signaler une erreur d'export, joignez la sortie complète de ffmpeg tirée du journal.

**La reconnaissance échoue avec `MemcpyToHost` ou `Failed to allocate memory`.** Le GPU a manqué de mémoire pendant la reconnaissance vocale d'un fichier long ou volumineux (issue #4). Fermez les autres programmes qui utilisent le GPU, ou réglez l'étape de reconnaissance sur **CPU** dans les paramètres, puis relancez.

**`CUDA execution provider is not enabled`, ou tout tombe sur un seul locuteur.** Les bibliothèques GPU manquent ou sont trop anciennes pour le pilote. Installez le pilote NVIDIA actuel, appuyez sur le bouton de téléchargement du runtime CUDA, de cuDNN et d'ONNX Runtime GPU dans les paramètres des modèles, ou basculez l'étape sur le CPU.

## Comment ça marche

`analyze()` est une première passe fixe : séparation → ASR avec timing par mot → diarisation → traduction contextuelle + vision (style des sous-titres / titres / marques) → OCR (mise en page / zones de flou). Le résultat est un document **Project** éditable. Chaque modification est un patch dessus avec aperçu ~0,17 s/image ; l'export ne rejoue que **les étapes salies**.

**Stack :** une coque native **Tauri 2 (Rust)** exécute `dub-server` (axum) dans le même processus sur `127.0.0.1:8793` (le MCP pour les agents est `/mcp` sur le même port) et ouvre une fenêtre sur la SPA —— React 19 + Vite + Tailwind + react-konva sur JASSUB. Moteurs : Parakeet-TDT ou Whisper (ASR) · Nemotron 3 Diarization (diarisation) · Gemma-4-12B GGUF (traduction + vision, llama.cpp) · Higgs Audio v3 (TTS) · Mel-Band Roformer (séparation, BSRoformer.cpp) · PP-OCR (ONNX) · ffmpeg/NVENC. **Aucun processus Python à l'exécution.**

### Compiler depuis les sources

```bash
git clone https://github.com/timoncool/dub-studio.git
cd dub-studio

cd frontend && npm install && npm run build && cd ..   # 1) SPA
cargo build --release -p dub-server                     # 2) serveur natif (axum)
cd desktop && npm install && npx tauri build            # 3) coque bureau (Tauri)
```

Nécessite Node 20+, Rust (toolchain MSVC) et WebView2. Les moteurs natifs n'ont pas besoin d'être recompilés —— l'app télécharge des binaires précompilés.

### Linux (expérimental)

Le .deb et l'AppImage pour Linux x86-64 sont **expérimentaux**. Ils viennent du même code avec les versions Linux des mêmes moteurs (llama.cpp, ONNX Runtime, BSRoformer.cpp, ffmpeg, le moteur Higgs pour Linux, yt-dlp, faster-whisper), mais l'auteur travaille sous Windows et ne les a pas lancés sur un vrai bureau Linux. **Si vous vivez sous Linux, ce serait génial que vous les peaufiniez et renvoyiez les corrections en pull request.**

- La voix locale demande une NVIDIA à partir de Turing (GTX 16, RTX 20 et plus récentes), comme sous Windows. Les voix dans le cloud fonctionnent sur n'importe quelle machine.
- Le pilote NVIDIA (580 ou plus récent), `libgomp1` et `libssl3` viennent du système ; les modèles, moteurs et bibliothèques CUDA, l'application les télécharge au premier lancement dans `~/.local/share/dub-studio` (`$XDG_DATA_HOME`).
- Compilés à la main : le workflow `Linux build (experimental)` (`.github/workflows/release-linux.yml`) ou `scripts/build-release-linux.sh <dossier avec models/ocr>`.

## Contributions et forks

**Les collaborateurs sont les bienvenus.** Je serais vraiment ravi de voir Dub Studio porté sur d'autres plateformes et GPU — l'architecture le permet, je n'ai simplement pas le temps de faire les portages moi-même. Si vous le voulez sur **GPU AMD / Intel, macOS ou Linux**, forkez-le — les PR sont les bienvenues. Linux a déjà une version expérimentale (voir *Linux (expérimental)*) : la peaufiner est l'aide la plus bienvenue.

**Les localisations supplémentaires** sont tout aussi bienvenues : l'app et la landing existent en 6 langues aujourd'hui — traduisez les fichiers de langue (`frontend/src/locales/` et le dictionnaire dans `docs/index.html`) et ouvrez une PR pour ajouter la vôtre.

## Auteurs

- **Nerual Dreming** —— [Telegram](https://t.me/nerual_dreming) | [neuro-cartel.com](https://neuro-cartel.com) | fondateur d'[ArtGeneration.me](https://artgeneration.me)
- **Neuro-Soft** —— [Telegram](https://t.me/neuroport) | applis IA portables

## Crédits

- **[Boson AI](https://huggingface.co/bosonai)** —— le modèle Higgs Audio v3, et **[drbaph / Higgs-Audio-v3-Studio](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio)** —— les quantifications GGUF et le moteur natif `audiocpp_engine.dll`.
- **[NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)** (CC-BY-4.0) —— ASR ; poids ONNX de [istupakov/parakeet-tdt-0.6b-v3-onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx), runtime [altunenes/parakeet-rs](https://github.com/altunenes/parakeet-rs).
- **[Parakeet Ultra](https://huggingface.co/moondream/parakeet-ultra)** par **[Moondream](https://huggingface.co/moondream)**, basé sur parakeet-tdt-0.6b-v3 de NVIDIA (CC-BY-4.0) —— ASR affiné optionnel, avec moins d'erreurs de reconnaissance ; export ONNX de [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs). int8: [Masterx/parakeet-tdt-0.6b-ultra-onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx).
- **[NVIDIA Nemotron 3 Diarization](https://huggingface.co/nvidia/Nemotron-3-Diarization)** (Streaming Sortformer v3, [OpenMDW-1.1](https://openmdw.ai/license/1-1/)) —— diarisation des locuteurs, jusqu'à 8 locuteurs ; export ONNX de [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs).
- **[Google Gemma](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf)** —— Gemma-4 12B (traduction et vision), les quantifications de [unsloth](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF) et [llama.cpp](https://github.com/ggml-org/llama.cpp) qui les exécute.
- **[chenmozhijin / BSRoformer.cpp](https://github.com/chenmozhijin/BSRoformer.cpp)** et **[GaboxR67](https://huggingface.co/GaboxR67)** —— le moteur natif de séparation avec ses modèles GGUF, et le point de contrôle Mel-Band Roformer.
- **[Systran / faster-whisper](https://github.com/SYSTRAN/faster-whisper)**, **[deepdml](https://huggingface.co/deepdml)** et **[Purfview](https://github.com/Purfview/whisper-standalone-win)** —— les modèles Whisper au format CTranslate2 et la version autonome qui les exécute.
- **[InsightFace](https://github.com/deepinsight/insightface)** (via [immich-app/buffalo_l](https://huggingface.co/immich-app/buffalo_l)), **[ByteDance LVFace](https://huggingface.co/bytedance-research/LVFace)**, **[deepghs](https://huggingface.co/deepghs)** (CCIP, détection de visages d'anime), **[FaceFusion](https://huggingface.co/facefusion/models-3.1.0)** et **[WeSpeaker](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM)** —— les modèles de visages et de voix du casting de personnages.
- **[PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)** —— PP-OCR, les modèles de détection et de reconnaissance du texte à l'écran.
- **[ONNX Runtime](https://github.com/microsoft/onnxruntime)**, **[FFmpeg](https://ffmpeg.org)** avec les builds de [BtbN](https://github.com/BtbN/FFmpeg-Builds), **[JASSUB](https://github.com/ThaUnknown/jassub)**, **[Tauri](https://tauri.app)** et **[ort](https://github.com/pykeio/ort)**.
- **NVIDIA CUDA runtime, cuBLAS, cuFFT et cuDNN** —— les bibliothèques sur lesquelles tournent les chemins GPU.
- **Serega (SilentBob)** —— version 3.1.0 : multi-take, référence émotionnelle, synchronisation et éditeur de sous-titres. **[@nevoin](https://github.com/nevoin)** —— le journal détaillé de [#1](https://github.com/timoncool/dub-studio/issues/1) à l'origine du correctif ffmpeg 8. **[LongNT2011](https://github.com/LongNT2011)** —— [PR #1](https://github.com/LongNT2011/dub-studio/pull/1) dans son fork, une correction du texte russe codé en dur dans l'interface anglaise, qui a inspiré le travail d'i18n.

## Soutenir l'auteur

Je crée des logiciels open source et je fais de la recherche en IA —— l'essentiel est en accès libre. Les dons me permettent de créer et chercher davantage.

**[Toutes les façons de soutenir](DONATE.md)** | **[dalink.to/nerual_dreming](https://dalink.to/nerual_dreming)** | **[boosty.to/neuro_art](https://boosty.to/neuro_art)**

- **BTC :** `1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC`
- **ETH (ERC20) :** `0xb5db65adf478983186d4897ba92fe2c25c594a0c`
- **USDT (TRC20) :** `TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C`

## Licence

Le code de l'application est sous [MIT](LICENSE). **Les modèles ne le sont pas** : chacun garde sa propre licence, et ce qui suit est ce qu'indiquent aujourd'hui leurs pages. Lisez-les avant de publier ou de vendre une vidéo doublée.

| Composant | Licence | Ce que cela signifie |
|---|---|---|
| Higgs Audio v3 (Boson AI) | [Boson Higgs TTS 3 Research and Non-Commercial License](https://huggingface.co/bosonai/higgs-tts-3-4b/blob/main/LICENSE) | Gratuit pour la recherche, l'usage personnel et, avec le Creator Use Grant, pour les créateurs numériques qui publient et monétisent leur propre contenu en créditant Higgs Audio de Boson AI. L'héberger, le redistribuer ou l'intégrer dans un produit ou un service destiné à des tiers, service de doublage compris, exige une licence commerciale de Boson |
| Parakeet-TDT 0.6B v3 (NVIDIA) et son export ONNX | CC-BY-4.0 | Créditer NVIDIA |
| Parakeet Ultra (Moondream, basé sur NVIDIA Parakeet-TDT) | CC-BY-4.0 | Créditer Moondream et NVIDIA |
| Nemotron 3 Diarization (NVIDIA) | [OpenMDW-1.1](https://openmdw.ai/license/1-1/) | Créditer NVIDIA ; l'export ONNX vient de altunenes/parakeet-rs |
| Gemma-4 12B (Google) | Apache-2.0 sur la page du modèle, qui renvoie aux [conditions Gemma 4 de Google](https://ai.google.dev/gemma/docs/gemma_4_license) | Lisez les deux |
| Mel-Band Roformer voc_fv6 (GaboxR67), GGUF de chenmozhijin | Aucune licence indiquée sur les pages des modèles ; le moteur BSRoformer.cpp est sous MIT | Demandez aux auteurs avant tout usage commercial |
| Modèles Whisper (Systran, deepdml) et faster-whisper | MIT | La version autonome de Purfview n'a pas de fichier de licence dans son dépôt |
| Détecteur de visages SCRFD (InsightFace buffalo_l) | InsightFace : modèles préentraînés **pour la recherche non commerciale uniquement** | Le casting de visages réels est soumis à cette restriction |
| LVFace (ByteDance) | MIT |  |
| CCIP (deepghs) | OpenRAIL | Lisez ses restrictions d'usage |
| anime_face_detection (deepghs) | MIT |  |
| Occulteur xseg_1 (modèles FaceFusion) | Aucune licence indiquée sur la page du modèle | Demandez aux auteurs avant tout usage commercial |
| WeSpeaker ResNet34-LM | CC-BY-4.0 | Créditer WeSpeaker |
| PP-OCR (PaddleOCR) | Apache-2.0 |  |
| llama.cpp, ONNX Runtime, BSRoformer.cpp, JASSUB | MIT |  |
| FFmpeg (build BtbN) | Build GPL de FFmpeg | Fourni comme programme séparé, non lié à l'application |
| NVIDIA CUDA runtime, cuBLAS, cuFFT, cuDNN | Conditions de licence propres à NVIDIA | Téléchargés chez NVIDIA et sur PyPI, absents du dépôt |
