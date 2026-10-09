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

## Casting, recording, voice library

casting-relabel = 按声音重新标注：按声音得到 { $count } 个角色（说话人分离找到 { $found } 个）
casting-skipped-no-speech = 已跳过选角：没有语音片段
casting-skipped-no-speakers = 已跳过选角：没有说话人
casting-speakers-ranked = 说话角色：{ $count } 个（按说话时长排序）
casting-faces-bound = 已关联到说话人的人脸：{ $count }/{ $total }
casting-bit-part-skipped = { $character }：说话人 { $speaker } 只有 1 句，既无人脸也无声音 -> 跳过（不选角的小角色）
casting-character-face = { $character }：说话人 { $speaker }，{ $lines } 句，说话 { $seconds } 秒，人脸：有
casting-character-no-face = { $character }：说话人 { $speaker }，{ $lines } 句，说话 { $seconds } 秒，人脸：无
casting-profile-other-type = 档案类型不同（{ $previous } ≠ { $current }）；已跳过跨集匹配
casting-cross-episode = 跨集：已沿用的名字/音色：{ $count }
casting-saved = casting.json 已就绪：{ $count } 个角色
casting-save-failed = 无法写入 casting.json：{ $error }
casting-faces-collected = 已收集人脸：{ $faces } 张（样本 { $samples } 个）
casting-face-clusters = 人脸身份（按向量聚类）：{ $count }
casting-anime-detector-missing = 未找到动漫检测器（{ $path }）；不生成头像
casting-anime-detector-failed = 动漫检测器加载失败：{ $error }；不生成头像
casting-scrfd-missing = 未找到 SCRFD；不生成头像（按声音选角）
casting-scrfd-failed = SCRFD 加载失败：{ $error }；不生成头像
casting-embedder-missing = 未找到 { $model }（{ $path }）；头像不带特征向量
casting-embedder-failed = { $model } 加载失败：{ $error }；头像不带特征向量
casting-no-frame = ffmpeg 未能提取帧
casting-voice-no-vocals = 已跳过声音特征：没有干净的人声
casting-voice-no-model = 已跳过声音：没有 WeSpeaker 模型（{ $path }）
casting-voice-model-failed = WeSpeaker 加载失败：{ $error }；已跳过声音
casting-voice-trim-failed = 声音样本 { $speaker }：裁剪失败：{ $error }
casting-voice-embedding-failed = 声音 { $speaker }：特征提取失败：{ $error }
casting-voice-embeddings = 声音特征：{ $count } 个
casting-applying-profile = 正在应用库档案：{ $slug }
casting-library-profile-missing = 未找到库档案“{ $slug }”；未应用
record-busy = 已在录音
record-mic-not-found = 未找到麦克风“{ $name }”：可能已断开或被重命名；请选择其他麦克风
record-no-mic = 未找到麦克风
record-mic-config = 麦克风配置：{ $error }
record-create-wav = 创建 wav：{ $error }
record-format-unsupported = 不支持格式 { $format }
record-mic-open = 打开麦克风：{ $error }
record-mic-start = 启动麦克风：{ $error }
record-not-recording = 当前没有录音
voices-not-file-list = { $page }：不是文件列表：{ $error }
voices-list-endless = { $page }：数据集文件列表没有尽头
voices-too-large = { $url }：超过固定的 { $size } 字节
voices-size-mismatch = { $url }：收到 { $got } 字节，固定为 { $size } 字节
voices-sha-mismatch = { $url }：SHA-256 { $got } 与固定值 { $want } 不符
voices-bad-name = 名称无效
voices-not-in-dataset = 数据集 { $dataset } 中没有音色 { $name }
voices-bad-size = { $name }：目录中的大小无效
voices-no-sha = { $name }：目录中没有 SHA-256
voices-pack-downloading = 正在下载音色包
voices-pack-unpacking = 正在解压
voices-open-zip = 打开 zip：{ $error }
voices-create-file = 创建 { $name }：{ $error }
voices-unpack-file = 解压 { $name }：{ $error }
voices-pack-done = 完成：{ $count } 个文件

