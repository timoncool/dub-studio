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

## Service port, shortening, frontend, launch defaults

common-corrupt = { $path } 已损坏：{ $error }
common-create-dir = 文件夹 { $path }：{ $error }
service-bad-port = { $value }：不是端口号（应为 1..65535）
service-exe-path = 服务 exe 路径：{ $error }
service-request-not-sent = 连接已建立，但请求未发出：{ $error }
service-no-health-answer = 连接已建立，但 /health 没有响应：{ $error }
service-not-http = 响应不是 HTTP
service-health-status = 一个 HTTP 服务器；/health 返回 { $code }
service-health-not-dub-studio = 一个 HTTP 服务器；/health 返回的不是 Dub Studio 的内容
service-health-not-json = 一个 HTTP 服务器；/health 返回的不是 JSON
service-health-no-fields = /health 自称 { $app }，但缺少服务字段：{ $error }
service-other-app = 其他应用（{ $app }）
service-health-no-app = 一个 HTTP 服务器；/health 没有应用名称
service-port-reserved = 系统不释放该端口，但没有程序在其上接受连接（可能处于 Windows 保留范围：{ $command }）
service-port-dub-studio = 其上运行着 Dub Studio { $version }（{ $executable }）
service-port-other = 被其他进程占用：{ $what }
service-port-busy = 端口 127.0.0.1:{ $port } 已被占用 { $seconds } 秒：{ $who }。错误：{ $error }。

    请关闭占用它的程序，或用环境变量 { $env } 指定其他端口（例如 { $env }={ $other_port }）。
shorten-line-done = 缩短 { $n }/{ $total }：{ $from } -> { $to } 个字符
shorten-line-rejected = 缩短 { $n }/{ $total }：回答未被接受（{ $reason }）
shorten-line-no-answer = 缩短 { $n }/{ $total }：LLM 没有回答：{ $error }
shorten-auto-start = { $count } 句放不进时间槽；正在缩短译文并只为这些句子配音
shorten-higgs-unloaded = 缩短译文期间已卸载 Higgs
shorten-auto-no-llm = 已跳过缩短译文：LLM 不可用：{ $error }
shorten-none-shortened = 缩短：{ $count } 句均未缩短；保持现有配音
shorten-done = 已缩短 { $count }/{ $total } 句
shorten-nothing = 无需缩短：所有句子都放得进时间槽
shorten-no-llm = 缩短：LLM 不可用：{ $error }
shorten-start = 正在缩短 { $count } 句：{ $provider }
shorten-all-failed = 缩短失败：LLM 没有回答任何一句（{ $id }：{ $error }）
spa-not-built = 前端尚未构建
settings-bad-speaker-count = speaker_count：应为 0 到 { $max } 的整数（0 表示自动）
settings-bad-vo-gain = vo_gain_db={ $value }：应为 { $min } 到 { $max } dB 之间的数
settings-bad-src-lang = src_lang={ $value }：既不是语言代码也不是 "auto"
settings-bad-tgt-lang = tgt_lang={ $value }：不是语言代码
settings-bad-casting-ref = casting_ref={ $value }：不是选角档案的 slug
settings-style-too-long = tr_style_custom 超过 { $max } 个字符
settings-too-many-slots = { $name }：超过 { $max } 个槽位
settings-unknown-field = 未知的启动默认值字段：{ $key }
settings-read-failed = 读取启动默认值：{ $error }
settings-patch-not-object = PATCH /settings/launch 的请求体应为字段对象
settings-write-failed = 写入启动默认值：{ $error }

## Takes, translation, TTS text, MCP arguments

