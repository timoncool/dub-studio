<div align="center">

<img src="frontend/public/favicon.svg" width="72" alt="Dub Studio"/>

# Dub Studio

**Estudio de doblaje de vídeo con IA, gratuito y sin conexión, para Windows —— redobla cualquier vídeo a otro idioma con voz clonada, subtítulos traducidos y localización del texto en pantalla. 100% local, cero Python: un `.exe` nativo (Rust + C++/CUDA); todos los modelos y motores se descargan con un botón.**

[![License](https://img.shields.io/github/license/timoncool/dub-studio?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/timoncool/dub-studio?style=flat-square)](https://github.com/timoncool/dub-studio/stargazers)
[![Latest release](https://img.shields.io/github/v/release/timoncool/dub-studio?include_prereleases&style=flat-square)](https://github.com/timoncool/dub-studio/releases)
[![Downloads](https://img.shields.io/github/downloads/timoncool/dub-studio/total?style=flat-square)](https://github.com/timoncool/dub-studio/releases)

[English](README.md) · [Русский](README.ru.md) · [中文](README.zh.md) · **Español** · [Português](README.pt.md) · [Français](README.fr.md)

### [🌐 Demo en vivo y showcase antes/después →](https://timoncool.github.io/dub-studio/)



</div>

## Míralo en acción

**[▶ Ver el showcase antes/después en el sitio →](https://timoncool.github.io/dub-studio/#showcase)** — clips reales doblados de principio a fin en una GPU local: distintos vídeos, modos e idiomas.

| ![dub](docs/shots/mode-dub-ru.png) | ![voiceover](docs/shots/mode-voiceover-es.png) | ![dub CJK](docs/shots/mode-dub-zh.png) |
|:--:|:--:|:--:|
| 🎙️ **Doblaje** · EN→RU | 🗣️ **Voz superpuesta** · EN→ES | 🈶 **Doblaje** · 中文 en el fotograma |
| ![subtitles](docs/shots/mode-subtitles-ru.png) | ![widescreen](docs/shots/mode-dub-cinema-fr.png) | ![transcript](docs/shots/mode-transcribe-pt.png) |
| 📝 **Subtítulos** · idioma original | 🎬 **Doblaje** · panorámico 16:9 | 🔤 **Transcripción** · diarización |

## Qué es

**Dub Studio** convierte cualquier vídeo en una versión doblada a otro idioma —— **con el timbre del hablante clonado, subtítulos traducidos y el texto incrustado localizado sobre el propio fotograma**. Suelta un clip y un pase automático inteligente crea el primer borrador; luego un editor en vivo pone **cada subtítulo, voz, caja de desenfoque, fuente y título** bajo tu control con vista previa instantánea.

Por defecto todo se ejecuta **localmente en tu equipo** —— sin nube ni suscripción: ni tu material ni tu voz salen del ordenador. Y si tu PC es limitado (no mueve el Gemma/Higgs local) o quieres más velocidad y calidad, las partes pesadas (traducción, visión, TTS, transcripción) pueden **opcionalmente** delegarse a la nube vía **OpenRouter** —— cada motor se elige por separado (local ↔ nube), con voces asignadas automáticamente por sexo del hablante (beta). La clave se guarda localmente; todo está desactivado por defecto.

Es **una reescritura totalmente nativa**. Sin Python embebido, sin torch, sin ruedas CUDA. Toda la canalización es **Rust + motores nativos C++/CUDA (GGUF/ONNX)**: un proceso, arranque rápido, poca VRAM. Los modelos, motores, runtime de CUDA/VC++ y ffmpeg los **descarga e instala la propia app** en el primer arranque. **La app está hecha y probada para una GPU NVIDIA**: la voz, la traducción y la visión se ejecutan en local sobre CUDA, y los subtítulos se queman en el vídeo con NVENC. La separación, la diarización y el reconocimiento pueden correr en la CPU y las etapas pesadas pueden ir a OpenRouter, pero un equipo sin NVIDIA no es una configuración probada: véase *Qué corre dónde* más abajo.

## Para agentes de IA

Mientras Dub Studio está abierto, ofrece un servidor MCP en `http://127.0.0.1:8793/mcp`: un agente como Claude Code, Claude Desktop, Cursor o Codex hace todo lo que hace la ventana, con el mismo código: crea proyectos a partir de archivos de vídeo, los analiza, corrige la traducción, los tiempos y los hablantes línea por línea, asigna las voces, da estilo a los subtítulos, añade títulos y zonas de desenfoque, renderiza, exporta el mismo vídeo a más idiomas, escribe SRT y TXT y guarda los resultados en una carpeta. En Ajustes, **Agente (MCP)** muestra si hay un agente conectado y qué pegar en el cliente.

Con este repositorio, un agente puede instalarlo todo y manejar el estudio por sí mismo:

1. Instalar el estudio desde la [última versión](https://github.com/timoncool/dub-studio/releases/latest) e iniciarlo.
2. Conectarse a su servidor MCP:
   ```bash
   claude mcp add --transport http dub-studio http://127.0.0.1:8793/mcp
   ```
   Otros clientes: `{ "mcpServers": { "dub-studio": { "type": "streamable-http", "url": "http://127.0.0.1:8793/mcp" } } }`
3. Leer la skill que sirve el servidor (recurso `studio://skill`, prompt `studio`), el mismo texto que [docs/mcp-skill.md](docs/mcp-skill.md) —todas las herramientas, las reglas básicas y recetas paso a paso— y empezar con la herramienta `studio_status`.

[llms.txt](llms.txt) dice lo mismo para las herramientas que lo buscan. Para tener la skill siempre en Claude Code, guarda [docs/mcp-skill.md](docs/mcp-skill.md) como `~/.claude/skills/dub-studio/SKILL.md`. Solo pueden conectarse los agentes de este ordenador y la propia ventana del estudio.

## Cinco modos, conmutables al vuelo

| Modo | Qué hace |
|------|----------|
| 🎙️ **Doblaje** | Redoblaje completo al idioma destino con el **timbre original clonado** —— reparto automático por hablante o voz a elección |
| 🗣️ **Voz superpuesta** | Voz traducida **sobre el original atenuado** —— el original sigue oyéndose debajo; equilibrio ajustable |
| 📝 **Subtítulos** | Subtítulos en el **idioma original**, conservando el audio original —— sin doblaje ni traducción |
| ✨ **Remix divertido** | Da un tema («como un pirata», «como un noticiero») → el modelo **reescribe todo el guion** y redobla |
| 🎬 **Transcripción** | **Transcripción diarizada** limpia con disposición por hablante, reproducción tipo karaoke, creación de voces con un clic, export `.srt`/`.txt` |

Carga un clip una vez y envíalo a cualquier modo dentro del editor.

## Funciones

- **Clonación de voz** —— el timbre original se clona y habla el nuevo idioma (motor nativo [Higgs Audio v3](https://huggingface.co/bosonai), GGUF). Reparto automático por hablante o tu propia voz de un pack.
- **Diarización de hablantes** —— quién habla y cuándo (NVIDIA **Nemotron 3 Diarization**, hasta 8 voces), una voz distinta por hablante.
- **Reparto de personajes (beta)** —— un personaje es un par **«cara + voz»**. La app reúne las caras de todo el vídeo, reconoce a la misma persona y **la vincula a un hablante por coaparición** (quien sale en primer plano recibe la voz, un oyente de fondo no); elige automáticamente el fotograma-avatar más nítido y **guarda un perfil de reparto para toda la serie** —— asignas voces y descripciones una vez y el **siguiente episodio las aplica solo**. Un interruptor **«caras reales / dibujos·anime»** cambia la detección de rostros según el contenido.
- **Elección de motor ASR** — transcribe con **Parakeet-TDT** (GPU, por defecto) o **Whisper** ([faster-whisper standalone de Purfview](https://github.com/Purfview/whisper-standalone-win), funciona en CPU) — elige el tamaño del modelo (tiny … large-v3-turbo) y el cuant (compute type) directamente en ajustes.
- **Importar subtítulos listos** — usa tu propio `.srt`/`.ass` como transcripción exacta: el texto y los tiempos vienen del archivo en vez del reconocimiento automático (los hablantes se asignan igual por diarización). Marca **«subtítulos ya en el idioma destino»** y también se omite la traducción — un vídeo en inglés + tus subtítulos en ruso → doblaje en ruso directo desde ellos.
- **Exportación multiidioma** — la **▾** junto a Exportar envía un vídeo a varios idiomas a la vez; cada uno hereda todas tus ediciones (diseño de subtítulos, estilos, cajas de desenfoque, voz clonada) — solo cambian la traducción y la voz.
- **Guardar y reabrir proyectos** — autoguardado, lista de proyectos recientes en la pantalla de inicio y vuelve al trabajo sin terminar en un clic.
- **Búsqueda en las listas de voces e idiomas** — escribe parte de un nombre para filtrar cientos de voces o más de 100 idiomas; los idiomas también coinciden por su nombre en el idioma de la interfaz.
- **Pipeline componible** — interruptores independientes en la entrada: audio (original / doblaje / voz superpuesta / transcripción) × subtítulos (ninguno / original / traducidos) × grabar en el vídeo sí/no × remix humorístico. Cualquier combinación — doblaje sin subtítulos, subtítulos traducidos sin doblaje, doblaje humorístico con tus propias voces — también en lote y en el editor.
- **Localización de texto en pantalla** —— OCR detecta el texto incrustado (**PP-OCR** ONNX), **desenfoca el original** e imprime encima un título localizado con estilo a juego —— una función que ninguna otra herramienta tiene.
- **Traducción + análisis visual de estilo** —— la transcripción se traduce localmente con **Gemma-4 12B** (GGUF, llama.cpp); un pase de visión lee el diseño del fotograma: estilo de subtítulos, títulos, marcas, zonas de texto.
- **Separación vocal SOTA** —— **Mel-Band Roformer** (BSRoformer.cpp nativo en CUDA) separa la voz de la música: la pista de fondo se **conserva** y el clon se engancha a un habla limpia.
- **26 preajustes de subtítulos** —— karaoke / palabra a palabra / hormozi / neón y más, renderizados **sobre tu fotograma** (WYSIWYG, JASSUB sobre el mismo `.ass` que graba ffmpeg).
- **Transcripción karaoke** —— reproduce el vídeo y sigue cómo se iluminan la línea y la **palabra** actuales en la transcripción.
- **Editor en vivo** —— edita transcripción, voces, estilo de subtítulos, cajas de desenfoque, títulos; **vista previa ~0,17 s/fotograma**, cada cambio se ve al instante.
- **Regeneración inteligente** —— al exportar solo se resintetizan los segmentos que cambiaste, no todo el clip.
- **Tus propias líneas** —— inserta frases personalizadas en la transcripción; cada una se dobla con la voz clonada del hablante y aparece en los subtítulos.
- **Procesamiento por lotes** —— cola de archivos, todos con una misma configuración, progreso por archivo.
- **Comparar antes/después** —— original y doblaje lado a lado.
- **100+ idiomas** —— dobla a cualquier idioma principal (español, chino, japonés, árabe, hindi y más), con autodetección del idioma origen.
- **Cualquier formato de vídeo** —— MP4, MOV, MKV, WEBM, AVI y más (decodificado con ffmpeg).
- **Instalación de un botón + autoactualización** —— modelos, motores, runtime CUDA/VC++ y ffmpeg se descargan en el primer arranque; la app se actualiza sola.
- **Descargas reanudables** —— los modelos grandes (10 GB+) se reanudan desde donde se cortaron tras una caída de conexión, en vez de reiniciar.
- **Ejecuta cada etapa donde quieras** —— la separación, la diarización y el reconocimiento cambian de forma independiente entre **GPU y CPU**, y el reconocimiento, la traducción, la visión y la voz pueden delegarse a **OpenRouter**. La tabla *Qué corre dónde* de más abajo indica qué puede usar realmente cada etapa.
- **Ajusta a tu hardware** —— cada motor trae varias cuantizaciones (TTS Q8/Q6/Q4, traducción Q4…Q8, ASR int8/fp32 o Whisper tiny…large-v3-turbo, separación Q8/Q5/Q4) — cámbialas en ajustes; limita el lote de prefill y la duración de la referencia para GPU de 8–12 GB y 32 GB de RAM.
- **Toda la app en tu idioma** — el progreso, los errores, la instalación, el casting y los informes que lee un agente llegan en los seis idiomas de la ventana.
- **Voces de Google Gemini** — Gemini TTS junto al motor local y OpenRouter, con su propia clave y una lista de modelos en vivo; **Batch** cuesta la mitad y un render detenido retoma el mismo lote ya pagado.
- **Número de hablantes** — automático o de 1 a 8 personas; las voces se emparejan en toda una grabación larga, que se transcribe en ventanas de unos 90 segundos.
- **Igualar el volumen** — un interruptor activado por defecto: frases al mismo nivel y el doblaje a -14 LUFS con techo de -1 dBTP; apagado, la mezcla queda como llegó.
- **Totalmente portátil** —— nada se escribe en tu perfil de usuario; borra la carpeta y no queda rastro.

## Capturas

Pantalla principal —— cinco modos, vista previa del vídeo elegido, selección de idioma, cualquier formato:

![Pantalla principal de Dub Studio](docs/screenshot-home.png)

Modo transcripción —— transcripción diarizada con disposición por hablante, karaoke y creación de voces desde cada hablante con un clic:

![Modo transcripción de Dub Studio](docs/screenshot-transcribe.png)

## Requisitos

- **SO:** Windows 10 / 11 (x64); Linux x86-64 como compilación experimental (ver *Linux (experimental)*)
- **GPU:** NVIDIA con 8 GB de VRAM o más (hay ajustes para 8, 12, 16, 24 y 32 GB) y un controlador reciente. La voz local (Higgs Audio), la traducción y la visión (Gemma) se ejecutan sobre CUDA, y los subtítulos se queman con NVENC. Sin NVIDIA solo pueden funcionar las etapas que *Qué corre dónde* marca para la CPU o la nube, y esa configuración no está probada
- **WebView2** —— preinstalado en Windows 11; en Windows 10 lo descarga el instalador (si falla, véase *Solución de problemas*)
- **Disco:** ~15 GB para los modelos por defecto, los motores y el runtime (se descargan en el primer arranque), más espacio para tus proyectos; las cuantizaciones alternativas y los modelos Whisper son adicionales

En un equipo con NVIDIA, lo único que instalas a mano es un **[controlador NVIDIA](https://www.nvidia.com/Download/index.aspx)** reciente. Todo lo demás —modelos (Higgs Audio v3, Gemma-4 12B + vision, Parakeet-TDT, Nemotron 3 Diarization, Mel-Band Roformer), motores, runtime de CUDA y ffmpeg— lo descarga la app con un botón en el primer arranque.

## Inicio rápido

1. **Descarga** la versión portátil desde [Releases](https://github.com/timoncool/dub-studio/releases) y descomprime donde quieras (o instala con `-setup.exe` / `.msi`).
2. **Ejecuta** `Dub Studio.exe`.
3. En el panel de **primer arranque** pulsa **Descargar todo** —— la app obtiene modelos, motores y runtime (~15 GB, una vez).
4. **Suelta un vídeo**, elige el idioma destino → el pase automático crea el primer borrador. Ajusta todo en el editor y pulsa **Exportar**.

> Todo se descarga y vive **dentro de la carpeta de la app**. Modelos, cachés y proyectos no van a ningún otro sitio.

Qué cambió y cuándo está en [CHANGELOG.md](CHANGELOG.md), y el botón de destellos de la parte superior de la app lo muestra. Reglas para colaboradores y agentes de programación: [AGENTS.md](AGENTS.md).

## Todo lo que descarga la app

El panel de primer arranque descarga todo esto con un botón. Detrás de un proxy, o donde Hugging Face está bloqueado, configura el proxy en Ajustes → **Red**: como en Windows, propio (HTTP, HTTPS, SOCKS5 o SOCKS4; el formato del vendedor `host:port:usuario:contraseña` sirve tal cual) o ninguno. Por él pasan las descargas de modelos, la nube y las actualizaciones de la app. También puedes descargar tú los archivos directos, colocarlos donde indica la última columna, contando desde la carpeta de la app (la que contiene `models\`), y pulsar **Importar desde carpeta**; los componentes en archivos comprimidos (`.zip`, `.whl`) los descarga la propia app.

Un archivo en el lugar indicado cuenta como instalado cuando su tamaño coincide exactamente con el indicado; cada descarga se comprueba contra el SHA-256 fijado del archivo antes de usarse, y un archivo que ya está en su sitio se comprueba igual antes de omitirlo. **Importar desde carpeta** (en el panel de primer arranque y en los ajustes de modelos) busca los archivos de un componente por nombre y tamaño exacto en la carpeta que elijas y los coloca con un enlace duro o una copia. Los componentes que llegan como archivos zip o wheel (motores, runtimes) no se importan y cuentan como instalados solo cuando la app los descargó ella misma: guarda un registro del archivo comprobado junto a los archivos descomprimidos. Los tamaños y los hashes son los del propio manifiesto de la app, `crates/dub-server/src/setup.rs`, y esta tabla se comprueba contra él.

El runtime de Visual C++ y los modelos PP-OCR vienen dentro de la versión y no se descargan; el controlador NVIDIA se instala aparte.

<!-- downloads:start -->
| Componente | Necesidad | Archivos (enlaces directos) | Tamaño | Dónde ponerlo |
|---|---|---|---|---|
| Higgs Audio v3 Q8_0 | obligatorio | [q8_0.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/q8_0.gguf) → `models\higgs-q8_0\q8_0.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/config.json) → `models\higgs-q8_0\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/chat_template.jinja) → `models\higgs-q8_0\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer.json) → `models\higgs-q8_0\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer_config.json) → `models\higgs-q8_0\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q8_0\higgs_audio_v2_tokenizer_config.json` | 5.5 GB | tal cual, en la ruta tras la flecha |
| audiocpp_engine.dll (Higgs engine) | obligatorio | [audiocpp_engine.dll](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/engines/audiocpp_engine.dll) → `models\higgs-engine\audiocpp_engine.dll` | 72 MB | tal cual, en la ruta tras la flecha |
| Gemma-4 12B QAT q4_0 + vision | obligatorio | [gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\gemma-4-12b-it-qat-q4_0.gguf`<br>[mmproj-gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/mmproj-gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\mmproj-gemma-4-12b-it-qat-q4_0.gguf` | 7.2 GB | tal cual, en la ruta tras la flecha |
| Gemma-4 12B Q5_K_M + vision | opcional | [gemma-4-12b-it-Q5_K_M.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q5_K_M.gguf) → `models\mt-q5_0\gemma-4-12b-it-Q5_K_M.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q5_0\mmproj-F16.gguf` | 8.6 GB | tal cual, en la ruta tras la flecha |
| Gemma-4 12B Q6_K + vision | opcional | [gemma-4-12b-it-Q6_K.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q6_K.gguf) → `models\mt-q6_k\gemma-4-12b-it-Q6_K.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q6_k\mmproj-F16.gguf` | 10.0 GB | tal cual, en la ruta tras la flecha |
| Gemma-4 12B Q8_0 + vision | opcional | [gemma-4-12b-it-Q8_0.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q8_0.gguf) → `models\mt-q8_0\gemma-4-12b-it-Q8_0.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q8_0\mmproj-F16.gguf` | 12.8 GB | tal cual, en la ruta tras la flecha |
| Parakeet-TDT 0.6B v3 int8 | obligatorio | [encoder-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx) → `models\tdt\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx) → `models\tdt\decoder_joint-model.int8.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt\config.json` | 671 MB | tal cual, en la ruta tras la flecha |
| Higgs Audio v3 Q6_K | opcional | [q6_k.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/q6_k.gguf) → `models\higgs-q6_k\q6_k.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/config.json) → `models\higgs-q6_k\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/chat_template.jinja) → `models\higgs-q6_k\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer.json) → `models\higgs-q6_k\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer_config.json) → `models\higgs-q6_k\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q6_k\higgs_audio_v2_tokenizer_config.json` | 5.0 GB | tal cual, en la ruta tras la flecha |
| Higgs Audio v3 Q4_K_M | opcional | [q4_k_m.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/q4_k_m.gguf) → `models\higgs-q4_k_m\q4_k_m.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/config.json) → `models\higgs-q4_k_m\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/chat_template.jinja) → `models\higgs-q4_k_m\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer.json) → `models\higgs-q4_k_m\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer_config.json) → `models\higgs-q4_k_m\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q4_k_m\higgs_audio_v2_tokenizer_config.json` | 4.1 GB | tal cual, en la ruta tras la flecha |
| Parakeet-TDT 0.6B v3 fp32 | opcional | [encoder-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx) → `models\tdt-fp32\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx.data) → `models\tdt-fp32\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.onnx) → `models\tdt-fp32\decoder_joint-model.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-fp32\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt-fp32\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-fp32\config.json` | 2.5 GB | tal cual, en la ruta tras la flecha |
| Parakeet Ultra 0.6B fp32 (Moondream) | opcional | [encoder-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx) → `models\tdt-ultra\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx.data) → `models\tdt-ultra\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/decoder_joint-model.onnx) → `models\tdt-ultra\decoder_joint-model.onnx`<br>[vocab.txt](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/vocab.txt) → `models\tdt-ultra\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/tdt/nemo128.onnx) → `models\tdt-ultra\nemo128.onnx`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-ultra\config.json` | 2.6 GB | tal cual, en la ruta tras la flecha |
| Parakeet Ultra 0.6B int8 (Moondream) | opcional | [encoder-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/encoder-model.int8.onnx) → `models\tdt-ultra-int8\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/decoder_joint-model.int8.onnx) → `models\tdt-ultra-int8\decoder_joint-model.int8.onnx`<br>[vocab.txt](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/vocab.txt) → `models\tdt-ultra-int8\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-ultra-int8\nemo128.onnx`<br>[config.json](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/config.json) → `models\tdt-ultra-int8\config.json` | 0.7 GB | tal cual, en la ruta tras la flecha |
| Whisper-Faster (faster-whisper standalone) | opcional | [Whisper-Faster_r192.3_windows.zip](https://github.com/Purfview/whisper-standalone-win/releases/download/faster-whisper/Whisper-Faster_r192.3_windows.zip) | 88 MB | descomprimir los archivos, sin subcarpetas, en `tools\whisper\` |
| Whisper CUDA (cuBLAS 11, cuDNN 8) | opcional | [libcublas-windows-x86_64-11.11.3.6-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-11.11.3.6-archive.zip)<br>[cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip](https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip) | 1.1 GB | tomar todos los .dll del archivo y ponerlos en `tools\whisper\` |
| Whisper tiny | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/model.bin) → `models\whisper\faster-whisper-tiny\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/config.json) → `models\whisper\faster-whisper-tiny\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/tokenizer.json) → `models\whisper\faster-whisper-tiny\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/vocabulary.txt) → `models\whisper\faster-whisper-tiny\vocabulary.txt` | 78 MB | tal cual, en la ruta tras la flecha |
| Whisper base | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/model.bin) → `models\whisper\faster-whisper-base\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/config.json) → `models\whisper\faster-whisper-base\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/tokenizer.json) → `models\whisper\faster-whisper-base\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/vocabulary.txt) → `models\whisper\faster-whisper-base\vocabulary.txt` | 148 MB | tal cual, en la ruta tras la flecha |
| Whisper small | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/model.bin) → `models\whisper\faster-whisper-small\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/config.json) → `models\whisper\faster-whisper-small\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/tokenizer.json) → `models\whisper\faster-whisper-small\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/vocabulary.txt) → `models\whisper\faster-whisper-small\vocabulary.txt` | 486 MB | tal cual, en la ruta tras la flecha |
| Whisper medium | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/model.bin) → `models\whisper\faster-whisper-medium\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/config.json) → `models\whisper\faster-whisper-medium\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/tokenizer.json) → `models\whisper\faster-whisper-medium\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/vocabulary.txt) → `models\whisper\faster-whisper-medium\vocabulary.txt` | 1.5 GB | tal cual, en la ruta tras la flecha |
| Whisper large-v3 | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/model.bin) → `models\whisper\faster-whisper-large-v3\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/config.json) → `models\whisper\faster-whisper-large-v3\config.json`<br>[preprocessor_config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/tokenizer.json) → `models\whisper\faster-whisper-large-v3\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/vocabulary.json) → `models\whisper\faster-whisper-large-v3\vocabulary.json` | 3.1 GB | tal cual, en la ruta tras la flecha |
| Whisper large-v3-turbo | opcional | [model.bin](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/model.bin) → `models\whisper\faster-whisper-large-v3-turbo\model.bin`<br>[config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/config.json) → `models\whisper\faster-whisper-large-v3-turbo\config.json`<br>[preprocessor_config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3-turbo\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/tokenizer.json) → `models\whisper\faster-whisper-large-v3-turbo\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/vocabulary.json) → `models\whisper\faster-whisper-large-v3-turbo\vocabulary.json` | 1.6 GB | tal cual, en la ruta tras la flecha |
| Nemotron 3 Diarization | recomendado | [nemotron3_diar_v3.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/nemotron3_diar_v3.onnx) → `models\nemotron-diar\nemotron3_diar_v3.onnx`<br>[LICENSE](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/LICENSE) → `models\nemotron-diar\LICENSE` | 401 MB | tal cual, en la ruta tras la flecha |
| Mel-Band Roformer voc_fv6 Q8_0 | recomendado | [voc_fv6-Q8_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q8_0.gguf) → `models\bsroformer\voc_fv6-Q8_0.gguf` | 252 MB | tal cual, en la ruta tras la flecha |
| Mel-Band Roformer voc_fv6 Q5_0 | opcional | [voc_fv6-Q5_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q5_0.gguf) → `models\bsroformer\voc_fv6-Q5_0.gguf` | 167 MB | tal cual, en la ruta tras la flecha |
| Mel-Band Roformer voc_fv6 Q4_0 | opcional | [voc_fv6-Q4_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q4_0.gguf) → `models\bsroformer\voc_fv6-Q4_0.gguf` | 139 MB | tal cual, en la ruta tras la flecha |
| Modelos de casting (caras y voz) | recomendado | [model.onnx](https://huggingface.co/immich-app/buffalo_l/resolve/d09715916a0778919a770c343533641e250b8699/detection/model.onnx) → `models\faces\det_10g.onnx`<br>[LVFace-L_Glint360K.onnx](https://huggingface.co/bytedance-research/LVFace/resolve/b12702ab1f5c721748e054a66dc90e1edd1f0724/LVFace-L_Glint360K/LVFace-L_Glint360K.onnx) → `models\faces\LVFace-L_Glint360K.onnx`<br>[model_feat.onnx](https://huggingface.co/deepghs/ccip_onnx/resolve/eb2acdd29af1703388d3d0c04221add322bc9110/ccip-caformer-24-randaug-pruned/model_feat.onnx) → `models\faces\ccip\model_feat.onnx`<br>[model.onnx](https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_s/model.onnx) → `models\faces\anime_face\model.onnx`<br>[xseg_1.onnx](https://huggingface.co/facefusion/models-3.1.0/resolve/c9e3a503d8e84e91c5cd89ee2d510fe5e793e570/xseg_1.onnx) → `models\faces\occluder\xseg_1.onnx`<br>[voxceleb_resnet34_LM.onnx](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/resolve/f0c48c298fd835726c27956a5d617bad7115627e/voxceleb_resnet34_LM.onnx) → `models\faces\wespeaker\voxceleb_resnet34_LM.onnx` | 1.3 GB | tal cual, en la ruta tras la flecha |
| BSRoformer.cpp (CUDA) | recomendado | [BSRoformer-windows-cuda-13.1.0.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-cuda-13.1.0.zip) | 165 MB | descomprimir los archivos, sin subcarpetas, en `tools\bsroformer\` |
| BSRoformer.cpp (CPU) | opcional | [BSRoformer-windows-x64-msvc.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-x64-msvc.zip) | 671 KB | descomprimir los archivos, sin subcarpetas, en `tools\bsroformer-cpu\` |
| llama.cpp server (CUDA) | obligatorio | [llama-b11146-bin-win-cuda-13.4-x64.zip](https://github.com/ggml-org/llama.cpp/releases/download/b11146/llama-b11146-bin-win-cuda-13.4-x64.zip) | 150 MB | descomprimir los archivos, sin subcarpetas, en `tools\llama\` |
| ONNX Runtime | obligatorio | [onnxruntime-win-x64-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-1.28.2.zip) | 79 MB | descomprimir con su árbol de carpetas en `models\runtime\` |
| ONNX Runtime GPU (CUDA) | recomendado | [onnxruntime-win-x64-gpu_cuda13-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-gpu_cuda13-1.28.2.zip) | 366 MB | descomprimir con su árbol de carpetas en `models\runtime\` |
| FFmpeg (static build) | obligatorio | [ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip](https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-31-13-27/ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip) | 171 MB | tomar ffmpeg.exe y ffprobe.exe del archivo y ponerlos en `tools\ffmpeg\` |
| yt-dlp + deno | opcional | [yt-dlp.exe](https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.exe) → `tools\yt-dlp\yt-dlp.exe`<br>[deno-x86_64-pc-windows-msvc.zip](https://github.com/denoland/deno/releases/download/v2.9.7/deno-x86_64-pc-windows-msvc.zip) | 60 MB | tal cual, en la ruta tras la flecha<br>descomprimir los archivos, sin subcarpetas, en `tools\yt-dlp\` |
| CUDA runtime (cudart, cuBLAS, cuFFT) | obligatorio | [nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/86/00/d5436004268f049214193659ebc36550b5ef3925c3d13b4cc980e13be6f5/nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl)<br>[nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/a3/df/f1246959833e2c437db8be3e5b477f66b87f8817821ed40de6c7561c9a36/nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl)<br>[libcufft-windows-x86_64-12.4.0.43-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcufft/windows-x86_64/libcufft-windows-x86_64-12.4.0.43-archive.zip) | 586 MB | tomar todos los .dll del archivo y ponerlos en `models\higgs-engine\` |
| cuDNN 9 | recomendado | [nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/87/6a/e55ff0ac26a5c6e2b21f41c9d04ad096b4ed6da593fba7e25845c61b0532/nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl) | 436 MB | tomar todos los .dll del archivo y ponerlos en `models\higgs-engine\` |
<!-- downloads:end -->

## Qué corre dónde

Cada etapa tiene su propio selector de dispositivo. *Sí* significa que el código tiene ese camino; la tabla no dice nada de velocidad ni de cobertura de pruebas, y los caminos de CPU son más lentos. Los caminos en la nube necesitan una clave de OpenRouter (Ajustes) y están desactivados por defecto.

| Etapa | Motor | GPU NVIDIA | CPU | OpenRouter (nube) |
|---|---|---|---|---|
| Separación | Mel-Band Roformer (BSRoformer.cpp) | sí (versión CUDA) | sí (versión CPU aparte, más lenta) | no |
| Diarización | Nemotron 3 Diarization | sí (ONNX Runtime CUDA) | sí | no |
| Reconocimiento de voz | Parakeet-TDT o Whisper-Faster | sí | sí | sí |
| Traducción y visión | Gemma-4 12B (llama.cpp) | sí (versión CUDA) | no | sí |
| Voz y clonación | Higgs Audio v3 | sí (CUDA) | no | sí |
| Texto en pantalla | PP-OCR | no | sí | no |
| Reparto (caras y voces) | SCRFD, LVFace, anime_face, CCIP, WeSpeaker | no | sí | no |
| Subtítulos quemados en el vídeo | ffmpeg | sí (NVENC) | no | no |

Sin NVIDIA, la voz, la traducción, la visión y el reconocimiento pueden ir a la nube y la separación y la diarización a la CPU, pero el quemado no tiene camino de CPU, así que un equipo así no es una configuración probada.

## Solución de problemas

**El instalador se detiene en WebView2.** La ventana de la app funciona sobre Microsoft Edge WebView2, y el instalador lo descarga cuando Windows no lo tiene. Con una conexión bloqueada o inestable, o en versiones de Windows 10 que rechazan el pequeño instalador de Microsoft (error 0x80040902), esa descarga falla. Instala WebView2 con el instalador independiente de Microsoft, [Evergreen Standalone x64](https://go.microsoft.com/fwlink/p/?LinkId=2124701), y ejecuta de nuevo el instalador de Dub Studio.

**Las descargas se atascan o fallan.** Los archivos grandes se reanudan donde quedaron, así que pulsa el botón otra vez. Si Hugging Face o GitHub están bloqueados para ti, configura un proxy en Ajustes → **Red**, o descarga a mano los archivos directos de la tabla, colócalos donde indica la última columna y pulsa **Importar desde carpeta**.

**La exportación se detiene con `Unrecognized option 'filter_complex_script'`.** Era un fallo con ffmpeg 8 y posteriores, corregido en la 3.1.1: actualiza la app. La app usa el ffmpeg de `tools\ffmpeg` y, si no hay ninguno, acepta el que encuentre en el `PATH`. Al informar de un error de exportación, adjunta la salida completa de ffmpeg del registro.

**El reconocimiento falla con `MemcpyToHost` o `Failed to allocate memory`.** La GPU se quedó sin memoria en el reconocimiento de voz de un archivo largo o grande (issue #4). Cierra otros programas que usen la GPU, o pon la etapa de reconocimiento en **CPU** en los ajustes, y vuelve a ejecutarla.

**`CUDA execution provider is not enabled`, o todo cae en un solo hablante.** Faltan las bibliotecas de la GPU o son demasiado antiguas para el controlador. Instala el controlador NVIDIA actual, pulsa el botón de descarga del runtime de CUDA, cuDNN y ONNX Runtime GPU en los ajustes de modelos, o cambia la etapa a CPU.

## Cómo funciona

`analyze()` es un primer pase fijo: separación → ASR con tiempos por palabra → diarización → traducción contextual + visión (estilo de subtítulos / títulos / marcas) → OCR (diseño / cajas de desenfoque). El resultado es un documento **Project** editable. Cada edición es un parche sobre él con vista previa ~0,17 s/fotograma; la exportación solo reejecuta **las etapas modificadas**.

**Stack:** un shell nativo **Tauri 2 (Rust)** ejecuta `dub-server` (axum) dentro del mismo proceso en `127.0.0.1:8793` (el MCP para agentes es `/mcp` en el mismo puerto) y abre una ventana sobre la SPA —— React 19 + Vite + Tailwind + react-konva sobre JASSUB. Motores: Parakeet-TDT o Whisper (ASR) · Nemotron 3 Diarization (diarización) · Gemma-4-12B GGUF (traducción + visión, llama.cpp) · Higgs Audio v3 (TTS) · Mel-Band Roformer (separación, BSRoformer.cpp) · PP-OCR (ONNX) · ffmpeg/NVENC. **Ni un solo proceso Python en tiempo de ejecución.**

### Compilar desde el código

```bash
git clone https://github.com/timoncool/dub-studio.git
cd dub-studio

cd frontend && npm install && npm run build && cd ..   # 1) SPA
cargo build --release -p dub-server                     # 2) servidor nativo (axum)
cd desktop && npm install && npx tauri build            # 3) shell de escritorio (Tauri)
```

Requiere Node 20+, Rust (toolchain MSVC) y WebView2. Los motores nativos no hace falta recompilarlos —— la app descarga binarios precompilados.

### Linux (experimental)

El .deb y el AppImage para Linux x86-64 son **experimentales**. Salen del mismo código con las versiones para Linux de los mismos motores (llama.cpp, ONNX Runtime, BSRoformer.cpp, ffmpeg, el motor Higgs para Linux, yt-dlp, faster-whisper), pero el autor trabaja en Windows y no los ha probado en un escritorio Linux real. **Si vives en Linux, sería genial que los pulieras y devolvieras los arreglos con un pull request.**

- La voz local necesita una NVIDIA desde Turing (GTX 16, RTX 20 o más nuevas), como en Windows. Las voces en la nube funcionan en cualquier equipo.
- El driver de NVIDIA (580 o más nuevo), `libgomp1` y `libssl3` vienen del sistema; los modelos, motores y bibliotecas CUDA la app los descarga en el primer arranque en `~/.local/share/dub-studio` (`$XDG_DATA_HOME`).
- Se compilan a mano: el workflow `Linux build (experimental)` (`.github/workflows/release-linux.yml`) o `scripts/build-release-linux.sh <carpeta con models/ocr>`.

## Estadísticas anónimas y noticias

La app pide noticias al servidor del autor al arrancar y luego cada hora. La petición no lleva ningún id, así que las noticias llegan elijas lo que elijas abajo: las nuevas aparecen arriba en Novedades o como una franja en la parte superior de la ventana y, sin conexión, la app muestra las noticias de su versión.

La pantalla del primer arranque tiene una casilla **Enviar estadísticas de uso anónimas**, marcada por defecto. El mismo interruptor está en Ajustes → Estadísticas anónimas, junto a **Qué se envía** (el informe exacto de hoy) y **Nuevo id de instalación**. Mientras esté marcada, la app envía el informe del día un minuto después de arrancar, cada seis horas, dos minutos después de terminar un trabajo y al cerrarse (un informe posterior del mismo día sustituye al anterior):

- un id de instalación aleatorio creado en este equipo, sin vínculo con el hardware ni con una cuenta; desmarcar la casilla lo borra;
- la app y su versión, el nombre y la versión del sistema, el idioma de la ventana;
- la tarjeta gráfica: fabricante, rango de memoria de vídeo (hasta 8, 12, 16, 24+ GB) y si CUDA funciona;
- cuántas tareas (análisis, doblaje, render, exportación, descarga y las demás) terminaron, fallaron o se cancelaron ese día.
- los modelos de cada etapa (voz, reconocimiento, traducción, análisis de fotogramas, separación) y cuántas tareas los usaron;
- por qué falló una tarea, en una línea de la que se quitan en este equipo rutas, nombres, enlaces y todo lo que va entre comillas antes de enviarla; el servidor guarda estos motivos 30 días.

Nunca: vídeos, transcripciones, traducciones, voces, nombres de archivo o rutas, nada personal. El servidor guarda el país que Cloudflare indica para la conexión, no la dirección IP. `DO_NOT_TRACK=1` o `STUDIO_TELEMETRY=0` en el entorno apagan las estadísticas por completo: no existe id y no se cuenta nada.

## Contribuciones y forks

**Los colaboradores son muy bienvenidos.** Me haría muy feliz ver Dub Studio adaptado a otras plataformas y GPUs — la arquitectura lo permite, simplemente no tengo tiempo para hacer los ports yo mismo. Si lo quieres en **GPUs AMD / Intel, macOS o Linux**, haz un fork y adelante — se agradecen los PR. Linux ya tiene una compilación experimental (ver *Linux (experimental)*): pulirla es la ayuda más bienvenida.

**Localizaciones adicionales** también son bienvenidas: hoy la app y la landing están en 6 idiomas — traduce los archivos de idioma (`frontend/src/locales/` y el diccionario en `docs/index.html`) y abre un PR con el tuyo.

## Autores

- **Nerual Dreming** —— [Telegram](https://t.me/nerual_dreming) | [neuro-cartel.com](https://neuro-cartel.com) | fundador de [ArtGeneration.me](https://artgeneration.me)
- **Neuro-Soft** —— [Telegram](https://t.me/neuroport) | apps de IA portátiles

## Créditos

- **[Boson AI](https://huggingface.co/bosonai)** —— el modelo Higgs Audio v3, y **[drbaph / Higgs-Audio-v3-Studio](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio)** —— las cuantizaciones GGUF y el motor nativo `audiocpp_engine.dll`.
- **[NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)** (CC-BY-4.0) —— ASR; pesos ONNX de [istupakov/parakeet-tdt-0.6b-v3-onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx), runtime [altunenes/parakeet-rs](https://github.com/altunenes/parakeet-rs).
- **[Parakeet Ultra](https://huggingface.co/moondream/parakeet-ultra)** de **[Moondream](https://huggingface.co/moondream)**, basado en parakeet-tdt-0.6b-v3 de NVIDIA (CC-BY-4.0) —— ASR ajustado opcional con menos errores de reconocimiento; exportación ONNX de [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs). int8: [Masterx/parakeet-tdt-0.6b-ultra-onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx).
- **[NVIDIA Nemotron 3 Diarization](https://huggingface.co/nvidia/Nemotron-3-Diarization)** (Streaming Sortformer v3, [OpenMDW-1.1](https://openmdw.ai/license/1-1/)) —— diarización de hablantes, hasta 8 hablantes; exportación ONNX de [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs).
- **[Google Gemma](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf)** —— Gemma-4 12B (traducción y visión), las cuantizaciones de [unsloth](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF) y [llama.cpp](https://github.com/ggml-org/llama.cpp), que las ejecuta.
- **[chenmozhijin / BSRoformer.cpp](https://github.com/chenmozhijin/BSRoformer.cpp)** y **[GaboxR67](https://huggingface.co/GaboxR67)** —— el motor nativo de separación con sus modelos GGUF y el punto de control de Mel-Band Roformer.
- **[Systran / faster-whisper](https://github.com/SYSTRAN/faster-whisper)**, **[deepdml](https://huggingface.co/deepdml)** y **[Purfview](https://github.com/Purfview/whisper-standalone-win)** —— los modelos Whisper en formato CTranslate2 y la versión independiente que los ejecuta.
- **[InsightFace](https://github.com/deepinsight/insightface)** (mediante [immich-app/buffalo_l](https://huggingface.co/immich-app/buffalo_l)), **[ByteDance LVFace](https://huggingface.co/bytedance-research/LVFace)**, **[deepghs](https://huggingface.co/deepghs)** (CCIP, detección de caras de anime), **[FaceFusion](https://huggingface.co/facefusion/models-3.1.0)** y **[WeSpeaker](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM)** —— los modelos de caras y voz del reparto de personajes.
- **[PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)** —— PP-OCR, los modelos de detección y reconocimiento de texto en pantalla.
- **[ONNX Runtime](https://github.com/microsoft/onnxruntime)**, **[FFmpeg](https://ffmpeg.org)** con las compilaciones de [BtbN](https://github.com/BtbN/FFmpeg-Builds), **[JASSUB](https://github.com/ThaUnknown/jassub)**, **[Tauri](https://tauri.app)** y **[ort](https://github.com/pykeio/ort)**.
- **NVIDIA CUDA runtime, cuBLAS, cuFFT y cuDNN** —— las bibliotecas sobre las que corren los caminos de GPU.
- **Serega (SilentBob)** —— versión 3.1.0: multi-take, referencia emocional, sincronización y editor de subtítulos. **[@nevoin](https://github.com/nevoin)** —— el registro detallado de [#1](https://github.com/timoncool/dub-studio/issues/1) que llevó a la corrección para ffmpeg 8. **[LongNT2011](https://github.com/LongNT2011)** —— [PR #1](https://github.com/LongNT2011/dub-studio/pull/1) en su fork, una corrección de texto ruso escrito a mano en la interfaz en inglés que inspiró el trabajo de i18n.

## Apoya al autor

Creo software de código abierto e investigo en IA —— la mayor parte es de acceso libre. Las donaciones me permiten crear e investigar más.

**[Todas las formas de apoyar](DONATE.md)** | **[dalink.to/nerual_dreming](https://dalink.to/nerual_dreming)** | **[boosty.to/neuro_art](https://boosty.to/neuro_art)**

- **BTC:** `1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC`
- **ETH (ERC20):** `0xb5db65adf478983186d4897ba92fe2c25c594a0c`
- **USDT (TRC20):** `TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C`

## Licencia

El código de la app es [MIT](LICENSE). **Los modelos no**: cada uno conserva su propia licencia, y lo que sigue es lo que dicen hoy sus páginas. Léelas antes de publicar o vender un vídeo doblado.

| Componente | Licencia | Qué significa |
|---|---|---|
| Higgs Audio v3 (Boson AI) | [Boson Higgs TTS 3 Research and Non-Commercial License](https://huggingface.co/bosonai/higgs-tts-3-4b/blob/main/LICENSE) | Gratis para investigación, uso personal y, con el Creator Use Grant, para creadores digitales que publican y monetizan su propio contenido, si citan Higgs Audio de Boson AI. Alojarlo, redistribuirlo o integrarlo en un producto o servicio para terceros, un servicio de doblaje incluido, requiere una licencia comercial de Boson |
| Parakeet-TDT 0.6B v3 (NVIDIA) y su exportación ONNX | CC-BY-4.0 | Citar a NVIDIA |
| Parakeet Ultra (Moondream, sobre NVIDIA Parakeet-TDT) | CC-BY-4.0 | Citar a Moondream y a NVIDIA |
| Nemotron 3 Diarization (NVIDIA) | [OpenMDW-1.1](https://openmdw.ai/license/1-1/) | Citar a NVIDIA; la exportación ONNX viene de altunenes/parakeet-rs |
| Gemma-4 12B (Google) | Apache-2.0 en la página del modelo, que enlaza a los [términos de Gemma 4 de Google](https://ai.google.dev/gemma/docs/gemma_4_license) | Lee ambos |
| Mel-Band Roformer voc_fv6 (GaboxR67), GGUF de chenmozhijin | Las páginas de los modelos no indican licencia; el motor BSRoformer.cpp es MIT | Pregunta a los autores antes de un uso comercial |
| Modelos Whisper (Systran, deepdml) y faster-whisper | MIT | La versión independiente de Purfview no tiene archivo de licencia en su repositorio |
| Detector de caras SCRFD (InsightFace buffalo_l) | InsightFace: modelos preentrenados **solo para investigación no comercial** | El reparto de caras reales queda bajo esta restricción |
| LVFace (ByteDance) | MIT |  |
| CCIP (deepghs) | OpenRAIL | Lee sus restricciones de uso |
| anime_face_detection (deepghs) | MIT |  |
| Oclusor xseg_1 (modelos de FaceFusion) | La página del modelo no indica licencia | Pregunta a los autores antes de un uso comercial |
| WeSpeaker ResNet34-LM | CC-BY-4.0 | Citar a WeSpeaker |
| PP-OCR (PaddleOCR) | Apache-2.0 |  |
| llama.cpp, ONNX Runtime, BSRoformer.cpp, JASSUB | MIT |  |
| FFmpeg (compilación de BtbN) | Compilación GPL de FFmpeg | Se entrega como programa aparte, no enlazado en la app |
| NVIDIA CUDA runtime, cuBLAS, cuFFT, cuDNN | Términos de licencia propios de NVIDIA | Se descargan de NVIDIA y de PyPI, no están en el repositorio |