## Downloads by link

url-no-fetch = 没有下载 { $id }
url-stopped = 已停止
url-already-downloading = 该链接已在下载：{ $id }
url-probe-not-json = yt-dlp -J 的输出不是 JSON：{ $error }；{ $stderr }
url-interrupted = 下载随工作室关闭而中断；已下载部分在其文件夹中，将从断点继续
url-not-a-link = “{ $url }”不是链接：{ $error }
url-not-http = “{ $url }”不是 http(s) 链接
url-bad-subs-lang = “{ $lang }”不是字幕语言代码
url-cookies-not-file = { $path } 不是不超过 1 MB 的 cookies.txt 文件
url-cookies-empty-or-large = cookies.txt 为空或超过 1 MB
url-thread-failed = 下载线程未能启动：{ $error }
url-cookies-gone = 此下载的 cookies.txt 已从其文件夹中消失；请携带 cookies 重新开始下载
url-disk-space = 需要约 { $need } MB，可用 { $free } MB
url-no-subs = 视频没有人工上传的“{ $lang }”字幕（现有：{ $have }）
url-no-subs-none = 视频没有人工上传的“{ $lang }”字幕（一个也没有）
url-no-media-file = yt-dlp 已结束，但 { $path } 中没有视频文件：{ $stderr }
url-subs-failed = “{ $lang }”字幕未能下载（{ $code }）：{ $error }
url-ytdlp-start = 启动 yt-dlp：{ $error }
url-ytdlp-wait = 等待 yt-dlp：{ $error }
url-still-downloading = { $id } 仍在下载
url-cancelled-removed = { $id } 已取消，已下载部分已删除：请开始新的下载
url-still-stopping = { $id } 仍在停止；请几秒后重试
url-cancel-first = { $id } 仍在下载；请先取消
url-no-subs-file = yt-dlp 已结束，但 { $path } 中没有字幕文件：{ $stderr }
url-no-parent = { $path } 没有上级文件夹
url-subs-empty = 字幕 { $path } 中没有任何台词
url-ytdlp-missing = 尚未下载 ytdlp 组件
url-bad-quality = 画质“{ $quality }”：应为 best、1080、720、480 或 audio

## yt-dlp

ytdlp-update-missing = yt-dlp 更新 { $version }（{ $path }）缺失；正在使用固定版本 { $pinned }
ytdlp-http-client = HTTP 客户端：{ $error }
ytdlp-tag-not-version = 标签为“{ $tag }”的 yt-dlp 发布不是版本号
ytdlp-no-checksum = { $url } 中没有 { $file }
ytdlp-new-says = 新的 yt-dlp { $tag } 自称“{ $said }”；保留 { $current }
ytdlp-new-failed = 新的 yt-dlp { $tag } 未能启动：{ $error }；保留 { $current }
ytdlp-too-large = { $url }：超过 { $limit } 字节
ytdlp-sha-mismatch = { $url }：SHA-256 { $got } 与 SHA2-256SUMS { $want } 不符
ytdlp-exit-code = 退出码 { $code }：{ $stderr }
ytdlp-component-missing = 尚未下载 ytdlp 组件（yt-dlp 和 deno）
ytdlp-no-ffmpeg = 没有 ffmpeg：既没有 ffmpeg 组件，PATH 中也没有 ffmpeg.exe
ytdlp-start = 启动 { $program }：{ $error }
ytdlp-timeout = yt-dlp 在 { $seconds } 秒内无响应：{ $stderr }
ytdlp-playlist = 该链接是播放列表或频道（{ $count } 个视频），不是单个视频
ytdlp-redirect-only = 该链接中没有视频本身，只有指向别处的链接
ytdlp-live = 直播（{ $status }）
ytdlp-thumbnail-not-image = 缩略图不是图片（{ $mime }）
ytdlp-thumbnail-too-large = 缩略图超过 4 MB
ytdlp-failed-silently = yt-dlp 出错退出，没有消息

