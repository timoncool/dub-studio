<div align="center">

<img src="frontend/public/favicon.svg" width="72" alt="Dub Studio"/>

# Dub Studio

**Estúdio de dublagem de vídeo com IA, gratuito e offline, para Windows —— redubla qualquer vídeo para outro idioma com voz clonada, legendas traduzidas e localização do texto na tela. 100% local, zero Python: um `.exe` nativo (Rust + C++/CUDA); todos os modelos e motores baixam com um botão.**

[![License](https://img.shields.io/github/license/timoncool/dub-studio?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/timoncool/dub-studio?style=flat-square)](https://github.com/timoncool/dub-studio/stargazers)
[![Latest release](https://img.shields.io/github/v/release/timoncool/dub-studio?include_prereleases&style=flat-square)](https://github.com/timoncool/dub-studio/releases)
[![Downloads](https://img.shields.io/github/downloads/timoncool/dub-studio/total?style=flat-square)](https://github.com/timoncool/dub-studio/releases)

[English](README.md) · [Русский](README.ru.md) · [中文](README.zh.md) · [Español](README.es.md) · **Português** · [Français](README.fr.md)

### [🌐 Demo ao vivo e showcase antes/depois →](https://timoncool.github.io/dub-studio/)



</div>

## Veja em ação

**[▶ Ver o showcase antes/depois no site →](https://timoncool.github.io/dub-studio/#showcase)** — clipes reais dublados de ponta a ponta numa GPU local: vídeos, modos e idiomas diferentes.

| ![dub](docs/shots/mode-dub-ru.png) | ![voiceover](docs/shots/mode-voiceover-es.png) | ![dub CJK](docs/shots/mode-dub-zh.png) |
|:--:|:--:|:--:|
| 🎙️ **Dublagem** · EN→RU | 🗣️ **Locução** · EN→ES | 🈶 **Dublagem** · 中文 no quadro |
| ![subtitles](docs/shots/mode-subtitles-ru.png) | ![widescreen](docs/shots/mode-dub-cinema-fr.png) | ![transcript](docs/shots/mode-transcribe-pt.png) |
| 📝 **Legendas** · idioma original | 🎬 **Dublagem** · panorâmico 16:9 | 🔤 **Transcrição** · diarização |

## O que é

**Dub Studio** transforma qualquer vídeo em uma versão dublada para outro idioma —— **com o timbre do falante clonado, legendas traduzidas e o texto embutido localizado sobre o próprio quadro**. Solte um clipe e um passe automático inteligente cria o primeiro rascunho; depois um editor ao vivo coloca **cada legenda, voz, caixa de desfoque, fonte e título** sob seu controle com pré-visualização instantânea.

Por padrão tudo roda **localmente na sua máquina** —— sem nuvem, sem assinatura: nem o seu material nem a sua voz saem do computador. E se o seu PC for fraco (não roda o Gemma/Higgs local) ou você quiser mais velocidade e qualidade, as partes pesadas (tradução, visão, TTS, transcrição) podem **opcionalmente** ir para a nuvem via **OpenRouter** —— cada motor escolhido separadamente (local ↔ nuvem), com vozes atribuídas automaticamente por sexo do falante (beta). A chave fica guardada localmente; tudo desativado por padrão.

Esta é **uma reescrita totalmente nativa**. Sem Python embutido, sem torch, sem wheels CUDA. Todo o pipeline é **Rust + motores nativos C++/CUDA (GGUF/ONNX)**: um processo, início rápido, baixo consumo de VRAM. Modelos, motores, runtime CUDA/VC++ e ffmpeg são **baixados e instalados pelo próprio app** no primeiro uso. **O app é feito e testado para uma GPU NVIDIA**: a voz, a tradução e a visão rodam localmente em CUDA, e as legendas são gravadas no vídeo com NVENC. A separação, a diarização e o reconhecimento podem rodar na CPU e as etapas pesadas podem ir para o OpenRouter, mas uma máquina sem NVIDIA não é uma configuração testada: veja *O que roda onde* abaixo.

## Para agentes de IA

Enquanto o Dub Studio está aberto, ele oferece um servidor MCP em `http://127.0.0.1:8793/mcp`: um agente como Claude Code, Claude Desktop, Cursor ou Codex faz tudo o que a janela faz, com o mesmo código — cria projetos a partir de arquivos de vídeo, analisa-os, corrige a tradução, os tempos e os falantes linha por linha, distribui as vozes, estiliza as legendas, adiciona títulos e áreas de desfoque, renderiza, exporta o mesmo vídeo para mais idiomas, gera SRT e TXT e salva os resultados em uma pasta. Em Configurações, **Agente (MCP)** mostra se há um agente conectado e o que colar no cliente.

Com este repositório, um agente pode instalar tudo e controlar o estúdio sozinho:

1. Instalar o estúdio a partir da [versão mais recente](https://github.com/timoncool/dub-studio/releases/latest) e iniciá-lo.
2. Conectar-se ao seu servidor MCP:
   ```bash
   claude mcp add --transport http dub-studio http://127.0.0.1:8793/mcp
   ```
   Outros clientes: `{ "mcpServers": { "dub-studio": { "type": "streamable-http", "url": "http://127.0.0.1:8793/mcp" } } }`
3. Ler a skill que o servidor oferece (recurso `studio://skill`, prompt `studio`), o mesmo texto de [docs/mcp-skill.md](docs/mcp-skill.md) — todas as ferramentas, as regras básicas e receitas passo a passo — e começar pela ferramenta `studio_status`.

[llms.txt](llms.txt) diz o mesmo para as ferramentas que o procuram. Para manter a skill no Claude Code, salve [docs/mcp-skill.md](docs/mcp-skill.md) como `~/.claude/skills/dub-studio/SKILL.md`. Só podem se conectar os agentes deste computador e a própria janela do estúdio.

## Cinco modos, alternáveis na hora

| Modo | O que faz |
|------|-----------|
| 🎙️ **Dublagem** | Redublagem completa para o idioma-alvo com o **timbre original clonado** —— elenco automático por falante ou voz à escolha |
| 🗣️ **Locução (voz sobreposta)** | Voz traduzida **sobre o original atenuado** —— o original ainda é ouvido embaixo; equilíbrio ajustável |
| 📝 **Legendas** | Legendas no **idioma original**, mantendo o áudio original —— sem dublagem nem tradução |
| ✨ **Remix engraçado** | Dê um tema («como um pirata», «como um telejornal») → o modelo **reescreve todo o roteiro** e redubla |
| 🎬 **Transcrição** | **Transcrição diarizada** limpa com layout por falante, reprodução tipo karaokê, criação de vozes com um clique, export `.srt`/`.txt` |

Carregue um clipe uma vez e envie-o para qualquer modo dentro do editor.

## Recursos

- **Clonagem de voz** —— o timbre original é clonado e fala o novo idioma (motor nativo [Higgs Audio v3](https://huggingface.co/bosonai), GGUF). Elenco automático por falante ou sua própria voz de um pack.
- **Diarização de falantes** —— quem fala e quando (NVIDIA **Nemotron 3 Diarization**, até 8 vozes), uma voz distinta por falante.
- **Elenco de personagens (beta)** —— um personagem é um par **«rosto + voz»**. O app reúne os rostos de todo o vídeo, reconhece a mesma pessoa e **a vincula a um falante por coocorrência** (quem aparece em close-up recebe a voz, um ouvinte ao fundo não); escolhe automaticamente o quadro-avatar mais nítido e **salva um perfil de elenco para a série inteira** —— você atribui vozes e descrições uma vez e o **próximo episódio aplica tudo sozinho**. Um interruptor **«rostos reais / desenho·anime»** troca a detecção de rostos conforme o conteúdo.
- **Escolha do motor de ASR** — transcreva com **Parakeet-TDT** (GPU, padrão) ou **Whisper** ([faster-whisper standalone da Purfview](https://github.com/Purfview/whisper-standalone-win), roda na CPU) — escolha o tamanho do modelo (tiny … large-v3-turbo) e o quant (compute type) direto nas configurações.
- **Importar legendas prontas** — use seu `.srt`/`.ass` como transcrição exata: o texto e o tempo vêm do arquivo em vez do reconhecimento automático (os falantes ainda são atribuídos pela diarização). Marque **«legendas já no idioma de destino»** e a tradução também é ignorada — um vídeo em inglês + suas legendas em russo → dublagem em russo direto delas.
- **Exportação multilíngue** — a **▾** ao lado de Exportar envia um vídeo para vários idiomas de uma vez; cada um herda todas as suas edições (layout de legendas, estilos, caixas de desfoque, voz clonada) — só mudam a tradução e a dublagem.
- **Salvar e reabrir projetos** — salvamento automático, lista de projetos recentes na tela inicial e volte ao trabalho inacabado em um clique.
- **Busca nas listas de vozes e idiomas** — digite parte de um nome para filtrar centenas de vozes ou mais de 100 idiomas; os idiomas também combinam pelo nome no idioma da interface.
- **Pipeline combinável** — interruptores independentes na entrada: áudio (original / dublagem / voz sobreposta / transcrição) × legendas (nenhuma / original / traduzidas) × gravar no vídeo sim/não × remix humorístico. Qualquer combinação — dublagem sem legendas, legendas traduzidas sem dublagem, dublagem humorística com suas próprias vozes — também no lote e no editor.
- **Localização de texto na tela** —— OCR detecta o texto embutido (**PP-OCR** ONNX), **desfoca o original** e imprime por cima um título localizado com estilo combinado —— um recurso que nenhuma outra ferramenta tem.
- **Tradução + análise visual de estilo** —— a transcrição é traduzida localmente com **Gemma-4 12B** (GGUF, llama.cpp); um passe de visão lê o layout do quadro: estilo de legenda, títulos, marcas, zonas de texto.
- **Separação vocal SOTA** —— **Mel-Band Roformer** (BSRoformer.cpp nativo em CUDA) separa voz de música: a faixa de fundo é **preservada** e o clone se prende à fala limpa.
- **26 predefinições de legenda** —— karaokê / palavra a palavra / hormozi / neon e mais, renderizadas **no seu quadro** (WYSIWYG, JASSUB sobre o mesmo `.ass` que o ffmpeg grava).
- **Transcrição karaokê** —— reproduza o vídeo e acompanhe a linha e a **palavra** atuais acendendo na transcrição.
- **Editor ao vivo** —— edite transcrição, vozes, estilo de legenda, caixas de desfoque, títulos; **prévia ~0,17 s/quadro**, cada mudança visível na hora.
- **Regeneração inteligente** —— ao exportar, só os segmentos alterados são ressintetizados, não o clipe inteiro.
- **Suas próprias falas** —— insira frases personalizadas na transcrição; cada uma é dublada com a voz clonada do falante e aparece nas legendas.
- **Processamento em lote** —— fila de arquivos, todos com uma configuração, progresso por arquivo.
- **Comparar antes/depois** —— original e dublagem lado a lado.
- **100+ idiomas** —— dublagem para qualquer idioma principal (espanhol, chinês, japonês, árabe, hindi e mais), com detecção automática do idioma de origem.
- **Qualquer formato de vídeo** —— MP4, MOV, MKV, WEBM, AVI e mais (decodificado via ffmpeg).
- **Instalação de um botão + atualização automática** —— modelos, motores, runtime CUDA/VC++ e ffmpeg baixam no primeiro uso; o app se atualiza sozinho.
- **Downloads retomáveis** —— modelos grandes (10 GB+) retomam de onde pararam após queda de conexão, em vez de recomeçar.
- **Rode cada etapa onde quiser** —— separação, diarização e reconhecimento alternam de forma independente entre **GPU e CPU**, e o reconhecimento, a tradução, a visão e a voz podem ser delegados ao **OpenRouter**. A tabela *O que roda onde*, abaixo, mostra o que cada etapa realmente pode usar.
- **Ajuste ao seu hardware** —— cada motor traz várias quantizações (TTS Q8/Q6/Q4, tradução Q4…Q8, ASR int8/fp32 ou Whisper tiny…large-v3-turbo, separação Q8/Q5/Q4) — troque nas configurações; limite o lote de prefill e a duração da referência para GPU de 8–12 GB e 32 GB de RAM.
- **Totalmente portátil** —— nada é escrito no seu perfil de usuário; apague a pasta e não sobra rastro.

## Capturas

Tela inicial —— cinco modos, prévia do vídeo escolhido, seleção de idioma, qualquer formato:

![Tela inicial do Dub Studio](docs/screenshot-home.png)

Modo transcrição —— transcrição diarizada com layout por falante, karaokê e criação de vozes de cada falante com um clique:

![Modo transcrição do Dub Studio](docs/screenshot-transcribe.png)

## Requisitos

- **SO:** Windows 10 / 11 (x64); Linux x86-64 como build experimental (veja *Linux (experimental)*)
- **GPU:** NVIDIA com 8 GB de VRAM ou mais (há predefinições para 8, 12, 16, 24 e 32 GB) e um driver recente. A voz local (Higgs Audio), a tradução e a visão (Gemma) rodam em CUDA, e as legendas são gravadas com NVENC. Sem NVIDIA, só podem funcionar as etapas que *O que roda onde* marca para a CPU ou a nuvem, e essa configuração não é testada
- **WebView2** —— pré-instalado no Windows 11; no Windows 10 o instalador o baixa (se falhar, veja *Solução de problemas*)
- **Disco:** ~15 GB para os modelos padrão, os motores e o runtime (baixados no primeiro uso), mais espaço para seus projetos; as quantizações alternativas e os modelos Whisper são extras

Numa máquina com NVIDIA, a única coisa que você instala à mão é um **[driver NVIDIA](https://www.nvidia.com/Download/index.aspx)** recente. Todo o resto —modelos (Higgs Audio v3, Gemma-4 12B + vision, Parakeet-TDT, Nemotron 3 Diarization, Mel-Band Roformer), motores, runtime CUDA e ffmpeg— o app baixa com um botão no primeiro uso.

## Início rápido

1. **Baixe** a versão portátil em [Releases](https://github.com/timoncool/dub-studio/releases) e descompacte onde quiser (ou instale via `-setup.exe` / `.msi`).
2. **Execute** `Dub Studio.exe`.
3. No painel de **primeiro uso** clique em **Baixar tudo** —— o app busca modelos, motores e runtime (~15 GB, uma vez).
4. **Solte um vídeo**, escolha o idioma-alvo → o passe automático cria o primeiro rascunho. Ajuste tudo no editor e clique em **Exportar**.

> Tudo baixa e fica **dentro da pasta do app**. Modelos, caches e projetos não vão para outro lugar.

O que mudou e quando está em [CHANGELOG.md](CHANGELOG.md), e o botão de brilhos no topo do app o mostra. Regras para colaboradores e agentes de programação: [AGENTS.md](AGENTS.md).

## Tudo o que o app baixa

O painel de primeiro uso baixa tudo isso com um botão. Atrás de um proxy, ou onde o Hugging Face está bloqueado, configure o proxy em Configurações → **Rede**: como no Windows, próprio (HTTP, HTTPS, SOCKS5 ou SOCKS4; o formato do vendedor `host:port:login:senha` serve como está) ou nenhum. Por ele passam os downloads de modelos, a nuvem e as atualizações do app. Você também pode baixar os arquivos diretos, colocá-los onde diz a última coluna, contando a partir da pasta do app (a que contém `models\`), e apertar **Importar da pasta**; os componentes em arquivos compactados (`.zip`, `.whl`) o próprio app baixa.

Um arquivo no lugar indicado conta como instalado quando o tamanho é exatamente o indicado; cada download é conferido com o SHA-256 fixado do arquivo antes de ser usado, e um arquivo que já está no lugar é conferido da mesma forma antes de ser pulado. **Importar da pasta** (no painel de primeiro uso e nas configurações de modelos) procura os arquivos de um componente por nome e tamanho exato na pasta escolhida e os coloca no lugar por link físico ou cópia. Componentes que vêm como arquivos zip ou wheel (motores, runtimes) não são importados e contam como instalados só quando o próprio app os baixou: ele guarda um registro do arquivo conferido ao lado dos arquivos extraídos. Os tamanhos e os hashes são os do próprio manifesto do app, `crates/dub-server/src/setup.rs`, e esta tabela é conferida com ele.

O runtime do Visual C++ e os modelos PP-OCR vêm dentro da versão e não são baixados; o driver NVIDIA é instalado à parte.

<!-- downloads:start -->
| Componente | Necessidade | Arquivos (links diretos) | Tamanho | Onde colocar |
|---|---|---|---|---|
| Higgs Audio v3 Q8_0 | obrigatório | [q8_0.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/q8_0.gguf) → `models\higgs-q8_0\q8_0.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/config.json) → `models\higgs-q8_0\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/chat_template.jinja) → `models\higgs-q8_0\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer.json) → `models\higgs-q8_0\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer_config.json) → `models\higgs-q8_0\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q8_0\higgs_audio_v2_tokenizer_config.json` | 5.5 GB | como está, no caminho após a seta |
| audiocpp_engine.dll (Higgs engine) | obrigatório | [audiocpp_engine.dll](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/engines/audiocpp_engine.dll) → `models\higgs-engine\audiocpp_engine.dll` | 72 MB | como está, no caminho após a seta |
| Gemma-4 12B QAT q4_0 + vision | obrigatório | [gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\gemma-4-12b-it-qat-q4_0.gguf`<br>[mmproj-gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/mmproj-gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\mmproj-gemma-4-12b-it-qat-q4_0.gguf` | 7.2 GB | como está, no caminho após a seta |
| Gemma-4 12B Q5_K_M + vision | opcional | [gemma-4-12b-it-Q5_K_M.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q5_K_M.gguf) → `models\mt-q5_0\gemma-4-12b-it-Q5_K_M.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q5_0\mmproj-F16.gguf` | 8.6 GB | como está, no caminho após a seta |
| Gemma-4 12B Q6_K + vision | opcional | [gemma-4-12b-it-Q6_K.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q6_K.gguf) → `models\mt-q6_k\gemma-4-12b-it-Q6_K.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q6_k\mmproj-F16.gguf` | 10.0 GB | como está, no caminho após a seta |
| Gemma-4 12B Q8_0 + vision | opcional | [gemma-4-12b-it-Q8_0.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q8_0.gguf) → `models\mt-q8_0\gemma-4-12b-it-Q8_0.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q8_0\mmproj-F16.gguf` | 12.8 GB | como está, no caminho após a seta |
| Parakeet-TDT 0.6B v3 int8 | obrigatório | [encoder-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx) → `models\tdt\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx) → `models\tdt\decoder_joint-model.int8.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt\config.json` | 671 MB | como está, no caminho após a seta |
| Higgs Audio v3 Q6_K | opcional | [q6_k.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/q6_k.gguf) → `models\higgs-q6_k\q6_k.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/config.json) → `models\higgs-q6_k\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/chat_template.jinja) → `models\higgs-q6_k\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer.json) → `models\higgs-q6_k\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer_config.json) → `models\higgs-q6_k\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q6_k\higgs_audio_v2_tokenizer_config.json` | 5.0 GB | como está, no caminho após a seta |
| Higgs Audio v3 Q4_K_M | opcional | [q4_k_m.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/q4_k_m.gguf) → `models\higgs-q4_k_m\q4_k_m.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/config.json) → `models\higgs-q4_k_m\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/chat_template.jinja) → `models\higgs-q4_k_m\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer.json) → `models\higgs-q4_k_m\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer_config.json) → `models\higgs-q4_k_m\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q4_k_m\higgs_audio_v2_tokenizer_config.json` | 4.1 GB | como está, no caminho após a seta |
| Parakeet-TDT 0.6B v3 fp32 | opcional | [encoder-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx) → `models\tdt-fp32\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx.data) → `models\tdt-fp32\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.onnx) → `models\tdt-fp32\decoder_joint-model.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-fp32\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt-fp32\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-fp32\config.json` | 2.5 GB | como está, no caminho após a seta |
| Parakeet Ultra 0.6B fp32 (Moondream) | opcional | [encoder-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx) → `models\tdt-ultra\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx.data) → `models\tdt-ultra\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/decoder_joint-model.onnx) → `models\tdt-ultra\decoder_joint-model.onnx`<br>[vocab.txt](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/vocab.txt) → `models\tdt-ultra\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/tdt/nemo128.onnx) → `models\tdt-ultra\nemo128.onnx`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-ultra\config.json` | 2.6 GB | como está, no caminho após a seta |
| Parakeet Ultra 0.6B int8 (Moondream) | opcional | [encoder-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/encoder-model.int8.onnx) → `models\tdt-ultra-int8\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/decoder_joint-model.int8.onnx) → `models\tdt-ultra-int8\decoder_joint-model.int8.onnx`<br>[vocab.txt](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/vocab.txt) → `models\tdt-ultra-int8\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-ultra-int8\nemo128.onnx`<br>[config.json](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/config.json) → `models\tdt-ultra-int8\config.json` | 0.7 GB | como está, no caminho após a seta |
| Whisper-Faster (faster-whisper standalone) | opcional | [Whisper-Faster_r192.3_windows.zip](https://github.com/Purfview/whisper-standalone-win/releases/download/faster-whisper/Whisper-Faster_r192.3_windows.zip) | 88 MB | descompactar os arquivos, sem subpastas, em `tools\whisper\` |
| Whisper CUDA (cuBLAS 11, cuDNN 8) | opcional | [libcublas-windows-x86_64-11.11.3.6-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-11.11.3.6-archive.zip)<br>[cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip](https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip) | 1.1 GB | pegar todos os .dll do arquivo e colocar em `tools\whisper\` |
| Whisper tiny | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/model.bin) → `models\whisper\faster-whisper-tiny\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/config.json) → `models\whisper\faster-whisper-tiny\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/tokenizer.json) → `models\whisper\faster-whisper-tiny\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/vocabulary.txt) → `models\whisper\faster-whisper-tiny\vocabulary.txt` | 78 MB | como está, no caminho após a seta |
| Whisper base | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/model.bin) → `models\whisper\faster-whisper-base\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/config.json) → `models\whisper\faster-whisper-base\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/tokenizer.json) → `models\whisper\faster-whisper-base\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/vocabulary.txt) → `models\whisper\faster-whisper-base\vocabulary.txt` | 148 MB | como está, no caminho após a seta |
| Whisper small | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/model.bin) → `models\whisper\faster-whisper-small\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/config.json) → `models\whisper\faster-whisper-small\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/tokenizer.json) → `models\whisper\faster-whisper-small\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/vocabulary.txt) → `models\whisper\faster-whisper-small\vocabulary.txt` | 486 MB | como está, no caminho após a seta |
| Whisper medium | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/model.bin) → `models\whisper\faster-whisper-medium\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/config.json) → `models\whisper\faster-whisper-medium\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/tokenizer.json) → `models\whisper\faster-whisper-medium\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/vocabulary.txt) → `models\whisper\faster-whisper-medium\vocabulary.txt` | 1.5 GB | como está, no caminho após a seta |
| Whisper large-v3 | opcional | [model.bin](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/model.bin) → `models\whisper\faster-whisper-large-v3\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/config.json) → `models\whisper\faster-whisper-large-v3\config.json`<br>[preprocessor_config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/tokenizer.json) → `models\whisper\faster-whisper-large-v3\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/vocabulary.json) → `models\whisper\faster-whisper-large-v3\vocabulary.json` | 3.1 GB | como está, no caminho após a seta |
| Whisper large-v3-turbo | opcional | [model.bin](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/model.bin) → `models\whisper\faster-whisper-large-v3-turbo\model.bin`<br>[config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/config.json) → `models\whisper\faster-whisper-large-v3-turbo\config.json`<br>[preprocessor_config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3-turbo\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/tokenizer.json) → `models\whisper\faster-whisper-large-v3-turbo\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/vocabulary.json) → `models\whisper\faster-whisper-large-v3-turbo\vocabulary.json` | 1.6 GB | como está, no caminho após a seta |
| Nemotron 3 Diarization | recomendado | [nemotron3_diar_v3.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/nemotron3_diar_v3.onnx) → `models\nemotron-diar\nemotron3_diar_v3.onnx`<br>[LICENSE](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/LICENSE) → `models\nemotron-diar\LICENSE` | 401 MB | como está, no caminho após a seta |
| Mel-Band Roformer voc_fv6 Q8_0 | recomendado | [voc_fv6-Q8_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q8_0.gguf) → `models\bsroformer\voc_fv6-Q8_0.gguf` | 252 MB | como está, no caminho após a seta |
| Mel-Band Roformer voc_fv6 Q5_0 | opcional | [voc_fv6-Q5_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q5_0.gguf) → `models\bsroformer\voc_fv6-Q5_0.gguf` | 167 MB | como está, no caminho após a seta |
| Mel-Band Roformer voc_fv6 Q4_0 | opcional | [voc_fv6-Q4_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q4_0.gguf) → `models\bsroformer\voc_fv6-Q4_0.gguf` | 139 MB | como está, no caminho após a seta |
| Modelos de casting (rostos e voz) | recomendado | [model.onnx](https://huggingface.co/immich-app/buffalo_l/resolve/d09715916a0778919a770c343533641e250b8699/detection/model.onnx) → `models\faces\det_10g.onnx`<br>[LVFace-L_Glint360K.onnx](https://huggingface.co/bytedance-research/LVFace/resolve/b12702ab1f5c721748e054a66dc90e1edd1f0724/LVFace-L_Glint360K/LVFace-L_Glint360K.onnx) → `models\faces\LVFace-L_Glint360K.onnx`<br>[model_feat.onnx](https://huggingface.co/deepghs/ccip_onnx/resolve/eb2acdd29af1703388d3d0c04221add322bc9110/ccip-caformer-24-randaug-pruned/model_feat.onnx) → `models\faces\ccip\model_feat.onnx`<br>[model.onnx](https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_s/model.onnx) → `models\faces\anime_face\model.onnx`<br>[xseg_1.onnx](https://huggingface.co/facefusion/models-3.1.0/resolve/c9e3a503d8e84e91c5cd89ee2d510fe5e793e570/xseg_1.onnx) → `models\faces\occluder\xseg_1.onnx`<br>[voxceleb_resnet34_LM.onnx](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/resolve/f0c48c298fd835726c27956a5d617bad7115627e/voxceleb_resnet34_LM.onnx) → `models\faces\wespeaker\voxceleb_resnet34_LM.onnx` | 1.3 GB | como está, no caminho após a seta |
| BSRoformer.cpp (CUDA) | recomendado | [BSRoformer-windows-cuda-13.1.0.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-cuda-13.1.0.zip) | 165 MB | descompactar os arquivos, sem subpastas, em `tools\bsroformer\` |
| BSRoformer.cpp (CPU) | opcional | [BSRoformer-windows-x64-msvc.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-x64-msvc.zip) | 671 KB | descompactar os arquivos, sem subpastas, em `tools\bsroformer-cpu\` |
| llama.cpp server (CUDA) | obrigatório | [llama-b11146-bin-win-cuda-13.4-x64.zip](https://github.com/ggml-org/llama.cpp/releases/download/b11146/llama-b11146-bin-win-cuda-13.4-x64.zip) | 150 MB | descompactar os arquivos, sem subpastas, em `tools\llama\` |
| ONNX Runtime | obrigatório | [onnxruntime-win-x64-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-1.28.2.zip) | 79 MB | descompactar com a árvore de pastas em `models\runtime\` |
| ONNX Runtime GPU (CUDA) | recomendado | [onnxruntime-win-x64-gpu_cuda13-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-gpu_cuda13-1.28.2.zip) | 366 MB | descompactar com a árvore de pastas em `models\runtime\` |
| FFmpeg (static build) | obrigatório | [ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip](https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-31-13-27/ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip) | 171 MB | pegar ffmpeg.exe e ffprobe.exe do arquivo e colocar em `tools\ffmpeg\` |
| yt-dlp + deno | opcional | [yt-dlp.exe](https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.exe) → `tools\yt-dlp\yt-dlp.exe`<br>[deno-x86_64-pc-windows-msvc.zip](https://github.com/denoland/deno/releases/download/v2.9.7/deno-x86_64-pc-windows-msvc.zip) | 60 MB | como está, no caminho após a seta<br>descompactar os arquivos, sem subpastas, em `tools\yt-dlp\` |
| CUDA runtime (cudart, cuBLAS, cuFFT) | obrigatório | [nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/86/00/d5436004268f049214193659ebc36550b5ef3925c3d13b4cc980e13be6f5/nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl)<br>[nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/a3/df/f1246959833e2c437db8be3e5b477f66b87f8817821ed40de6c7561c9a36/nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl)<br>[libcufft-windows-x86_64-12.4.0.43-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcufft/windows-x86_64/libcufft-windows-x86_64-12.4.0.43-archive.zip) | 586 MB | pegar todos os .dll do arquivo e colocar em `models\higgs-engine\` |
| cuDNN 9 | recomendado | [nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/87/6a/e55ff0ac26a5c6e2b21f41c9d04ad096b4ed6da593fba7e25845c61b0532/nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl) | 436 MB | pegar todos os .dll do arquivo e colocar em `models\higgs-engine\` |
<!-- downloads:end -->

## O que roda onde

Cada etapa tem seu próprio seletor de dispositivo. *Sim* significa que o código tem esse caminho; a tabela não diz nada sobre velocidade nem cobertura de testes, e os caminhos de CPU são mais lentos. Os caminhos na nuvem precisam de uma chave do OpenRouter (Configurações) e vêm desativados por padrão.

| Etapa | Motor | GPU NVIDIA | CPU | OpenRouter (nuvem) |
|---|---|---|---|---|
| Separação | Mel-Band Roformer (BSRoformer.cpp) | sim (versão CUDA) | sim (versão CPU separada, mais lenta) | não |
| Diarização | Nemotron 3 Diarization | sim (ONNX Runtime CUDA) | sim | não |
| Reconhecimento de fala | Parakeet-TDT ou Whisper-Faster | sim | sim | sim |
| Tradução e visão | Gemma-4 12B (llama.cpp) | sim (versão CUDA) | não | sim |
| Voz e clonagem | Higgs Audio v3 | sim (CUDA) | não | sim |
| Texto na tela | PP-OCR | não | sim | não |
| Casting (rostos e vozes) | SCRFD, LVFace, anime_face, CCIP, WeSpeaker | não | sim | não |
| Legendas gravadas no vídeo | ffmpeg | sim (NVENC) | não | não |

Sem NVIDIA, a voz, a tradução, a visão e o reconhecimento podem ir para a nuvem e a separação e a diarização para a CPU, mas a gravação das legendas não tem caminho de CPU, então uma máquina assim não é uma configuração testada.

## Estatísticas anônimas e novidades

O app pede novidades ao servidor do autor ao iniciar e a cada seis horas. A requisição não leva nenhum id, então as novidades chegam seja qual for a sua escolha abaixo: as novas aparecem no topo de Novidades e, sem conexão, o app mostra as novidades da sua versão.

A tela do primeiro início tem uma caixa **Enviar estatísticas de uso anônimas**, marcada por padrão. O mesmo controle fica em Configurações → Estatísticas anônimas, ao lado de **O que é enviado** (o relatório exato de hoje) e **Novo id de instalação**. Enquanto estiver marcada, o app envia uma vez por dia:

- um id de instalação aleatório criado neste computador, sem ligação com o hardware ou uma conta; desmarcar a caixa o apaga;
- o app e a sua versão, o nome e a versão do sistema, o idioma da janela;
- a placa de vídeo: fabricante, faixa de memória de vídeo (até 8, 12, 16, 24+ GB) e se o CUDA funciona;
- quantas tarefas (análise, dublagem, render, exportação, download e as demais) terminaram, falharam ou foram canceladas no dia.

Nunca: vídeos, transcrições, traduções, vozes, nomes de arquivos ou caminhos, nada pessoal. O servidor guarda o país que a Cloudflare informa para a conexão, não o endereço IP. `DO_NOT_TRACK=1` ou `STUDIO_TELEMETRY=0` no ambiente desligam as estatísticas por completo: não existe id e nada é contado.

## Solução de problemas

**O instalador para no WebView2.** A janela do app roda sobre o Microsoft Edge WebView2, e o instalador o baixa quando o Windows não o tem. Com uma conexão bloqueada ou instável, ou em versões do Windows 10 que recusam o pequeno inicializador da Microsoft (erro 0x80040902), esse download falha. Instale o WebView2 com o instalador independente da Microsoft, [Evergreen Standalone x64](https://go.microsoft.com/fwlink/p/?LinkId=2124701), e execute o instalador do Dub Studio de novo.

**Os downloads travam ou falham.** Arquivos grandes retomam de onde pararam, então aperte o botão outra vez. Se o Hugging Face ou o GitHub estiverem bloqueados para você, configure um proxy em Configurações → **Rede**, ou baixe à mão os arquivos diretos da tabela, coloque-os onde diz a última coluna e aperte **Importar da pasta**.

**A exportação para com `Unrecognized option 'filter_complex_script'`.** Era uma falha no ffmpeg 8 e mais novos, corrigida na 3.1.1: atualize o app. O app usa o ffmpeg de `tools\ffmpeg` e, se não houver nenhum, aceita o que estiver no `PATH`. Ao relatar um erro de exportação, anexe a saída completa do ffmpeg que está no log.

**O reconhecimento falha com `MemcpyToHost` ou `Failed to allocate memory`.** A GPU ficou sem memória no reconhecimento de fala de um arquivo longo ou grande (issue #4). Feche outros programas que usam a GPU, ou ponha a etapa de reconhecimento em **CPU** nas configurações, e execute de novo.

**`CUDA execution provider is not enabled`, ou tudo cai em um só falante.** As bibliotecas da GPU estão ausentes ou são antigas demais para o driver. Instale o driver NVIDIA atual, aperte o botão de download do runtime CUDA, do cuDNN e do ONNX Runtime GPU nas configurações de modelos, ou mude a etapa para CPU.

## Como funciona

`analyze()` é um primeiro passe fixo: separação → ASR com tempos por palavra → diarização → tradução contextual + visão (estilo de legenda / títulos / marcas) → OCR (layout / caixas de desfoque). O resultado é um documento **Project** editável. Cada edição é um patch sobre ele com prévia ~0,17 s/quadro; a exportação só reexecuta **os estágios alterados**.

**Stack:** um shell nativo **Tauri 2 (Rust)** executa `dub-server` (axum) dentro do mesmo processo em `127.0.0.1:8793` (o MCP para agentes é `/mcp` na mesma porta) e abre uma janela sobre a SPA —— React 19 + Vite + Tailwind + react-konva sobre JASSUB. Motores: Parakeet-TDT ou Whisper (ASR) · Nemotron 3 Diarization (diarização) · Gemma-4-12B GGUF (tradução + visão, llama.cpp) · Higgs Audio v3 (TTS) · Mel-Band Roformer (separação, BSRoformer.cpp) · PP-OCR (ONNX) · ffmpeg/NVENC. **Nenhum processo Python em tempo de execução.**

### Compilar do código

```bash
git clone https://github.com/timoncool/dub-studio.git
cd dub-studio

cd frontend && npm install && npm run build && cd ..   # 1) SPA
cargo build --release -p dub-server                     # 2) servidor nativo (axum)
cd desktop && npm install && npx tauri build            # 3) shell de desktop (Tauri)
```

Requer Node 20+, Rust (toolchain MSVC) e WebView2. Os motores nativos não precisam ser recompilados —— o app baixa binários pré-compilados.

### Linux (experimental)

O .deb e o AppImage para Linux x86-64 são **experimentais**. Eles vêm do mesmo código com as versões para Linux dos mesmos motores (llama.cpp, ONNX Runtime, BSRoformer.cpp, ffmpeg, o motor Higgs para Linux, yt-dlp, faster-whisper), mas o autor trabalha no Windows e não os rodou num desktop Linux de verdade. **Se você vive no Linux, seria ótimo se você os lapidasse e mandasse as correções de volta num pull request.**

- A voz local precisa de uma NVIDIA RTX 30 ou mais nova: o motor Higgs para Linux é compilado só para sm 86, 89 e 120. As vozes na nuvem funcionam em qualquer máquina.
- O driver da NVIDIA (580 ou mais novo), `libgomp1` e `libssl3` vêm do sistema; modelos, motores e bibliotecas CUDA o app baixa na primeira execução em `~/.local/share/dub-studio` (`$XDG_DATA_HOME`).
- Compilados à mão: o workflow `Linux build (experimental)` (`.github/workflows/release-linux.yml`) ou `scripts/build-release-linux.sh <pasta com models/ocr>`.

## Contribuições e forks

**Colaboradores são muito bem-vindos.** Eu ficaria genuinamente feliz em ver o Dub Studio portado para outras plataformas e GPUs — a arquitetura permite, eu simplesmente não tenho fôlego para fazer os ports sozinho. Se você quer rodá-lo em **GPUs AMD / Intel, macOS ou Linux**, faça um fork e vá em frente — PRs são bem-vindos. O Linux já tem um build experimental (veja *Linux (experimental)*): lapidá-lo é a ajuda mais bem-vinda.

**Localizações adicionais** também são bem-vindas: hoje o app e a landing estão em 6 idiomas — traduza os arquivos de idioma (`frontend/src/locales/` e o dicionário em `docs/index.html`) e abra um PR com o seu.

## Autores

- **Nerual Dreming** —— [Telegram](https://t.me/nerual_dreming) | [neuro-cartel.com](https://neuro-cartel.com) | fundador da [ArtGeneration.me](https://artgeneration.me)
- **Neuro-Soft** —— [Telegram](https://t.me/neuroport) | apps de IA portáteis

## Créditos

- **[Boson AI](https://huggingface.co/bosonai)** —— o modelo Higgs Audio v3, e **[drbaph / Higgs-Audio-v3-Studio](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio)** —— as quantizações GGUF e o motor nativo `audiocpp_engine.dll`.
- **[NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)** (CC-BY-4.0) —— ASR; pesos ONNX de [istupakov/parakeet-tdt-0.6b-v3-onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx), runtime [altunenes/parakeet-rs](https://github.com/altunenes/parakeet-rs).
- **[Parakeet Ultra](https://huggingface.co/moondream/parakeet-ultra)** da **[Moondream](https://huggingface.co/moondream)**, baseado no parakeet-tdt-0.6b-v3 da NVIDIA (CC-BY-4.0) —— ASR ajustado opcional, com menos erros de reconhecimento; exportação ONNX de [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs). int8: [Masterx/parakeet-tdt-0.6b-ultra-onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx).
- **[NVIDIA Nemotron 3 Diarization](https://huggingface.co/nvidia/Nemotron-3-Diarization)** (Streaming Sortformer v3, [OpenMDW-1.1](https://openmdw.ai/license/1-1/)) —— diarização de falantes, até 8 falantes; exportação ONNX de [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs).
- **[Google Gemma](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf)** —— Gemma-4 12B (tradução e visão), as quantizações da [unsloth](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF) e o [llama.cpp](https://github.com/ggml-org/llama.cpp) que as executa.
- **[chenmozhijin / BSRoformer.cpp](https://github.com/chenmozhijin/BSRoformer.cpp)** e **[GaboxR67](https://huggingface.co/GaboxR67)** —— o motor nativo de separação com seus modelos GGUF e o checkpoint do Mel-Band Roformer.
- **[Systran / faster-whisper](https://github.com/SYSTRAN/faster-whisper)**, **[deepdml](https://huggingface.co/deepdml)** e **[Purfview](https://github.com/Purfview/whisper-standalone-win)** —— os modelos Whisper no formato CTranslate2 e a versão independente que os executa.
- **[InsightFace](https://github.com/deepinsight/insightface)** (via [immich-app/buffalo_l](https://huggingface.co/immich-app/buffalo_l)), **[ByteDance LVFace](https://huggingface.co/bytedance-research/LVFace)**, **[deepghs](https://huggingface.co/deepghs)** (CCIP, detecção de rostos de anime), **[FaceFusion](https://huggingface.co/facefusion/models-3.1.0)** e **[WeSpeaker](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM)** —— os modelos de rostos e vozes do casting de personagens.
- **[PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)** —— PP-OCR, os modelos de detecção e reconhecimento de texto na tela.
- **[ONNX Runtime](https://github.com/microsoft/onnxruntime)**, **[FFmpeg](https://ffmpeg.org)** com as builds da [BtbN](https://github.com/BtbN/FFmpeg-Builds), **[JASSUB](https://github.com/ThaUnknown/jassub)**, **[Tauri](https://tauri.app)** e **[ort](https://github.com/pykeio/ort)**.
- **NVIDIA CUDA runtime, cuBLAS, cuFFT e cuDNN** —— as bibliotecas sobre as quais rodam os caminhos de GPU.
- **Serega (SilentBob)** —— versão 3.1.0: multi-take, referência emocional, sincronização e editor de legendas. **[@nevoin](https://github.com/nevoin)** —— o log detalhado de [#1](https://github.com/timoncool/dub-studio/issues/1) que levou à correção para o ffmpeg 8. **[LongNT2011](https://github.com/LongNT2011)** —— [PR #1](https://github.com/LongNT2011/dub-studio/pull/1) no fork dele, uma correção de texto russo fixo na interface em inglês que inspirou o trabalho de i18n.

## Apoie o autor

Faço software de código aberto e pesquisa em IA —— a maior parte é de acesso livre. Doações me permitem criar e pesquisar mais.

**[Todas as formas de apoiar](DONATE.md)** | **[dalink.to/nerual_dreming](https://dalink.to/nerual_dreming)** | **[boosty.to/neuro_art](https://boosty.to/neuro_art)**

- **BTC:** `1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC`
- **ETH (ERC20):** `0xb5db65adf478983186d4897ba92fe2c25c594a0c`
- **USDT (TRC20):** `TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C`

## Licença

O código do app é [MIT](LICENSE). **Os modelos não são**: cada um mantém a própria licença, e o que segue é o que suas páginas dizem hoje. Leia-as antes de publicar ou vender um vídeo dublado.

| Componente | Licença | O que significa |
|---|---|---|
| Higgs Audio v3 (Boson AI) | [Boson Higgs TTS 3 Research and Non-Commercial License](https://huggingface.co/bosonai/higgs-tts-3-4b/blob/main/LICENSE) | Gratuito para pesquisa, uso pessoal e, com o Creator Use Grant, para criadores digitais que publicam e monetizam o próprio conteúdo, desde que creditem o Higgs Audio da Boson AI. Hospedá-lo, redistribuí-lo ou embuti-lo em um produto ou serviço para terceiros, inclusive um serviço de dublagem, exige uma licença comercial da Boson |
| Parakeet-TDT 0.6B v3 (NVIDIA) e sua exportação ONNX | CC-BY-4.0 | Creditar a NVIDIA |
| Parakeet Ultra (Moondream, sobre NVIDIA Parakeet-TDT) | CC-BY-4.0 | Creditar a Moondream e a NVIDIA |
| Nemotron 3 Diarization (NVIDIA) | [OpenMDW-1.1](https://openmdw.ai/license/1-1/) | Creditar a NVIDIA; a exportação ONNX vem de altunenes/parakeet-rs |
| Gemma-4 12B (Google) | Apache-2.0 na página do modelo, que aponta para os [termos do Gemma 4 do Google](https://ai.google.dev/gemma/docs/gemma_4_license) | Leia os dois |
| Mel-Band Roformer voc_fv6 (GaboxR67), GGUF de chenmozhijin | As páginas dos modelos não indicam licença; o motor BSRoformer.cpp é MIT | Pergunte aos autores antes de uso comercial |
| Modelos Whisper (Systran, deepdml) e faster-whisper | MIT | A versão independente da Purfview não tem arquivo de licença no repositório |
| Detector de rostos SCRFD (InsightFace buffalo_l) | InsightFace: modelos pré-treinados **somente para pesquisa não comercial** | O casting de rostos reais está sujeito a essa restrição |
| LVFace (ByteDance) | MIT |  |
| CCIP (deepghs) | OpenRAIL | Leia suas restrições de uso |
| anime_face_detection (deepghs) | MIT |  |
| Oclusor xseg_1 (modelos do FaceFusion) | A página do modelo não indica licença | Pergunte aos autores antes de uso comercial |
| WeSpeaker ResNet34-LM | CC-BY-4.0 | Creditar o WeSpeaker |
| PP-OCR (PaddleOCR) | Apache-2.0 |  |
| llama.cpp, ONNX Runtime, BSRoformer.cpp, JASSUB | MIT |  |
| FFmpeg (build da BtbN) | Build GPL do FFmpeg | Vem como programa separado, não ligado ao app |
| NVIDIA CUDA runtime, cuBLAS, cuFFT, cuDNN | Termos de licença próprios da NVIDIA | Baixados da NVIDIA e do PyPI, não estão no repositório |
