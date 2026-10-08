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
- **完全便携** —— 不写入用户配置；删除文件夹不留痕迹。

## 截图

主界面 —— 五种模式、所选视频预览、语言选择、任意视频格式：

![Dub Studio 主界面](docs/screenshot-home.png)

转录模式 —— 说话人分离转录、逐说话人排布、卡拉OK跟随播放、一键从每个说话人生成声音：

![Dub Studio 转录模式](docs/screenshot-transcribe.png)

## 环境要求

- **系统：** Windows 10 / 11 (x64)
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
| audiocpp_engine.dll (Higgs engine) | 必需 | [audiocpp_engine.dll](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/engines/audiocpp_engine.dll) → `models\higgs-engine\audiocpp_engine.dll` | 72 MB | 原样放入，路径见箭头后 |
| Gemma-4 12B QAT q4_0 + vision | 必需 | [gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\gemma-4-12b-it-qat-q4_0.gguf`<br>[mmproj-gemma-4-12b-it-qat-q4_0.gguf](https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/mmproj-gemma-4-12b-it-qat-q4_0.gguf) → `models\mt\mmproj-gemma-4-12b-it-qat-q4_0.gguf` | 7.2 GB | 原样放入，路径见箭头后 |
| Gemma-4 12B Q5_K_M + vision | 可选 | [gemma-4-12b-it-Q5_K_M.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q5_K_M.gguf) → `models\mt-q5_0\gemma-4-12b-it-Q5_K_M.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q5_0\mmproj-F16.gguf` | 8.6 GB | 原样放入，路径见箭头后 |
| Gemma-4 12B Q6_K + vision | 可选 | [gemma-4-12b-it-Q6_K.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q6_K.gguf) → `models\mt-q6_k\gemma-4-12b-it-Q6_K.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q6_k\mmproj-F16.gguf` | 10.0 GB | 原样放入，路径见箭头后 |
| Gemma-4 12B Q8_0 + vision | 可选 | [gemma-4-12b-it-Q8_0.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q8_0.gguf) → `models\mt-q8_0\gemma-4-12b-it-Q8_0.gguf`<br>[mmproj-F16.gguf](https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf) → `models\mt-q8_0\mmproj-F16.gguf` | 12.8 GB | 原样放入，路径见箭头后 |
| Parakeet-TDT 0.6B v3 int8 | 必需 | [encoder-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx) → `models\tdt\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx) → `models\tdt\decoder_joint-model.int8.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt\config.json` | 671 MB | 原样放入，路径见箭头后 |
| Higgs Audio v3 Q6_K | 可选 | [q6_k.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/q6_k.gguf) → `models\higgs-q6_k\q6_k.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/config.json) → `models\higgs-q6_k\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/chat_template.jinja) → `models\higgs-q6_k\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer.json) → `models\higgs-q6_k\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer_config.json) → `models\higgs-q6_k\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q6_k\higgs_audio_v2_tokenizer_config.json` | 5.0 GB | 原样放入，路径见箭头后 |
| Higgs Audio v3 Q4_K_M | 可选 | [q4_k_m.gguf](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/q4_k_m.gguf) → `models\higgs-q4_k_m\q4_k_m.gguf`<br>[config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/config.json) → `models\higgs-q4_k_m\config.json`<br>[chat_template.jinja](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/chat_template.jinja) → `models\higgs-q4_k_m\chat_template.jinja`<br>[tokenizer.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer.json) → `models\higgs-q4_k_m\tokenizer.json`<br>[tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer_config.json) → `models\higgs-q4_k_m\tokenizer_config.json`<br>[higgs_audio_v2_tokenizer_config.json](https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/higgs_audio_v2_tokenizer_config.json) → `models\higgs-q4_k_m\higgs_audio_v2_tokenizer_config.json` | 4.1 GB | 原样放入，路径见箭头后 |
| Parakeet-TDT 0.6B v3 fp32 | 可选 | [encoder-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx) → `models\tdt-fp32\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx.data) → `models\tdt-fp32\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.onnx) → `models\tdt-fp32\decoder_joint-model.onnx`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-fp32\nemo128.onnx`<br>[vocab.txt](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt) → `models\tdt-fp32\vocab.txt`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-fp32\config.json` | 2.5 GB | 原样放入，路径见箭头后 |
| Parakeet Ultra 0.6B fp32 (Moondream) | 可选 | [encoder-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx) → `models\tdt-ultra\encoder-model.onnx`<br>[encoder-model.onnx.data](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx.data) → `models\tdt-ultra\encoder-model.onnx.data`<br>[decoder_joint-model.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/decoder_joint-model.onnx) → `models\tdt-ultra\decoder_joint-model.onnx`<br>[vocab.txt](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/vocab.txt) → `models\tdt-ultra\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/tdt/nemo128.onnx) → `models\tdt-ultra\nemo128.onnx`<br>[config.json](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json) → `models\tdt-ultra\config.json` | 2.6 GB | 原样放入，路径见箭头后 |
| Parakeet Ultra 0.6B int8 (Moondream) | 可选 | [encoder-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/encoder-model.int8.onnx) → `models\tdt-ultra-int8\encoder-model.int8.onnx`<br>[decoder_joint-model.int8.onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/decoder_joint-model.int8.onnx) → `models\tdt-ultra-int8\decoder_joint-model.int8.onnx`<br>[vocab.txt](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/vocab.txt) → `models\tdt-ultra-int8\vocab.txt`<br>[nemo128.onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx) → `models\tdt-ultra-int8\nemo128.onnx`<br>[config.json](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/config.json) → `models\tdt-ultra-int8\config.json` | 0.7 GB | 原样放入，路径见箭头后 |
| Whisper-Faster (faster-whisper standalone) | 可选 | [Whisper-Faster_r192.3_windows.zip](https://github.com/Purfview/whisper-standalone-win/releases/download/faster-whisper/Whisper-Faster_r192.3_windows.zip) | 88 MB | 解压文件（不含子文件夹）到 `tools\whisper\` |
| Whisper CUDA (cuBLAS 11, cuDNN 8) | 可选 | [libcublas-windows-x86_64-11.11.3.6-archive.zip](https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-11.11.3.6-archive.zip)<br>[cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip](https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip) | 1.1 GB | 取出压缩包中所有 .dll 放入 `tools\whisper\` |
| Whisper tiny | 可选 | [model.bin](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/model.bin) → `models\whisper\faster-whisper-tiny\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/config.json) → `models\whisper\faster-whisper-tiny\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/tokenizer.json) → `models\whisper\faster-whisper-tiny\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/vocabulary.txt) → `models\whisper\faster-whisper-tiny\vocabulary.txt` | 78 MB | 原样放入，路径见箭头后 |
| Whisper base | 可选 | [model.bin](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/model.bin) → `models\whisper\faster-whisper-base\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/config.json) → `models\whisper\faster-whisper-base\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/tokenizer.json) → `models\whisper\faster-whisper-base\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/vocabulary.txt) → `models\whisper\faster-whisper-base\vocabulary.txt` | 148 MB | 原样放入，路径见箭头后 |
| Whisper small | 可选 | [model.bin](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/model.bin) → `models\whisper\faster-whisper-small\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/config.json) → `models\whisper\faster-whisper-small\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/tokenizer.json) → `models\whisper\faster-whisper-small\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/vocabulary.txt) → `models\whisper\faster-whisper-small\vocabulary.txt` | 486 MB | 原样放入，路径见箭头后 |
| Whisper medium | 可选 | [model.bin](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/model.bin) → `models\whisper\faster-whisper-medium\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/config.json) → `models\whisper\faster-whisper-medium\config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/tokenizer.json) → `models\whisper\faster-whisper-medium\tokenizer.json`<br>[vocabulary.txt](https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/vocabulary.txt) → `models\whisper\faster-whisper-medium\vocabulary.txt` | 1.5 GB | 原样放入，路径见箭头后 |
| Whisper large-v3 | 可选 | [model.bin](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/model.bin) → `models\whisper\faster-whisper-large-v3\model.bin`<br>[config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/config.json) → `models\whisper\faster-whisper-large-v3\config.json`<br>[preprocessor_config.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/tokenizer.json) → `models\whisper\faster-whisper-large-v3\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/vocabulary.json) → `models\whisper\faster-whisper-large-v3\vocabulary.json` | 3.1 GB | 原样放入，路径见箭头后 |
| Whisper large-v3-turbo | 可选 | [model.bin](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/model.bin) → `models\whisper\faster-whisper-large-v3-turbo\model.bin`<br>[config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/config.json) → `models\whisper\faster-whisper-large-v3-turbo\config.json`<br>[preprocessor_config.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/preprocessor_config.json) → `models\whisper\faster-whisper-large-v3-turbo\preprocessor_config.json`<br>[tokenizer.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/tokenizer.json) → `models\whisper\faster-whisper-large-v3-turbo\tokenizer.json`<br>[vocabulary.json](https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/vocabulary.json) → `models\whisper\faster-whisper-large-v3-turbo\vocabulary.json` | 1.6 GB | 原样放入，路径见箭头后 |
| Nemotron 3 Diarization | 推荐 | [nemotron3_diar_v3.onnx](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/nemotron3_diar_v3.onnx) → `models\nemotron-diar\nemotron3_diar_v3.onnx`<br>[LICENSE](https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/LICENSE) → `models\nemotron-diar\LICENSE` | 401 MB | 原样放入，路径见箭头后 |
| Mel-Band Roformer voc_fv6 Q8_0 | 推荐 | [voc_fv6-Q8_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q8_0.gguf) → `models\bsroformer\voc_fv6-Q8_0.gguf` | 252 MB | 原样放入，路径见箭头后 |
| Mel-Band Roformer voc_fv6 Q5_0 | 可选 | [voc_fv6-Q5_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q5_0.gguf) → `models\bsroformer\voc_fv6-Q5_0.gguf` | 167 MB | 原样放入，路径见箭头后 |
| Mel-Band Roformer voc_fv6 Q4_0 | 可选 | [voc_fv6-Q4_0.gguf](https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q4_0.gguf) → `models\bsroformer\voc_fv6-Q4_0.gguf` | 139 MB | 原样放入，路径见箭头后 |
| 选角模型（人脸与声音） | 推荐 | [model.onnx](https://huggingface.co/immich-app/buffalo_l/resolve/d09715916a0778919a770c343533641e250b8699/detection/model.onnx) → `models\faces\det_10g.onnx`<br>[LVFace-L_Glint360K.onnx](https://huggingface.co/bytedance-research/LVFace/resolve/b12702ab1f5c721748e054a66dc90e1edd1f0724/LVFace-L_Glint360K/LVFace-L_Glint360K.onnx) → `models\faces\LVFace-L_Glint360K.onnx`<br>[model_feat.onnx](https://huggingface.co/deepghs/ccip_onnx/resolve/eb2acdd29af1703388d3d0c04221add322bc9110/ccip-caformer-24-randaug-pruned/model_feat.onnx) → `models\faces\ccip\model_feat.onnx`<br>[model.onnx](https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_s/model.onnx) → `models\faces\anime_face\model.onnx`<br>[xseg_1.onnx](https://huggingface.co/facefusion/models-3.1.0/resolve/c9e3a503d8e84e91c5cd89ee2d510fe5e793e570/xseg_1.onnx) → `models\faces\occluder\xseg_1.onnx`<br>[voxceleb_resnet34_LM.onnx](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/resolve/f0c48c298fd835726c27956a5d617bad7115627e/voxceleb_resnet34_LM.onnx) → `models\faces\wespeaker\voxceleb_resnet34_LM.onnx` | 1.3 GB | 原样放入，路径见箭头后 |
| BSRoformer.cpp (CUDA) | 推荐 | [BSRoformer-windows-cuda-13.1.0.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-cuda-13.1.0.zip) | 165 MB | 解压文件（不含子文件夹）到 `tools\bsroformer\` |
| BSRoformer.cpp (CPU) | 可选 | [BSRoformer-windows-x64-msvc.zip](https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-x64-msvc.zip) | 671 KB | 解压文件（不含子文件夹）到 `tools\bsroformer-cpu\` |
| llama.cpp server (CUDA) | 必需 | [llama-b11146-bin-win-cuda-13.4-x64.zip](https://github.com/ggml-org/llama.cpp/releases/download/b11146/llama-b11146-bin-win-cuda-13.4-x64.zip) | 150 MB | 解压文件（不含子文件夹）到 `tools\llama\` |
| ONNX Runtime | 必需 | [onnxruntime-win-x64-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-1.28.2.zip) | 79 MB | 保留文件夹结构解压到 `models\runtime\` |
| ONNX Runtime GPU (CUDA) | 推荐 | [onnxruntime-win-x64-gpu_cuda13-1.28.2.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-gpu_cuda13-1.28.2.zip) | 366 MB | 保留文件夹结构解压到 `models\runtime\` |
| FFmpeg (static build) | 必需 | [ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip](https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-31-13-27/ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip) | 171 MB | 从压缩包中取出 ffmpeg.exe 和 ffprobe.exe 放入 `tools\ffmpeg\` |
| yt-dlp + deno | 可选 | [yt-dlp.exe](https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.exe) → `tools\yt-dlp\yt-dlp.exe`<br>[deno-x86_64-pc-windows-msvc.zip](https://github.com/denoland/deno/releases/download/v2.9.7/deno-x86_64-pc-windows-msvc.zip) | 60 MB | 原样放入，路径见箭头后<br>解压文件（不含子文件夹）到 `tools\yt-dlp\` |
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

## 参与贡献与分支

**非常欢迎协作者。** 我会由衷高兴看到 Dub Studio 被移植到其他平台和显卡上 —— 架构完全支持，我只是没有精力亲自做这些移植。如果你想让它跑在 **AMD / Intel 显卡、macOS 或 Linux** 上，尽管 fork —— 欢迎 PR。

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