## Render: voicing, QC, mix, mux

render-input = 输入 { $width }x{ $height } 时长={ $duration }s
render-nodub-original = nodub：原始音轨
render-done-audio = 完成（仅音频）-> { $path }
render-building-ass = 正在生成 ASS（标题 + 配音字幕）
render-burning = 正在烧录字幕 + 模糊（ffmpeg + libass，NVENC）
render-burn-off = 字幕/标题已关闭（subs.burn=off）
render-muxing = 正在封装视频 + 音频
render-track-dub = { $lang }（配音）
render-track-original = { $lang }（原声）
render-two-tracks = 两条音轨：{ $dub } + { $original } -> { $container }
render-multitrack-failed = 多音轨封装失败（{ $error }）-> 单音轨
render-subtitle-tracks = 字幕作为 mkv 轨道：{ $tracks }
render-subtitle-tracks-failed = 字幕作为 mkv 轨道：{ $error }
render-mp4-companion-failed = 未生成配套 mp4（{ $error }）；播放器将打开 mkv（VLC 可用）
render-done = 完成 -> { $path }
render-dub-audio-done = 配音音频已完成
render-synth-thread-ended = 合成线程结束但没有结果
render-synth-timeout = 合成超时（>{ $seconds } 秒）；已取消，引擎空闲
render-engine-stuck = 合成超过 { $seconds } 秒仍无法取消；渲染已中止（引擎在 DLL 中卡死）
render-higgs-load-failed = 加载 Higgs DLL：{ $error }
render-defect-runaway = 失控过长
render-defect-cutoff = 截断
render-defect-silence = 静音
render-defect-hum = 嗡鸣
render-recognition-no-answer = 识别没有返回结果
render-second-pass-overflow = 第二轮配音请求缩短，但该轮已关闭缩短
render-no-translated-lines = 没有已翻译的句子 -> 静音，使用原音轨
render-takes-of-removed-lines = 已清除删除句子的录音历史：{ $count }
render-extracting-audio = 正在提取音频（ffmpeg 44.1k 立体声）
render-separator-missing = 未找到分离引擎 -> 不保留背景（keep_music off）
render-ref-from-mix = 克隆参考取自未分离的混音：其中也包含原始背景声
render-emotion-ref-failed = 片段 { $segment }：未能剪出情感参考（{ $error }）；使用说话人身份参考
render-cloud-voices = 按说话人分配的云端音色：{ $voices }
render-synth-keys-reset = { $error }；合成键已重新开始
render-synthesizing = 正在合成 { $total } 个片段中的 { $count } 个
render-voicing-cached = 使用缓存的配音：{ $count } 个片段
render-cloud-tts-parallel = 云端 TTS：{ $count } 个片段，{ $threads } 个并行线程
render-cloud-tts-ready = 云端 TTS：预合成完成（{ $count } 个片段）
render-takes-quarantined = { $error }；第 { $line } 句的录音历史已移到 { $path } 并重新开始
render-pinned-take = 第 { $line } 句：播放已固定的录音；新配音不会替换它
render-take-unpinned = 第 { $line } 句：台词文本已更改，已取消固定录音
render-selected-take = 第 { $line } 句：播放选定的第 { $take } 条录音；新配音不会替换它
render-write-cloud-segment = 写入云端 seg{ $line }：{ $error }
render-cloud-tts-failed = ⚠ 片段 { $line }：云端 TTS 失败（{ $error }）；保留原声
render-loading-higgs = 正在加载 Higgs
render-failures-kept-generated = ⚠ 片段 { $line }：合成失败 { $attempts } 次（{ $error }）；采用生成的配音（动态范围 { $range } dB）
render-failures-kept-original = ⚠ 片段 { $line }：合成失败/超时 { $attempts } 次（{ $error }）；保留原台词
render-regenerating = 片段 { $line }：{ $error }；正在重新生成（{ $attempt }/{ $attempts }）
render-defects-kept-generated = ⚠ 片段 { $line }：{ $attempts } 次尝试均有缺陷（{ $defect }）；采用生成的配音（动态范围 { $range } dB）
render-silent-kept-original = ⚠ 片段 { $line }：{ $attempts } 次尝试均无声音；改用原声
render-retry-alt-ref = 换参考
render-retry-temperature = 提高温度
render-defect-regenerating = 片段 { $line }：合成缺陷（{ $defect }）；正在重新生成（{ $via } { $attempt }/{ $attempts }）
render-write-segment = 写入 seg{ $line }：{ $error }
render-too-many-artifacts = TTS：嗡鸣伪影过多（连续 { $in_a_row } 次，累计重试 { $retries } 次）；重新生成无效。问题可能出在设备（模型/显存）或参考音频片段。已在片段 { $line } 停止。
render-multi-take = 片段 { $line }：多次录音；选用最贴合时间槽的一条（偏差 { $deviation } 秒）
render-stretch-over-cap = 片段 { $line }：需要拉伸 x{ $needed }（时间槽 { $slot } 秒），上限 x{ $cap }；文本语速快于正常
render-silence-trimmed = 裁剪 TTS 静音：从 { $lines } 句中裁掉 { $seconds } 秒（其中停顿 { $pauses } 秒）；裁剪使 { $into_cap } 句的加速回到上限内
render-fit-summary = 排布：{ $over }/{ $total } 个片段超出上限（{ $share }%）
render-fit-summary-drift = 排布：{ $over }/{ $total } 个片段超出上限（{ $share }%），{ $drift } 处追回同步
render-qc-start = 质检：正在通过转写核对 { $count } 句
render-qc-unheard = 质检：{ $total } 句中有 { $count } 句未核对；识别失败：{ $reason }
render-qc-mismatch = 质检：{ $count } 句与译文不符；重新合成
render-qc-resynthesized = 质检：片段 { $line } 已重新合成（第 { $attempt } 次）
render-qc-unconfirmed = ⚠ 质检：片段 { $line }（“{ $text }”）无法确认；请手动检查该句
render-qc-kept-mismatch = ⚠ 质检：片段 { $line } 与译文不符；保留生成的配音
render-qc-resynth-unheard = 质检：{ $count } 句重新合成的台词未核对；识别失败：{ $reason }
render-qc-summary = 质检结果：已修正 { $fixed }/{ $total }，仍标记 { $flagged }，未核对 { $unheard }
render-qc-all-confirmed = 质检：所有句子均已通过转写确认 ✓
render-qc-rest-confirmed = 质检：其余句子已通过转写确认
render-laying-out = 正在将配音排布到时间线
render-peak-limiter = 峰值限制器：{ $lines } 句中超过 { $ceiling } 上限的 { $samples } 个采样已无削波地压低
render-tempo-fit = 整条音轨速度适配 x{ $factor }
render-voiceover-envelope = 画外音：原声在译文下方 { $db } dB，停顿处恢复（动态包络，{ $blocks } 个区块）
render-voiceover-flat = 画外音：包络不可用 -> 平坦衰减
render-mix-no-ducking = 混音：伴奏 + 配音人声（闪避关闭，背景完整）
render-mix-ducking = 混音：伴奏 + 配音人声（闪避开启，包络，{ $blocks } 个区块）
render-mix-sidechain = 包络不可用 -> 侧链闪避
render-mix-plain = 侧链不可用 -> 直接混音
render-loudness-off = 响度均衡已关闭：保持混音原样
render-loudness-normalizing = 正在标准化响度（EBU R128，真峰值）
render-loudnorm-skipped = 已跳过 loudnorm（{ $error }）
render-track-gain = 音轨增益 { $db } dB
render-dub-timings-not-written = 未写入字幕的配音时间：排布（{ $lines } 句，{ $spans } 段）与片段（{ $segments }）不匹配；字幕沿用原始时间
render-dub-timing-mismatch = 配音时间：合成视图中的句子 { $line } 与项目第 { $index } 个片段不匹配
render-word-timings = 字幕逐词时间：正在识别 { $count } 句配音
render-refs-unchecked = 核对参考：{ $count } 个候选未经核对即被采用；识别失败：{ $error }
render-refs-all-failed = ⚠ 说话人 { $speaker }：所有参考候选均未通过核对（听到：“{ $heard }”）；按得分取最佳
render-speaker-ref-failed = 说话人 { $speaker } 的参考：{ $error }
render-speaker-ref = 说话人 { $speaker } 的参考：“{ $text }”（{ $seconds } 秒，{ $candidates } 个候选，核对通过）
render-speaker-ref-unchecked = 说话人 { $speaker } 的参考：“{ $text }”（{ $seconds } 秒，{ $candidates } 个候选，核对 ⚠ 未通过）

