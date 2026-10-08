asr-windowed = 在 GPU 上分窗转写（{ $speakers } 位说话人）

## Separation, OCR, benchmark, casting library, cloud ASR and TTS

atomic-extract-separation-audio = 正在提取 44.1k 音频用于分离
atomic-separating = 正在分离（{ $model }）
atomic-separation-failed = 分离：{ $error }
ocr-detecting-burned-text = 正在检测内嵌文字（{ $model }）
bench-stage = ⏱ { $name }：{ $seconds } 秒 | GPU ~{ $gpu_avg }%（峰值 { $gpu_max }%）| CPU ~{ $cpu_avg }% | 显存 { $vram } MB | 瓶颈：{ $bound }
bench-total = ⏱ { $label } 合计：{ $seconds } 秒
casting-no-casting-json = 项目中没有 casting.json（尚未运行选角）
casting-profile-delete-failed = 删除档案 { $slug }：{ $error }
casting-profile-not-found = 未找到档案“{ $slug }”
cloud-asr-no-key = 已启用云端 ASR，但未设置 OpenRouter 密钥
cloud-asr-no-model = 设置中未选择 OpenRouter 语音识别模型
cloud-asr-failed = 云端 ASR：{ $error }
cloud-asr-empty = 云端语音识别返回了空的转写
cloud-tts-no-key = 已启用云端 TTS，但未设置 OpenRouter 密钥
cloud-tts-no-model = 设置中未选择 TTS 模型（云端模型 · OpenRouter）
cloud-tts-no-voice = 设置中未指定 TTS 音色（每个模型有各自的音色）
cloud-tts-failed = 云端 TTS：{ $error }
cloud-tts-too-short = 云端 TTS：音频过短（{ $bytes } 字节）
cloud-tts-read-wav = 读取云端 wav：{ $error }

## Shared messages, compositing, downloads, dub timing, endpoints, preview frame

common-read = 读取 { $path }：{ $error }
common-parse = 解析 { $path }：{ $error }
common-ffmpeg-start = 启动 ffmpeg：{ $error }
compose-summary = 合成：标题={ $titles }（bbox），模糊 { $boxes } 个区域，sub_px={ $sub_px }
compose-taglines-no-mt = 标语：机器翻译不可用（{ $error }）-> 仅模糊
compose-taglines-translating = 标语：正在翻译标题卡文字
compose-taglines-failed = 标语：翻译失败；仅模糊
downloads-interrupted = 下载随应用关闭而中断；已下载部分保存在 .part 中，将从断点继续
downloads-nothing-to-download = 所选 id 中没有可下载的组件
downloads-busy = 已有下载正在进行
downloads-thread-failed = 下载线程未能启动：{ $error }
timing-words-not-recognized = { $count } 句配音的逐词时间未能识别（{ $examples }）——这些句子按长度高亮单词
openrouter-catalog-failed = OpenRouter 目录：{ $error }
openrouter-empty-key = 密钥为空
llm-server-no-address = 未设置服务器地址
llm-server-no-answer = 服务器 { $base } 无响应：{ $reason }
llm-server-status = { $endpoint } 返回 { $status }
llm-server-not-json = { $endpoint } 返回的不是 JSON：{ $error }
llm-server-not-model-list = { $endpoint } 返回的不是 OpenAI 模型列表（缺少 data 字段）
remix-start = 正在改写 { $count } 行 → { $instruction }
remix-no-llm = 改写：LLM 不可用：{ $error }
remix-lines-changed = 改写：改写期间台词发生了变化；项目未修改，请重新运行改写
frame-empty-ass = 空 ASS：{ $error }
frame-read-preview = 读取预览帧：{ $error }
frame-read-source = 读取原始帧：{ $error }

## Glossary, hardware, job records, LLM providers

