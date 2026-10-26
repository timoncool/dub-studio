<div align="center">

<img src="frontend/public/favicon.svg" width="72" alt="Dub Studio"/>

# Dub Studio

**面向 Windows 的免费离线 AI 视频配音工作室 —— 用克隆的声音、翻译字幕和画面文字本地化，把任意视频重新配音成另一种语言。100% 本地运行，零 Python：一个原生 `.exe`（Rust + C++/CUDA），所有模型与引擎一键下载。**

[![License](https://img.shields.io/github/license/timoncool/dub-studio?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/timoncool/dub-studio?style=flat-square)](https://github.com/timoncool/dub-studio/stargazers)
[![Latest release](https://img.shields.io/github/v/release/timoncool/dub-studio?include_prereleases&style=flat-square)](https://github.com/timoncool/dub-studio/releases)
[![Downloads](https://img.shields.io/github/downloads/timoncool/dub-studio/total?style=flat-square)](https://github.com/timoncool/dub-studio/releases)

[English](README.md) · [Русский](README.ru.md) · **中文** · [Español](README.es.md) · [Português](README.pt.md) · [Français](README.fr.md)

### [🌐 在线演示与前后对比展示 →](https://timoncool.github.io/dub-studio/)



</div>

## 实际效果

**[▶ 在网站上观看前后对比视频展示 →](https://timoncool.github.io/dub-studio/#showcase)** — 真实片段，全部在本地 GPU 上端到端完成：不同的视频、模式和语言。

| ![dub](docs/shots/mode-dub-ru.png) | ![voiceover](docs/shots/mode-voiceover-es.png) | ![dub CJK](docs/shots/mode-dub-zh.png) |
|:--:|:--:|:--:|
| 🎙️ **配音** · EN→RU | 🗣️ **旁白配音** · EN→ES | 🈶 **配音** · 画面上的中文 |
| ![subtitles](docs/shots/mode-subtitles-ru.png) | ![widescreen](docs/shots/mode-dub-cinema-fr.png) | ![transcript](docs/shots/mode-transcribe-pt.png) |
| 📝 **字幕** · 原语言 | 🎬 **配音** · 宽屏 16:9 | 🔤 **转录** · 说话人分离 |

## 这是什么

**Dub Studio** 把任意视频变成另一种语言的配音版本 —— **克隆说话人本人的音色、翻译字幕、并在画面上就地本地化嵌入文字**。拖入一个片段，智能自动流程先出初稿；随后实时编辑器让你掌控**每一条字幕、声音、模糊框、字体和标题**，即时预览。

默认一切都在**你自己的电脑上本地运行** —— 无云端、无订阅：你的素材和声纹绝不离开电脑。如果电脑较弱（跑不动本地 Gemma/Higgs）或你想要更快更好的效果，繁重的部分（翻译、视觉、TTS、语音识别）可以**可选地**通过 **OpenRouter** 交给云端 —— 每个引擎单独选择（本地 ↔ 云端），并按说话人性别自动分配声音（测试版）。密钥保存在本地，默认全部关闭。

这是**完全原生重写**。没有内嵌 Python、没有 torch、没有 CUDA wheel。整条流水线是 **Rust + 原生 C++/CUDA 引擎（GGUF/ONNX）**：单进程、启动快、显存占用低。模型、引擎、CUDA/VC++ 运行库和 ffmpeg 都由应用在首次运行时**自行一键下载安装**。**本应用是为 NVIDIA 显卡构建并测试的**：配音、翻译和视觉在本地通过 CUDA 运行，字幕通过 NVENC 烧录进视频。分离、说话人分离和识别可以在 CPU 上运行，繁重的阶段也可以交给 OpenRouter，但没有 NVIDIA 的机器不属于经过测试的配置——见下文*各阶段运行在哪里*。

## 面向 AI 智能体

Dub Studio 打开时会在 `http://127.0.0.1:8793/mcp` 提供 MCP 服务：Claude Code、Claude Desktop、Cursor、Codex 等智能体可以通过同一套代码完成窗口能做的一切——用视频文件创建项目、进行分析、逐行修改译文、时间轴和说话人、分配配音、设置字幕样式、添加标题和模糊区域、渲染、把同一视频导出为更多语言、写出 SRT 和 TXT，并把结果保存到文件夹。在设置的 **智能体 (MCP)** 部分可以看到智能体是否已连接，以及要粘贴到客户端的内容。

拿到这个仓库后，智能体可以自行完成安装并操控工作室：

1. 从[最新版本](https://github.com/timoncool/dub-studio/releases/latest)安装工作室并启动。
2. 连接到它的 MCP 服务：
   ```bash
   claude mcp add --transport http dub-studio http://127.0.0.1:8793/mcp
   ```
   其他客户端：`{ "mcpServers": { "dub-studio": { "type": "streamable-http", "url": "http://127.0.0.1:8793/mcp" } } }`
3. 阅读服务器提供的技能说明（资源 `studio://skill`，提示词 `studio`），内容与 [docs/mcp-skill.md](docs/mcp-skill.md) 相同——包含所有工具、基本规则和分步操作——然后从工具 `studio_status` 开始。

[llms.txt](llms.txt) 为会查找它的工具提供同样的信息。若想在 Claude Code 中长期保留该技能，请把 [docs/mcp-skill.md](docs/mcp-skill.md) 保存为 `~/.claude/skills/dub-studio/SKILL.md`。只有本机上的智能体和工作室自己的窗口可以连接。

## 五种模式，随时切换

| 模式 | 作用 |
|------|------|
| 🎙️ **配音** | 完整重新配音到目标语言，**克隆原始音色** —— 按说话人自动分配或自选声音 |
| 🗣️ **旁白（画外音）** | 翻译人声**叠加在减弱的原声之上** —— 原声仍在下方可听，平衡可调 |
| 📝 **字幕** | 烧录**原语言**字幕、保留原声 —— 不配音、不翻译 |
| ✨ **趣味改编** | 给个主题（“像海盗”“像新闻播报”）→ 模型**重写整个脚本**再配音 |
| 🎬 **转录** | 干净的**说话人分离转录**、逐说话人排布、卡拉OK跟随播放、一键生成声音、导出 `.srt`/`.txt` |

加载一次片段，即可在编辑器里送入任意模式。

## 功能

- **声音克隆** —— 克隆原始音色并说出新语言（原生 [Higgs Audio v3](https://huggingface.co/bosonai) 引擎，GGUF）。按说话人自动分配或使用自带声音包。
- **说话人分离** —— 谁在何时说话（NVIDIA **Nemotron 3 Diarization**，最多 8 个声音），每个说话人不同声音。
- **角色选角（测试版）** —— 一个角色就是**「人脸 + 声音」的配对**。应用在整段视频中收集人脸、识别同一个人，并**按共同出现把他绑定到某个说话人**（近景出镜者获得声音，背景旁听者则否）；自动挑选最清晰的一帧作头像，并**为整部剧集保存选角档案** —— 声音和角色描述只需指定一次，**下一集自动套用**。**「真实人脸 / 卡通·动漫」**开关按内容切换人脸识别。
- **可选 ASR 引擎** —— 用 **Parakeet-TDT**（GPU，默认）或 **Whisper**（[Purfview faster-whisper 独立版](https://github.com/Purfview/whisper-standalone-win)，可在 CPU 上运行）转写 —— 在设置里直接选择模型大小（tiny … large-v3-turbo）和量化（compute type）。
- **导入现成字幕** —— 用你自己的 `.srt`/`.ass` 作为精确文稿：文本和时间轴直接取自文件，而非自动识别（说话人仍由声纹分离自动分配）。勾选 **“字幕已是目标语言”** 可连翻译一起跳过 —— 英文视频 + 你的俄语字幕 → 直接生成俄语配音，无需识别与翻译。
- **多语言导出** —— 导出按钮旁的 **▾** 可把一个视频一次导出为多种语言；每种都继承你的全部编辑（字幕排版、样式、模糊框、克隆音色）—— 只重新翻译与配音。
- **保存与重开项目** —— 自动保存、启动页的近期项目列表，一键回到未完成的工作。
- **语音与语言列表可搜索** —— 输入名称的一部分即可从数百个语音或 100+ 种语言中筛选；语言也可按界面语言中的名称匹配。
- **可组合流水线** —— 输入端独立开关：音频（原声 / 配音 / 旁白 / 转写）× 字幕（无 / 原文 / 翻译）× 是否烧录到视频 × 搞笑改写。任意组合 —— 配音但不加字幕、翻译字幕但不配音、用自己的声音做搞笑配音 —— 批量和编辑器中同样适用。
- **画面文字本地化** —— OCR 检测嵌入文字（**PP-OCR** ONNX），**模糊原文**并以匹配风格叠印本地化标题 —— 其他工具没有的功能。
- **翻译 + 视觉风格分析** —— 通过 **Gemma-4 12B**（GGUF，llama.cpp）本地翻译整段转录；视觉流程解析画面排布：字幕风格、标题、品牌、文字区域。
- **SOTA 人声分离** —— **Mel-Band Roformer**（CUDA 上的原生 BSRoformer.cpp）将人声与音乐分离：背景音乐**得以保留**，克隆锁定干净语音。
- **26 种字幕预设** —— karaoke / 逐词 / hormozi / 霓虹等，直接**在你的画面上**渲染（所见即所得，JASSUB 覆盖同一份 ffmpeg 烧录的 `.ass`）。
- **卡拉OK转录** —— 播放视频，转录中当前行与当前**词**同步高亮。
- **实时编辑器** —— 编辑转录、声音、字幕风格、模糊框、标题；**约 0.17 秒/帧预览**，每次修改即时可见。
- **智能重生成** —— 导出时只重新合成你改动过的片段，而非整段。
- **自定义台词** —— 在转录中插入自己的短语；每句都用说话人的克隆声音配音并显示在字幕中。
- **批量处理** —— 文件队列，统一设置，逐文件进度。
- **前后对比** —— 原片与配音并排。
- **100+ 种语言** —— 配音到任何主要语言（西班牙语、中文、日语、阿拉伯语、印地语等），自动检测源语言。
- **任意视频格式** —— MP4、MOV、MKV、WEBM、AVI 等（ffmpeg 解码）。
- **一键安装 + 应用内自动更新** —— 首次运行下载模型、引擎、运行库与 ffmpeg；应用自我更新。
- **可续传下载** —— 大模型（10GB+）断线后从中断处续传，而非重新开始。
- **想在哪算就在哪算** —— 分离、说话人分离和识别各自在 **GPU 与 CPU** 之间独立切换，识别、翻译、视觉和配音还可以交给 **OpenRouter**。下方的*各阶段运行在哪里*表列出了每个阶段实际能用什么。
- **按硬件调优** —— 每个引擎都有多种量化（TTS Q8/Q6/Q4、翻译 Q4…Q8、ASR int8/fp32 或 Whisper tiny…large-v3-turbo、分离 Q8/Q5/Q4），在设置中切换；可限制 prefill 批大小与参考片段时长，以适配 8–12 GB 显卡和 32 GB 内存。
- **整个应用使用你的语言** — 进度、错误、安装、选角以及智能体读取的报告都以窗口的六种语言之一显示。
- **Google Gemini 声音** — 在本地引擎和 OpenRouter 之外可选 Gemini TTS，有独立密钥和实时模型列表；**Batch** 价格减半，停止的渲染会接上同一个已付费批次。
- **说话人数量** — 自动或 1 到 8 人；长录音中的声音统一匹配，录音按约 90 秒的窗口识别。
- **响度均衡** — 默认开启的开关：各句电平一致，配音为 -14 LUFS、-1 dBTP 上限；关闭则混音保持原样。
- **完全便携** —— 不写入用户配置；删除文件夹不留痕迹。

## 截图

主界面 —— 五种模式、所选视频预览、语言选择、任意视频格式：

![Dub Studio 主界面](docs/screenshot-home.png)

转录模式 —— 说话人分离转录、逐说话人排布、卡拉OK跟随播放、一键从每个说话人生成声音：

![Dub Studio 转录模式](docs/screenshot-transcribe.png)

## 环境要求

- **系统：** Windows 10 / 11 (x64)；Linux x86-64 为实验性版本（见 *Linux（实验性）*）
- **显卡：** 显存 8 GB 及以上的 NVIDIA 显卡（提供 8、12、16、24 和 32 GB 的预设）及较新的驱动。本地配音（Higgs Audio）、翻译和视觉（Gemma）通过 CUDA 运行，字幕通过 NVENC 烧录。没有 NVIDIA 时，只有*各阶段运行在哪里*中标为 CPU 或云端的阶段可以运行，且该配置未经测试
- **WebView2** —— Windows 11 已预装；在 Windows 10 上由安装程序下载（若失败见*故障排除*）
- **磁盘：** 默认模型、引擎和运行库约 15 GB（首次运行时获取），另需项目空间；备用量化版本和 Whisper 模型为额外占用

在有 NVIDIA 的机器上，唯一需要手动安装的是较新的 **[NVIDIA 驱动](https://www.nvidia.com/Download/index.aspx)**。其余一切 —— 模型（Higgs Audio v3、Gemma-4 12B + vision、Parakeet-TDT、Nemotron 3 Diarization、Mel-Band Roformer）、引擎、CUDA 运行库与 ffmpeg —— 应用在首次运行时一键下载。

## 快速开始

1. 从 [Releases](https://github.com/timoncool/dub-studio/releases) **下载**便携版并解压到任意文件夹（或用 `-setup.exe` / `.msi` 安装）。
2. **运行** `Dub Studio.exe`。
3. 在**首次运行**面板点击**全部下载** —— 应用获取模型、引擎与运行库（约 15 GB，一次）。若缺 NVIDIA 驱动，按钮会打开下载页。
4. **拖入视频**，选择目标语言 → 自动流程出初稿。在编辑器里微调后点击**导出**。

> 一切都下载并存放在**应用文件夹内**。模型、缓存与项目不会去别处。

改动内容与时间见 [CHANGELOG.md](CHANGELOG.md)，应用顶部的星光按钮也会显示。贡献者与编码代理的规则见 [AGENTS.md](AGENTS.md)。

## 应用会下载的全部内容

“首次运行”面板用一个按钮获取以上全部内容。若处于代理之后，或无法访问 Hugging Face，请在 设置 → **网络** 中设置代理：跟随 Windows、自定义（HTTP、HTTPS、SOCKS5 或 SOCKS4；卖家给的 `host:port:login:password` 可以直接使用）或不使用代理。模型下载、云端请求和应用更新都会经过它。也可以自行下载直接文件，放到最后一列所示的位置（路径从应用文件夹，即包含 `models\` 的文件夹算起），再点击**从文件夹导入**；以压缩包（`.zip`、`.whl`）提供的组件由应用自己下载。

位于所列位置、大小与所列大小完全一致的文件视为已安装；每次下载都会先与该文件固定的 SHA-256 核对后才使用，已经在位的文件在跳过下载前也会同样核对。**从文件夹导入**（在“首次运行”面板和模型设置中）会在你选择的文件夹中按文件名和精确大小查找组件的文件，并以硬链接或复制的方式放到位。以 zip 或 wheel 压缩包提供的组件（引擎、运行库）不会被导入，只有由应用自己下载时才算已安装：应用会在解压出的文件旁保存已核对压缩包的记录。大小和哈希取自应用自己的清单 `crates/dub-server/src/setup.rs`，本表会与其核对。

Visual C++ 运行库和 PP-OCR 模型随发行包提供，不会下载；NVIDIA 驱动需单独安装。

<!-- downloads:start -->
| 组件 | 必要性 | 文件（直接链接） | 大小 | 存放位置 |
|---|---|---|---|---|
| Higgs Audio v3 Q8_0 | 必需 | [q8_0.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/q8_0.gguf) → `models\higgs-q8_0\q8_0.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/config.json) → `models\higgs-q8_0\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/chat_template.jinja) → `models\higgs-q8_0\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer.json) → `models\higgs-q8_0\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer_config.json) → `models\higgs-q8_0\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q8_0\higgs_audio_v2_tokenizer_config.json` | 5.5 GB | 原样放入，路径见箭头后 |
| audiocpp_engine.dll (Higgs engine) | 必需 | [audiocpp_engine.dll](https://github.com/timoncool/Higgs-Ultimate/releases/download/engine-0.2.3-turing/audiocpp_engine.dll) → `models\higgs-engine\audiocpp_engine.dll` | 129 MB | 原样放入，路径见箭头后 |
| Gemma-4 12B QAT q4_0 + vision | 必需 | [gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\gemma-4-12b-it-qat-q4_0.gguf`<br>[mmproj-gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/mmproj-gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\mmproj-gemma-4-12b-it-qat-q4_0.gguf` | 7.2 GB | 原样放入，路径见箭头后 |
| Gemma-4 12B Q5_K_M + vision | 可选 | [gemma-4-12b-it-Q5_K_M.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q5_K_M.gguf) → `models\mt-q5_0\gemma-4-12b-it-Q5_K_M.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q5_0\mmproj-F16.gguf` | 8.6 GB | 原样放入，路径见箭头后 |
| Gemma-4 12B Q6_K + vision | 可选 | [gemma-4-12b-it-Q6_K.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q6_K.gguf) → `models\mt-q6_k\gemma-4-12b-it-Q6_K.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q6_k\mmproj-F16.gguf` | 10.0 GB | 原样放入，路径见箭头后 |
| Gemma-4 12B Q8_0 + vision | 可选 | [gemma-4-12b-it-Q8_0.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q8_0.gguf) → `models\mt-q8_0\gemma-4-12b-it-Q8_0.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q8_0\mmproj-F16.gguf` | 12.8 GB | 原样放入，路径见箭头后 |
| Parakeet-TDT 0.6B v3 Q8_0 (transcribe.cpp) | 必需 | [parakeet-tdt-0.6b-v3-Q8_0.gguf](https://huggingface.co/handy-computer/parakeet-tdt-0.6b-v3-gguf/resolve/d8d8c32cb2017c57e5307767343e97870f8a7f53/parakeet-tdt-0.6b-v3-Q8_0.gguf) → `models\asr-gguf-parakeet-v3\parakeet-tdt-0.6b-v3-Q8_0.gguf` | 740 MB | 原样放入，路径见箭头后 |
| transcribe.cpp engine (CUDA) | 必需 | [transcribe_engine.dll](https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.3/transcribe_engine.dll) → `models\transcribe-engine\transcribe_engine.dll` | 144 MB | 原样放入，路径见箭头后 |
| transcribe.cpp engine (CPU) | 可选 | [transcribe_engine-cpu.dll](https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.3/transcribe_engine-cpu.dll) → `models\transcribe-engine-cpu\transcribe_engine.dll` | 4 MB | 原样放入，路径见箭头后 |
| Higgs Audio v3 Q6_K | 可选 | [q6_k.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/q6_k.gguf) → `models\higgs-q6_k\q6_k.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/config.json) → `models\higgs-q6_k\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/chat_template.jinja) → `models\higgs-q6_k\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer.json) → `models\higgs-q6_k\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer_config.json) → `models\higgs-q6_k\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q6_k\higgs_audio_v2_tokenizer_config.json` | 5.0 GB | 原样放入，路径见箭头后 |
| Higgs Audio v3 Q4_K_M | 可选 | [q4_k_m.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/q4_k_m.gguf) → `models\higgs-q4_k_m\q4_k_m.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/config.json) → `models\higgs-q4_k_m\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/chat_template.jinja) → `models\higgs-q4_k_m\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer.json) → `models\higgs-q4_k_m\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer_config.json) → `models\higgs-q4_k_m\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q4_k_m\higgs_audio_v2_tokenizer_config.json` | 4.1 GB | 原样放入，路径见箭头后 |
| Fish Audio Drama 3 Q4 + drama_engine (24 GB+ GPU) | 可选 | [.gitattributes](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/.gitattributes) → `models\drama3\.gitattributes`<br>[LICENSE](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/LICENSE) → `models\drama3\LICENSE`<br>[NOTICE](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/NOTICE) → `models\drama3\NOTICE`<br>[README.md](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/README.md) → `models\drama3\README.md`<br>[codec.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/codec.safetensors) → `models\drama3\codec.safetensors`<br>[drama.json](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/drama.json) → `models\drama3\drama.json`<br>[globals.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/globals.safetensors) → `models\drama3\globals.safetensors`<br>[layer00.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer00.safetensors) → `models\drama3\layer00.safetensors`<br>[layer01.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer01.safetensors) → `models\drama3\layer01.safetensors`<br>[layer02.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer02.safetensors) → `models\drama3\layer02.safetensors`<br>[layer03.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer03.safetensors) → `models\drama3\layer03.safetensors`<br>[layer04.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer04.safetensors) → `models\drama3\layer04.safetensors`<br>[layer05.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer05.safetensors) → `models\drama3\layer05.safetensors`<br>[layer06.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer06.safetensors) → `models\drama3\layer06.safetensors`<br>[layer07.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer07.safetensors) → `models\drama3\layer07.safetensors`<br>[layer08.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer08.safetensors) → `models\drama3\layer08.safetensors`<br>[layer09.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer09.safetensors) → `models\drama3\layer09.safetensors`<br>[layer10.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer10.safetensors) → `models\drama3\layer10.safetensors`<br>[layer11.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer11.safetensors) → `models\drama3\layer11.safetensors`<br>[layer12.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer12.safetensors) → `models\drama3\layer12.safetensors`<br>[layer13.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer13.safetensors) → `models\drama3\layer13.safetensors`<br>[layer14.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer14.safetensors) → `models\drama3\layer14.safetensors`<br>[layer15.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer15.safetensors) → `models\drama3\layer15.safetensors`<br>[layer16.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer16.safetensors) → `models\drama3\layer16.safetensors`<br>[layer17.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer17.safetensors) → `models\drama3\layer17.safetensors`<br>[layer18.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer18.safetensors) → `models\drama3\layer18.safetensors`<br>[layer19.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer19.safetensors) → `models\drama3\layer19.safetensors`<br>[layer20.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer20.safetensors) → `models\drama3\layer20.safetensors`<br>[layer21.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer21.safetensors) → `models\drama3\layer21.safetensors`<br>[layer22.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer22.safetensors) → `models\drama3\layer22.safetensors`<br>[layer23.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer23.safetensors) → `models\drama3\layer23.safetensors`<br>[layer24.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer24.safetensors) → `models\drama3\layer24.safetensors`<br>[layer25.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer25.safetensors) → `models\drama3\layer25.safetensors`<br>[layer26.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer26.safetensors) → `models\drama3\layer26.safetensors`<br>[layer27.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer27.safetensors) → `models\drama3\layer27.safetensors`<br>[layer28.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer28.safetensors) → `models\drama3\layer28.safetensors`<br>[layer29.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer29.safetensors) → `models\drama3\layer29.safetensors`<br>[layer30.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer30.safetensors) → `models\drama3\layer30.safetensors`<br>[layer31.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer31.safetensors) → `models\drama3\layer31.safetensors`<br>[layer32.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer32.safetensors) → `models\drama3\layer32.safetensors`<br>[layer33.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer33.safetensors) → `models\drama3\layer33.safetensors`<br>[layer34.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer34.safetensors) → `models\drama3\layer34.safetensors`<br>[layer35.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer35.safetensors) → `models\drama3\layer35.safetensors`<br>[layer36.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer36.safetensors) → `models\drama3\layer36.safetensors`<br>[layer37.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer37.safetensors) → `models\drama3\layer37.safetensors`<br>[layer38.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer38.safetensors) → `models\drama3\layer38.safetensors`<br>[layer39.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer39.safetensors) → `models\drama3\layer39.safetensors`<br>[tokenizer.json](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/tokenizer.json) → `models\drama3\tokenizer.json`<br>[tower.safetensors](https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/tower.safetensors) → `models\drama3\tower.safetensors`<br>[drama_engine.dll](https://github.com/timoncool/drama-rs/releases/download/engine-0.1.3/drama_engine.dll) → `models\drama-engine\drama_engine.dll` | 23.0 GB | 原样放入，路径见箭头后 |
| qwen.dll + ggml (Qwen3-TTS engine) | 可选 | [qwen.dll](https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.1.0/qwen.dll) → `models\qwen3-tts\qwen.dll`<br>[ggml.dll](https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.1.0/ggml.dll) → `models\qwen3-tts\ggml.dll`<br>[ggml-base.dll](https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.1.0/ggml-base.dll) → `models\qwen3-tts\ggml-base.dll`<br>[ggml-cpu.dll](https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.1.0/ggml-cpu.dll) → `models\qwen3-tts\ggml-cpu.dll`<br>[ggml-cuda.dll](https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.1.0/ggml-cuda.dll) → `models\qwen3-tts\ggml-cuda.dll` | 143 MB | 原样放入，路径见箭头后 |
| Qwen3-TTS 1.7B Q4_K_M | 可选 | [qwen-talker-1.7b-customvoice-Q4_K_M.gguf](https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-talker-1.7b-customvoice-Q4_K_M.gguf) → `models\qwen3-tts\qwen-talker-1.7b-customvoice-Q4_K_M.gguf`<br>[qwen-talker-1.7b-base-Q4_K_M.gguf](https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-talker-1.7b-base-Q4_K_M.gguf) → `models\qwen3-tts\qwen-talker-1.7b-base-Q4_K_M.gguf` | 2.4 GB | 原样放入，路径见箭头后 |
| Qwen3-TTS 1.7B Q8_0 | 可选 | [qwen-talker-1.7b-customvoice-Q8_0.gguf](https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-talker-1.7b-customvoice-Q8_0.gguf) → `models\qwen3-tts\qwen-talker-1.7b-customvoice-Q8_0.gguf`<br>[qwen-talker-1.7b-base-Q8_0.gguf](https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-talker-1.7b-base-Q8_0.gguf) → `models\qwen3-tts\qwen-talker-1.7b-base-Q8_0.gguf` | 4.1 GB | 原样放入，路径见箭头后 |
| Qwen3-TTS Russian stress | 可选 | [adapter.safetensors](https://huggingface.co/siriusfreak/qwen3-tts-12hz-1.7b-ru-stress-cf/resolve/c16b3af3db3095120efc1ebb73f6522442bcb3b1/lora/adapter.safetensors) → `models\qwen3-tts\qwen-talker-1.7b-base-ru-stress-lora.safetensors` | 35 MB | 原样放入，路径见箭头后 |
| Supertonic 3 int8 | 可选 | [LICENSE](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/LICENSE) → `models\supertonic3\LICENSE`<br>[duration_predictor.onnx](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/duration_predictor.onnx) → `models\supertonic3\onnx\duration_predictor.onnx`<br>[text_encoder.onnx](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/text_encoder.onnx) → `models\supertonic3\onnx\text_encoder.onnx`<br>[vector_estimator.onnx](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/vector_estimator.onnx) → `models\supertonic3\onnx\vector_estimator.onnx`<br>[vocoder.onnx](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/vocoder.onnx) → `models\supertonic3\onnx\vocoder.onnx`<br>[tts.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/tts.json) → `models\supertonic3\onnx\tts.json`<br>[unicode_indexer.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/unicode_indexer.json) → `models\supertonic3\onnx\unicode_indexer.json`<br>[F1.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F1.json) → `models\supertonic3\voice_styles\F1.json`<br>[F2.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F2.json) → `models\supertonic3\voice_styles\F2.json`<br>[F3.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F3.json) → `models\supertonic3\voice_styles\F3.json`<br>[F4.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F4.json) → `models\supertonic3\voice_styles\F4.json`<br>[F5.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F5.json) → `models\supertonic3\voice_styles\F5.json`<br>[M1.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M1.json) → `models\supertonic3\voice_styles\M1.json`<br>[M2.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M2.json) → `models\supertonic3\voice_styles\M2.json`<br>[M3.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M3.json) → `models\supertonic3\voice_styles\M3.json`<br>[M4.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M4.json) → `models\supertonic3\voice_styles\M4.json`<br>[M5.json](https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M5.json) → `models\supertonic3\voice_styles\M5.json` | 108 MB | 原样放入，路径见箭头后 |
| RUAccent | 可选 | [model.onnx](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_accent/model.onnx) → `models\ruaccent\nn\nn_accent\model.onnx`<br>[config.json](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_accent/config.json) → `models\ruaccent\nn\nn_accent\config.json`<br>[vocab.txt](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_accent/vocab.txt) → `models\ruaccent\nn\nn_accent\vocab.txt`<br>[model.onnx](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_omograph/tiny2/model.onnx) → `models\ruaccent\nn\nn_omograph\tiny2\model.onnx`<br>[tokenizer.json](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_omograph/tiny2/tokenizer.json) → `models\ruaccent\nn\nn_omograph\tiny2\tokenizer.json`<br>[accents.json.gz](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/dictionary/accents.json.gz) → `models\ruaccent\dictionary\accents.json.gz`<br>[omographs.json.gz](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/dictionary/omographs.json.gz) → `models\ruaccent\dictionary\omographs.json.gz`<br>[yo_words.json.gz](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/dictionary/yo_words.json.gz) → `models\ruaccent\dictionary\yo_words.json.gz`<br>[yo_homographs.json.gz](https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/dictionary/yo_homographs.json.gz) → `models\ruaccent\dictionary\yo_homographs.json.gz` | 34 MB | 原样放入，路径见箭头后 |
| RUAccent omograph big_poetry int8 | 可选 | [model.onnx](https://huggingface.co/nerualdreming/ruaccent-omograph-int8/resolve/d5c4eb6864d55cfc24ae45530eb4ef96843efb2b/big_poetry/model.onnx) → `models\ruaccent\nn\nn_omograph\big_poetry\model.onnx`<br>[tokenizer.json](https://huggingface.co/nerualdreming/ruaccent-omograph-int8/resolve/d5c4eb6864d55cfc24ae45530eb4ef96843efb2b/big_poetry/tokenizer.json) → `models\ruaccent\nn\nn_omograph\big_poetry\tokenizer.json` | 182 MB | 原样放入，路径见箭头后 |
| Parakeet Ultra 0.6B Q8_0 (Moondream, transcribe.cpp) | 可选 | [parakeet-ultra-Q8_0.gguf](https://huggingface.co/handy-computer/parakeet-ultra-gguf/resolve/a1539be1b849ea7463b88343e5277b373d0c4ea9/parakeet-ultra-Q8_0.gguf) → `models\asr-gguf-parakeet-ultra\parakeet-ultra-Q8_0.gguf` | 740 MB | 原样放入，路径见箭头后 |
| CrisperWhisper 2.0 turbo (audio.cpp) | 可选 | [LICENSE.md](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/LICENSE.md) → `models\crisperwhisper2-turbo\LICENSE.md`<br>[README.md](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/README.md) → `models\crisperwhisper2-turbo\README.md`<br>[added_tokens.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/added_tokens.json) → `models\crisperwhisper2-turbo\added_tokens.json`<br>[config.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/config.json) → `models\crisperwhisper2-turbo\config.json`<br>[generation_config.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/generation_config.json) → `models\crisperwhisper2-turbo\generation_config.json`<br>[merges.txt](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/merges.txt) → `models\crisperwhisper2-turbo\merges.txt`<br>[model.safetensors](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/model.safetensors) → `models\crisperwhisper2-turbo\model.safetensors`<br>[normalizer.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/normalizer.json) → `models\crisperwhisper2-turbo\normalizer.json`<br>[preprocessor_config.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/preprocessor_config.json) → `models\crisperwhisper2-turbo\preprocessor_config.json`<br>[special_tokens_map.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/special_tokens_map.json) → `models\crisperwhisper2-turbo\special_tokens_map.json`<br>[tokenizer.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/tokenizer.json) → `models\crisperwhisper2-turbo\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/tokenizer_config.json) → `models\crisperwhisper2-turbo\tokenizer_config.json`<br>[vocab.json](https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/vocab.json) → `models\crisperwhisper2-turbo\vocab.json` | 1.6 GB | 原样放入，路径见箭头后 |
| VoxLingua107 language id (transcribe.cpp) | 可选 | [lang-id-voxlingua107-ecapa-Q8_0.gguf](https://huggingface.co/handy-computer/lang-id-voxlingua107-ecapa-gguf/resolve/e59f0f3847e77723b7d58447ef9456fa0c7e9436/lang-id-voxlingua107-ecapa-Q8_0.gguf) → `models\langid-voxlingua107\lang-id-voxlingua107-ecapa-Q8_0.gguf` | 27 MB | 原样放入，路径见箭头后 |
| Nemotron 3 Diarization | 推荐 | [nemotron3_diar_v3.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/nemotron3_diar_v3.onnx) → `models\nemotron-diar\nemotron3_diar_v3.onnx`<br>[LICENSE](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/LICENSE) → `models\nemotron-diar\LICENSE` | 401 MB | 原样放入，路径见箭头后 |
| Mel-Band Roformer voc_fv6 Q8_0 | 推荐 | [voc_fv6-Q8_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q8_0.gguf) → `models\bsroformer\voc_fv6-Q8_0.gguf` | 252 MB | 原样放入，路径见箭头后 |
| Mel-Band Roformer voc_fv6 Q5_0 | 可选 | [voc_fv6-Q5_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q5_0.gguf) → `models\bsroformer\voc_fv6-Q5_0.gguf` | 167 MB | 原样放入，路径见箭头后 |
| Mel-Band Roformer voc_fv6 Q4_0 | 可选 | [voc_fv6-Q4_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q4_0.gguf) → `models\bsroformer\voc_fv6-Q4_0.gguf` | 139 MB | 原样放入，路径见箭头后 |
| 选角模型（人脸与声音） | 推荐 | [model.onnx](https://huggingface.co/immich-app/buffalo_l/resolve/d09715916a0778919a770c343533641e250b8699/detection/model.onnx) → `models\faces\det_10g.onnx`<br>[LVFace-L_Glint360K.onnx](https://huggingface.co/bytedance-research/LVFace/resolve/b12702ab1f5c721748e054a66dc90e1edd1f0724/LVFace-L_Glint360K/LVFace-L_Glint360K.onnx) → `models\faces\LVFace-L_Glint360K.onnx`<br>[model_feat.onnx](https://huggingface.co/deepghs/ccip_onnx/resolve/eb2acdd29af1703388d3d0c04221add322bc9110/ccip-caformer-24-randaug-pruned/model_feat.onnx) → `models\faces\ccip\model_feat.onnx`<br>[model.onnx](https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_s/model.onnx) → `models\faces\anime_face\model.onnx`<br>[xseg_1.onnx](https://huggingface.co/facefusion/models-3.1.0/resolve/c9e3a503d8e84e91c5cd89ee2d510fe5e793e570/xseg_1.onnx) → `models\faces\occluder\xseg_1.onnx`<br>[voxceleb_resnet34_LM.onnx](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/resolve/f0c48c298fd835726c27956a5d617bad7115627e/voxceleb_resnet34_LM.onnx) → `models\faces\wespeaker\voxceleb_resnet34_LM.onnx` | 1.3 GB | 原样放入，路径见箭头后 |
| BSRoformer.cpp (CUDA) | 推荐 | [BSRoformer-windows-cuda-13.1.0.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-cuda-13.1.0.zip) | 165 MB | 解压文件（不含子文件夹）到 `tools\bsroformer\` |
| BSRoformer.cpp (CPU) | 可选 | [BSRoformer-windows-x64-msvc.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-x64-msvc.zip) | 671 KB | 解压文件（不含子文件夹）到 `tools\bsroformer-cpu\` |
| llama.cpp server (CUDA) | 必需 | [llama-b11900-bin-win-cuda-13.4-x64.zip](https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-win-cuda-13.4-x64.zip) | 165 MB | 解压文件（不含子文件夹）到 `tools\llama\` |
| ONNX Runtime | 必需 | [onnxruntime-win-x64-1.31.0.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-win-x64-1.31.0.zip) | 83 MB | 保留文件夹结构解压到 `models\runtime\` |
| ONNX Runtime GPU (CUDA) | 推荐 | [onnxruntime-win-x64-gpu_cuda13-1.31.0.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-win-x64-gpu_cuda13-1.31.0.zip) | 310 MB | 保留文件夹结构解压到 `models\runtime\` |
| FFmpeg (static build) | 必需 | [ffmpeg-N-127021-ge0c94b2d1c-win64-gpl.zip](https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-09-30-13-08/ffmpeg-N-127021-ge0c94b2d1c-win64-gpl.zip) | 199 MB | 从压缩包中取出 ffmpeg.exe 和 ffprobe.exe 放入 `tools\ffmpeg\` |
| yt-dlp + deno | 可选 | [yt-dlp.exe](https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.21/yt-dlp.exe) → `tools\yt-dlp\yt-dlp.exe`<br>[deno-x86_64-pc-windows-msvc.zip](https://github.com/denoland/deno/releases/download/v2.9.8/deno-x86_64-pc-windows-msvc.zip) | 61 MB | 原样放入，路径见箭头后<br>解压文件（不含子文件夹）到 `tools\yt-dlp\` |
| CUDA runtime (cudart, cuBLAS, cuFFT) | 必需 | [nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/86/00/d5436004268f049214193659ebc36550b5ef3925c3d13b4cc980e13be6f5/nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl)<br>[nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/a3/df/f1246959833e2c437db8be3e5b477f66b87f8817821ed40de6c7561c9a36/nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl)<br>[libcufft-windows-x86_64-12.4.0.43-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcufft/windows-x86_64/libcufft-windows-x86_64-12.4.0.43-archive.zip) | 586 MB | 取出压缩包中所有 .dll 放入 `models\higgs-engine\` |
| cuDNN 9 | 推荐 | [nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl](https://files.pythonhosted.org/packages/87/6a/e55ff0ac26a5c6e2b21f41c9d04ad096b4ed6da593fba7e25845c61b0532/nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl) | 436 MB | 取出压缩包中所有 .dll 放入 `models\higgs-engine\` |
<!-- downloads:end -->

## 各阶段运行在哪里

每个阶段都有各自的设备开关。*是*表示代码中存在该路径；本表不说明速度或测试覆盖情况，CPU 路径较慢。云端路径需要 OpenRouter 密钥（设置），默认关闭。

| 阶段 | 引擎 | NVIDIA 显卡 | CPU | OpenRouter（云端） |
|---|---|---|---|---|
| 分离 | Mel-Band Roformer（BSRoformer.cpp） | 是（CUDA 版） | 是（单独的 CPU 版，较慢） | 否 |
| 说话人分离 | Nemotron 3 Diarization | 是（ONNX Runtime CUDA） | 是 | 否 |
| 语音识别 | Parakeet-TDT 或 Whisper-Faster | 是 | 是 | 是 |
| 翻译与视觉 | Gemma-4 12B（llama.cpp） | 是（CUDA 版） | 否 | 是 |
| 配音与声音克隆 | Higgs Audio v3 | 是（CUDA） | 否 | 是 |
| 画面文字 | PP-OCR | 否 | 是 | 否 |
| 选角（人脸与声音） | SCRFD、LVFace、anime_face、CCIP、WeSpeaker | 否 | 是 | 否 |
| 把字幕烧录进视频 | ffmpeg | 是（NVENC） | 否 | 否 |

没有 NVIDIA 时，配音、翻译、视觉和识别可以交给云端，分离和说话人分离可以用 CPU，但烧录没有 CPU 路径，因此这样的机器不属于经过测试的配置。

## 故障排除

**安装程序停在 WebView2。** 应用窗口运行在 Microsoft Edge WebView2 上，Windows 缺少它时由安装程序下载。连接被屏蔽或不稳定，或在拒绝微软小型引导程序的 Windows 10 版本上（错误 0x80040902），下载会失败。请使用微软的离线安装包 [Evergreen Standalone x64](https://go.microsoft.com/fwlink/p/?LinkId=2124701) 安装 WebView2，然后再次运行 Dub Studio 的安装程序。

**下载卡住或失败。** 大文件会从中断处继续，所以再按一次按钮即可。如果 Hugging Face 或 GitHub 对你被屏蔽，请在 设置 → **网络** 中设置代理，或从表中手动下载直接文件，放到最后一列所示的位置，然后点击**从文件夹导入**。

**导出因 `Unrecognized option 'filter_complex_script'` 而中止。** 这是 ffmpeg 8 及更新版本上的故障，已在 3.1.1 中修复：请更新应用。应用使用 `tools\ffmpeg` 中的 ffmpeg，没有时接受 `PATH` 中找到的那个。报告导出错误时，请附上日志中完整的 ffmpeg 输出。

**识别因 `MemcpyToHost` 或 `Failed to allocate memory` 失败。** 处理较长或较大的文件时，语音识别用尽了显存（issue #4）。请关闭其他占用 GPU 的程序，或在设置中把识别阶段改为 **CPU**，然后重试。

**`CUDA execution provider is not enabled`，或所有台词都归到同一个说话人。** GPU 库缺失，或对当前驱动来说太旧。请安装最新的 NVIDIA 驱动，在模型设置中点击 CUDA 运行库、cuDNN 和 ONNX Runtime GPU 的下载按钮，或把该阶段切换到 CPU。

## 工作原理

`analyze()` 是固定的第一遍：分离 → 带词级时间戳的 ASR → 说话人分离 → 上下文翻译 + 视觉（字幕风格 / 标题 / 品牌）→ OCR（排布 / 模糊框）。产出一个可编辑的 **Project** 文档。每次编辑都是对它的补丁，约 0.17 秒/帧预览；导出只重跑**被弄脏的阶段**。

**技术栈：** 原生 **Tauri 2（Rust）** 外壳在同一进程内于 `127.0.0.1:8793` 启动 `dub-server`（axum；面向智能体的 MCP 为同一端口上的 `/mcp`），并把窗口打开到 SPA —— React 19 + Vite + Tailwind + react-konva 覆盖 JASSUB。引擎：Parakeet-TDT 或 Whisper（ASR）· Nemotron 3 Diarization（说话人分离）· Gemma-4-12B GGUF（翻译 + 视觉，llama.cpp）· Higgs Audio v3（TTS）· Mel-Band Roformer（人声分离，BSRoformer.cpp）· PP-OCR（ONNX）· ffmpeg/NVENC。**运行时没有任何 Python 进程。**

### 从源码构建

```bash
git clone https://github.com/timoncool/dub-studio.git
cd dub-studio

cd frontend && npm install && npm run build && cd ..   # 1) SPA
cargo build --release -p dub-server                     # 2) 原生服务器 (axum)
cd desktop && npm install && npx tauri build            # 3) 桌面外壳 (Tauri)
```

需要 Node 20+、Rust（MSVC 工具链）与 WebView2。原生引擎无需重建 —— 应用会下载预编译二进制。

### Linux（实验性）

Linux x86-64 的 .deb 和 AppImage 是**实验性**的。它们出自同一份代码，配的是同一批引擎的 Linux 版本（llama.cpp、ONNX Runtime、BSRoformer.cpp、ffmpeg、Linux 版 Higgs 引擎、yt-dlp、faster-whisper），但作者在 Windows 上工作，没有在真实的 Linux 桌面上跑过。**如果你日常用 Linux，非常欢迎你把它们打磨好，再通过 pull request 把修复合回来。**

- 本地配音需要 Turing 及更新的 NVIDIA 显卡（GTX 16、RTX 20 及以上），与 Windows 相同。云端声音在任何机器上都能用。
- NVIDIA 驱动（580 或更新）、`libgomp1` 和 `libssl3` 由系统提供；模型、引擎和 CUDA 库由应用在首次运行时下载到 `~/.local/share/dub-studio`（`$XDG_DATA_HOME`）。
- 手动构建：`Linux build (experimental)` workflow（`.github/workflows/release-linux.yml`）或 `scripts/build-release-linux.sh <含 models/ocr 的文件夹>`。

## 匿名统计与新闻

应用在启动时向作者的服务器获取新闻，之后每小时一次。请求不带任何 id，所以无论下面如何选择，新闻都会到达：新条目显示在“新功能”顶部或窗口顶部的横条中，离线时应用显示其版本自带的新闻。

首次启动界面有一个复选框 **发送匿名使用统计**，默认勾选。同样的开关在 设置 → 匿名统计 中，旁边是 **发送的内容**（今天的确切报告）和 **新的安装 ID**。勾选期间，应用在启动一分钟后、每六小时、工作结束两分钟后以及关闭时发送当天的报告（同一天较晚的报告会替换较早的）：

- 在本机生成的随机安装 id，不与硬件或账户绑定；取消勾选即删除它；
- 应用及其版本、操作系统名称和版本、界面语言；
- 显卡：厂商、显存档位（8 及以下、12、16、24+ GB）以及 CUDA 是否可用；
- 当天有多少任务（分析、配音、渲染、导出、下载等）完成、失败或被取消。
- 各阶段（配音、识别、翻译、画面分析、人声分离）使用的模型及其任务数；
- 任务失败原因，一行文字，路径、名称、链接和引号内内容在发送前于本机删除；服务器保留 30 天。

绝不发送：视频、转录、翻译、声音、文件名或路径，以及任何个人信息。服务器保存 Cloudflare 报告的连接所在国家，而不是 IP 地址。环境变量 `DO_NOT_TRACK=1` 或 `STUDIO_TELEMETRY=0` 会完全关闭统计：不会生成 id，也不计数任何内容。

## 参与贡献与分支

**非常欢迎协作者。** 我会由衷高兴看到 Dub Studio 被移植到其他平台和显卡上 —— 架构完全支持，我只是没有精力亲自做这些移植。如果你想让它跑在 **AMD / Intel 显卡、macOS 或 Linux** 上，尽管 fork —— 欢迎 PR。Linux 已经有实验性版本（见 *Linux（实验性）*）：把它打磨好是最受欢迎的帮助。

**额外的本地化**同样欢迎：目前应用和落地页支持 6 种语言 —— 翻译语言文件（`frontend/src/locales/` 及 `docs/index.html` 中的字典）并提交 PR 加入你的语言。

## 作者

- **Nerual Dreming** —— [Telegram](https://t.me/nerual_dreming) | [neuro-cartel.com](https://neuro-cartel.com) | [ArtGeneration.me](https://artgeneration.me) 创始人
- **Neuro-Soft** —— [Telegram](https://t.me/neuroport) | 便携 AI 应用

## 致谢

- **[Boson AI](https://huggingface.co/bosonai)** —— Higgs Audio v3 模型；**[drbaph / Higgs-Audio-v3-Studio](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio)** —— GGUF 量化与原生 `audiocpp_engine.dll`。
- **[NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)**（CC-BY-4.0）—— ASR；ONNX 权重来自 [istupakov/parakeet-tdt-0.6b-v3-onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx)，运行时 [altunenes/parakeet-rs](https://github.com/altunenes/parakeet-rs)。
- **[Parakeet Ultra](https://huggingface.co/moondream/parakeet-ultra)**，由 **[Moondream](https://huggingface.co/moondream)** 基于 NVIDIA 的 parakeet-tdt-0.6b-v3 微调（CC-BY-4.0）—— 可选的微调 ASR，识别错误更少；ONNX 导出来自 [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs)。 int8: [Masterx/parakeet-tdt-0.6b-ultra-onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx).
- **[NVIDIA Nemotron 3 Diarization](https://huggingface.co/nvidia/Nemotron-3-Diarization)**（Streaming Sortformer v3，[OpenMDW-1.1](https://openmdw.ai/license/1-1/)）—— 说话人分离，最多 8 个说话人；ONNX 导出来自 [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs)。
- **[Google Gemma](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf)** —— Gemma-4 12B（翻译与视觉）、[unsloth](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF) 的量化版本，以及运行它们的 [llama.cpp](https://github.com/ggml-org/llama.cpp)。
- **[chenmozhijin / BSRoformer.cpp](https://github.com/chenmozhijin/BSRoformer.cpp)** 与 **[GaboxR67](https://huggingface.co/GaboxR67)** —— 原生分离引擎及其 GGUF 模型，以及 Mel-Band Roformer 检查点。
- **[Systran / faster-whisper](https://github.com/SYSTRAN/faster-whisper)**、**[deepdml](https://huggingface.co/deepdml)** 与 **[Purfview](https://github.com/Purfview/whisper-standalone-win)** —— CTranslate2 格式的 Whisper 模型及运行它们的独立版本。
- **[InsightFace](https://github.com/deepinsight/insightface)**（经由 [immich-app/buffalo_l](https://huggingface.co/immich-app/buffalo_l)）、**[ByteDance LVFace](https://huggingface.co/bytedance-research/LVFace)**、**[deepghs](https://huggingface.co/deepghs)**（CCIP、动漫人脸检测）、**[FaceFusion](https://huggingface.co/facefusion/models-3.1.0)** 与 **[WeSpeaker](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM)** —— 角色选角所用的人脸与声音模型。
- **[PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)** —— PP-OCR，画面文字检测与识别模型。
- **[ONNX Runtime](https://github.com/microsoft/onnxruntime)**、**[FFmpeg](https://ffmpeg.org)** 及 [BtbN 构建](https://github.com/BtbN/FFmpeg-Builds)、**[JASSUB](https://github.com/ThaUnknown/jassub)**、**[Tauri](https://tauri.app)** 与 **[ort](https://github.com/pykeio/ort)**。
- **NVIDIA CUDA runtime、cuBLAS、cuFFT 与 cuDNN** —— GPU 路径所依赖的库。
- **Serega (SilentBob)** —— 3.1.0 版本：multi-take、情绪参考、时间对齐与字幕编辑器。**[@nevoin](https://github.com/nevoin)** —— [#1](https://github.com/timoncool/dub-studio/issues/1) 中详细的日志，促成了 ffmpeg 8 的修复。**[LongNT2011](https://github.com/LongNT2011)** —— 其分叉中的 [PR #1](https://github.com/LongNT2011/dub-studio/pull/1)，修复了英文界面中硬编码的俄文文本，启发了 i18n 相关工作。

## 支持作者

我做开源软件与 AI 研究，绝大部分成果都公开。捐助让我能做和研究更多。

**[所有支持方式](DONATE.md)** | **[dalink.to/nerual_dreming](https://dalink.to/nerual_dreming)** | **[boosty.to/neuro_art](https://boosty.to/neuro_art)**

- **BTC:** `1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC`
- **ETH (ERC20):** `0xb5db65adf478983186d4897ba92fe2c25c594a0c`
- **USDT (TRC20):** `TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C`

## 许可证

应用代码采用 [MIT](LICENSE) 许可。**模型并非如此**：每个模型保留自己的许可证，下面列出的是各模型页面目前所写的内容。发布或出售配音视频之前请先阅读。

| 组件 | 许可证 | 含义 |
|---|---|---|
| Higgs Audio v3（Boson AI） | [Boson Higgs TTS 3 Research and Non-Commercial License](https://huggingface.co/bosonai/higgs-tts-3-4b/blob/main/LICENSE) | 可免费用于研究和个人用途；依据 Creator Use Grant，数字创作者发布并变现自己的内容时也可免费使用，但需注明 Boson AI 的 Higgs Audio。向他人托管、再分发，或将其嵌入面向第三方的产品或服务（包括配音服务），需要 Boson 的商业许可 |
| Parakeet-TDT 0.6B v3（NVIDIA）及其 ONNX 导出 | CC-BY-4.0 | 需注明 NVIDIA |
| Parakeet Ultra（Moondream，基于 NVIDIA Parakeet-TDT） | CC-BY-4.0 | 需注明 Moondream 与 NVIDIA |
| Nemotron 3 Diarization（NVIDIA） | [OpenMDW-1.1](https://openmdw.ai/license/1-1/) | 需注明 NVIDIA；ONNX 导出来自 altunenes/parakeet-rs |
| Gemma-4 12B（Google） | 模型页面为 Apache-2.0，并链接到 [Google 的 Gemma 4 条款](https://ai.google.dev/gemma/docs/gemma_4_license) | 请两者都阅读 |
| Mel-Band Roformer voc_fv6（GaboxR67），chenmozhijin 的 GGUF | 模型页面未注明许可证；引擎 BSRoformer.cpp 为 MIT | 商用前请询问作者 |
| Whisper 模型（Systran、deepdml）与 faster-whisper | MIT | Purfview 的独立版本在其仓库中没有许可证文件 |
| SCRFD 人脸检测器（InsightFace buffalo_l） | InsightFace：预训练模型**仅限非商业研究** | 对真实人脸的角色选角受此限制 |
| LVFace（ByteDance） | MIT |  |
| CCIP（deepghs） | OpenRAIL | 请阅读其使用限制 |
| anime_face_detection（deepghs） | MIT |  |
| xseg_1 遮挡模型（FaceFusion models） | 模型页面未注明许可证 | 商用前请询问作者 |
| WeSpeaker ResNet34-LM | CC-BY-4.0 | 需注明 WeSpeaker |
| PP-OCR（PaddleOCR） | Apache-2.0 |  |
| llama.cpp、ONNX Runtime、BSRoformer.cpp、JASSUB | MIT |  |
| FFmpeg（BtbN 构建） | FFmpeg 的 GPL 构建 | 作为独立程序提供，不链接进应用 |
| NVIDIA CUDA runtime、cuBLAS、cuFFT、cuDNN | NVIDIA 自己的许可条款 | 从 NVIDIA 和 PyPI 下载，不在仓库中 |