## Setup: components

setup-comp-higgs-purpose = 配音合成与声音克隆（TTS）
setup-comp-higgs-engine-name = Higgs 引擎（audiocpp_engine.dll）
setup-comp-higgs-engine-purpose = Higgs 原生 TTS 引擎（C ABI）
setup-comp-gemma-purpose = 翻译以及字幕/标题的视觉编排
setup-comp-gemma-q5-0-purpose = 翻译与视觉，比 q4_0 更准确
setup-comp-gemma-q6-k-purpose = 翻译与视觉，更加准确
setup-comp-gemma-q8-0-purpose = 翻译与视觉，最高准确度
setup-comp-parakeet-purpose = 带逐词时间戳的语音识别（ASR）
setup-comp-higgs-q6-k-purpose = 配音合成与声音克隆（TTS），比 Q8_0 更轻
setup-comp-higgs-q4-k-m-purpose = 配音合成与声音克隆（TTS），最轻量版本
setup-comp-parakeet-fp32-purpose = 语音识别（ASR），完整 fp32 精度
setup-comp-parakeet-ultra-purpose = 语音识别（ASR），Moondream 微调版，错误更少
setup-comp-whisper-engine-name = Whisper-Faster（ASR 引擎）
setup-comp-whisper-engine-purpose = 替代 Parakeet 的另一种语音识别引擎（faster-whisper）
setup-comp-whisper-cuda-name = Whisper CUDA 加速（cuBLAS + cuDNN）
setup-comp-whisper-cuda-purpose = 在 GPU 上运行 Whisper 推理（否则在 CPU 上识别，慢数倍）
setup-comp-whisper-tiny-name = Whisper tiny（ASR 模型）
setup-comp-whisper-tiny-purpose = Whisper ASR，最轻最快的模型
setup-comp-whisper-base-name = Whisper base（ASR 模型）
setup-comp-whisper-base-purpose = Whisper ASR，轻量模型，比 tiny 更准确
setup-comp-whisper-small-name = Whisper small（ASR 模型）
setup-comp-whisper-small-purpose = Whisper ASR，均衡模型
setup-comp-whisper-medium-name = Whisper medium（ASR 模型）
setup-comp-whisper-medium-purpose = Whisper ASR，高准确度
setup-comp-whisper-large-v3-name = Whisper large-v3（ASR 模型）
setup-comp-whisper-large-v3-purpose = Whisper ASR，最高准确度（large-v3）
setup-comp-whisper-large-v3-turbo-name = Whisper large-v3-turbo（ASR 模型）
setup-comp-whisper-large-v3-turbo-purpose = Whisper ASR，接近 large-v3 但明显更快（turbo）
setup-comp-sortformer-name = Nemotron 3 Diarization（最多 8 位说话人）
setup-comp-sortformer-purpose = 区分说话人（谁在何时说话），最多 8 个声音
setup-comp-roformer-purpose = 人声/伴奏分离模型
setup-comp-roformer-q5-purpose = 分离，比 Q8_0 更轻
setup-comp-roformer-q4-purpose = 分离，最轻量版本
setup-comp-casting-name = 角色选角模型（人脸 + 声音）
setup-comp-casting-purpose = 人脸检测/特征（真人 + 动漫）+ 用于选角的声音特征
setup-comp-bsroformer-engine-name = BSRoformer.cpp 引擎（CUDA）
setup-comp-bsroformer-engine-purpose = 原生分离引擎（bs_roformer-cli + ggml-CUDA）
setup-comp-bsroformer-engine-cpu-name = BSRoformer.cpp 引擎（CPU）
setup-comp-bsroformer-engine-cpu-purpose = 在处理器上分离，无 NVIDIA 模式（较慢，功能完整）
setup-comp-llama-name = llama.cpp 服务器（CUDA 13.4）
setup-comp-llama-purpose = Gemma 的伴随服务器（翻译/视觉）
setup-comp-onnxruntime-purpose = ASR/OCR/说话人分离运行时（严格 1.28.x）
setup-comp-onnxruntime-gpu-purpose = 在 GPU 上运行说话人分离/Parakeet 的 CUDA 提供程序（local_backend=gpu 模式）
setup-comp-ffmpeg-purpose = 视频和音频解码/编码（NVENC）
setup-comp-ytdlp-name = 链接下载（yt-dlp + deno）
setup-comp-ytdlp-purpose = 通过链接下载视频（YouTube 及 yt-dlp 支持的其他网站）到新项目
setup-comp-cuda-runtime-purpose = 供引擎和 onnxruntime CUDA EP 使用的可再分发 CUDA DLL（无需 CUDA Toolkit）
setup-comp-cudnn-purpose = onnxruntime CUDA 提供程序在 GPU 上运行说话人分离/Parakeet 时需要
setup-comp-vcruntime-purpose = 引擎所需的系统 DLL（已随附）
setup-comp-ocr-name = OCR 模型（PP-OCR ONNX）
setup-comp-ocr-purpose = 内嵌文字检测 → 模糊（已随附）
setup-comp-nvidia-driver-name = NVIDIA 驱动
setup-comp-nvidia-driver-purpose = GPU 加速（需单独安装，不由应用安装）

