<div align="center">

<img src="frontend/public/favicon.svg" width="72" alt="Логотип Dub Studio"/>

# Dub Studio

**Бесплатная офлайн ИИ-студия дубляжа видео для Windows — переозвучивает любой ролик на другой язык с клоном голоса, переведёнными субтитрами и локализацией вшитого текста. 100% локально, ноль Python: один нативный `.exe` (Rust + C++/CUDA); все модели и движки качаются кнопкой.**

[![License](https://img.shields.io/github/license/timoncool/dub-studio?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/timoncool/dub-studio?style=flat-square)](https://github.com/timoncool/dub-studio/stargazers)
[![Latest release](https://img.shields.io/github/v/release/timoncool/dub-studio?include_prereleases&style=flat-square)](https://github.com/timoncool/dub-studio/releases)
[![Downloads](https://img.shields.io/github/downloads/timoncool/dub-studio/total?style=flat-square)](https://github.com/timoncool/dub-studio/releases)

[English](README.md) · **Русский** · [中文](README.zh.md) · [Español](README.es.md) · [Português](README.pt.md) · [Français](README.fr.md)

### [🌐 Живое демо и showcase «до/после» →](https://timoncool.github.io/dub-studio/)



</div>

## Как это выглядит

**[▶ Смотреть showcase «до/после» на сайте →](https://timoncool.github.io/dub-studio/#showcase)** — реальные ролики, продублированные локально на GPU: разные видео, режимы и языки.

| ![dub](docs/shots/mode-dub-ru.png) | ![voiceover](docs/shots/mode-voiceover-es.png) | ![dub CJK](docs/shots/mode-dub-zh.png) |
|:--:|:--:|:--:|
| 🎙️ **Дубляж** · EN→RU | 🗣️ **Закадровый** · EN→ES | 🈶 **Дубляж** · →中文 на кадре |
| ![subtitles](docs/shots/mode-subtitles-ru.png) | ![widescreen](docs/shots/mode-dub-cinema-fr.png) | ![transcript](docs/shots/mode-transcribe-pt.png) |
| 📝 **Субтитры** · язык оригинала | 🎬 **Дубляж** · широкий 16:9 | 🔤 **Транскрипт** · диаризация |

## Что это

**Dub Studio** превращает любой ролик в дубляж на другом языке — **с клонированным тембром спикера, переведёнными субтитрами и локализованным вшитым текстом прямо на кадре**. Закидываешь клип — умный авто-проход делает первый вариант; дальше живой редактор отдаёт под правку **каждый субтитр, голос, блюр-бокс, шрифт и тайтл** с мгновенным превью.

По умолчанию всё работает **локально на твоём компьютере** — без облака, без подписки: ни видео, ни голос никуда не уходят. А если ПК слабый (не тянет локальную Gemma/Higgs) или нужна скорость и качество — тяжёлые части (перевод, vision, озвучку, распознавание) можно **опционально** вынести в облако через **OpenRouter**: каждый движок выбирается отдельно (локально ↔ облако), голоса кастятся автоматически по полу спикера (бета). Ключ хранится локально, всё выключено по умолчанию.

Это **полностью нативный порт**. Ни embeddable-Python, ни torch, ни CUDA-wheel'ов. Весь пайплайн — **Rust + нативные C++/CUDA-движки (GGUF/ONNX)**: один процесс, быстрый старт, низкое потребление VRAM. Модели, движки, CUDA/VC++-рантайм и ffmpeg приложение **скачивает и ставит само** при первом запуске. **Приложение создано и проверено под видеокарту NVIDIA**: озвучка, перевод и vision локально идут на CUDA, а субтитры вжигаются в видео через NVENC. Сепарация, диаризация и распознавание могут работать на CPU, а тяжёлые стадии можно вынести в OpenRouter, но машина без NVIDIA — непроверенная конфигурация, см. ниже *Что где работает*.

## Для ИИ-агентов

Пока Dub Studio открыта, она отдаёт MCP-сервер по адресу `http://127.0.0.1:8793/mcp`: агент вроде Claude Code, Claude Desktop, Cursor или Codex делает всё, что делает окно, тем же кодом — создаёт проекты из видеофайлов, анализирует их, правит перевод, тайминги и спикеров по строкам, раздаёт голоса, настраивает стиль субтитров, добавляет титры и зоны размытия, рендерит, экспортирует то же видео на другие языки, пишет SRT и TXT и сохраняет результат в папку. В настройках, раздел **Агент (MCP)**, видно, подключён ли агент, и что вставить в клиент.

Получив этот репозиторий, агент может сам всё поставить и работать со студией:

1. Установить студию из [последнего релиза](https://github.com/timoncool/dub-studio/releases/latest) и запустить её.
2. Подключиться к её MCP-серверу:
   ```bash
   claude mcp add --transport http dub-studio http://127.0.0.1:8793/mcp
   ```
   Другие клиенты: `{ "mcpServers": { "dub-studio": { "type": "streamable-http", "url": "http://127.0.0.1:8793/mcp" } } }`
3. Прочитать скилл, который отдаёт сервер (ресурс `studio://skill`, промпт `studio`), — тот же текст, что [docs/mcp-skill.md](docs/mcp-skill.md): все инструменты, базовые правила и пошаговые рецепты, — и начать с инструмента `studio_status`.

[llms.txt](llms.txt) говорит то же для инструментов, которые ищут этот файл. Чтобы скилл был в Claude Code постоянно, сохраните [docs/mcp-skill.md](docs/mcp-skill.md) как `~/.claude/skills/dub-studio/SKILL.md`. Подключиться могут только агенты этого компьютера и собственное окно студии.

## Пять режимов, переключаемых на лету

| Режим | Что делает |
|-------|------------|
| 🎙️ **Дубляж** | Полная переозвучка на целевой язык с **клоном оригинального тембра** — авто-каст по спикерам или свой голос |
| 🗣️ **Закадровый** | Перевод **поверх приглушённого оригинала** — исходник слышно снизу; баланс регулируется |
| 📝 **Субтитры** | Субтитры **на языке оригинала**, оригинальный звук сохранён — без дубляжа и перевода |
| ✨ **Шуточный ремикс** | Задай тему («как пират», «как новости») → модель **переписывает весь скрипт** и переозвучивает |
| 🎬 **Транскрипт** | Чистый **диаризованный транскрипт** с раскладкой по спикерам, караоке-плей, создание голосов в один клик, экспорт `.srt`/`.txt` |

Загрузил ролик один раз — и отправляешь в любой режим прямо в редакторе.

## Возможности

- **Клонирование голоса** — оригинальный тембр клонируется и говорит на новом языке (нативный движок [Higgs Audio v3](https://huggingface.co/bosonai), GGUF). Авто-каст по спикерам или свой голос из пака.
- **Диаризация спикеров** — кто и когда говорит (NVIDIA **Nemotron 3 Diarization**, до 8 голосов), разный голос на каждого.
- **Кастинг персонажей (бета)** — персонаж это **связка «лицо + голос»**. Приложение собирает лица по всему видео, узнаёт одного и того же человека и **привязывает его к спикеру по со-встречаемости** реплик (кто в кадре крупным планом — получает голос, фоновый слушатель — нет); автоматически подбирает самый чёткий кадр-аватар и **сохраняет профиль кастинга на весь сериал** — назначил голоса и описания один раз, на **новой серии всё применяется само**. Тумблер **«Реальные лица / Мультфильм·аниме»** переключает распознавание лиц под тип контента.
- **Выбор ASR-движка** — транскрипция через **Parakeet-TDT** (GPU, по умолчанию) или **Whisper** ([standalone faster-whisper от Purfview](https://github.com/Purfview/whisper-standalone-win), работает на CPU) — размер модели (tiny … large-v3-turbo) и квант (compute type) выбираются прямо в настройках.
- **Импорт готовых субтитров** — загрузи свой `.srt`/`.ass` как готовый транскрипт: текст и тайминг берутся прямо из файла вместо авто-распознавания (спикеров всё равно раздаёт диаризация). Галочка **«субтитры уже на языке перевода»** пропускает и перевод — англ. ролик + твои русские сабы → русский дубляж напрямую из них, без ASR и без MT.
- **Мультиязычный экспорт** — стрелка **▾** у кнопки «Экспорт» отправляет одно видео сразу на несколько языков; каждый наследует все правки (раскладку субтитров, стили, блюр-боксы, клон голоса) — меняются только перевод и озвучка.
- **Сохранение и открытие проектов** — автосейв, список недавних проектов на старте, возврат к незаконченному в один клик.
- **Поиск в списках голосов и языков** — печатаешь часть имени, список сотен голосов или 100+ языков сужается; языки ищутся и по названию на языке интерфейса.
- **Композируемый пайплайн** — независимые тумблеры на входе: аудио (оригинал / дубляж / закадровый / транскрипт) × субтитры (нет / оригинал / перевод) × вжигать на видео × шуточный ремикс. Любые сочетания — дубляж без субтитров, перевод субтитров без дубляжа, шуточный дубляж своими голосами — в пакете и в редакторе тоже.
- **Локализация вшитого текста** — OCR детектит текст на кадре (**PP-OCR** ONNX), **блюрит оригинал** и печатает поверх локализованный титр в подобранном стиле — фича, которой нет ни у одного другого инструмента.
- **Перевод + vision-анализ стиля** — весь транскрипт переводится локально через **Gemma-4 12B** (GGUF, llama.cpp); vision-проход разбирает раскладку кадра: стиль субтитров, тайтлы, бренды, зоны текста.
- **SOTA вокал-сепарация** — **Mel-Band Roformer** (нативный BSRoformer.cpp на CUDA) отделяет голос от музыки: фон **сохраняется**, клон цепляется за чистую речь.
- **26 пресетов субтитров** — karaoke / word-by-word / hormozi / neon и другие, отрисованные **на твоём кадре** (WYSIWYG, JASSUB поверх того же `.ass`, что уходит в ffmpeg-burn).
- **Караоке-транскрипт** — воспроизводишь видео, а в транскрипте подсвечивается текущая строка и текущее **слово**.
- **Живой редактор** — правь транскрипт, голоса, стиль субтитров, блюр-боксы, тайтлы; **превью ~0.17 с/кадр**, каждая правка видна сразу.
- **Умный ре-ген** — при экспорте пересчитываются **только правленые сегменты**, а не весь ролик.
- **Свои реплики** — добавляй в транскрипт собственные фразы; каждая озвучивается склонированным голосом спикера и попадает в субтитры.
- **Пакетная обработка** — очередь файлов, все одними настройками, прогресс по каждому.
- **Сравнение до/после** — оригинал и дубляж бок о бок.
- **100+ языков** — дубляж на любой крупный язык (испанский, китайский, японский, арабский, хинди и другие), авто-детект языка источника.
- **Любые видео-форматы** — MP4, MOV, MKV, WEBM, AVI и другие (декод через ffmpeg).
- **Установка в одну кнопку + авто-обновление** — модели, движки, CUDA/VC++-рантайм и ffmpeg тянутся при первом запуске; приложение обновляет себя само.
- **Устойчивая докачка** — большие модели (10 ГБ+) при обрыве связи докачиваются с места, а не заново.
- **Считай где хочешь** — сепарация, диаризация и распознавание по отдельности переключаются между **GPU и CPU**, а распознавание, перевод, vision и озвучку можно вынести в **OpenRouter**. Таблица *Что где работает* ниже показывает, что реально может использовать каждая стадия.
- **Настройка под железо** — у каждого движка несколько квантов (TTS Q8/Q6/Q4, перевод Q4…Q8, ASR int8/fp32 или Whisper tiny…large-v3-turbo, сепарация Q8/Q5/Q4) — переключаются в настройках; лимиты prefill-батча и длины реф-клипа под 8–12 ГБ GPU и 32 ГБ RAM.
- **Полностью портативная** — ничего не пишется в профиль пользователя; удалил папку — не осталось следа.

## Скриншоты

Главный экран — пять режимов, превью выбранного видео, выбор языков, любые видео-форматы:

![Главный экран Dub Studio](docs/screenshot-home.png)

Режим «Транскрипт» — диаризованный транскрипт с раскладкой по спикерам, караоке-плеем и созданием голосов из каждого спикера в один клик:

![Режим транскрипции Dub Studio](docs/screenshot-transcribe.png)

## Требования

- **ОС:** Windows 10 / 11 (x64); Linux x86-64 — экспериментальная сборка (см. *Linux (экспериментально)*)
- **GPU:** NVIDIA от 8 ГБ VRAM (есть пресеты для 8, 12, 16, 24 и 32 ГБ) и свежий драйвер. Локальные озвучка (Higgs Audio), перевод и vision (Gemma) идут на CUDA, а субтитры вжигаются через NVENC. Без NVIDIA могут работать только стадии, которые в таблице *Что где работает* отмечены для CPU или облака, и такая конфигурация не проверялась
- **WebView2** — предустановлен в Windows 11; в Windows 10 его скачивает установщик (если не вышло — см. *Решение проблем*)
- **Диск:** ~15 ГБ на модели по умолчанию, движки и рантайм (тянутся при первом запуске) + место под проекты; альтернативные кванты и модели Whisper — сверх этого

На машине с NVIDIA вручную ставится только свежий **[драйвер NVIDIA](https://www.nvidia.com/Download/index.aspx)**. Всё остальное — модели (Higgs Audio v3, Gemma-4 12B + vision, Parakeet-TDT, Nemotron 3 Diarization, Mel-Band Roformer), движки, CUDA-рантайм и ffmpeg — приложение скачивает кнопкой при первом запуске.

## Быстрый старт

1. **Скачай** портативную сборку из [Releases](https://github.com/timoncool/dub-studio/releases) и распакуй в любую папку (или поставь через `-setup.exe` / `.msi`).
2. **Запусти** `Dub Studio.exe`.
3. В панели **«Первый запуск»** нажми **«Скачать всё»** — приложение тянет модели, движки и рантайм (~15 ГБ, один раз). Нет драйвера NVIDIA — кнопка откроет сайт.
4. **Закинь видео**, выбери язык перевода → авто-проход делает первый вариант. Дальше правишь всё в редакторе и жмёшь **«Экспорт»**.

> Всё качается и хранится **внутри папки приложения**. Модели, кэши и проекты никуда больше не попадают.

Что и когда менялось — в [CHANGELOG.md](CHANGELOG.md), а кнопка с искрами вверху приложения показывает то же самое. Правила для контрибьюторов и кодинг-агентов: [AGENTS.md](AGENTS.md).

## Всё, что скачивает приложение

Панель «Первый запуск» тянет всё это одной кнопкой. За прокси или там, где закрыт Hugging Face, задай прокси в Настройках → **Сеть**: как в Windows, свой (HTTP, HTTPS, SOCKS5 или SOCKS4; запись продавца `host:port:логин:пароль` подходит как есть) или без прокси. Через него идут закачка моделей, облако и обновления приложения. Прямые файлы можно скачать и самому: положи их туда, что указано в последней колонке, считая от папки приложения (той, где лежит `models\`), и нажми **«Импорт из папки»**; компоненты-архивы (`.zip`, `.whl`) приложение качает само.

Файл на указанном месте считается установленным, если его размер в точности совпадает с указанным; каждая закачка сверяется с закреплённым SHA-256 файла до того, как файл пойдёт в дело, а уже лежащий файл перед пропуском проверяется так же. Кнопка **«Импорт из папки»** (в панели «Первый запуск» и в настройках моделей) ищет файлы компонента по имени и точному размеру в выбранной папке и ставит их на место жёсткой ссылкой или копией. Компоненты из zip и wheel (движки, рантаймы) не импортируются и считаются установленными, только если их скачало само приложение: рядом с распакованными файлами оно хранит запись о проверенном архиве. Размеры и хэши взяты из собственного манифеста приложения, `crates/dub-server/src/setup.rs`, и таблица сверяется с ним.

Рантайм Visual C++ и модели PP-OCR идут в комплекте релиза и не скачиваются; драйвер NVIDIA ставится отдельно.

<!-- downloads:start -->
| Компонент | Нужен | Файлы (прямые ссылки) | Размер | Куда положить |
|---|---|---|---|---|
| Higgs Audio v3 Q8_0 | обязателен | [q8_0.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/q8_0.gguf) → `models\higgs-q8_0\q8_0.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/config.json) → `models\higgs-q8_0\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/chat_template.jinja) → `models\higgs-q8_0\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer.json) → `models\higgs-q8_0\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer_config.json) → `models\higgs-q8_0\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q8_0\higgs_audio_v2_tokenizer_config.json` | 5.5 GB | как есть, по пути после стрелки |
| audiocpp_engine.dll (Higgs engine) | обязателен | [audiocpp_engine.dll](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/engines/audiocpp_engine.dll) → `models\higgs-engine\audiocpp_engine.dll` | 72 MB | как есть, по пути после стрелки |
| Gemma-4 12B QAT q4_0 + vision | обязателен | [gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\gemma-4-12b-it-qat-q4_0.gguf`<br>[mmproj-gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/mmproj-gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\mmproj-gemma-4-12b-it-qat-q4_0.gguf` | 7.2 GB | как есть, по пути после стрелки |
| Gemma-4 12B Q5_K_M + vision | по желанию | [gemma-4-12b-it-Q5_K_M.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q5_K_M.gguf) → `models\mt-q5_0\gemma-4-12b-it-Q5_K_M.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q5_0\mmproj-F16.gguf` | 8.6 GB | как есть, по пути после стрелки |
| Gemma-4 12B Q6_K + vision | по желанию | [gemma-4-12b-it-Q6_K.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q6_K.gguf) → `models\mt-q6_k\gemma-4-12b-it-Q6_K.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q6_k\mmproj-F16.gguf` | 10.0 GB | как есть, по пути после стрелки |
| Gemma-4 12B Q8_0 + vision | по желанию | [gemma-4-12b-it-Q8_0.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q8_0.gguf) → `models\mt-q8_0\gemma-4-12b-it-Q8_0.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q8_0\mmproj-F16.gguf` | 12.8 GB | как есть, по пути после стрелки |
| Parakeet-TDT 0.6B v3 int8 | обязателен | [encoder-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx) → `models\tdt\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx) → `models\tdt\decoder_joint-model.int8.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt\config.json` | 671 MB | как есть, по пути после стрелки |
| Higgs Audio v3 Q6_K | по желанию | [q6_k.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/q6_k.gguf) → `models\higgs-q6_k\q6_k.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/config.json) → `models\higgs-q6_k\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/chat_template.jinja) → `models\higgs-q6_k\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer.json) → `models\higgs-q6_k\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer_config.json) → `models\higgs-q6_k\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q6_k\higgs_audio_v2_tokenizer_config.json` | 5.0 GB | как есть, по пути после стрелки |
| Higgs Audio v3 Q4_K_M | по желанию | [q4_k_m.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/q4_k_m.gguf) → `models\higgs-q4_k_m\q4_k_m.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/config.json) → `models\higgs-q4_k_m\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/chat_template.jinja) → `models\higgs-q4_k_m\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer.json) → `models\higgs-q4_k_m\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer_config.json) → `models\higgs-q4_k_m\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q4_k_m\higgs_audio_v2_tokenizer_config.json` | 4.1 GB | как есть, по пути после стрелки |
| Parakeet-TDT 0.6B v3 fp32 | по желанию | [encoder-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx) → `models\tdt-fp32\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx.data) → `models\tdt-fp32\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.onnx) → `models\tdt-fp32\decoder_joint-model.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-fp32\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt-fp32\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-fp32\config.json` | 2.5 GB | как есть, по пути после стрелки |
| Parakeet Ultra 0.6B fp32 (Moondream) | по желанию | [encoder-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx) → `models\tdt-ultra\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx.data) → `models\tdt-ultra\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/decoder_joint-model.onnx) → `models\tdt-ultra\decoder_joint-model.onnx`<br>[vocab.txt](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/vocab.txt) → `models\tdt-ultra\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/tdt/nemo128.onnx) → `models\tdt-ultra\nemo128.onnx`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-ultra\config.json` | 2.6 GB | как есть, по пути после стрелки |
| Parakeet Ultra 0.6B int8 (Moondream) | по желанию | [encoder-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/encoder-model.int8.onnx) → `models\tdt-ultra-int8\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/decoder_joint-model.int8.onnx) → `models\tdt-ultra-int8\decoder_joint-model.int8.onnx`<br>[vocab.txt](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/vocab.txt) → `models\tdt-ultra-int8\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-ultra-int8\nemo128.onnx`<br>[config.json](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/config.json) → `models\tdt-ultra-int8\config.json` | 0.7 GB | как есть, по пути после стрелки |
| Whisper-Faster (faster-whisper standalone) | по желанию | [Whisper-Faster_r192.3_windows.zip](https://github.com/Purfview/whisper-standalone-win/releases/download/faster-whisper/Whisper-Faster_r192.3_windows.zip) | 88 MB | распаковать файлы без подпапок в `tools\whisper\` |
| Whisper CUDA (cuBLAS 11, cuDNN 8) | по желанию | [libcublas-windows-x86_64-11.11.3.6-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-11.11.3.6-archive.zip)<br>[cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip](https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip) | 1.1 GB | взять все .dll из архива в `tools\whisper\` |
| Whisper tiny | по желанию | [model.bin](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/model.bin) → `models\whisper\faster-whisper-tiny\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/config.json) → `models\whisper\faster-whisper-tiny\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/tokenizer.json) → `models\whisper\faster-whisper-tiny\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/vocabulary.txt) → `models\whisper\faster-whisper-tiny\vocabulary.txt` | 78 MB | как есть, по пути после стрелки |
| Whisper base | по желанию | [model.bin](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/model.bin) → `models\whisper\faster-whisper-base\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/config.json) → `models\whisper\faster-whisper-base\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/tokenizer.json) → `models\whisper\faster-whisper-base\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/vocabulary.txt) → `models\whisper\faster-whisper-base\vocabulary.txt` | 148 MB | как есть, по пути после стрелки |
| Whisper small | по желанию | [model.bin](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/model.bin) → `models\whisper\faster-whisper-small\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/config.json) → `models\whisper\faster-whisper-small\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/tokenizer.json) → `models\whisper\faster-whisper-small\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/vocabulary.txt) → `models\whisper\faster-whisper-small\vocabulary.txt` | 486 MB | как есть, по пути после стрелки |
| Whisper medium | по желанию | [model.bin](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/model.bin) → `models\whisper\faster-whisper-medium\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/config.json) → `models\whisper\faster-whisper-medium\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/tokenizer.json) → `models\whisper\faster-whisper-medium\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/vocabulary.txt) → `models\whisper\faster-whisper-medium\vocabulary.txt` | 1.5 GB | как есть, по пути после стрелки |
| Whisper large-v3 | по желанию | [model.bin](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/model.bin) → `models\whisper\faster-whisper-large-v3\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/config.json) → `models\whisper\faster-whisper-large-v3\config.json`<br>[preprocessor_config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/tokenizer.json) → `models\whisper\faster-whisper-large-v3\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/vocabulary.json) → `models\whisper\faster-whisper-large-v3\vocabulary.json` | 3.1 GB | как есть, по пути после стрелки |
| Whisper large-v3-turbo | по желанию | [model.bin](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/model.bin) → `models\whisper\faster-whisper-large-v3-turbo\model.bin`<br>[config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/config.json) → `models\whisper\faster-whisper-large-v3-turbo\config.json`<br>[preprocessor_config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3-turbo\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/tokenizer.json) → `models\whisper\faster-whisper-large-v3-turbo\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/vocabulary.json) → `models\whisper\faster-whisper-large-v3-turbo\vocabulary.json` | 1.6 GB | как есть, по пути после стрелки |
| Nemotron 3 Diarization | рекомендуется | [nemotron3_diar_v3.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/nemotron3_diar_v3.onnx) → `models\nemotron-diar\nemotron3_diar_v3.onnx`<br>[LICENSE](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/LICENSE) → `models\nemotron-diar\LICENSE` | 401 MB | как есть, по пути после стрелки |
| Mel-Band Roformer voc_fv6 Q8_0 | рекомендуется | [voc_fv6-Q8_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q8_0.gguf) → `models\bsroformer\voc_fv6-Q8_0.gguf` | 252 MB | как есть, по пути после стрелки |
| Mel-Band Roformer voc_fv6 Q5_0 | по желанию | [voc_fv6-Q5_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q5_0.gguf) → `models\bsroformer\voc_fv6-Q5_0.gguf` | 167 MB | как есть, по пути после стрелки |
| Mel-Band Roformer voc_fv6 Q4_0 | по желанию | [voc_fv6-Q4_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q4_0.gguf) → `models\bsroformer\voc_fv6-Q4_0.gguf` | 139 MB | как есть, по пути после стрелки |
| Модели кастинга (лица и голос) | рекомендуется | [model.onnx](https://huggingface.co/immich-app/buffalo_l/resolve/d09715916a0778919a770c343533641e250b8699/detection/model.onnx) → `models\faces\det_10g.onnx`<br>[LVFace-L_Glint360K.onnx](https://huggingface.co/bytedance-research/LVFace/resolve/b12702ab1f5c721748e054a66dc90e1edd1f0724/LVFace-L_Glint360K/LVFace-L_Glint360K.onnx) → `models\faces\LVFace-L_Glint360K.onnx`<br>[model_feat.onnx](https://huggingface.co/deepghs/ccip_onnx/resolve/eb2acdd29af1703388d3d0c04221add322bc9110/ccip-caformer-24-randaug-pruned/model_feat.onnx) → `models\faces\ccip\model_feat.onnx`<br>[model.onnx](https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_s/model.onnx) → `models\faces\anime_face\model.onnx`<br>[xseg_1.onnx](https://huggingface.co/facefusion/models-3.1.0/resolve/c9e3a503d8e84e91c5cd89ee2d510fe5e793e570/xseg_1.onnx) → `models\faces\occluder\xseg_1.onnx`<br>[voxceleb_resnet34_LM.onnx](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/resolve/f0c48c298fd835726c27956a5d617bad7115627e/voxceleb_resnet34_LM.onnx) → `models\faces\wespeaker\voxceleb_resnet34_LM.onnx` | 1.3 GB | как есть, по пути после стрелки |
| BSRoformer.cpp (CUDA) | рекомендуется | [BSRoformer-windows-cuda-13.1.0.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-cuda-13.1.0.zip) | 165 MB | распаковать файлы без подпапок в `tools\bsroformer\` |
| BSRoformer.cpp (CPU) | по желанию | [BSRoformer-windows-x64-msvc.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-x64-msvc.zip) | 671 KB | распаковать файлы без подпапок в `tools\bsroformer-cpu\` |
| llama.cpp server (CUDA) | обязателен | [llama-b11146-bin-win-cuda-13.4-x64.zip](https://github.com/ggml-org/llama.cpp/releases/download/b11146/llama-b11146-bin-win-cuda-13.4-x64.zip) | 150 MB | распаковать файлы без подпапок в `tools\llama\` |
| ONNX Runtime | обязателен | [onnxruntime-win-x64-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-1.28.2.zip) | 79 MB | распаковать с деревом папок в `models\runtime\` |
| ONNX Runtime GPU (CUDA) | рекомендуется | [onnxruntime-win-x64-gpu_cuda13-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-gpu_cuda13-1.28.2.zip) | 366 MB | распаковать с деревом папок в `models\runtime\` |
| FFmpeg (static build) | обязателен | [ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip](https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-31-13-27/ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip) | 171 MB | взять ffmpeg.exe и ffprobe.exe из архива в `tools\ffmpeg\` |
| yt-dlp + deno | по желанию | [yt-dlp.exe](https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.exe) → `tools\yt-dlp\yt-dlp.exe`<br>[deno-x86_64-pc-windows-msvc.zip](https://github.com/denoland/deno/releases/download/v2.9.7/deno-x86_64-pc-windows-msvc.zip) | 60 MB | как есть, по пути после стрелки<br>распаковать файлы без подпапок в `tools\yt-dlp\` |
| CUDA runtime (cudart, cuBLAS, cuFFT) | обязателен | [nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/86/00/d5436004268f049214193659ebc36550b5ef3925c3d13b4cc980e13be6f5/nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl)<br>[nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/a3/df/f1246959833e2c437db8be3e5b477f66b87f8817821ed40de6c7561c9a36/nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl)<br>[libcufft-windows-x86_64-12.4.0.43-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcufft/windows-x86_64/libcufft-windows-x86_64-12.4.0.43-archive.zip) | 586 MB | взять все .dll из архива в `models\higgs-engine\` |
| cuDNN 9 | рекомендуется | [nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/87/6a/e55ff0ac26a5c6e2b21f41c9d04ad096b4ed6da593fba7e25845c61b0532/nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl) | 436 MB | взять все .dll из архива в `models\higgs-engine\` |
<!-- downloads:end -->

## Что где работает

У каждой стадии свой переключатель устройства. *Да* значит, что в коде есть такой путь; таблица ничего не говорит о скорости и покрытии тестами, а пути на CPU медленнее. Облачные пути требуют ключ OpenRouter (Настройки) и по умолчанию выключены.

| Стадия | Движок | GPU NVIDIA | CPU | OpenRouter (облако) |
|---|---|---|---|---|
| Сепарация | Mel-Band Roformer (BSRoformer.cpp) | да (CUDA-сборка) | да (отдельная CPU-сборка, медленнее) | нет |
| Диаризация | Nemotron 3 Diarization | да (ONNX Runtime CUDA) | да | нет |
| Распознавание речи | Parakeet-TDT или Whisper-Faster | да | да | да |
| Перевод и vision | Gemma-4 12B (llama.cpp) | да (CUDA-сборка) | нет | да |
| Озвучка и клон голоса | Higgs Audio v3 | да (CUDA) | нет | да |
| Вшитый текст | PP-OCR | нет | да | нет |
| Кастинг (лица и голоса) | SCRFD, LVFace, anime_face, CCIP, WeSpeaker | нет | да | нет |
| Вжигание субтитров в видео | ffmpeg | да (NVENC) | нет | нет |

Без NVIDIA озвучку, перевод, vision и распознавание можно отправить в облако, а сепарацию и диаризацию — на CPU, но у вжигания нет пути на CPU, поэтому такая машина — непроверенная конфигурация.

## Решение проблем

**Установщик останавливается на WebView2.** Окно приложения работает на Microsoft Edge WebView2, и установщик скачивает его, если в Windows его нет. При закрытом или нестабильном соединении, а также на сборках Windows 10, которые отвергают маленький загрузчик Microsoft (ошибка 0x80040902), скачивание не удаётся. Поставь WebView2 автономным установщиком Microsoft, [Evergreen Standalone x64](https://go.microsoft.com/fwlink/p/?LinkId=2124701), и запусти установщик Dub Studio снова.

**Загрузка зависает или падает.** Большие файлы докачиваются с места обрыва, так что нажми кнопку ещё раз. Если Hugging Face или GitHub для тебя закрыты, задай прокси в Настройках → **Сеть** или скачай прямые файлы из таблицы вручную, положи их туда, что указано в последней колонке, и нажми **«Импорт из папки»**.

**Экспорт падает с `Unrecognized option 'filter_complex_script'`.** Это была ошибка на ffmpeg 8 и новее, исправлена в 3.1.1: обнови приложение. Приложение использует ffmpeg из `tools\ffmpeg`, а если его там нет — принимает тот, что найден в `PATH`. Сообщая об ошибке экспорта, приложи полный вывод ffmpeg из журнала.

**Распознавание падает с `MemcpyToHost` или `Failed to allocate memory`.** На распознавании речи в длинном или большом файле закончилась память GPU (issue #4). Закрой другие программы, использующие GPU, или поставь стадию распознавания на **CPU** в настройках и запусти снова.

**`CUDA execution provider is not enabled` или все реплики у одного спикера.** GPU-библиотек нет или они слишком старые для драйвера. Поставь свежий драйвер NVIDIA, нажми кнопку загрузки CUDA-рантайма, cuDNN и ONNX Runtime GPU в настройках моделей или переключи стадию на CPU.

## Как это работает

`analyze()` — фиксированный первый проход: сепарация → ASR со словными таймингами → диаризация → контекстный перевод + vision (стиль субтитров / тайтлы / бренды) → OCR (раскладка / блюр-боксы). На выходе — редактируемый документ **Project**. Каждая правка это патч Project с превью ~0.17 с/кадр; экспорт пере-прогоняет **только загрязнённые стадии**.

**Стек:** нативная оболочка **Tauri 2 (Rust)** поднимает `dub-server` (axum) внутри того же процесса на `127.0.0.1:8793` (MCP для агентов — `/mcp` на том же порту) и открывает окно на SPA — React 19 + Vite + Tailwind + react-konva поверх JASSUB. Движки: Parakeet-TDT или Whisper (ASR) · Nemotron 3 Diarization (диаризация) · Gemma-4-12B GGUF (перевод + vision, llama.cpp) · Higgs Audio v3 (TTS) · Mel-Band Roformer (сепарация, BSRoformer.cpp) · PP-OCR (ONNX) · ffmpeg/NVENC. **Ни одного Python-процесса в рантайме.**

### Сборка из исходников

```bash
git clone https://github.com/timoncool/dub-studio.git
cd dub-studio

cd frontend && npm install && npm run build && cd ..   # 1) SPA
cargo build --release -p dub-server                     # 2) нативный сервер (axum)
cd desktop && npm install && npx tauri build            # 3) десктоп-оболочка (Tauri)
```

Требуется Node 20+, Rust (MSVC toolchain) и WebView2. Нативные движки пересобирать не нужно — приложение качает готовые.

### Linux (экспериментально)

Сборки для Linux x86-64 (.deb и AppImage) — **экспериментальные**. Они собраны из того же кода с Linux-версиями тех же движков (llama.cpp, ONNX Runtime, BSRoformer.cpp, ffmpeg, движок Higgs для Linux, yt-dlp, faster-whisper), но автор работает на Windows и на живом Linux-десктопе их не гонял. **Будет круто, если кто-то, кто сидит на Linux, дошлифует их и вольёт правки обратно пулл-реквестом.**

- Локальной озвучке нужна NVIDIA RTX 30 или новее: движок Higgs для Linux собран только под sm 86, 89 и 120. Облачные голоса работают на любой машине.
- Драйвер NVIDIA (580 или новее), `libgomp1` и `libssl3` берутся из системы; модели, движки и CUDA-библиотеки приложение скачивает при первом запуске в `~/.local/share/dub-studio` (`$XDG_DATA_HOME`).
- Собираются вручную: workflow `Linux build (experimental)` (`.github/workflows/release-linux.yml`) или `scripts/build-release-linux.sh <папка с models/ocr>`.

## Другие портативные нейросети

| Проект | Описание |
|--------|----------|
| [Higgs Ultimate](https://github.com/timoncool/Higgs-Ultimate) | Нативный синтез и клонирование речи (Higgs Audio v3) |
| [ACE-Step Studio](https://github.com/timoncool/ACE-Step-Studio) | AI-студия музыки — песни, вокал, каверы, клипы |
| [YuE2 Studio](https://github.com/timoncool/YuE2-Studio) | Генератор песен с редактируемой партитурой — нативное приложение для Windows, без Python |
| [MiniMax Music3 Studio](https://github.com/timoncool/MiniMax-Music3-Studio) | Нативная локальная/облачная музыкальная студия на MiniMax Music 3 |
| [Foundation Music Lab](https://github.com/timoncool/Foundation-Music-Lab) | Генерация музыки + редактор таймлайна |
| [Qwen3-TTS](https://github.com/timoncool/Qwen3-TTS_portable_rus) | Портативный TTS с клонированием голоса |
| [VibeVoice ASR](https://github.com/timoncool/VibeVoice_ASR_portable_ru) | Портативное распознавание речи |
| [SuperCaption Qwen3-VL](https://github.com/timoncool/SuperCaption_Qwen3-VL) | Портативное описание изображений |

## Контрибьюторы и форки

**Коллабораторам всегда рады.** Я был бы искренне рад увидеть форки Dub Studio на другие платформы и видеокарты — архитектура это позволяет, у меня просто нет сил заниматься портами самому. Хочешь запустить его на **AMD / Intel GPU, macOS или Linux** — форкай и вперёд, PR приветствуются. Для Linux уже есть экспериментальная сборка (см. *Linux (экспериментально)*): дошлифовать её — самая желанная помощь.

**Дополнительные локализации** тоже приветствуются: сейчас приложение и лендинг на 6 языках — переведи файлы локалей (`frontend/src/locales/` и словарь в `docs/index.html`) и открой PR со своим языком.

## Авторы

- **Nerual Dreming** — [Telegram](https://t.me/nerual_dreming) | [neuro-cartel.com](https://neuro-cartel.com) | основатель [ArtGeneration.me](https://artgeneration.me)
- **Нейро-Софт** — [Telegram](https://t.me/neuroport) | портативные нейросети

## Благодарности

- **[Boson AI](https://huggingface.co/bosonai)** — модель Higgs Audio v3, и **[drbaph / Higgs-Audio-v3-Studio](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio)** — GGUF-кванты и нативный движок `audiocpp_engine.dll`.
- **[NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)** (CC-BY-4.0) — ASR; ONNX-веса из [istupakov/parakeet-tdt-0.6b-v3-onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx), рантайм [altunenes/parakeet-rs](https://github.com/altunenes/parakeet-rs).
- **[Parakeet Ultra](https://huggingface.co/moondream/parakeet-ultra)** от **[Moondream](https://huggingface.co/moondream)** на базе parakeet-tdt-0.6b-v3 от NVIDIA (CC-BY-4.0) — опциональный дообученный ASR с меньшим числом ошибок; ONNX-экспорт из [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs). int8: [Masterx/parakeet-tdt-0.6b-ultra-onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx).
- **[NVIDIA Nemotron 3 Diarization](https://huggingface.co/nvidia/Nemotron-3-Diarization)** (Streaming Sortformer v3, [OpenMDW-1.1](https://openmdw.ai/license/1-1/)) — диаризация спикеров, до 8 голосов; ONNX-экспорт из [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs).
- **[Google Gemma](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf)** — Gemma-4 12B (перевод и vision), кванты от [unsloth](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF) и [llama.cpp](https://github.com/ggml-org/llama.cpp), на котором они запускаются.
- **[chenmozhijin / BSRoformer.cpp](https://github.com/chenmozhijin/BSRoformer.cpp)** и **[GaboxR67](https://huggingface.co/GaboxR67)** — нативный движок сепарации с GGUF-моделями и чекпойнт Mel-Band Roformer.
- **[Systran / faster-whisper](https://github.com/SYSTRAN/faster-whisper)**, **[deepdml](https://huggingface.co/deepdml)** и **[Purfview](https://github.com/Purfview/whisper-standalone-win)** — модели Whisper в формате CTranslate2 и автономная сборка, которая их запускает.
- **[InsightFace](https://github.com/deepinsight/insightface)** (через [immich-app/buffalo_l](https://huggingface.co/immich-app/buffalo_l)), **[ByteDance LVFace](https://huggingface.co/bytedance-research/LVFace)**, **[deepghs](https://huggingface.co/deepghs)** (CCIP, детектор аниме-лиц), **[FaceFusion](https://huggingface.co/facefusion/models-3.1.0)** и **[WeSpeaker](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM)** — модели лиц и голоса для кастинга персонажей.
- **[PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)** — PP-OCR, модели детекции и распознавания вшитого текста.
- **[ONNX Runtime](https://github.com/microsoft/onnxruntime)**, **[FFmpeg](https://ffmpeg.org)** со сборками [BtbN](https://github.com/BtbN/FFmpeg-Builds), **[JASSUB](https://github.com/ThaUnknown/jassub)**, **[Tauri](https://tauri.app)** и **[ort](https://github.com/pykeio/ort)**.
- **NVIDIA CUDA runtime, cuBLAS, cuFFT и cuDNN** — библиотеки, на которых работают GPU-пути.
- **Серёга (SilentBob)** — релиз 3.1.0: multi-take, эмоциональный референс, тайминг и редактор субтитров. **[@nevoin](https://github.com/nevoin)** — подробный лог в [#1](https://github.com/timoncool/dub-studio/issues/1), по которому найден баг ffmpeg 8. **[LongNT2011](https://github.com/LongNT2011)** — [PR #1](https://github.com/LongNT2011/dub-studio/pull/1) в его форке: исправление захардкоженного русского текста в английском интерфейсе, которое вдохновило работу над i18n.

## Поддержать автора

Я создаю опенсорс-софт и занимаюсь ИИ-исследованиями — большая часть в открытом доступе. Пожертвования позволяют делать и исследовать больше.

**[Все способы поддержки](DONATE.md)** | **[dalink.to/nerual_dreming](https://dalink.to/nerual_dreming)** | **[boosty.to/neuro_art](https://boosty.to/neuro_art)**

- **BTC:** `1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC`
- **ETH (ERC20):** `0xb5db65adf478983186d4897ba92fe2c25c594a0c`
- **USDT (TRC20):** `TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C`

## Лицензия

Код приложения — [MIT](LICENSE). **Модели — нет**: у каждой своя лицензия, а ниже то, что сейчас указано на страницах моделей. Прочитай их, прежде чем публиковать или продавать дублированное видео.

| Компонент | Лицензия | Что это значит |
|---|---|---|
| Higgs Audio v3 (Boson AI) | [Boson Higgs TTS 3 Research and Non-Commercial License](https://huggingface.co/bosonai/higgs-tts-3-4b/blob/main/LICENSE) | Бесплатно для исследований, личного использования и, по Creator Use Grant, для авторов, которые публикуют и монетизируют собственный контент, указывая Boson AI's Higgs Audio. Размещение модели для других, её распространение или встраивание в продукт или сервис для третьих лиц, включая сервис дубляжа, требует коммерческой лицензии Boson |
| Parakeet-TDT 0.6B v3 (NVIDIA) и его ONNX-экспорт | CC-BY-4.0 | Указывать NVIDIA |
| Parakeet Ultra (Moondream, на основе NVIDIA Parakeet-TDT) | CC-BY-4.0 | Указывать Moondream и NVIDIA |
| Nemotron 3 Diarization (NVIDIA) | [OpenMDW-1.1](https://openmdw.ai/license/1-1/) | Указывать NVIDIA; ONNX-экспорт из altunenes/parakeet-rs |
| Gemma-4 12B (Google) | Apache-2.0 на странице модели, которая ссылается на [условия Gemma 4 от Google](https://ai.google.dev/gemma/docs/gemma_4_license) | Прочитай оба |
| Mel-Band Roformer voc_fv6 (GaboxR67), GGUF от chenmozhijin | На страницах моделей лицензия не указана; движок BSRoformer.cpp — MIT | Перед коммерческим использованием спроси авторов |
| Модели Whisper (Systran, deepdml) и faster-whisper | MIT | У автономной сборки Purfview в репозитории нет файла лицензии |
| Детектор лиц SCRFD (InsightFace buffalo_l) | InsightFace: предобученные модели **только для некоммерческих исследований** | Кастинг реальных лиц подпадает под это ограничение |
| LVFace (ByteDance) | MIT |  |
| CCIP (deepghs) | OpenRAIL | Прочитай ограничения на использование |
| anime_face_detection (deepghs) | MIT |  |
| Окклюдер xseg_1 (модели FaceFusion) | На странице модели лицензия не указана | Перед коммерческим использованием спроси авторов |
| WeSpeaker ResNet34-LM | CC-BY-4.0 | Указывать WeSpeaker |
| PP-OCR (PaddleOCR) | Apache-2.0 |  |
| llama.cpp, ONNX Runtime, BSRoformer.cpp, JASSUB | MIT |  |
| FFmpeg (сборка BtbN) | GPL-сборка FFmpeg | Идёт отдельной программой и не линкуется в приложение |
| NVIDIA CUDA runtime, cuBLAS, cuFFT, cuDNN | Собственные условия NVIDIA | Скачиваются у NVIDIA и с PyPI, в репозитории не лежат |