takes-delete-old = 删除旧录音 { $path }：{ $error }
takes-missing = 句子 { $line } 的历史中没有第 { $take } 条录音
translate-no-vision = 未确定内容类型：视觉不可用（{ $error }）
translate-subs-already-translated = 字幕已是目标语言 -> 不做机器翻译（仅配音）
translate-transcribe-only = 转写：tgt=原文，不翻译
translate-same-language = 同一语言 -> 不做机器翻译（tgt=原文）
translate-no-llm = 翻译失败：LLM 不可用：{ $error }
translate-ctx-pass = 上下文处理：视觉版面/场景 + 翻译转写
translate-failed = 翻译失败：{ $error }
translate-untranslated = 翻译失败：{ $total } 行中有 { $left } 行仍为原语言（详情见日志和 logs/llama-server.log）
translate-untranslated-auto = 翻译失败：{ $total } 行中有 { $left } 行仍为原语言；如果视频中的语音已是目标语言，请指定原始语言，这样就无需翻译（详情见日志和 logs/llama-server.log）
translate-done = 翻译完成：{ $done }/{ $total } 行，标题={ $titles }
translate-left-untranslated = { $total } 行中有 { $left } 行仍为原语言（详情见 logs/llama-server.log）
translate-coverage-retry = 翻译覆盖：{ $count } 行未翻译；正在补译
translate-coverage-failed = 翻译覆盖：补译失败（{ $error }）
translate-coverage-left = 翻译覆盖：仍有 { $count } 行未翻译
tts-silent-after-cleanup = 清理文本后没有可读内容，保持静音：{ $count } 句（{ $lines }）
mcp-bad-speaker-count = speaker_count：应为 0 到 { $max } 的整数
mcp-speakers-without-diarize = speaker_count 大于 1 时不能与 diarize=false 同用

## Routes: projects, voices, setup, translation, export, jobs, alignment

project-serialize = 序列化 project.json：{ $error }
project-delete-failed = 删除项目 { $path }：{ $error }
setup-no-ids = ids 为空
setup-fetching-missing = 正在下载此功能缺少的模型…
setup-diarization-missing = 说话人分离模型未能下载（{ $error }）；分析将不区分说话人
setup-pick-model-files = 模型文件
setup-pick-models-folder = 包含现成模型的文件夹
setup-no-folder = 文件夹 { $path } 不存在
voices-not-found = 在 voices/ 中未找到音色 { $name }
voices-no-vocals = 没有可测量 F0 的人声；请先分析
voices-speakers-changed = 按槽位分配音色：测量音色期间说话人发生了变化；项目未修改，请重新运行
analyze-args-not-object = analyze 参数：应为对象
cost-analyze = OpenRouter：本次分析花费 ${ $spent }（累计 ${ $total }）
cost-run = OpenRouter：本次运行花费 ${ $spent }（累计 ${ $total }）
translate-lines-to = 正在翻译 { $count } 行 → { $lang }
translate-note = 翻译：{ $note }
translate-titles-failed = 标题未翻译（{ $error }）；视频中将保留原语言
translate-titles-error = 翻译标题：{ $error }
translate-lines-changed = 翻译：翻译期间台词或标题发生了变化；项目未修改，请重新翻译
export-pick-folder = 结果保存位置
export-copy-failed = 复制到 { $path }：{ $error }
jobs-wait-not-number = wait：应为秒数
jobs-unknown-kind = job.json 中的任务类型未知：{ $kind }
jobs-not-resumable = { $kind } 任务无法继续
align-no-source-text = 按语音对齐：台词没有原语言文本（字幕以目标语言导入）
align-no-vocals = 按语音对齐：项目没有人声轨；请先分析
align-recognizing = 按语音对齐：正在识别单词
align-recognition-failed = 按语音对齐：识别：{ $error }
align-no-speech = 按语音对齐：未识别到语音；时间未更改
align-mismatch = 按语音对齐：台词与语音不匹配（匹配 { $share }%）；时间未更改
align-lines-changed = 按语音对齐：识别期间台词发生了变化；时间未更改，请重新对齐
align-done = 已按语音对齐：{ $share }% 的台词按单词对齐，{ $changed } 句时间已更改；偏移 { $offset } 秒

## Analysis