## Setup: downloads and installation

setup-http-status = { $url }：状态 { $status }
setup-write = 写入：{ $error }
setup-not-zip = 不是 zip：{ $error }
setup-zip-entry = zip 条目：{ $error }
setup-open = 打开 { $path }：{ $error }
setup-verifying = 正在校验 { $file } 的 SHA-256…
setup-install-record = 安装记录：{ $error }
setup-disk-space = 空间不足：需要 { $need } GB，可用 { $free } GB（{ $path }）
setup-unknown-component = 没有组件 { $id }
setup-not-removable = { $id } 不由应用安装
setup-component-busy = 该组件正在下载；请先暂停下载
setup-paused = 下载已暂停
setup-rate-limited = { $url }：服务器已持续 { $minutes } 分钟返回 { $status }
setup-proxy-scheme = 代理 { $proxy }：方案 { $scheme } 不适用于下载（http、https、socks4、socks5）
setup-proxy-no-host = 代理 { $proxy }：没有主机
setup-proxy-no-port = 代理 { $proxy }：没有端口
setup-proxy-credentials = 代理 { $proxy }：模型下载（ureq）无法向代理传递这样的用户名或密码：其中含有 / ? #、空格、非 ASCII 字符或（SOCKS5 下）密码中的冒号；通过该代理的云端请求正常，下载需要不含这些字符的密码
setup-start-failed = { $url }：{ $retries } 次尝试均未能开始：{ $error }
setup-chunk-manifest = 分块清单：{ $error }
setup-range-incomplete = 区段不完整：{ $got }/{ $want } 字节
setup-range-failed = 区段 { $start }-{ $end } 经 { $retries } 次尝试后：{ $error }
setup-range-status = 区段 { $start }-{ $end }：状态 { $status }（应为 206）
setup-range-read = 读取区段：{ $error }
setup-create = 创建 { $path }：{ $error }
setup-read = 读取：{ $error }
setup-download-failed = { $url } 经 { $retries } 次尝试后：{ $error }
setup-size-mismatch = { $file }：已下载 { $got } 字节，固定为 { $want } 字节；文件已删除，下次尝试将重新开始
setup-hash-mismatch = { $file }：SHA-256 { $got } 与固定值 { $want } 不符；文件已删除，下次尝试将重新开始
setup-unpacking = 正在解压 { $file }…
setup-rename = 重命名 { $path }：{ $error }
setup-waiting-other = 正在等待同一组件的另一个下载…
setup-delete = 删除 { $path }：{ $error }
setup-downloading = 正在下载模型…
setup-source-changed = { $url }：服务器提供 { $got } 字节，固定为 { $want } 字节；来源已改变
setup-manifest-write = 清单 { $path }：{ $error }
setup-archive-no-files = 压缩包 { $path } 中没有所需文件
setup-wheel-not-zip = wheel 不是 zip：{ $error }
setup-wheel-entry = wheel 条目：{ $error }
setup-archive-no-dll = 压缩包 { $path } 中没有 DLL
setup-unpack = 解压 { $path }：{ $error }
setup-finalize = 完成 { $path }：{ $error }