glossary-bad-format = format“{ $format }”：应为 json 或 tsv
glossary-series-unreadable = 无法读取剧集词汇表：{ $error }
glossary-no-text = 项目中没有文本：词汇表由识别出的语音生成，请先分析
glossary-lines = 词汇表：{ $count } 行文本
glossary-no-llm = 词汇表：LLM 不可用：{ $error }
glossary-failed = 词汇表：{ $error }
glossary-proposed = 词汇表：建议了 { $count } 条
glossary-series-profile-missing = 未找到剧集档案“{ $slug }”；未应用其词汇表
glossary-series-slug-unreadable = 无法读取剧集“{ $slug }”的词汇表：{ $error }
glossary-series-applied = 剧集词汇表“{ $slug }”：{ $count } 条，已向项目添加 { $added } 条
hw-no-nvidia = 没有 NVIDIA GPU
jobs-serialize-record = 序列化 job.json：{ $error }
jobs-record-missing = { $dir } 中没有 { $file }
llm-llama-server-missing = 未找到 llama-server（{ $path }）
llm-gemma-missing = 未找到 Gemma 的 GGUF（{ $path }）
llm-mmproj-missing = 未找到 Gemma 视觉投影器（mmproj）（{ $path }）
llm-chat-client = 聊天客户端：{ $error }
llm-local-no-text-model = 已选择本地服务器（{ $url }）用于翻译，但未选择模型
llm-local-no-vision-model = 已选择本地服务器（{ $url }）用于视觉，但未选择模型
llm-local-client = 本地服务器客户端：{ $error }
llm-local-label = 本地服务器 { $url } · { $model }
llm-openrouter-no-key = 已选择 OpenRouter，但未设置密钥
llm-openrouter-no-text-model = 已选择 OpenRouter 用于翻译，但未选择模型
llm-openrouter-no-vision-model = 已选择 OpenRouter 用于视觉，但未选择模型
llm-openrouter-not-text = OpenRouter 模型 { $model } 不能输出文本；请为翻译选择其他模型
llm-openrouter-not-vision = OpenRouter 模型 { $model } 不接受图片；请选择视觉模型
llm-vision-missing = 无（{ $reason }）
llm-pair = 翻译：{ $text }；视觉：{ $vision }

## Media tools and OCR

common-ffmpeg-exit = ffmpeg 退出码 { $code }：
    { $tail }
media-ffprobe-start = ffprobe 启动失败：{ $error }
media-ffprobe-exit = ffprobe 退出码 { $code }：{ $stderr }
media-ffprobe-no-streams = ffprobe：没有流
media-no-streams = 输入中既没有视频流也没有音频流
media-no-duration = 无法确定时长
media-no-wav = ffmpeg 没有生成 wav
media-ffmpeg-hung = ffmpeg 在 { $seconds } 秒内未结束，已被终止（卡死）
media-ffprobe-duration-exit = ffprobe duration 退出码 { $code }
media-env-filter-script = 包络滤镜脚本：{ $error }
media-no-sample-rate = { $path }：未读取到采样率（{ $stderr }）
ocr-no-blur = { $error }；不模糊
ocr-models-missing = 未找到 OCR 模型
ocr-detection-failed = OCR 检测失败（{ $error }）
ocr-summary = OCR：{ $regions } 个区域，{ $localize } 个待本地化，{ $bands } 个条带，sub_y={ $sub_y }

## Hardware presets

common-write = 写入 { $what }：{ $error }
preset-rtx5090-title = RTX 5090（32 GB）
preset-rtx4090-title = RTX 4090（24 GB）
preset-top-subtitle = 最高质量：本地运行最高量化
preset-gpu16-title = 16 GB GPU
preset-gpu16-subtitle = 高质量（4080/4070 Ti 等）
preset-gpu12-title = 12 GB GPU
preset-gpu12-subtitle = 均衡（3060/4070 等）
preset-gpu8-title = 8 GB GPU
preset-gpu8-subtitle = 经济：轻量量化（3060 Ti/4060）
preset-weak-nvidia-cloud-title = 弱 NVIDIA + 云端
preset-weak-nvidia-cloud-subtitle = 重任务（翻译/视觉/配音）交给 OpenRouter，分离和 ASR 在你的 GPU 上
preset-cloud-title = CPU + 云端（无 NVIDIA）
preset-cloud-subtitle = 重任务交给 OpenRouter，本地任务在处理器上运行；无需显卡（需要密钥）
preset-custom-title = 自定义
preset-custom-subtitle = 我会手动设置每个参数
preset-reason-no-gpu = 未找到 NVIDIA GPU，使用“CPU + 云端”模式：重任务交给 OpenRouter，本地任务在处理器上运行（较慢，但可用）
preset-reason-top-card = 检测到 { $gpu }：使用最高量化
preset-reason-by-vram = { $gpu } · { $vram } GB 显存：{ $preset }
preset-reason-low-vram = { $gpu } · { $vram } GB 显存不足以运行本地模型；云端更可靠
preset-unknown = 未知预设：{ $id }