analyze-serialize = 序列化 { $what }：{ $error }
analyze-read-model = 读取模型 { $path }：{ $error }
analyze-stage-cache-unreadable = 阶段 { $stage } 的缓存无法读取（{ $error }）；重新计算
analyze-checkpoint-not-saved = 阶段 { $stage } 的检查点未保存：{ $error }
analyze-diarize-continuous = 说话人分离：预计 { $speakers } 位说话人，对整段录音连续处理，不在整点处重置标签
analyze-diarize-fragments = 说话人分离：预计 { $speakers } 位说话人，在片段间匹配声音
analyze-wespeaker-needed = 指定说话人数量需要 WeSpeaker：{ $error }
analyze-align-skipped = 已跳过按语音对齐：字幕为目标语言，语音为原语言
analyze-align-recognizing = 按语音对齐字幕：正在识别单词
analyze-align-recognition-failed = 对齐字幕：语音识别：{ $error }
analyze-align-no-speech = 按语音对齐：未识别到语音；保留文件中的时间
analyze-align-mismatch = 字幕与语音不匹配（{ $share }% 的台词匹配）；保留文件中的时间
analyze-align-done = 字幕已按语音对齐：{ $share }% 的台词按单词对齐，其余随相邻台词平移；文件偏移 { $offset } 秒
analyze-more = 另外 { $count } 条
analyze-window-plan = 长音轨 { $duration } 秒：计划分 { $windows } 个窗口（第一个约 { $first } 秒）；分窗 ASR 尚未启用，整体处理
analyze-audio-cached = 使用缓存的音频（源未改变）；跳过 ffmpeg
analyze-extracting-audio = 正在提取音频（ffmpeg -> 16k 单声道）
analyze-stems-stale = 分轨来自之前的提取；重新分离
analyze-separating = 正在分离人声（{ $model }）：为说话人分离/ASR 提供干净人声
analyze-separation-cached = 使用缓存的分离结果（分轨已完成）
analyze-separation-failed = 分离失败（{ $error }）；在原始音频上进行说话人分离/ASR
analyze-separator-missing = 未找到 { $model }；在原始音频上进行说话人分离/ASR
analyze-diarizing = 说话人分离（{ $model }）
analyze-diarization-cached = 使用缓存的说话人分离
analyze-diarization-count-failed = 指定说话人数量的说话人分离：{ $error }
analyze-diarization-failed = 说话人分离失败（{ $error }）；按单一说话人继续
analyze-diarization-model-missing-count = 未找到 { $model } 模型：无法应用指定的说话人数量
analyze-subs-no-diarization = 字幕：不做说话人分离（整段处理）
analyze-diarization-model-missing = 未找到说话人分离模型；按单一说话人继续
analyze-fewer-speakers = 预计 { $expected } 位说话人，区分出 { $found } 位：未添加缺少的声音
analyze-speakers-matched = 已在片段间匹配声音：{ $speakers } 位说话人
analyze-transcript-cached = 使用缓存的转写：{ $segments } 个片段
analyze-read-subs = 读取字幕 { $path }：{ $error }
analyze-subs-empty = 字幕无法识别或为空：{ $path }
analyze-subs-imported = 字幕已导入：{ $lines } 句，{ $speakers } 位说话人
analyze-cloud-asr = 正在云端转写（OpenRouter STT）
analyze-cloud-stt-failed = 云端语音识别：{ $error }
analyze-cloud-done = 云端：{ $lines } 句，{ $speakers } 位说话人
analyze-hallucination-filter = ASR 幻觉过滤：{ $error }
analyze-hidden-hallucinations = 已隐藏 ASR 幻觉句（无人声）：{ $count } 句：{ $lines }
analyze-hidden-by-text = 已按文本隐藏 ASR 标题和声音（该区间有声音，人声未与音乐分离）：{ $count } 句：{ $lines }
analyze-voiced-suspects = 看似幻觉但有人声，已保留并标记：{ $count } 句：{ $lines }
analyze-merged-fragments = 合并碎片：{ $before } -> { $after } 个片段
analyze-characters-by-voice = 按声音划分的角色：{ $count }
analyze-segments-speakers = { $segments } 个片段，{ $speakers } 位说话人
analyze-project-unparsable = 无法解析 project.json（{ $error }）：其中包含项目词汇表，分析已停止以免丢失
analyze-glossary-fixed = 词汇表：已修正 { $count } 处术语识别错误
analyze-no-speech-nodub = 没有语音片段；保留原音轨（nodub）
analyze-auto-nodub = 自动：没有适合配音的语音 -> NODUB（原声 + 本地化屏幕文字）
analyze-casting-style = 选角档案中的角色描述 -> 翻译风格（{ $chars } 个字符）
analyze-empty-removed = 已移除无文字的片段：{ $count }
analyze-translation-cached = 使用缓存的翻译
analyze-ocr-cached = 使用缓存的屏幕文字检测
analyze-audio-no-ocr = 音频模式：没有视频，无需检测屏幕文字
analyze-ocr-off = 内嵌文字检测已关闭（复选框）
analyze-content-type = 内容类型（自动）：{ $kind }
analyze-casting-cached = 使用缓存的选角
analyze-casting-checkpoint = 选角检查点未保存：{ $error }
analyze-profile-voices-missing = 在 voices/ 中未找到档案音色 -> 克隆：{ $voices }
analyze-profile-voices-applied = 已将档案音色应用到配音（{ $count } 个角色）
analyze-audio-no-casting = 音频模式：没有视频，无需角色选角
analyze-cache-not-saved = 无法保存 cache.json：{ $error }（不影响使用）