## Settings applied after analysis

post-analyze-bad-vo-gain = vo_gain：应为 dB 数值，收到 { $value }
post-analyze-bad-flag = { $name }：应为 0 或 1，收到 { $value }
post-analyze-bad-container = container：应为 mp4 或 mkv，收到 { $value }
post-analyze-bad-voice-slots = voice_slots：应为对象 {"{"}male:[…], female:[…]{"}"}
post-analyze-edit-failed = 分析后的设置 { $edit }：{ $error }

## Messages of the engines and libraries: glossary, LLM, ASR, translation, separation, faces, TTS, captions, OCR

glossary-over-limit = 术语表有 { $total } 条，超过上限 { $max }
glossary-empty-term = 第 { $entry } 条没有术语
glossary-field-too-long = 第 { $entry } 条（“{ $term }”）：有字段超过 { $max } 个字符
glossary-duplicate = 术语“{ $term }”出现了两次（第 { $first } 条和第 { $second } 条）
glossary-tsv-keep = 第 { $line } 行：keep 为“{ $value }”，应为 1 或 0
glossary-tsv-empty-term = 第 { $line } 行没有术语
glossary-one-of = 只能提交条目或 TSV 之一
glossary-nothing = 没有可保存的内容：既无条目也无 TSV
llm-spawn-failed = llama-server 未能启动：{ $error }
llm-gguf-missing = 未找到 GGUF 模型（{ $path }）
llm-log-file = llama-server 日志 { $path }：{ $error }
llm-exited-early = llama-server 在就绪前退出（{ $status }）；stderr：{ $stderr }
llm-not-ready = llama-server 在 { $secs } 秒内未启动（端口 { $port }）；stderr：{ $stderr }
llm-http = 向模型发送请求失败：{ $error }
llm-api = 模型 API：{ $error }
llm-rejected = 模型 API 拒绝了请求（{ $status }）：{ $body }
llm-empty-answer = 模型 { $model } 返回了空回答（未说明原因）
llm-empty-answer-reason = 模型 { $model } 返回了空回答（finish_reason={ $reason }）
llm-cut-short = 模型 { $model } 达到了 { $max_tokens } 个 token 的上限（finish_reason=length）：回答不完整；它可能把预算花在了推理上——请选择不强制思考的模型
llm-prompt-cut = 服务器只读取了 { $chars } 个字符的请求中的 { $read } 个 token，并丢弃了开头——它的上下文太小：请在 Ollama 中调大 num_ctx，或在 LM Studio 中调大模型的 Context Length
