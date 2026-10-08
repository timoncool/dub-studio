//! Рендер-ядро (порт render-половины pipeline.run + _build_dub TTS-ветки + assemble + compose/mix +
//! captions.build/burn + mux). От Project с переводом до готового дублированного MP4.
//!
//! Конвейер (последовательный, GPU-стадии по очереди — VRAM-инвариант):
//!   probe -> extract 44.1k -> separate (vocals/instrumental через dub-sep) ->
//!   per-segment Higgs clone TTS (реф из вокала) -> обрезка тишины клипа (tts_trim) ->
//!   fit_to_slot (atempo) -> timeline ->
//!   mix (instrumental + dub) -> build ASS (титры/субтитры/sub_style из Project) ->
//!   burn (blur боксы из project.captions.blur_boxes) -> mux.
//!
//! regen (на экспорте): ре-TTS ТОЛЬКО dirty-сегментов. Кэш per-segment WAV в work_dir (seg_XXX.wav):
//! не-dirty переиспользуются, dirty пере-синтезируются (улучшение против питон-_regen_dub, что гнал
//! весь дубляж заново — правка #10). tgt-текст пустой -> сегмент молчит (как в питоне).

use audiocpp::AudiocppEngine;
use dub_captions::{BlurBox, Sub, SubStyle as CapSubStyle, Title as CapTitle};
use dub_core::{Project, SubStyle as CoreSubStyle, Title as CoreTitle};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::media;
use crate::wavio;

/// Транскрибировать реф-клипы спикеров выбранным ASR-движком, заполнив ref_texts (уже заданные — пропуск).
/// Пустой транскрипт не пишем. Общий шаг pack-рефов и обрезанных клон-рефов.
fn fill_ref_texts(
    asr: &mut dyn dub_asr::AsrEngine,
    refs: &std::collections::BTreeMap<String, PathBuf>,
    ref_texts: &mut std::collections::BTreeMap<String, String>,
) {
    for (spk, refp) in refs {
        if ref_texts.contains_key(spk) {
            continue;
        }
        if let Ok(rsegs) = asr.transcribe(refp, "auto") {
            let txt = rsegs
                .iter()
                .map(|s| s.text.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            // токены уже trim'нуты и непусты, join(" ") не даёт краевых пробелов -> повторный trim не нужен.
            if !txt.is_empty() {
                ref_texts.insert(spk.clone(), txt);
            }
        }
    }
}

/// Пути к моделям/движкам для рендера (резолвятся из AppState).
pub struct RenderPaths {
    pub input: PathBuf,        // исходное видео
    pub work_dir: PathBuf,     // workspace/<pid>
    pub output: PathBuf,       // output.mp4
    pub bsroformer_cli: PathBuf,
    pub bsroformer_model: PathBuf,
    pub higgs_dll: PathBuf,
    pub higgs_model_root: PathBuf,
    pub higgs_quant: String,   // квант выбранного варианта Higgs (q8_0/q6_k/q4_k_m) — для audiocpp load_model
    pub fonts_dir: PathBuf,
    pub higgs_backend: String, // "cuda" | "cpu"
    pub higgs_device: i32,
    pub higgs_threads: i32,
    pub max_stretch: f64,
    pub voices_dir: PathBuf,   // каталог голосов-паков + записей с микрофона
    pub asr: crate::models::AsrChoice, // выбранный ASR-движок — авто-транскрипция реф-клипа (ref_text клона)
    pub bench: bool,           // пер-стадийный бенчмарк (галка настроек, ВЫКЛ по умолчанию)
    pub ref_secs: f64,         // длина реф-клипа клона голоса, сек (настройка «Экономия RAM», дефолт 12.0)
    pub models_root: PathBuf,  // каталог моделей (active.json) — читаем настройки облачного TTS OpenRouter
    pub llama_bin: PathBuf,    // llama-server и GGUF своей Gemma — LLM сокращения перевода не влезших фраз
    pub mt_model: PathBuf,
}

pub type Progress<'a> = dyn Fn(Value) + Send + Sync + 'a;

fn emit(progress: &Progress, stage: &str, msg: &str) {
    progress(json!({ "stage": stage, "msg": msg }));
}

/// Ключи синтеза сегментов {sid: key} рядом с seg-файлами. Пишется атомарно сразу после каждого
/// сегмента: оборванный рендер при повторе синтезирует только то, чего нет или что изменилось.
pub const SEG_CKPT_FILE: &str = "seg_ckpt.json";
/// Запись «в seg-файле оригинальная реплика (keep_original), а не синтез» — не совпадает ни с одним ключом.
const SEG_ORIGINAL: &str = "original";
/// Запись «синтез провалился, в seg-файле оригинальная реплика»: пока дорожка не собрана целиком,
/// продолжение прерванного прогона синтезирует такой сегмент снова.
const SEG_FALLBACK: &str = "fallback";

/// Запись в seg_ckpt.json — настоящий ключ синтеза (а не метка оригинальной реплики).
pub fn is_synth_key(k: &str) -> bool {
    k != SEG_ORIGINAL && k != SEG_FALLBACK
}
/// Поле Segment.extra с нонсом «перегенерировать»: patch regen/regen_all меняет его, меняя и ключ.
pub const REGEN_NONCE: &str = "regen";
/// Версия ключа: поднимать при смене лестницы/опций синтеза, меняющей звучание уже озвученного.
const SEG_KEY_VER: &str = "seg-key-v1";

pub struct SegCkpts {
    path: PathBuf,
    map: std::collections::BTreeMap<String, String>,
}

impl SegCkpts {
    /// Нет файла — пусто; битый — ошибка с причиной.
    pub fn load(wd: &Path) -> Result<Self, String> {
        let path = wd.join(SEG_CKPT_FILE);
        let map = Self::read(&path)?;
        Ok(SegCkpts { path, map })
    }

    /// Пустой набор ключей вместо файла, который не читается (запись поверх него).
    pub(crate) fn start_over(wd: &Path) -> Result<Self, String> {
        let ckpts = SegCkpts { path: wd.join(SEG_CKPT_FILE), map: Default::default(),
        };
        let body = serde_json::to_vec_pretty(&ckpts.map).map_err(|e| e.to_string())?;
        dub_core::atomic::write(&ckpts.path, &body)?;
        Ok(ckpts)
    }

    fn read(path: &Path) -> Result<std::collections::BTreeMap<String, String>, String> {
        match std::fs::read_to_string(path) {
            Ok(t) => {
                serde_json::from_str(&t).map_err(|e| format!("разбор {}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Default::default()),
            Err(e) => Err(format!("чтение {}: {e}", path.display())),
        }
    }

    pub fn get(&self, sid: &str) -> Option<&str> {
        self.map.get(sid).map(String::as_str)
    }

    /// Пишет поверх того, что в файле сейчас, а не своей копии: ключ, записанный другим писателем за время
    /// рендера (выбор дубля), не откатывается.
    pub(crate) fn set(&mut self, sid: &str, key: &str) -> Result<(), String> {
        static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _held = WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        self.map = Self::read(&self.path)?;
        if self.get(sid) == Some(key) {
            return Ok(());
        }
        self.map.insert(sid.to_string(), key.to_string());
        let body = serde_json::to_vec_pretty(&self.map).map_err(|e| e.to_string())?;
        dub_core::atomic::write(&self.path, &body)
    }
}

/// Имя seg-файла по id сегмента (только [A-Za-z0-9_]); None — в id нет допустимых символов.
pub fn seg_file_id(id: &str) -> Option<String> {
    let sid: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if sid.is_empty() { None } else { Some(sid) }
}

/// Ключ синтеза сегмента: текст, спикер, слот, голос, реф, движок/квант и опции синтеза, нонс
/// «перегенерировать». Совпал с записанным — синтез не нужен, даже если сегмент dirty.
fn seg_key(s: &dub_core::Segment, voice: &str, reference: &str, engine: &str, opts: &str,
) -> String {
    let regen = s.extra.get(REGEN_NONCE).map(|v| v.to_string()).unwrap_or_default();
    let slot = format!("{:.2}-{:.2}", s.start, s.end);
    let mut h = blake3::Hasher::new();
    for part in [
        SEG_KEY_VER,
        s.tgt_text.trim(),
        s.speaker.as_deref().unwrap_or("0"),
        slot.as_str(),
        voice,
        reference,
        engine,
        opts,
        regen.as_str(),
    ] {
        h.update(part.as_bytes());
        h.update(b"\x1f");
    }
    let style = crate::cloud_tts::style(s);
    if engine.starts_with("cloud:") && !style.is_empty() {
        h.update(b"speech_metadata\x1f");
        h.update(style.as_bytes());
    }
    h.finalize().to_hex().to_string()
}

/// Нужен ли синтез: файла нет -> да; есть записанный ключ -> если не совпал; ключа нет (проект до
/// чекпоинтов) -> прежнее правило dirty, чтобы не переозвучивать весь старый проект.
fn seg_needs_synth(raw_exists: bool, recorded: Option<&str>, key: &str, legacy_dirty: bool,
) -> bool {
    if !raw_exists {
        return true;
    }
    match recorded {
        Some(r) => r != key,
        None => legacy_dirty,
    }
}

/// Отпечаток реф-файла для ключа — по содержимому: рефы пересобираются каждый рендер, и при смене
/// источника вокала (сепарация, keep_music) имя и длина остаются прежними, а звук другой.
fn ref_tag(p: &Path) -> String {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match std::fs::read(p) {
        Ok(bytes) => format!("{name}:{}", blake3::hash(&bytes).to_hex()),
        Err(_) => format!("{name}:missing"),
    }
}

/// Готовый результат рендера.
#[allow(dead_code)]
pub struct RenderResult {
    pub output: PathBuf,
    /// Проект с репликами, сокращёнными циклом подгонки (уже записан в project.json); None — не сокращали.
    pub project: Option<Project>,
}

/// Отрендерить Project -> output.mp4. `regen_dub` — ре-синтез dirty-сегментов дубляжа (иначе кэш).
pub fn run(
    proj: &Project,
    paths: &RenderPaths,
    regen_dub: bool,
    progress: &Progress,
) -> Result<RenderResult, String> {
    dub_captions::set_fonts_dir(&paths.fonts_dir);
    let wd = &paths.work_dir;
    std::fs::create_dir_all(wd).map_err(|e| e.to_string())?;
    // Бенчмаркинг рендера (галка настроек, ВЫКЛ по умолчанию): probe / dub_audio / burn / mux -> bench.json.
    let mut bench = crate::bench::Bench::start(wd, "render", paths.bench);
    bench.stage("probe");

    // probe: длительность/размеры.
    let meta = media::probe(&paths.input)?;
    let total = if proj.meta.duration > 0.0 {
        proj.meta.duration
    } else {
        meta.duration
    };
    let (vw, vh) = (
        if proj.meta.width > 0 { proj.meta.width } else { meta.width },
        if proj.meta.height > 0 { proj.meta.height } else { meta.height },
    );
    let src_codec = if !proj.meta.src_codec.is_empty() {
        proj.meta.src_codec.clone()
    } else {
        meta.src_codec.clone()
    };
    emit(progress, "probe", &format!("вход {}x{} dur={:.1}s", vw, vh, total),
    );

    // voiceover (закадровый) = как dub, но оригинал слышно приглушённым ПОД переведённым голосом.
    let is_voiceover = proj.mode == "voiceover";
    let is_dub = proj.mode == "dub" || is_voiceover;
    let keep_music = proj.audio.keep_music;

    // ── АУДИО ──────────────────────────────────────────────────────────────────
    // Готовим финальную аудио-дорожку new_audio: dub (клон) поверх инструментала, либо оригинал.
    bench.stage("dub_audio");
    let mut shortened: Option<Project> = None;
    let new_audio: PathBuf = if is_dub {
        let (audio, updated) = build_dub(proj, paths, total, keep_music, is_voiceover, regen_dub, progress,
        )?;
        shortened = updated;
        audio
    } else {
        // nodub/transcribe: оставляем оригинальную дорожку — mux возьмёт её из исходного видео.
        emit(progress, "mix", "nodub: оригинальная аудиодорожка");
        paths.input.clone()
    };
    let proj: &Project = shortened.as_ref().unwrap_or(proj);

    // ── АУДИО-РЕЖИМ (вход без видео) ────────────────────────────────────────────
    // Нет видеокадра (vw/vh<=0) -> результат = сведённый дубляж как WAV. Ни бёрна, ни титров, ни mux
    // (нечего накладывать/муксить). Пачка WAV -> пачка озвученных WAV.
    if vw <= 0 || vh <= 0 {
        let out_wav = paths.output.with_extension("wav");
        media::to_wav(&new_audio, &out_wav)?;
        discard_mix(&new_audio, wd);
        // Прибрать stale output.mp4/.mkv от прошлого прогона: find_output отдаёт их приоритетнее wav (#116).
        for ext in ["mp4", "mkv"] {
            let stale = paths.output.with_extension(ext);
            if stale.is_file() {
                let _ = std::fs::remove_file(&stale);
            }
        }
        emit(progress, "done", &format!("готово (только аудио) -> {}", out_wav.display()),
        );
        return Ok(RenderResult { output: out_wav, project: shortened,
        });
    }

    // ── КАПШЕНЫ + BURN (только если subs.burn) ─────────────────────────────────
    // subs.burn=false -> НИКАКИХ наложений (ни субтитров, ни титров/блюра): чистое видео + новая
    // дорожка. Композируемость: дубляж/закадр без субтитров на картинке.
    // Есть ли ВООБЩЕ что накладывать? subs=none + нет титров + нет блюр-боксов (band уже исключён при
    // subs=none) -> накладывать нечего, полный ffmpeg-транскод бессмыслен (экономия времени, баг-репорт).
    let has_overlay = proj.subs.mode != "none"
        || !proj.captions.titles.is_empty()
        || !collect_blur_boxes(proj).is_empty();
    crate::jobs::check_cancelled()?;
    bench.stage("burn");
    let captioned = if proj.subs.burn && has_overlay {
        emit(progress, "build", "сборка ASS (титры + дублированные субтитры)",
        );
        let ass_path = wd.join("caps.ass");
        let sub_covers = build_ass(proj, &ass_path, Some(wd), vw, vh, total)?;
        emit(progress, "burn", "вжигание субтитров + блюр (ffmpeg + libass, NVENC)",
        );
        let mut blur_boxes = collect_blur_boxes(proj);
        blur_boxes.extend(sub_covers.iter().map(cover_to_blur)); // блюр-подложка ПОД нашим текстом
        let captioned = wd.join("captioned.mp4");
        dub_captions::burn(
            &paths.input,
            &ass_path,
            &captioned,
            &blur_boxes,
            Some((vw, vh)),
            proj.render.blur,
            true, // gpu_encode (NVENC)
            true, // gpu_decode
            proj.render.burn_cq,
            Some(&src_codec),
            proj.render.blur_sigma,
        )?;
        captioned
    } else {
        emit(progress, "burn", "субтитры/титры отключены (subs.burn=off)");
        paths.input.clone()
    };

    // ── MUX ────────────────────────────────────────────────────────────────────
    crate::jobs::check_cancelled()?;
    bench.stage("mux");
    emit(progress, "mux", "муксирование видео + аудио");
    // Экспорт с ОРИГИНАЛЬНОЙ дорожкой (#113): дубляж (default, 1-я) + оригинал (2-я). Только dub/voiceover
    // (в nodub/transcribe оригинал уже основной — вторая дорожка ни к чему). Контейнер mp4|mkv из настроек.
    // Выход — output.<container>; при ошибке мультитрек-mux — фолбэк на обычный одинодорожечный mux.
    let mut out_path = paths.output.clone();
    // voiceover тоже может нести оригинал 2-й дорожкой: 1-я = микс (перевод поверх приглушённого ориг.),
    // 2-я = ЧИСТЫЙ оригинал без перевода — своя ценность, НЕ дубль (бай-дизайн). Требует аудио в источнике.
    let two_track = is_dub && proj.audio.keep_original_track && media::has_audio(&paths.input);
    let mkv = two_track && proj.audio.container == "mkv";
    let mut muxed = false;
    if two_track {
        let container = if mkv { "mkv" } else { "mp4" };
        out_path = paths.output.with_extension(container);
        let dub_lang = media::iso639_1_to_2(&proj.tgt_lang);
        let src_code = proj
            .meta
            .extra
            .get("src_lang")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let orig_lang = media::iso639_1_to_2(src_code);
        let dub_title = format!("{} (дубляж)", lang_display(&proj.tgt_lang));
        let orig_title = format!("{} (оригинал)", lang_display(src_code));
        emit(progress, "mux", &format!("две дорожки: {dub_title} + {orig_title} -> {container}"),
        );
        match media::mux_multitrack(
            &captioned, &new_audio, &paths.input, &out_path,
            dub_lang, orig_lang, &dub_title, &orig_title,
        ) {
            Ok(()) => muxed = true,
            Err(e) => {
                emit(progress, "mux", &format!("мультитрек-mux не удался ({e}) -> одна дорожка"),
                );
                out_path = paths.output.clone();
            }
        }
    }
    if !muxed {
        if is_dub || new_audio != paths.input {
            media::mux(&captioned, &new_audio, &out_path)?;
        } else {
            // nodub/транскрипт: дубляжа нет — оригинальную дорожку КОПИРУЕМ без перекода (каналы 5.1/
            // частота/битрейт как есть). Раньше mux() форсил stereo+AAC и портил звук на ровном месте.
            media::mux_keep_audio(&captioned, &paths.input, &out_path)?;
        }
    }
    // MKV-компаньон (#116, находка [3]): WebView2 не играет Matroska -> плеер редактора мёртв. Всегда
    // держим playable output.mp4 (дубляж-дорожка, copy без перекода) РЯДОМ с output.mkv: плеер тянет mp4,
    // «Сохранить» отдаёт mkv. Успешный mkv-mux -> ремукс лёгкого mp4; иначе mp4 уже основной выход.
    let mp4_companion = paths.output.with_extension("mp4");
    if mkv && muxed {
        // Субтитры отдельными дорожками mkv (перевод и/или оригинал со своими языковыми метками).
        let timing = crate::dub_timing::DubTiming::load(wd)?;
        let src_code = proj.meta.extra.get("src_lang").and_then(|v| v.as_str()).unwrap_or("");
        let tracks = crate::subtracks::tracks(proj, timing.as_ref(), &lang_display(&proj.tgt_lang), &lang_display(src_code),
        );
        if !tracks.is_empty() {
            let names: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
            emit(progress, "mux", &format!("субтитры дорожками mkv: {}", names.join(" + ")),
            );
            crate::subtracks::add_to_mkv(&out_path, &tracks, wd, proj.subs.burn)
                .map_err(|e| format!("субтитры дорожками mkv: {e}"))?;
        }
        match media::remux_playable_mp4(&out_path, &mp4_companion) {
            Ok(()) => {} // валидный playable-компаньон рядом с output.mkv
            Err(e) => {
                // Ремукс упал -> компаньон битый/частичный ИЛИ остался stale mp4 от прошлого прогона.
                // Обязательно удалить (find_output отдаёт mp4 приоритетнее mkv -> иначе плеер получит
                // битьё/старьё вместо свежего mkv, регресс #116 находки [0][1]). VLC играет mkv напрямую.
                let _ = std::fs::remove_file(&mp4_companion);
                emit(progress, "mux", &format!("mp4-компаньон не собран ({e}) — плеер откроет mkv (VLC ок)"),
                );
            }
        }
    } else {
        // не-mkv выход: прибрать stale output.mkv от прошлого экспорта (find_output отдаёт mkv приоритетнее).
        let stale_mkv = paths.output.with_extension("mkv");
        if stale_mkv != out_path && stale_mkv.is_file() {
            let _ = std::fs::remove_file(&stale_mkv);
        }
    }

    discard_mix(&new_audio, wd);
    emit(progress, "done", &format!("готово -> {}", out_path.display()),
    );
    bench.finish(|m| emit(progress, "bench", m));
    Ok(RenderResult { output: out_path, project: shortened,
    })
}

/// Несжатые файлы микса (media::lossless_out), которые после финального кодирования больше не нужны:
/// float-стерео длинного ролика занимает сотни МБ на каждый проект.
const MIX_TEMPS: [&str; 4] = ["new_audio.wav", "orig_ducked.wav", "final_audio.wav", "gained_audio.wav",
];

/// Удалить отработавший файл микса. Исходник, дорожку дубля и всё вне каталога проекта не трогает.
fn discard_mix(path: &Path, wd: &Path) {
    let ours = path.parent() == Some(wd)
        && path.file_name().and_then(|n| n.to_str()).is_some_and(|n| MIX_TEMPS.contains(&n));
    if ours {
        let _ = std::fs::remove_file(path);
    }
}

/// Человекочитаемое имя языка для title дорожки. Нативных имён в проекте нет — берём английское имя из
/// dub_translate::WHISPER_LANGS (единый источник языков), для незнакомого/auto — код заглавными.
fn lang_display(code: &str) -> String {
    let lc = code.trim().to_lowercase();
    dub_translate::WHISPER_LANGS
        .iter()
        .find(|(k, _)| *k == lc.as_str())
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| {
            if lc.is_empty() || lc == "auto" {
                "Original".to_string()
            } else {
                code.to_uppercase()
            }
        })
}

/// Только ДУБ-АУДИО (без бёрна субтитров и mux видео): TTS+fit+timeline+mix -> work_dir/dub_audio.m4a
/// (одно кодирование AAC из несжатого микса).
/// Нужно, чтобы озвучку можно было СЛУШАТЬ в редакторе сразу после анализа, НЕ собирая финальное видео
/// (само видео на превью не нужно — кадры показывает per-frame preview). Порт _build_dub-ветки без бёрна.
/// Второе в ответе — проект с репликами, сокращёнными циклом подгонки (None — не сокращали).
pub fn dub_audio(
    proj: &Project,
    paths: &RenderPaths,
    regen_dub: bool,
    progress: &Progress,
) -> Result<(PathBuf, Option<Project>), String> {
    let wd = &paths.work_dir;
    std::fs::create_dir_all(wd).map_err(|e| e.to_string())?;
    let meta = media::probe(&paths.input)?;
    let total = if proj.meta.duration > 0.0 { proj.meta.duration } else { meta.duration };
    let (src, shortened): (PathBuf, Option<Project>) = if proj.mode == "dub" || proj.mode == "voiceover" {
        build_dub(proj, paths, total, proj.audio.keep_music, proj.mode == "voiceover", regen_dub, progress,
            )?
    } else {
        (paths.input.clone(), None) // nodub/transcribe -> оригинальная дорожка
    };
    crate::jobs::check_cancelled()?;
    let out = wd.join("dub_audio.m4a");
    // browser-playable aac/m4a: build_dub отдаёт несжатый WAV, nodub — звук оригинала.
    media::encode_preview_aac(&src, &out)?;
    discard_mix(&src, wd);
    emit(progress, "done", "дуб-аудио готово");
    Ok((out, shortened))
}

/// Нижний предел приглушения оригинала в режиме voiceover (закадровый): -40 dB ≈ почти тихо.
/// Само значение регулирует пользователь (proj.audio.voiceover_gain_db, дефолт -6 dB).
const VOICEOVER_DUCK_MIN_DB: f64 = -40.0;

/// Анти-артефактный ретрай синтеза фразы: до MAX_TTS_ATTEMPTS попыток; детект дефектов — synth_defect
/// (in-memory по сэмплам voice_clone, ~мкс, без ffmpeg-субпроцесса). Если ПОДРЯД больше CONSECUTIVE_ABORT
/// артефакт-фраз ИЛИ суммарно ретраев больше бюджета (по длине ролика) — стоп с ошибкой (стенд/рефы).
/// Лестница: 1 — дефолт; 2-3 — temp-бамп (0.9/1.2); 4-5 — АЛЬТЕРНАТИВНЫЙ реф спикера (+temp).
/// Смена рефа выбивает вырожденный гул там, где сэмплинг бессилен (QC-вывод: битые кучкуются
/// по коротким фразам с конкретными рефами).
const MAX_TTS_ATTEMPTS: usize = 5;
const CONSECUTIVE_ABORT: usize = 8;

/// Грейс после cancel(): ждём, пока отменённая генерация РЕАЛЬНО выйдет из DLL, прежде чем движок снова
/// тронут ДРУГИМ вызовом. Контракт audiocpp::Engine — single-caller (Send+Sync годен ТОЛЬКО под сериализацией
/// джоб-очереди, engine.rs). Параллельно тронуть движок, пока прошлый вызов в DLL, = гонка/порча.
const GUARD_GRACE_SECS: u64 = 20;
/// Префикс ошибки «движок застрял в DLL после cancel»: вызывающий НЕ ретраит (это был бы конкурентный FFI —
/// гонка), а ОБРЫВАЕТ рендер. Единственный воркер разблокируется (ошибка), состояние движка не портим.
const ENGINE_STUCK: &str = "ENGINE_STUCK";

/// voice_clone с ЖЁСТКИМ таймаутом. Higgs (прекомпил-DLL) СТОХАСТИЧЕСКИ виснет на редких сегментах; in-process
/// FFI не убить, но есть C-ABI `cancel()`. Гоним синтез в отдельном потоке с таймаутом. По таймауту: cancel()
/// + ЖДЁМ grace, пока поток реально ВЫЙДЕТ из DLL (нельзя трогать движок конкурентно — single-caller контракт).
/// Вышел -> движок свободен, Err(таймаут) => вызывающий ретраит. Не вышел -> Err(ENGINE_STUCK) => вызывающий
/// обрывает рендер (НЕ гоняет движок параллельно с зависшим потоком). Так воркер не блокируется навечно и
/// не ловит гонку FFI.
fn voice_clone_guarded(
    engine: &Arc<AudiocppEngine>,
    text: &str,
    ref_wav: &str,
    ref_text: Option<&str>,
    opts: &str,
    timeout: Duration,
) -> Result<(Vec<f32>, i32), String> {
    let (tx, rx) = mpsc::channel();
    let eng = engine.clone();
    let (t, rw, rt, op) = (
        text.to_string(),
        ref_wav.to_string(),
        ref_text.map(|s| s.to_string()),
        opts.to_string(),
    );
    std::thread::spawn(move || {
        let r = eng
            .voice_clone(&t, &rw, rt.as_deref(), &op)
            .map_err(|e| e.to_string());
        let _ = tx.send(r); // получателя уже нет по таймауту — send вернёт Err, не паникуем
    });
    // Ждём порциями, чтобы отмена джобы доходила до движка, не дожидаясь таймаута синтеза.
    let ctl = crate::jobs::current();
    let deadline = std::time::Instant::now() + timeout;
    let cancelled = loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left.min(Duration::from_millis(200))) {
            Ok(r) => return r,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("поток синтеза завершился без результата".to_string());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let cancelled = ctl.as_ref().is_some_and(|c| c.is_cancelled());
                if cancelled || left.is_zero() {
                    break cancelled;
                }
            }
        }
    };
    engine.cancel(); // сигнал движку остановить текущую генерацию
    // Ждём фактического выхода отменённого вызова из DLL — только тогда движок снова можно трогать.
    match rx.recv_timeout(Duration::from_secs(GUARD_GRACE_SECS)) {
        Ok(_) if cancelled => Err(crate::jobs::CANCELLED.to_string()),
        Ok(_) => Err(format!("таймаут синтеза >{}с — отменён, движок свободен", timeout.as_secs())),
        Err(_) => Err(format!(
            "{ENGINE_STUCK}: синтез не отменяется >{}с — рендер прерван (движок завис в DLL)",
            timeout.as_secs() + GUARD_GRACE_SECS
        )),
    }
}

/// Загрузить локальный Higgs (DLL + модель выбранного кванта).
fn load_higgs(paths: &RenderPaths) -> Result<Arc<AudiocppEngine>, String> {
    let e = Arc::new(AudiocppEngine::load(&paths.higgs_dll).map_err(|e| format!("загрузка Higgs DLL: {e}"))?,
    );
    e.load_model(
        &paths.higgs_model_root,
        &paths.higgs_backend,
        paths.higgs_device,
        paths.higgs_threads,
        Some(paths.higgs_quant.as_str()),
    )
    .map_err(|e| format!("Higgs load_model: {e}"))?;
    Ok(e)
}

/// Ошибка синтеза, после которой лестницу ретраев не продолжают: движок завис или джобу отменили.
fn synth_abort(e: &str) -> bool {
    e.starts_with(ENGINE_STUCK) || e == crate::jobs::CANCELLED
}

/// Детект TTS-артефакта «гудение» по сэмплам фразы (in-memory, прямо из voice_clone). Речь на клипе >0.4с
/// всегда имеет паузы (тихие кадры) и большой размах громкости; непрерывный гул — почти без пауз и с
/// плоской огибающей. Покадровый RMS (окно 25мс): silent_frac (доля кадров тише peak−30дБ = паузы) и
/// range_db (размах peak↔trough). Артефакт, если silent_frac < 5% И range_db < 14дБ. Эмпирика: 4 реальных
/// гудящих фразы → тишина 0%, размах 3-11дБ; 90 нормальных → размах в среднем 53дБ (0 ложных). Быстрее и
/// проще ffmpeg-субпроцесса: сэмплы уже в руках, один проход арифметики, ноль зависимостей.
/// Детектор дефектов синтеза v2. Пороги подобраны ПО ДАННЫМ QC-прогона (69 битых / 80 чистых,
/// ноль ложных на чистой выборке, 2026-07-17):
/// - "runaway": клип длиннее max(6с, 0.4с×символ) — модель ушла в гул до токен-капа (факт: фраза
///   2.5с → клип 40.7с; таких найдено 7+, часть с ПРАВИЛЬНЫМ началом — ASR-sim их не ловил);
/// - "обрыв": ≥4 символов, а клип < 0.045с/симв (факт: «Погнали!» за 0.36с);
/// - "тишина": пик покадрового RMS < 0.02 (минимум чистых 0.0213; провалы 0.014-0.0198 —
///   СТАРЫЙ детектор их намеренно пропускал гейтом «peak<0.0056 = не судим»);
/// - "гул": размах < 16дБ при почти нулевых паузах (уточнённый старый паттерн).
/// None ≠ гарантия чистоты: финальную правду даёт ASR-верификация (QC-пасс после синтеза).
fn synth_defect(samples: &[f32], sr: i32, tgt_chars: usize) -> Option<&'static str> {
    if sr <= 0 || samples.is_empty() {
        return None;
    }
    let srn = sr as usize;
    let dur = samples.len() as f64 / srn as f64;
    if tgt_chars >= 1 && dur > (tgt_chars as f64 * 0.4).max(6.0) {
        return Some("runaway");
    }
    if tgt_chars >= 4 && dur < tgt_chars as f64 * 0.045 {
        return Some("обрыв");
    }
    let w = srn / 40;
    if w == 0 || samples.len() < w * 4 {
        return None;
    }
    let frames = samples.len() / w;
    let mut rms: Vec<f64> = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut acc = 0.0f64;
        for i in 0..w {
            let v = samples[f * w + i] as f64;
            acc += v * v;
        }
        rms.push((acc / w as f64).sqrt());
    }
    let peak = rms.iter().cloned().fold(0.0f64, f64::max);
    if peak < 0.02 {
        return Some("тишина");
    }
    let thr = peak * 0.0316;
    let silent = rms.iter().filter(|&&r| r < thr).count() as f64 / frames as f64;
    let trough = rms.iter().cloned().filter(|&r| r > 1e-9).fold(peak, f64::min);
    let range = 20.0 * (peak / trough.max(1e-9)).log10();
    if range < 16.0 && silent < 0.03 {
        return Some("гул");
    }
    None
}

/// Похожесть ожидаемого перевода и услышанного ASR: нормализация (lowercase, ё→е, только буквы/цифры)
/// + доля общих слов от максимума. Мягкая метрика: ловим «совсем не то/тишину», не орфографию.
fn qc_similarity(expected: &str, heard: &str) -> f64 {
    let norm = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .replace('ё', "е")
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .map(|w| w.to_string())
            .collect()
    };
    let a = norm(expected);
    let b = norm(heard);
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    // Односложный обрывок («При...», «Не»): точное словное сравнение даёт ложный брак на правильном
    // синтезе («Пре-», «Ниии...») и жжёт пересинтезы. Сравниваем ПРЕФИКСЫ (3 буквы); совсем короткое
    // (≤2 букв) с пустым ASR — «сомнительно» (0.5), не брак: VAD системно молчит на клипах <0.5с,
    // а настоящую тишину ловит акустический префильтр (synth_defect).
    if a.len() == 1 && a[0].chars().count() <= 6 {
        if b.is_empty() {
            return if a[0].chars().count() <= 2 { 0.5 } else { 0.0 };
        }
        // Вой-паттерн: услышанное кратно длиннее ожидания («Ну» -> «Нуууу…», «О,» -> «ОООО…») —
        // префикс совпадает, но это артефакт, не речь.
        let total_b: usize = b.iter().map(|w| w.chars().count()).sum();
        if total_b > a[0].chars().count() * 3 + 4 {
            return 0.0;
        }
        let ap: String = a[0].chars().take(3).collect();
        return if b.iter().any(|w| w.starts_with(ap.as_str())) { 1.0 } else { 0.0 };
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let bs: std::collections::HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let hit = a.iter().filter(|w| bs.contains(w.as_str())).count();
    hit as f64 / a.len().max(b.len()) as f64
}

/// Текст i-й фразы пакета распознавания; None — фраза не распознана, причина добавлена в `failed`.
fn heard_text<'a>(heard: &'a [Result<String, dub_asr::AsrError>], i: usize, failed: &mut Vec<String>,
) -> Option<&'a str> {
    match heard.get(i) {
        Some(Ok(text)) => Some(text.as_str()),
        Some(Err(e)) => {
            failed.push(e.to_string());
            None
        }
        None => {
            failed.push("распознавание не вернуло ответ".to_string());
            None
        }
    }
}

/// Динамический размах фразы (дБ, peak↔trough покадрового RMS 25мс) — скалярный «скор чистоты» для
/// выбора наименее плохой попытки, когда ВСЕ ретраи с артефактом: речь ~53дБ, гул 3-11дБ (та же
/// эмпирика, что у гул-ветки synth_defect). Невалидный/короткий клип -> 0.0 (хуже всех).
fn hum_range_db(samples: &[f32], sr: i32) -> f64 {
    if sr <= 0 {
        return 0.0;
    }
    let sr = sr as usize;
    let w = sr / 40;
    if w == 0 || samples.len() < w * 4 {
        return 0.0;
    }
    let frames = samples.len() / w;
    let mut rms: Vec<f64> = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut acc = 0.0f64;
        for i in 0..w {
            let v = samples[f * w + i] as f64;
            acc += v * v;
        }
        rms.push((acc / w as f64).sqrt());
    }
    let peak = rms.iter().cloned().fold(0.0f64, f64::max);
    let trough = rms.iter().cloned().filter(|&r| r > 1e-9).fold(peak, f64::min);
    20.0 * (peak / trough.max(1e-9)).log10()
}

/// Полный аудио-конвейер дубляжа -> путь к new_audio. Порт _build_dub/_regen_dub (TTS+fit+timeline+mix).
/// С «Сокращать перевод, если фраза не влезла» первый проход, найдя такие фразы, останавливается до
/// укладки: их перевод переписывается (shorten::render_overflow) и второй проход озвучивает только их,
/// остальное берёт из кэша. Второе в ответе — проект с сокращёнными репликами.
fn build_dub(
    proj: &Project,
    paths: &RenderPaths,
    total: f64,
    keep_music: bool,
    voiceover: bool,
    regen_dub: bool,
    progress: &Progress,
) -> Result<(PathBuf, Option<Project>), String> {
    let mut engine: Option<Arc<AudiocppEngine>> = None;
    let first = PassCfg { shorten: crate::fitplan::auto_shorten_on(&paths.models_root), kept: Default::default(),
    };
    let over = match build_dub_pass(proj, paths, total, keep_music, voiceover, regen_dub, &first, &mut engine, progress,
    )? {
        DubPass::Mixed(audio) => return Ok((audio, None)),
        DubPass::Overflow(over) => over,
    };
    let log = |m: String| emit(progress, "tts", &m);
    let updated = crate::shorten::render_overflow(proj, paths, &over, &mut engine, &log)?;
    let second = PassCfg { shorten: false, kept: over.kept,
    };
    let proj2 = updated.as_ref().unwrap_or(proj);
    match build_dub_pass(proj2, paths, total, keep_music, voiceover, regen_dub, &second, &mut engine, progress,
    )? {
        DubPass::Mixed(audio) => Ok((audio, updated)),
        DubPass::Overflow(_) => {
            Err("второй проход озвучки запросил сокращение, которое в нём выключено".to_string())
        }
    }
}

/// Проход озвучки: `shorten` — остановиться до укладки, если есть не влезшие фразы; `kept` — сегменты,
/// где синтез уже провалился в этом рендере (стоит оригинал, заново не синтезируются).
struct PassCfg {
    shorten: bool,
    kept: std::collections::HashSet<String>,
}

enum DubPass {
    Mixed(PathBuf),
    Overflow(crate::shorten::Overflow),
}

#[allow(clippy::too_many_arguments)]
fn build_dub_pass(
    proj: &Project,
    paths: &RenderPaths,
    total: f64,
    keep_music: bool,
    voiceover: bool,
    regen_dub: bool,
    pass: &PassCfg,
    engine: &mut Option<Arc<AudiocppEngine>>,
    progress: &Progress,
) -> Result<DubPass, String> {
    let shown = proj;
    let tts_view = crate::tts_text::synthesis_view(proj, progress);
    let proj = &tts_view;
    let wd = &paths.work_dir;
    // Сегменты с непустым tgt (как в питоне: только строки с текстом синтезируются). Несём индекс в
    // ПОЛНОМ списке proj.segments — слот next.start считается по индексу i+1 полного списка (порт
    // pipeline.py:207: nxt = segs[i+1].start, где segs — весь транскрипт, пустые пропускаются continue,
    // но индекс i+1 идёт по полному списку).
    // Порт project.write_artifacts: HIDDEN строки исключаются целиком (нет ни дубляжа, ни субтитра);
    // keep_original — остаются (сплайсим оригинал, без TTS), даже если tgt непустой.
    let seg_hidden = |s: &dub_core::Segment| {
        s.extra.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false)
    };
    let seg_keep = |s: &dub_core::Segment| {
        s.extra.get("keep_original").and_then(|v| v.as_bool()).unwrap_or(false)
    };
    let segs: Vec<(usize, &dub_core::Segment)> = proj
        .segments
        .iter()
        .enumerate()
        .filter(|(_, s)| !seg_hidden(s) && (seg_keep(s) || !s.tgt_text.trim().is_empty()))
        .collect();
    if segs.is_empty() {
        emit(progress, "tts", "нет строк с переводом -> тишина, оригинальная дорожка",
        );
        return Ok(DubPass::Mixed(paths.input.clone()));
    }

    if media::drop_stale_separation(wd)? {
        emit(progress, "separate", "стемы посчитаны из звука прежнего извлечения — сепарация заново",
        );
    }
    let live: std::collections::HashSet<String> =
        proj.segments.iter().enumerate().map(|(fi, s)| seg_file_id(&s.id).unwrap_or_else(|| format!("i{fi}"))).collect();
    let gone = crate::takes::drop_orphans(wd, &live)?;
    if gone > 0 {
        emit(progress, "tts", &format!("истории дублей удалённых фраз убраны: {gone}"),
        );
    }
    // 1) extract 44.1k stereo.
    emit(progress, "extract_audio", "извлечение аудио (ffmpeg 44.1k stereo)",
    );
    let audio_hq = wd.join("audio_hq.wav");
    media::extract_audio(&paths.input, &audio_hq, 44100, 2)?;

    // 2) сепарация (vocals/instrumental) через dub-sep, если keep_music.
    //    voiceover не сепарирует: нужен ВЕСЬ оригинал (голос+музыка) приглушённым под переводом.
    let (vocals, instrumental): (PathBuf, Option<PathBuf>) = if keep_music && !voiceover {
        // Кэш сепарации: stems зависят ТОЛЬКО от исходного аудио, не от правок сегментов. Повторный
        // рендер / дуб-аудио (regen или удаление фразы) переиспользует посчитанные stems — сепарация
        // самый долгий аудио-шаг, гонять её на каждую мелкую правку незачем.
        let stems = wd.join("stems");
        let cached_voc = stems.join("vocals.wav");
        let cached_inst = stems.join("instrumental.wav");
        if cached_voc.is_file() && cached_inst.is_file() {
            emit(progress, "separate", "сепарация из кэша (stems уже посчитаны)",
            );
            (cached_voc, Some(cached_inst))
        } else if paths.bsroformer_cli.is_file() && paths.bsroformer_model.is_file() {
            emit(progress, "separate", "сепарация (Mel-Band Roformer voc_fv6-Q8_0)",
            );
            let sep = dub_sep::separate(&audio_hq, &stems, &paths.bsroformer_cli, &paths.bsroformer_model,
            )
                .map_err(|e| format!("сепарация: {e}"))?;
            media::mark_separation(&stems)?;
            (sep.vocals, Some(sep.instrumental))
        } else {
            emit(progress, "separate", "движок сепарации не найден -> без фона (keep_music off)",
            );
            (audio_hq.clone(), None)
        }
    } else {
        (audio_hq.clone(), None)
    };
    // Рефы клона режутся из вокала в полной полосе, кто бы его ни посчитал (сепарация этого рендера или
    // анализа): voiceover и рендер без фона сами не сепарируют, их vocals — микс. vocals остаётся дорожкой
    // оригинальных реплик и микса.
    let (ref_src, ref_from_mix) = ref_source(wd, &audio_hq);

    // 3) клон-референс. voice.mode="voice" -> реф из пака/записи (voices/<name>.wav|mp3) НА КАЖДОГО спикера.
    //    Имя — CSV; позиция = отсортированный спикер (как во фронте: speaker ?? "0", лексикографически),
    //    пустой слот берёт первое непустое имя. Иначе (clone) — длиннейшая реплика спикера из вокала.
    //    Слот "-" (CLONE_SLOT, авто-распределение #114) — этот спикер остаётся на КЛОНИРОВАНИИ: пак-реф
    //    не строим, его identity-реф добавляется ниже из вокала (spk_refs). Существующие CSV без "-"
    //    ведут себя как раньше.
    let mut clone_slot_spks: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pack_names: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let pack_refs: std::collections::BTreeMap<String, PathBuf> = if proj.audio.voice.mode == "voice" {
        let names: Vec<&str> = proj.audio.voice.name.as_deref().unwrap_or("").split(',').map(|s| s.trim()).collect();
        let first_named = names
            .iter()
            .copied()
            .find(|n| !n.is_empty() && *n != crate::voice_slots::CLONE_SLOT);
        let mut map = std::collections::BTreeMap::new();
        if first_named.is_some() {
            let sorted: Vec<String> = proj
                .segments
                .iter()
                .map(|s| s.speaker.clone().unwrap_or_else(|| "0".to_string()))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            for (i, spk) in sorted.iter().enumerate() {
                let nm = names.get(i).copied().filter(|s| !s.is_empty()).or(first_named).unwrap_or("");
                pack_names.insert(spk.clone(), nm.to_string());
                if nm == crate::voice_slots::CLONE_SLOT {
                    clone_slot_spks.insert(spk.clone()); // спикер на клоне — identity-реф ниже
                    continue;
                }
                let src = ["wav", "mp3"].iter().map(|e| paths.voices_dir.join(format!("{nm}.{e}"))).find(|p| p.is_file());
                if let Some(src) = src {
                    let out = wd.join(format!("ref_pack_{i}.wav"));
                    // реф КАПИТСЯ до paths.ref_secs (дефолт 12с; на слабой RAM юзер уменьшает в настройках —
                    // длинный реф раздувает prefill-граф Higgs -> OOM на 32ГБ).
                    if media::trim_ref(&src, &out, 0.0, paths.ref_secs).is_ok() {
                        map.insert(spk.clone(), out);
                    }
                }
            }
        }
        map
    } else {
        std::collections::BTreeMap::new()
    };
    let use_pack = !pack_refs.is_empty();
    // Облачный TTS (OpenRouter) вместо локального Higgs: клон-рефы ему не нужны.
    let cloud_tts_on = crate::models::cloud_tts_on(&paths.models_root);
    if mix_ref_noted(ref_from_mix, cloud_tts_on, !use_pack || !clone_slot_spks.is_empty(),
    ) {
        emit(progress, "tts", "реф клона из микса без сепарации: в нём звучит и фон оригинала",
        );
    }
    // ref_texts: расшифровка реф-клипа НА СПИКЕРА (Higgs клонирует качественнее с ref_text). Клон-режим —
    // src_text выбранного сегмента; пак-режим — АВТОТРАНСКРИПЦИЯ 12с-клипа (как Higgs build_speaker_reference;
    // пак-.txt = полный 3-мин транскрипт, к 12с не подходит). ASR best-effort: сбой -> None (не хуже прежнего).
    let (spk_refs, mut ref_texts, alt_refs) = if use_pack {
        if clone_slot_spks.is_empty() {
            (std::collections::BTreeMap::new(), std::collections::BTreeMap::new(), std::collections::BTreeMap::new(),
            )
        } else {
            // Клон-слоты "-" (#114): identity-рефы из вокала ТОЛЬКО для спикеров на клоне —
            // build_speaker_refs строит рефы по спикерам переданных сегментов, фильтруем их.
            let segs_clone: Vec<(usize, &dub_core::Segment)> = segs
                .iter()
                .filter(|(_, s)| clone_slot_spks.contains(s.speaker.as_deref().unwrap_or("0")))
                .cloned()
                .collect();
            let mut asr = crate::models::build_engine(&paths.asr);
            build_speaker_refs(&segs_clone, &ref_src, wd, paths.ref_secs, asr.as_mut(), progress,
            )?
        }
    } else {
        // Скоринг кандидатов + REF-QC (транскрипт каждого кандидата сверяется с текстом его окна,
        // брак -> следующий) — ref_text выставляется ВНУТРИ по фактически услышанному.
        let mut asr = crate::models::build_engine(&paths.asr);
        build_speaker_refs(&segs, &ref_src, wd, paths.ref_secs, asr.as_mut(), progress)?
    };
    if use_pack {
        // Реф-транскрипция выбранным движком (Parakeet/Whisper), а НЕ захардкоженным Parakeet — иначе у
        // Whisper-only юзера (без Parakeet-модели) ref_text молча не считался бы. build_engine сам решает.
        let mut asr = crate::models::build_engine(&paths.asr);
        fill_ref_texts(asr.as_mut(), &pack_refs, &mut ref_texts);
    }
    let first_ref = pack_refs
        .values()
        .next()
        .cloned()
        .or_else(|| spk_refs.values().next().cloned())
        .unwrap_or_else(|| wd.join("ref.wav"));
    let ref_of = |s: &dub_core::Segment| -> PathBuf {
        let key = s.speaker.as_deref().unwrap_or("0");
        if use_pack {
            // клон-слот "-" (#114): спикер вне pack_refs берёт свой identity-реф из вокала (spk_refs).
            return pack_refs
                .get(key)
                .or_else(|| spk_refs.get(key))
                .cloned()
                .unwrap_or_else(|| first_ref.clone());
        }
        s.speaker
            .as_deref()
            .and_then(|k| spk_refs.get(k))
            .cloned()
            .unwrap_or_else(|| first_ref.clone())
    };
    let reftext_of = |s: &dub_core::Segment| -> Option<String> {
        let key = s.speaker.as_deref().unwrap_or("0");
        if let Some(t) = ref_texts.get(key) {
            return Some(t.clone());
        }
        // у спикера есть СВОЁ реф-аудио, но текста нет -> None (чужой текст к чужому аудио хуже, чем без
        // текста). Спикер без своего аудио (fit -> first_ref) берёт текст первого спикера (совпадает с ним).
        let has_own = if use_pack {
            pack_refs.contains_key(key) || spk_refs.contains_key(key) // клон-слот "-" тоже «своё» аудио
        } else {
            spk_refs.contains_key(key)
        };
        if has_own {
            None
        } else {
            ref_texts.values().next().cloned()
        }
    };

    // Per-segment ЭМОЦ-реф (BORROWINGS #2 «локальный эмоц-реф» / идея «скользящее окно для голоса»):
    // клон наследует крик/шёпот/плач оригинала В ЭТОТ момент, если реф взять из САМОГО сегмента, а не из
    // одного ровного identity-клипа на весь фильм. Гейт: (1) clone-режим (пак — фикс-голос юзера, эмоцию
    // источника не переносим); (2) реплика ЧИСТАЯ (нет оверлапа чужого спикера -> в реф не попадёт чужой
    // голос); (3) длина ≥REF_MIN_AFTER_TRIM (Higgs нужен минимум тембра). Иначе -> стабильный identity-реф
    // спикера (ref_of). Файл — свой на сегмент (по id), не конфликтует с seg_*.wav дубляжа. ref_text для
    // эмоц-рефа = src_text ЭТОГО же сегмента (совпадает с аудио по построению, перетранскрипция не нужна).
    // Порт коротких/1-спикер путей неизменен: при паке и на грязных/коротких репликах ведём себя как раньше.
    let emo_ref_on = crate::models::load_selection(&paths.models_root)
        .get("emo_ref_on")
        .and_then(|v| v.as_str())
        .map(|v| v != "0")
        .unwrap_or(true);
    let emo_enabled = emo_ref_on;
    let emo_eligible = |s: &dub_core::Segment| -> bool {
        // пак — фикс-голос юзера, эмоцию источника не переносим; короткая реплика — мало тембра;
        // оверлап чужого спикера -> не чистый эмоц-реф.
        emo_enabled
            && !use_pack
            && (s.end - s.start) >= REF_MIN_AFTER_TRIM
            && seg_is_clean(s, s.speaker.as_deref().unwrap_or("0"), &segs)
    };
    // Эмоц-реф и его ref_text: текст — ровно то, что звучит в окне (ref_window), либо None.
    let emo_ref_of = |s: &dub_core::Segment, sid: &str| -> Option<(PathBuf, Option<String>)> {
        if !emo_eligible(s) {
            return None;
        }
        let out = wd.join(format!("emoref_{sid}.wav"));
        // кап длины сверху ref_secs (не раздувать prefill-граф Higgs), как для identity-рефа.
        let cap = paths.ref_secs.min(REF_IDEAL_HI).max(REF_MIN_AFTER_TRIM);
        let (a, b, text) = ref_window(s, s.start, s.end.min(s.start + cap));
        match media::trim_ref(&ref_src, &out, a, b.max(a + 0.05)) {
            Ok(()) => Some((out, text)),
            Err(e) => {
                emit(progress, "tts", &format!("сегмент {sid}: эмоц-реф не вырезан ({e}) — identity-реф спикера"),
                );
                None
            }
        }
    };

    // 4) TTS каждый сегмент через Higgs (audiocpp). Кэш: seg_XXX.wav + ключ синтеза в seg_ckpt.json.
    crate::jobs::check_cancelled()?;
    // Облачный TTS (OpenRouter) вместо локального Higgs: тяжёлую DLL + модель НЕ грузим вовсе — в этом и
    // смысл (снять самую тяжёлую часть). engine=None; синтез идёт по облачной ветке ниже.
    if cloud_tts_on {
        emit(progress, "tts",
            &format!("TTS: {}", crate::models::tts_provider(&paths.models_root)),
        );
    }
    // Локальный Higgs грузится при первом сегменте, которому нужен синтез: продолжение, где всё уже
    // озвучено, не тратит время и VRAM на загрузку модели. Движок живёт между проходами (build_dub).

    // Автокастинг облачных голосов по полу спикера: мужскому спикеру — мужской голос, женскому — женский,
    // разным спикерам — разные. Пол — F0-замер (как в кастинге), голоса модели — динамически из API, пол
    // голоса — из спеки провайдера. Только в облачном режиме (локальный Higgs клонирует реальные рефы).
    // Пусто -> облачный TTS уйдёт на дефолтный голос настроек (or_tts_voice).
    let cloud_voice_map: std::collections::HashMap<String, String> = if cloud_tts_on {
        let mut m: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        // (1) РУЧНОЙ выбор голосов из кастинга: proj.audio.voice — CSV, позиционный по отсортированным
        // спикерам (как локальный пак-путь выше). В облачном режиме имена = ОБЛАЧНЫЕ голоса (casting_apply
        // их не валидирует против локальных voices/). "-"/пусто -> не задан, добьёт автокаст.
        if proj.audio.voice.mode == "voice" {
            let names: Vec<&str> =
                proj.audio.voice.name.as_deref().unwrap_or("").split(',').map(|s| s.trim()).collect();
            let sorted: Vec<String> = proj
                .segments
                .iter()
                .map(|s| s.speaker.clone().unwrap_or_else(|| "0".to_string()))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            for (i, spk) in sorted.iter().enumerate() {
                if let Some(nm) = names.get(i).copied() {
                    if !nm.is_empty() && nm != crate::voice_slots::CLONE_SLOT {
                        m.insert(spk.clone(), nm.to_string());
                    }
                }
            }
        }
        // (2) АВТОКАСТ по полу спикера добивает тех, кому голос вручную не назначен.
        if crate::models::openrouter_autocast(&paths.models_root) {
            let mut spk_ids: Vec<String> = proj
                .segments
                .iter()
                .map(|s| s.speaker.clone().unwrap_or_else(|| "0".into()))
                .collect();
            spk_ids.sort();
            spk_ids.dedup();
            let genders = crate::casting::speaker_genders_wd(&paths.work_dir, proj, &spk_ids);
            for (spk, v) in crate::cloud_voices::assign(&paths.models_root, &genders, &proj.tgt_lang) {
                m.entry(spk).or_insert(v);
            }
        }
        if !m.is_empty() {
            let mut desc: Vec<String> = m.iter().map(|(k, v)| format!("{k}→{v}")).collect();
            desc.sort();
            emit(progress, "tts", &format!("облачные голоса по спикерам: {}", desc.join(", ")),
            );
        }
        m
    } else {
        std::collections::HashMap::new()
    };

    // placed = [(at, wav_path, dur)]. cursor-aware fit (как в питоне). dur мерится ЗДЕСЬ (в цикле
    // укладки) и хранится рядом — дакинг-блоки строятся из него БЕЗ повторного ffprobe (перф [22]).
    // Имя seg-файла и слот next.start — по индексу fi в ПОЛНОМ списке proj.segments.
    let mut placed: Vec<(f64, PathBuf, f64)> = Vec::with_capacity(segs.len());
    let mut cursor = 0.0f64;
    let n_all = proj.segments.len();
    // Анти-артефактные счётчики на весь ролик: consec — проблемные фразы ПОДРЯД (сброс на чистой);
    // retry_budget — суммарный лимит ретраев по длине ролика (лестница из 5 попыток длиннее старой).
    let mut consec = 0usize;
    let mut total_retries = 0usize;
    let retry_budget = ((total / 10.0).ceil() as usize).max(20);
    // Альтернативные рефы спикеров (ступени 4-5 лестницы ретраев) — из того же скоринга/REF-QC,
    // что и main-рефы (alt_refs построены выше в build_speaker_refs).
    // QC-список синтезированных в этом прогоне фраз: (fi, индекс в placed, raw-wav, tgt-текст, спикер,
    // room слота, путь fit-файла) — после цикла сверяем транскрипцией и пересинтезируем несовпавшие.
    let mut qc_list: Vec<(usize, usize, PathBuf, String, String, f64, PathBuf)> = Vec::new();
    // Телеметрия укладки (#107): сколько сегментов пришлось растягивать выше капа (rate>1.25) и общий
    // счётчик уложенных — для итоговой доли «слишком быстрого текста».
    let mut fit_total = 0usize;
    let mut fit_over_cap = 0usize;
    let mut drift_escalations = 0usize; // сегменты, где кап atempo эскалирован для догона синка (#116)
    // Обрезка тишины TTS: фраз с обрезкой, снято секунд (из них паузами), фраз, чьё ускорение вернулось в
    // кап сегмента только благодаря обрезке.
    let (mut trim_phrases, mut trim_secs, mut trim_pause_secs, mut trim_into_cap) = (0usize, 0.0f64, 0.0f64, 0usize);
    // Multi-take: генерировать 3 дубля и выбирать лучший по близости к target-длительности.
    let multitake_on = crate::models::load_selection(&paths.models_root)
        .get("multitake")
        .and_then(|v| v.as_str())
        .map(|v| v == "1")
        .unwrap_or(false);
    // Speech Rate: динамическая адаптация темпа генерации нейросети под длину текста/слота.
    let speech_rate_on = crate::models::load_selection(&paths.models_root)
        .get("speech_rate_on")
        .and_then(|v| v.as_str())
        .map(|v| v != "0")
        .unwrap_or(true);
    // Ключ синтеза каждого сегмента: совпал с записанным в seg_ckpt.json и файл на месте -> озвучка
    // переиспользуется (продолжение после сбоя, отката правки, смены голоса туда-обратно).
    let engine_tag = if cloud_tts_on {
        if crate::models::tts_provider(&paths.models_root) == "openrouter" {
            format!(
            "cloud:{}:{}",
            crate::models::tts_model(&paths.models_root),
                crate::models::openrouter_tts_voice(&paths.models_root)
            )
        } else {
            format!(
                "cloud:{}:{}:{}:{}",
                crate::models::tts_provider(&paths.models_root),
                if crate::models::google_tts_batch(&paths.models_root) {
                    "batch"
                } else {
                    "standard"
                },
                crate::models::tts_model(&paths.models_root),
            crate::models::openrouter_tts_voice(&paths.models_root)
        )
    }
    } else {
        format!(
            "higgs:{}:{}",
            paths.higgs_model_root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            paths.higgs_quant
        )
    };
    let opts_tag = format!(
        "sr{}:mt{}:emo{}:ref{}",
        u8::from(speech_rate_on),
        u8::from(multitake_on),
        u8::from(emo_enabled),
        paths.ref_secs
    );
    let ref_tags: std::cell::RefCell<std::collections::HashMap<PathBuf, String>> = Default::default();
    let key_of = |s: &dub_core::Segment| -> String {
        let spk = s.speaker.as_deref().unwrap_or("0");
        let (voice, reference) = if cloud_tts_on {
            (cloud_voice_map.get(spk).cloned().unwrap_or_default(), String::new(),
            )
        } else {
            let voice = if use_pack { pack_names.get(spk).cloned().unwrap_or_default() } else { "clone".to_string() };
            let reference = if emo_eligible(s) {
                format!("emo:{}:{}", if ref_from_mix { "mix" } else { "vocals" }, s.src_text.trim())
            } else {
                let rp = ref_of(s);
                let tag = ref_tags.borrow_mut().entry(rp.clone()).or_insert_with(|| ref_tag(&rp)).clone();
                format!("{tag}|{}", reftext_of(s).unwrap_or_default())
            };
            (voice, reference)
        };
        seg_key(s, &voice, &reference, &engine_tag, &opts_tag)
    };
    // Голос фразы для истории дублей и отчёта укладки.
    let or_voice = crate::models::openrouter_tts_voice(&paths.models_root);
    let voice_of = |s: &dub_core::Segment| -> String {
        let spk = s.speaker.as_deref().unwrap_or("0");
        let v = if cloud_tts_on {
            cloud_voice_map.get(spk).cloned().unwrap_or_else(|| or_voice.clone())
        } else if use_pack {
            pack_names.get(spk).cloned().unwrap_or_default()
        } else {
            String::new()
        };
        if v.is_empty() || v == crate::voice_slots::CLONE_SLOT { "clone".to_string() } else { v }
    };
    let cloud_params = format!(
        "cloud:{}:{}:{}", crate::models::tts_provider(&paths.models_root),
        crate::models::tts_model(&paths.models_root),
        if crate::models::google_tts_batch(&paths.models_root) {
            "batch"
        } else {
            "standard"
        }
    );
    let fit_rules = crate::fitplan::rules(&paths.models_root, paths.max_stretch);
    let sid_of = |fi: usize, s: &dub_core::Segment| seg_file_id(&s.id).unwrap_or_else(|| format!("i{fi}"));
    let keys: Vec<String> = segs
        .iter()
        .map(|&(_, s)| {
            if seg_keep(s) { SEG_ORIGINAL.to_string() } else { key_of(s) }
        })
        .collect();
    let mut ckpts = match SegCkpts::load(wd) {
        Ok(c) => c,
        Err(e) => {
            emit(progress, "tts", &format!("{e} — ключи синтеза начаты заново"),
            );
            SegCkpts::start_over(wd)?
        }
    };
    let needs = |ckpts: &SegCkpts, idx: usize| -> bool {
        let (fi, s) = segs[idx];
        let sid = sid_of(fi, s);
        let raw = wd.join(format!("seg_{sid}.wav"));
        let recorded = ckpts.get(&sid).or(s.ckpt.as_deref());
        seg_needs_synth(raw.is_file(), recorded, &keys[idx], regen_dub && s.dirty)
    };
    let failed_before = |idx: usize| pass.kept.contains(&sid_of(segs[idx].0, segs[idx].1));
    let to_synth = (0..segs.len()).filter(|&i| !seg_keep(segs[i].1) && needs(&ckpts, i) && !failed_before(i)).count();
    let synthable = segs.iter().filter(|(_, s)| !seg_keep(s)).count();
    emit(progress, "tts", &format!("синтез {to_synth} из {} сегментов", segs.len()),
    );
    if to_synth < synthable {
        crate::jobs::emit_resumed(progress, "tts", &format!("озвучка из кэша: {} сегментов", synthable - to_synth),
        );
    }

    // ПАРАЛЛЕЛЬНЫЙ ПРЕ-СИНТЕЗ облачного TTS: OpenRouter держит десятки конкурентных запросов, поэтому все
    // сегменты к синтезу гоним в N потоков (настройка or_concurrency) ДО последовательной укладки — она
    // потом просто подхватит уже готовые seg-файлы (network-latency больше не по одному). Провал сегмента ->
    // ключ не записан -> цикл ниже синтезирует его сам (ретрай/фолбэк).
    let mut batch_new: std::collections::HashSet<usize> = std::collections::HashSet::new();
    if cloud_tts_on {
        let conc = crate::models::openrouter_concurrency(&paths.models_root);
        let mut jobs = Vec::new();
        let mut job_segs: Vec<usize> = Vec::new();
        for (idx, &(_, s)) in segs.iter().enumerate() {
            if seg_keep(s) || !needs(&ckpts, idx) || failed_before(idx) {
                continue;
            }
            let tgt = s.tgt_text.trim();
            if tgt.is_empty() {
                continue;
            }
            // Закреплённый дубль этого текста или дубль с этим ключом — цикл ниже возьмёт его без синтеза.
            let sid = sid_of(segs[idx].0, s);
            let shown_tgt = shown.segments.get(segs[idx].0).map_or(tgt, |o| o.tgt_text.trim());
            if crate::takes::History::load(wd, &sid).is_ok_and(|h| h.covers(&keys[idx], shown_tgt, s.extra.get(REGEN_NONCE))) {
                continue;
            }
            let raw = wd.join(format!("seg_{}.wav", sid_of(segs[idx].0, s)));
            let voice = cloud_voice_map.get(s.speaker.as_deref().unwrap_or("0")).cloned().unwrap_or_else(|| or_voice.clone());
            jobs.push(crate::cloud_tts::Job {
                out: raw,
                key: format!("{}:{}", sid, keys[idx]),
                text: tgt.to_string(), voice,
                style: crate::cloud_tts::style(s).into(),
            });
            job_segs.push(idx);
        }
        if !jobs.is_empty()
            && (crate::models::tts_provider(&paths.models_root) == "google"
                || (jobs.len() > 1 && conc > 1))
        {
            emit(progress, "tts", &format!("облачный TTS: {} сегментов в {} параллельных потоков", jobs.len(), conc),
            );
            let done = crate::cloud_tts::synth_batch(&paths.models_root, wd, jobs, conc, progress)?;
            let mut ok = 0usize;
            for (&idx, &good) in job_segs.iter().zip(&done) {
                if good {
                    ckpts.set(&sid_of(segs[idx].0, segs[idx].1), &keys[idx])?;
                    batch_new.insert(idx);
                    ok += 1;
                }
            }
            emit(progress, "tts", &format!("облачный TTS: пре-синтез готов ({ok} сегментов)"),
            );
            crate::jobs::check_cancelled()?;
        }
    }
    // Сегменты, где синтез провалился и стоит оригинальная реплика: их ключ записывается только когда
    // дорожка собрана целиком (как раньше сброс dirty) — прерванный прогон при продолжении их повторит.
    let mut fallback_keys: Vec<(String, String)> = Vec::new();
    // Отчёт укладки на каждую запись placed (None — оригинальная реплика) и фразы этого прохода, которые
    // цикл сокращения может переписать: (индекс в placed, id реплики).
    let mut fit_recs: Vec<Option<crate::dub_timing::FitRecord>> = Vec::with_capacity(segs.len());
    let mut shorten_cands: Vec<(usize, String)> = Vec::new();
    // Запись прошлого рендера: текст клипов, озвученных до истории дублей.
    let prior_timing = crate::dub_timing::DubTiming::load(wd)?;
    // Дубль истории, озвученный в этом проходе, для каждой записи qc_list.
    let mut qc_takes: Vec<Option<u32>> = Vec::new();
    for (idx, &(fi, s)) in segs.iter().enumerate() {
        crate::jobs::check_cancelled()?;
        // Кэш-файл сегмента — ПО ЕГО ID, не по индексу fi. Кэш переиспользуется между рендерами (не-dirty
        // сегменты не ре-синтезируются). При индекс-имени удаление/перестановка сегмента сдвигает индексы —
        // и чистый сегмент подхватил бы seg_{fi}.wav ПРЕДЫДУЩЕГО жильца индекса => чужая речь/длительность =
        // ДРИФТ дубляжа (регресс кэша порта; питон синтезил заново каждый рендер). ID стабилен -> кэш привязан
        // к контенту. Слот next.start (nxt) остаётся по индексу — это про таймлайн-позицию, не про кэш.
        let sid = sid_of(fi, s);
        let raw = wd.join(format!("seg_{sid}.wav"));
        // 'оставить оригинал': вырезаем ИСХОДНУЮ речь сюда, без TTS и без atempo-подгонки (порт _build_dub keep-ветки).
        // Режем СРАЗУ в 24к моно (питон media.trim(..., sr=24000)) — timeline кладёт по sr ПЕРВОГО файла (TTS=24к),
        // без ресемпла; 44.1к-вырез играл бы не на той скорости. Без промежуточного 16к (не терять ВЧ).
        if seg_keep(s) {
            media::trim(&vocals, &raw, s.start, s.end, 24_000)?;
            ckpts.set(&sid, SEG_ORIGINAL)?;
            let at = s.start.max(cursor);
            let d = media::duration(&raw)?;
            cursor = at + d;
            placed.push((at, raw, d));
            fit_recs.push(None);
            continue;
        }
        let tgt = s.tgt_text.trim();
        // История дублей хранит показанный текст реплики (его сверяют выбор и закрепление дубля в правках
        // проекта), синтез идёт текстом для синтеза `tgt`.
        let shown_tgt = shown.segments.get(fi).map_or(tgt, |o| o.tgt_text.trim());
        let shortened = shown.segments.get(fi).is_some_and(crate::shorten::already_shortened);
        // Синтез ТОЛЬКО если нет файла или ключ синтеза (текст/спикер/голос/реф/движок/нонс regen) не
        // совпал с записанным. dirty — флаг UI; для проектов без записанных ключей решает он.
        let key = keys[idx].as_str();
        let recorded = ckpts.get(&sid).or(s.ckpt.as_deref()).map(str::to_string);
        let mut need_synth = needs(&ckpts, idx);
        // Полный провал лестницы на КОРОТКОМ сегменте -> оригинальная реплика вместо артефакта
        // (объявлен на уровне итерации: ниже гейтит и ASR-QC этого сегмента).
        let mut kept_original = false;
        let mut hist = match crate::takes::History::load(wd, &sid) {
            Ok(h) => h,
            Err(e) => {
                let aside = crate::takes::History::quarantine(wd, &sid)?;
                emit(progress, "tts", &format!("{e} — история дублей фразы {fi} отложена в {} и начата заново", aside.display()),
                );
                crate::takes::History::default()
            }
        };
        // Закреплённый дубль рендер не заменяет, пока текст реплики тот, что в нём озвучен.
        let mut pinned_key: Option<String> = None;
        if let Some(p) = hist.pinned_for(shown_tgt).cloned() {
            hist = crate::takes::History::update(wd, &sid, |h| h.restore(wd, &sid, p.n, &raw))?.0;
            if need_synth {
                emit(progress, "tts", &format!("фраза {fi}: звучит закреплённый дубль — новая озвучка его не заменяет"),
                );
            }
            need_synth = false;
            pinned_key = Some(p.key);
        } else if hist.pinned_take().is_some() {
            let (fresh, unpinned) = crate::takes::History::update(wd, &sid, |h| {
                if h.pinned_take().is_none() || h.pinned_for(shown_tgt).is_some() {
                    return Ok(false);
                }
                h.pinned = None;
                h.save(wd, &sid)?;
                Ok(true)
            })?;
            hist = fresh;
            if unpinned {
                emit(progress, "tts", &format!("фраза {fi}: закрепление дубля снято — текст реплики изменён"),
                );
            }
        }
        // Выбранный из истории дубль звучит, пока текст и нонс те, что в нём, даже если ключ синтеза с тех пор
        // сменился (голос, референс, опции): выбор пользователя не откатывается молча.
        if pinned_key.is_none() {
            if let Some(sel) = hist.selected_for(shown_tgt, s.extra.get(REGEN_NONCE)).cloned() {
                if sel.key != key {
                    hist = crate::takes::History::update(wd, &sid, |h| {
                        h.restore(wd, &sid, sel.n, &raw)
                    })?.0;
                    emit(progress, "tts", &format!("фраза {fi}: звучит выбранный дубль {} — новая озвучка его не заменяет", sel.n),
                    );
                    need_synth = false;
                    pinned_key = Some(sel.key);
                }
            }
        }
        // Клип, озвученный до истории дублей, уходит в неё, прежде чем его заменит новая озвучка.
        if need_synth && hist.takes.is_empty() && raw.is_file() {
            if let Some((rk, text)) = recorded.clone().zip(prior_text(&prior_timing, s)) {
                let meta = crate::takes::NewTake { text, key: rk, nonce: None, voice: String::new(), reference: String::new(), params: String::new(), source: "synth",
                };
                let dur = media::duration(&raw)?;
                hist = crate::takes::History::update(wd, &sid, |h| h.add(wd, &sid, &raw, meta, dur))?.0;
            }
        }
        let mut from_history = false;
        if need_synth && failed_before(idx) {
            media::trim(&vocals, &raw, s.start, s.end, 24_000)?;
            kept_original = true;
            need_synth = false;
        } else if need_synth {
            if let Some(n) = hist.by_key(key).map(|t| t.n) {
                crate::takes::History::update(wd, &sid, |h| h.restore(wd, &sid, n, &raw))?;
                need_synth = false;
                from_history = true;
            }
        }
        let mut take_params = if cloud_tts_on { cloud_params.clone() } else { String::new() };
        let mut take_ref = String::new();
        if need_synth {
            if cloud_tts_on {
            // Облачный TTS: wav-байты OpenRouter пишем ПРЯМО в seg-файл (без декода/перекодировки).
            // Голос — из автокастинга по полу спикера (пусто -> дефолт настроек). Провал -> оригинал
            // сегмента (ноль немых мест), как локальный фолбэк.
            let cv = cloud_voice_map
                .get(s.speaker.as_deref().unwrap_or("0"))
                .map(String::as_str)
                .unwrap_or("");
                if crate::models::tts_provider(&paths.models_root) == "google" {
                    return Err(format!(
                        "Google TTS segment {fi} is absent after pre-synthesis; resume the project"
                    ));
                }
                match crate::cloud_tts::synth_audio(&paths.models_root, tgt, cv,
                    crate::cloud_tts::style(s),
                ) {
                Ok(wav) => {
                    dub_core::atomic::write(&raw, &wav).map_err(|e| format!("запись облачного seg{fi}: {e}"))?;
                }
                Err(e) => {
                    emit(progress, "tts", &format!("⚠ сегмент {fi}: облачный TTS не удался ({e}) — оригинал"),
                        );
                    media::trim(&vocals, &raw, s.start, s.end, 24_000)?;
                    kept_original = true;
                }
            }
            } else {
            // Реф: сначала пробуем per-segment ЭМОЦ-реф (обрезок вокала самой этой чистой реплики ≥2.5с)
            // — клон наследует эмоцию оригинала в этот момент. Не подошёл (грязная/короткая реплика/пак) ->
            // стабильный identity-реф спикера. ref_text эмоц-рефа — текст его окна (ref_window); для
            // identity-рефа — заранее посчитанный reftext_of. Считаем ТОЛЬКО при
            // синтезе (не тратить ffmpeg-обрезку на закэшированные не-dirty сегменты).
            if engine.is_none() {
                emit(progress, "tts", "загрузка Higgs");
                *engine = Some(load_higgs(paths)?);
            }
            let (ref_wav, ref_text): (PathBuf, Option<String>) = match emo_ref_of(s, &sid) {
                Some(emo) => emo,
                None => (ref_of(s), reftext_of(s)),
            };
            // Анти-артефактный ретрай: иногда Higgs выдаёт «гудение» (непрерывный гул без речи). Детект
            // in-memory (synth_defect) по сэмплам; перегенерируем — синтез стохастичен, повтор обычно
            // даёт валидный дубль. Вариативность ретрая (BORROWINGS #17 + ревью-находка D): у Higgs
            // подтверждён рычаг сэмплинга `temperature` (PROSODY_FINDINGS §6.1); поле `seed` НЕ подтверждено
            // (DLL прекомпилена, C++ нет). Если варьировать ТОЛЬКО seed и DLL его игнорит — все 3 попытки
            // битово идентичны → тот же гул → жёсткий abort. Поэтому на ПОВТОРНОЙ попытке бампаем
            // temperature (0.9→1.2) — выше рандом сэмплинга → выход из вырожденного гула; seed добавляем
            // бонусом. Первая попытка — пустой opts = дефолт движка (питон-паритет).
            let spk_key = s.speaker.clone().unwrap_or_else(|| "0".into());
            let alt = alt_refs.get(&spk_key);
            let tgt_chars = tgt.chars().filter(|c| c.is_alphanumeric()).count();
            // КАП ТОКЕНОВ ПО ДЛИТЕЛЬНОСТИ (ENGINES_FINDINGS §1.1, issue #151): в движке НЕТ авто-капа —
            // без него короткая фраза может уйти в 40с гула до max_new_tokens=2048. Кодек 25-75 ток/с
            // (версии разнятся) — берём консервативные 75: cap = ceil(dur×75×1.5)+32, floor 64.
            // Таргет-длительность = длительность оригинальной реплики (у дубля тот же слот).
            let expected_dur = (s.end - s.start).max(0.6);
            let tok_cap: u32 = (((expected_dur * 75.0 * 1.5).ceil() as u32) + 32).clamp(64, 2048);
            
            // Динамическая адаптация темпа (Speech Rate): если текст плотный (>14 знаков/сек), понижаем
            // температуру и зажимаем повторы, выговаривая текст собранно; если редкий — повышаем.
            let rate_ratio = if speech_rate_on && expected_dur > 0.1 && tgt_chars > 0 {
                let ideal_dur = (tgt_chars as f64) / dub_core::fit::table_cps(&proj.tgt_lang);
                (ideal_dur / expected_dur).clamp(0.70, 1.40)
            } else {
                1.0
            };
            let base_temp = if rate_ratio > 1.12 { 0.18 } else if rate_ratio < 0.88 { 0.32 } else { 0.30 };
            let base_ras_rep = if rate_ratio > 1.12 { ",\"ras_win_max_num_repeat\":1" } else { "" };

            let mut attempt = 0usize;
            let mut retried = false;
            // Лучшая из ДЕФЕКТНЫХ попыток (по размаху) — на случай полного провала лестницы.
            let mut best_bad: Option<(Vec<f32>, i32, f64)> = None;
            let (samples, sr) = loop {
                // Лестница (ENGINES_FINDINGS §1.3/1.8: на КВАНТЕ temperature ВНИЗ 0.3→0.1, НЕ вверх;
                // офиц. voice-clone примеры = 0.3): 0 — temp 0.3 + кап + RAS7; 1 — жёстче RAS (repeat=1),
                // temp 0.2, seed; 2 — temp 0.1, top_p 0.9, seed; 3-4 — АЛЬТ-РЕФ спикера (0.3 / 0.15+RAS1).
                let use_alt = attempt >= 3 && alt.is_some();
                let (rw, rt): (&PathBuf, Option<&str>) = if use_alt {
                    let (p, t) = alt.unwrap();
                    (p, t.as_deref())
                } else {
                    (&ref_wav, ref_text.as_deref())
                };
                let seed = (fi as u64) * 1000 + attempt as u64;
                let opts = match attempt {
                    0 | 3 => format!(
                        "{{\"temperature\":{base_temp:.2},\"top_p\":0.95,\"top_k\":50,\"max_new_tokens\":{tok_cap},\"ras_win_len\":7{base_ras_rep},\"return_audio_in_tokens\":true}}"
                    ),
                    1 => format!(
                        "{{\"temperature\":0.20,\"top_p\":0.95,\"top_k\":50,\"max_new_tokens\":{tok_cap},\"ras_win_len\":7,\"ras_win_max_num_repeat\":1,\"return_audio_in_tokens\":true,\"seed\":{seed}}}"
                    ),
                    2 => format!(
                        "{{\"temperature\":0.10,\"top_p\":0.90,\"top_k\":50,\"max_new_tokens\":{tok_cap},\"ras_win_len\":7,\"return_audio_in_tokens\":true,\"seed\":{seed}}}"
                    ),
                    _ => format!(
                        "{{\"temperature\":0.15,\"top_p\":0.90,\"top_k\":50,\"max_new_tokens\":{tok_cap},\"ras_win_len\":7,\"ras_win_max_num_repeat\":1,\"return_audio_in_tokens\":true,\"seed\":{seed}}}"
                    ),
                };
                attempt += 1;
                take_params = opts.clone();
                take_ref = rw.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                // Таймаут ~8× длины слота (флор 45с): нормальный синтез быстрее реалтайма, зависание —
                // минуты, так что порог чисто разделяет. Ошибка/таймаут -> как дефект (ретрай стохастику
                // обычно лечит); исчерпали попытки -> ОРИГИНАЛ (сегмент дороже потерять, чем зависший рендер).
                let vc_to = Duration::from_secs((((s.end - s.start) * 8.0).ceil() as u64).max(45));
                let eng = engine.as_ref().expect("локальный Higgs (не облако)");
                let (samples, sr) = match voice_clone_guarded(eng, tgt, &rw.to_string_lossy(), rt, &opts, vc_to,
                    ) {
                    Ok(v) => v,
                    Err(e) if synth_abort(&e) => return Err(e), // движок завис в DLL или отмена — обрыв, не гоняем параллельно
                    Err(e) => {
                        retried = true;
                        total_retries += 1;
                        if attempt >= MAX_TTS_ATTEMPTS {
                            if let Some((sm, r, rng)) = best_bad.take() {
                                emit(progress, "tts", &format!(
                                    "⚠ сегмент {fi}: {MAX_TTS_ATTEMPTS} сбоев синтеза ({e}) — взята сгенерированная озвучка (размах {rng:.0} дБ)"
                                ));
                                break (sm, r);
                            } else {
                                media::trim(&vocals, &raw, s.start, s.end, 24_000)?;
                                kept_original = true;
                                emit(progress, "tts", &format!(
                                    "⚠ сегмент {fi}: {MAX_TTS_ATTEMPTS} сбоев/таймаутов синтеза ({e}) — оставлена оригинальная реплика"
                                ));
                                break (Vec::new(), 24_000);
                            }
                        }
                        emit(progress, "tts", &format!("сегмент {fi}: {e} — регенерация ({}/{})", attempt + 1, MAX_TTS_ATTEMPTS),
                            );
                        continue;
                    }
                };
                match synth_defect(&samples, sr, tgt_chars) {
                    None => break (samples, sr), // дефектов не видно — берём
                    Some(kind) => {
                        let rng = hum_range_db(&samples, sr);
                        if best_bad.as_ref().map_or(true, |(_, _, b)| rng > *b) {
                            best_bad = Some((samples, sr, rng));
                        }
                        if attempt >= MAX_TTS_ATTEMPTS {
                            // Всегда используем сгенерированный TTS-звук (даже для коротких фраз / выкриков / хоров),
                            // избегая сброса на оригинальный вокал.
                            if let Some((sm, r, rng)) = best_bad.take() {
                                emit(progress, "tts", &format!(
                                    "⚠ сегмент {fi}: все {MAX_TTS_ATTEMPTS} попыток с дефектом ({kind}) — взята сгенерированная озвучка (размах {rng:.0} дБ)"
                                ));
                                break (sm, r);
                            } else {
                                media::trim(&vocals, &raw, s.start, s.end, 24_000)?;
                                kept_original = true;
                                emit(progress, "tts", &format!(
                                    "⚠ сегмент {fi}: {MAX_TTS_ATTEMPTS} попыток без звука — подставлен оригинал"
                                ));
                                break (Vec::new(), sr);
                            }
                        }
                        retried = true;
                        total_retries += 1;
                        let via = if attempt >= 3 { "альт-реф" } else { "temp-бамп" };
                        emit(progress, "tts", &format!("сегмент {fi}: дефект синтеза ({kind}), регенерация ({via} {}/{})", attempt + 1, MAX_TTS_ATTEMPTS));
                    }
                }
            };
            if !kept_original {
                let wav = AudiocppEngine::encode_wav(&samples, sr, 1);
                dub_core::atomic::write(&raw, &wav).map_err(|e| format!("запись seg{fi}: {e}"))?;
            }
            // Много ретраев подряд/суммарно = систем. проблема (стенд/VRAM или реф-клипы) → стоп с ошибкой.
            if retried {
                consec += 1;
                if consec > CONSECUTIVE_ABORT || total_retries > retry_budget {
                    return Err(format!(
                        "TTS: слишком много артефактов-гудения (подряд {consec}, всего ретраев {total_retries}) — регенерация не помогает. Вероятно проблема со стендом (модель/VRAM) или с реф-клипами голосов. Остановлено на сегменте {fi}."
                    ));
                }
            } else {
                consec = 0; // чистая фраза сбрасывает серию
            }
            } // конец локальной (Higgs) ветки — при облаке wav уже записан выше
        }
        // Новая озвучка — в историю дублей (активной); облачный пре-синтез тоже озвучил её в этом проходе.
        // При закреплённом дубле в файле сегмента он, а не новая озвучка.
        let synthesized = pinned_key.is_none() && ((need_synth && !kept_original) || batch_new.contains(&idx));
        let mut take_n: Option<u32> = None;
        if synthesized {
            let source = if shortened { "shorten" } else { "synth" };
            let meta = crate::takes::NewTake {
                text: shown_tgt.to_string(),
                key: key.to_string(),
                nonce: s.extra.get(REGEN_NONCE).cloned(),
                voice: voice_of(s),
                reference: take_ref.clone(),
                params: take_params.clone(),
                source,
            };
            let dur = media::duration(&raw)?;
            take_n = Some(crate::takes::History::update(wd, &sid, |h| h.add(wd, &sid, &raw, meta, dur))?.1,
            );
        }
        // слот: от текущего onset до старта СЛЕДУЮЩЕГО сегмента ПО ИНДЕКСУ (fi+1) полного списка /
        // конца видео (питон nxt = segs[i+1].start if i+1<len else total). Целевая длительность при
        // «Динамическом темпе речи» — точные границы реплики, иначе до старта следующей; кап — штатный
        // предел ускорения слота (dub_core::fit, одна формула с прогнозом редактора).
        let at = s.start.max(cursor);
        let nxt = if fi + 1 < n_all { proj.segments[fi + 1].start } else { total };
        let slot = dub_core::fit::slot(s.start, s.end, at, nxt, &fit_rules);
        let target_slot = slot.target;
        let fitp = wd.join(format!("seg_{:03}_fit.wav", fi));

        // ── MULTI-TAKE: генерируем 2 доп. дубля и выбираем лучший по близости к target-длительности ──
        // Все годные дубли остаются в истории, активным становится ближайший к слоту.
        if multitake_on && need_synth && !kept_original && !cloud_tts_on && engine.is_some() {
            let target = target_slot;
            // Дубли сравниваются по длине, которая ляжет на таймлайн: после обрезки тишины под этот слот.
            let placed_secs = |x: &[f32], sr: u32| {
                crate::tts_trim::tighten(x, sr, target * FIT_NOOP_HI).samples.len() as f64 / sr.max(1) as f64
            };
            let raw_dur = {
                let (x, sr) = wavio::read_mono_f32(&raw)?;
                placed_secs(&x, sr)
            };
            let mut best = take_n;
            let mut best_score = (raw_dur - target).abs();
            let tgt = s.tgt_text.trim();
            let (ref_wav_mt, ref_text_mt) = match emo_ref_of(s, &sid) {
                Some(emo) => emo,
                None => (ref_of(s), reftext_of(s)),
            };
            let tok_cap: u32 = ((((s.end - s.start).max(0.6) * 75.0 * 1.5).ceil() as u32) + 32).clamp(64, 2048);
            for take_i in 1..=2u64 {
                crate::jobs::check_cancelled()?;
                let take_path = wd.join(format!("seg_{sid}_take{take_i}.wav"));
                let seed = (fi as u64) * 10000 + take_i * 100 + 77;
                let temp = if take_i == 1 { 0.25 } else { 0.35 };
                let opts = format!(
                    "{{\"temperature\":{temp:.2},\"top_p\":0.95,\"top_k\":50,\"max_new_tokens\":{tok_cap},\"ras_win_len\":7,\"return_audio_in_tokens\":true,\"seed\":{seed}}}"
                );
                let rt_mt = ref_text_mt.as_deref();
                let vc_to = Duration::from_secs((((s.end - s.start) * 8.0).ceil() as u64).max(45));
                let eng = engine.as_ref().unwrap();
                match voice_clone_guarded(eng, tgt, &ref_wav_mt.to_string_lossy(), rt_mt, &opts, vc_to,
                ) {
                    Ok((samples, sr)) => {
                        if synth_defect(&samples, sr, tgt.chars().filter(|c| c.is_alphanumeric()).count(),
                        ).is_none() {
                            let wav = AudiocppEngine::encode_wav(&samples, sr, 1);
                            dub_core::atomic::write(&take_path, &wav)?;
                            let td = media::duration(&take_path)?;
                            let meta = crate::takes::NewTake {
                                text: shown_tgt.to_string(),
                                key: key.to_string(),
                                nonce: s.extra.get(REGEN_NONCE).cloned(),
                                voice: voice_of(s),
                                reference: ref_wav_mt.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                                params: opts.clone(),
                                source: "multitake",
                            };
                            let n = crate::takes::History::update(wd, &sid, |h| {
                                h.add(wd, &sid, &take_path, meta, td)
                            })?.1;
                            std::fs::remove_file(&take_path).map_err(|e| format!("{}: {e}", take_path.display()))?;
                            let score = (placed_secs(&samples, sr as u32) - target).abs();
                            if score < best_score {
                                best_score = score;
                                best = Some(n);
                            }
                        }
                    }
                    Err(e) if synth_abort(&e) => return Err(e),
                    Err(_) => {} // провал дубля — пропускаем, используем имеющийся лучший
                }
            }
            if let Some(bn) = best {
                crate::takes::History::update(wd, &sid, |h| h.restore(wd, &sid, bn, &raw))?;
                if best != take_n {
                    emit(progress, "tts", &format!("сегмент {fi}: multi-take — выбран дубль ближе к слоту ({best_score:.2}с отклонение)"));
                }
                take_n = best;
            }
        }
        // Файл сегмента готов (синтез с выбранным дублем или принятый старый файл без ключа): ключ пишется
        // только теперь, чтобы обрыв посреди дублей не выдал первый дубль за готовый.
        if kept_original {
            ckpts.set(&sid, SEG_FALLBACK)?;
            fallback_keys.push((sid.clone(), key.to_string()));
        } else if let Some(pk) = &pinned_key {
            ckpts.set(&sid, pk)?;
        } else if need_synth || from_history || recorded.is_none() {
            ckpts.set(&sid, key)?;
        }

        // Дрейф-кап (#116, находка [4]): рассинхрон дороже темпа. Кап 1.25 при cursor-ripple копит сдвиг
        // на плотном диалоге — фразы всё позже. Если дубль уже отстал (cursor > s.start), эскалируем кап
        // до нужного, чтобы догнать слот (потолок 2.0), ценой временной спешки. При «Динамическом темпе
        // речи» — прямое ускорение под точный размер реплики (до 4.0x).
        let seg_cap = slot.cap;
        let drift = (cursor - s.start).max(0.0);
        // Клип TTS до подгонки темпа теряет тишину по краям, а если и так не влезает — длинные паузы.
        // Оригинальная реплика (сбой синтеза) остаётся как вырезана.
        let (clip, raw_dur, untrimmed) = if kept_original {
            let d = media::duration(&raw).unwrap_or(0.0);
            (raw.clone(), d, d)
        } else {
            let t = tighten_clip(&raw, target_slot, &wd.join(format!("seg_{fi:03}_tight.wav")),
            )?;
            if t.after < t.before {
                trim_phrases += 1;
                trim_secs += t.before - t.after;
                trim_pause_secs += t.pauses;
                if trimmed_into_cap(t.before, t.after, target_slot, seg_cap) {
                    trim_into_cap += 1;
                }
            }
            (t.path, t.after, t.before)
        };
        let needed = dub_core::fit::needed(raw_dur, target_slot);
        let eff_cap = dub_core::fit::eff_cap(seg_cap, needed, drift, &fit_rules);
        if drift > dub_core::fit::DRIFT_ESCALATE && eff_cap > seg_cap {
            drift_escalations += 1;
        }
        // Телеметрия укладки (#107): needed>eff_cap -> дубль не влезает даже с (эскалированным) капом,
        // текст пойдёт быстрее нормы. raw_dur==0 (сбой duration) -> сегмент не считаем в статистику.
        if raw_dur > 0.0 {
            fit_total += 1;
            if dub_core::fit::over(needed, eff_cap) {
                fit_over_cap += 1;
                emit(progress, "mix", &format!(
                    "сегмент {fi}: нужно растянуть x{needed:.2} (слот {target_slot:.2}с), кап x{eff_cap:.2} — текст быстрее нормы"
                ));
            }
        }
        let (fit, d) = fit_to_slot(&clip, target_slot, &fitp, eff_cap, untrimmed)?;
        cursor = at + d;
        placed.push((at, fit, d));
        fit_recs.push((!kept_original && raw_dur > 0.0).then(|| crate::dub_timing::FitRecord {
            raw: raw_dur,
            slot: target_slot,
            needed,
            cap: seg_cap,
            eff_cap,
            speaker: s.speaker.clone().unwrap_or_else(|| "0".into()),
            voice: voice_of(s),
            lang: proj.tgt_lang.clone(),
        }),
        );
        if pass.shorten && synthesized && pinned_key.is_none() && !shortened {
            shorten_cands.push((placed.len() - 1, s.id.clone()));
        }
        // В QC — только реально синтезированное в этом прогоне (кэш уже проверялся в своём прогоне).
        // kept_original (оригинальная реплика вместо неспасаемого выкрика) НЕ сверяем: там исходный
        // язык, ASR-QC счёл бы его браком и пересинтезировал обратно в артефакт.
        // Higgs-QC (ASR-сверка + пересинтез) — только для локального движка; облачный TTS артефактов-гула
        // не даёт, а его валидация покрытия идёт отдельным гейтом.
        if !cloud_tts_on && need_synth && !kept_original && !s.tgt_text.trim().is_empty() {
            qc_list.push((
                fi,
                placed.len() - 1,
                raw.clone(),
                s.tgt_text.trim().to_string(),
                s.speaker.clone().unwrap_or_else(|| "0".into()),
                target_slot,
                fitp,
            ));
            qc_takes.push(take_n);
        }
    }
    if trim_phrases > 0 {
        emit(progress, "mix", &format!(
            "обрезка тишины TTS: снято {trim_secs:.1} с у {trim_phrases} фраз (из них паузы {trim_pause_secs:.1} с); \
             ускорение ушло в кап благодаря обрезке у {trim_into_cap} фраз"
        ));
    }
    // Итоговая доля «слишком быстрого текста» (#107) + дрейф-эскалации (#116).
    if fit_total > 0 {
        let frac = 100.0 * fit_over_cap as f64 / fit_total as f64;
        let drift = if drift_escalations > 0 { format!(", догон синка на {drift_escalations}") } else { String::new() };
        emit(progress, "mix", &format!(
            "укладка: {fit_over_cap}/{fit_total} сегментов выше капа ({frac:.0}%){drift}"
        ),
        );
    }

    // ── QC: ASR-верификация синтеза (выполняется только если qc_asr="1" в настройках) ──
    let run_qc_asr = crate::models::load_selection(&paths.models_root)
        .get("qc_asr")
        .and_then(|v| v.as_str())
        .map(|v| v == "1")
        .unwrap_or(false);
    if run_qc_asr && !qc_list.is_empty() {
        emit(progress, "tts", &format!("QC: сверка {} фраз транскрипцией", qc_list.len()),
        );
        let mut qc_asr = crate::models::build_engine(&paths.asr);
        let files: Vec<PathBuf> = qc_list.iter().map(|q| q.2.clone()).collect();
        let heard = qc_asr.transcribe_many(&files, &proj.tgt_lang);
        // Сходство QC — в дубль истории, который проверялся.
        let qc_note = |fi: usize, n: Option<u32>, sim: f64| -> Result<(), String> {
            let Some(n) = n else { return Ok(()) };
            let sid = sid_of(fi, &proj.segments[fi]);
            crate::takes::History::update(wd, &sid, |h| h.set_qc(wd, &sid, n, sim)).map(|_| ())
        };
        let mut bad_idx: Vec<usize> = Vec::new();
        let mut unheard: Vec<String> = Vec::new();
        for (i, q) in qc_list.iter().enumerate() {
            // Междометия НЕ пропускаем: вой «О,»->«ОООО…» жил именно на них (QC-скан R5b);
            // ложные капризы ASR на коротких гасит префикс-режим qc_similarity (0.5 на пустом ASR).
            let Some(h) = heard_text(&heard, i, &mut unheard) else { continue;
            };
            let sim = qc_similarity(&q.3, h);
            qc_note(q.0, qc_takes[i], sim)?;
            if sim < 0.35 {
                bad_idx.push(i);
            }
        }
        if let Some(why) = unheard.first() {
            emit(progress, "tts", &format!("QC: {} из {} фраз не сверены — распознавание не удалось: {why}", unheard.len(), qc_list.len()),
            );
        }
        if !bad_idx.is_empty() {
            emit(progress, "tts", &format!("QC: {} фраз не совпали с переводом — пересинтез", bad_idx.len()),
            );
            for &i in &bad_idx {
                crate::jobs::check_cancelled()?;
                let (fi, pidx, raw, tgtq, spk, room, fitp) = &qc_list[i];
                let s = &proj.segments[*fi];
                let main_rw = ref_of(s);
                let main_rt = reftext_of(s);
                let alt = alt_refs.get(spk);
                let tgt_chars = tgtq.chars().filter(|c| c.is_alphanumeric()).count();
                // до 3 свежих попыток (низкая temperature по ENGINES_FINDINGS §1.3 + кап токенов §1.1):
                // альт-реф 0.3 → альт-реф 0.15+RAS1 → основной 0.10 с новым seed
                let e_dur = (s.end - s.start).max(0.6);
                let cap: u32 = (((e_dur * 75.0 * 1.5).ceil() as u32) + 32).clamp(64, 2048);
                let base = format!("\"top_k\":50,\"max_new_tokens\":{cap},\"ras_win_len\":7,\"return_audio_in_tokens\":true");
                let mut fixed = false;
                for (k, (rw, rt, opts)) in {
                    let mut plan: Vec<(&PathBuf, Option<&str>, String)> = Vec::new();
                    if let Some((ap, at_)) = alt {
                        plan.push((ap, at_.as_deref(), format!("{{\"temperature\":0.30,\"top_p\":0.95,{base}}}")));
                        plan.push((ap, at_.as_deref(), format!("{{\"temperature\":0.15,\"top_p\":0.90,\"ras_win_max_num_repeat\":1,{base},\"seed\":{}}}", (*fi as u64) * 1000 + 77)));
                    }
                    plan.push((&main_rw, main_rt.as_deref(), format!("{{\"temperature\":0.10,\"top_p\":0.90,{base},\"seed\":{}}}", (*fi as u64) * 1000 + 88)));
                    plan
                }
                .into_iter()
                .enumerate()
                {
                    let vc_to = Duration::from_secs((((s.end - s.start) * 8.0).ceil() as u64).max(45));
                    let (smp, r) = match voice_clone_guarded(engine.as_ref().expect("локальный Higgs (QC не для облака)"), tgtq, &rw.to_string_lossy(), rt, &opts, vc_to) {
                        Ok(v) => v,
                        Err(e) if synth_abort(&e) => return Err(e), // движок завис или отмена — обрыв, не гоняем параллельно
                        Err(_) => continue,
                    };
                    if synth_defect(&smp, r, tgt_chars).is_some() {
                        continue;
                    }
                    let wav = AudiocppEngine::encode_wav(&smp, r, 1);
                    if dub_core::atomic::write(raw, &wav).is_ok() {
                        // пере-fit в тот же слот и подмена в placed (позиция at не меняется, длит. обновляем).
                        // Кап = потолок дрейфа (2.0): основной проход мог дрейф-капнуть этот сегмент выше
                        // seg_cap; пересинтез с seg_cap дал бы более ДЛИННЫЙ дубль и порвал синк (#116 [6]).
                        let tight = wd.join(format!("seg_{fi:03}_tight.wav"));
                        let refit = tighten_clip(raw, *room, &tight)
                            .and_then(|t| fit_to_slot(&t.path, *room, fitp, dub_core::fit::CAP_DRIFT, t.before).map(|f| (f, t.after)));
                        if let Ok(((nf, nd), placed_dur)) = refit {
                            placed[*pidx].1 = nf;
                            placed[*pidx].2 = nd;
                            fixed = true;
                            emit(progress, "tts", &format!("QC: сегмент {fi} пересинтезирован (попытка {})", k + 1));
                            let raw_dur = media::duration(raw)?;
                            let sid = sid_of(*fi, s);
                            let meta = crate::takes::NewTake {
                                text: shown.segments.get(*fi).map_or(tgtq.as_str(), |o| o.tgt_text.trim()).to_string(),
                                key: keys[segs.iter().position(|(f, _)| f == fi).expect("QC-фраза из segs")].clone(),
                                nonce: s.extra.get(REGEN_NONCE).cloned(),
                                voice: voice_of(s),
                                reference: rw.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                                params: opts.clone(),
                                source: "qc",
                            };
                            qc_takes[i] = Some(crate::takes::History::update(wd, &sid, |h| h.add(wd, &sid, raw, meta, raw_dur))?.1);
                            if let Some(rec) = fit_recs[*pidx].as_mut() {
                                rec.raw = placed_dur;
                                rec.needed = dub_core::fit::needed(placed_dur, *room);
                                rec.eff_cap = dub_core::fit::CAP_DRIFT;
                            }
                            break;
                        }
                    }
                }
                if !fixed {
                    emit(progress, "tts", &format!("⚠ QC: сегмент {fi} («{}») не удалось подтвердить — проверь фразу вручную", tgtq.chars().take(40).collect::<String>()));
                }
            }
            // финальная сверка пересинтезированных — честный отчёт в журнал
            let files2: Vec<PathBuf> = bad_idx.iter().map(|&i| qc_list[i].2.clone()).collect();
            let heard2 = qc_asr.transcribe_many(&files2, &proj.tgt_lang);
            let mut still = 0usize;
            let mut unheard2: Vec<String> = Vec::new();
            for (j, &i) in bad_idx.iter().enumerate() {
                let Some(h) = heard_text(&heard2, j, &mut unheard2) else { continue;
                };
                let sim = qc_similarity(&qc_list[i].3, h);
                qc_note(qc_list[i].0, qc_takes[i], sim)?;
                if sim < 0.35 {
                    // Отключён сброс на оригинальное аудио. Сгенерированный TTS-звук ВСЕГДА остаётся
                    // на таймлайне, даже если QC (сверка через ASR) не подтвердил совпадение текста.
                    still += 1;
                    emit(progress, "tts", &format!("⚠ QC: сегмент {} не совпадает с текстом перевода — оставлена сгенерированная озвучка", qc_list[i].0));
                }
            }
            if let Some(why) = unheard2.first() {
                emit(progress, "tts", &format!("QC: {} пересинтезированных фраз не сверены — распознавание не удалось: {why}", unheard2.len()));
            }
            emit(progress, "tts", &format!(
                "QC итог: исправлено {}/{}, осталось помеченных {}, не сверено {}",
                bad_idx.len() - still - unheard2.len(), bad_idx.len(), still, unheard2.len()
            ),
            );
        } else if unheard.is_empty() {
            emit(progress, "tts", "QC: все фразы подтверждены транскрипцией ✓",
            );
        } else {
            emit(progress, "tts", "QC: остальные фразы подтверждены транскрипцией",
            );
        }
    }

    // Замкнутая подгонка: фразы этого прохода, которым нужно ускорение сверх капа, с которым рендер их
    // подогнал (как у телеметрии «выше капа»), уходят на сокращение перевода (build_dub) — укладка и микс
    // будут во втором проходе.
    if pass.shorten {
        let picks: Vec<(String, f64)> = shorten_cands
            .iter()
            .filter_map(|(pidx, id)| {
                let r = fit_recs[*pidx].as_ref()?;
                dub_core::fit::over(r.needed, r.eff_cap).then(|| (id.clone(), r.slot))
            })
            .collect();
        if !picks.is_empty() {
            let samples = segs
                .iter()
                .zip(&fit_recs)
                .filter_map(|((_, s), r)| {
                    r.as_ref().map(|r| {
                        (r.speaker.clone(), dub_core::fit::text_units(&s.tgt_text), r.raw,
                        )
                    })
                })
                .collect();
            let kept = fallback_keys.iter().map(|(sid, _)| sid.clone()).collect();
            return Ok(DubPass::Overflow(crate::shorten::Overflow { picks, samples, kept,
            }));
        }
    }

    // 5) timeline -> dub_vocals.wav. Возвращает фактические спаны укладки.
    emit(progress, "mix", "укладка дубляжа на таймлайн");
    let dub = wd.join("dub_vocals.wav");
    let breath_on = crate::models::load_selection(&paths.models_root)
        .get("breath_on")
        .and_then(|v| v.as_str())
        .map(|v| v == "1")
        .unwrap_or(false);
    let (laid_spans, (limited_phrases, limited_samples)) = timeline(&placed, total, &dub, breath_on)?;
    if limited_phrases > 0 {
        emit(progress, "mix", &format!(
            "лимитер пиков: {limited_phrases} фраз, {limited_samples} сэмплов выше полки {VOICE_CEILING} опущены без клипа"
        ));
    }
    // Речевые блоки для дакинга (#106) — из ФАКТИЧЕСКИХ спанов timeline (единый источник: с учётом
    // cursor-ripple и QC-пересинтеза), а не из onset'ов placed.
    let mut speech_blocks = build_speech_blocks(&laid_spans);
    // HARD-гарантия: дубляж не длиннее видео (tempo-fit всей дорожки, если переполз).
    let mut dub = dub;
    let mut track_sf = 1.0f64;
    let dub_dur = media::duration(&dub)?;
    if dub_dur > total + 0.15 {
        let fit = wd.join("dub_fit.wav");
        let sf = dub_dur / total;
        media::time_stretch(&dub, &fit, sf)?;
        emit(progress, "mix", &format!("tempo-fit всей дорожки x{:.2}", sf),
        );
        dub = fit;
        track_sf = sf;
        // огибающая дакинга едет вместе с дорожкой: границы блоков делим на тот же фактор.
        for b in &mut speech_blocks {
            b.start /= sf;
            b.end /= sf;
        }
    }

    record_dub_timing(shown, paths, &segs, &placed, &fit_recs, &laid_spans, track_sf, progress,
    )?;

    // 6) свести дорожку.
    let mixed = if voiceover {
        // Закадровый (UN-style voice-over): оригинал ЗВУЧИТ ПОЛНЫМ между репликами перевода (слышно
        // исходного спикера/эмоцию) и ДИНАМИЧЕСКИ приглушается на voiceover_gain_db ПОД переводом,
        // восстанавливаясь после — best-practice (IVA/Wikipedia). Прежде оригинал давился ПЛОСКО на всю
        // дорожку (−12 дБ навсегда, в т.ч. в паузах) — «странная настройка», оригинал не поднимался.
        let duck_db = proj.audio.voiceover_gain_db.clamp(VOICEOVER_DUCK_MIN_DB, 0.0);
        emit(progress, "mix", &format!(
            "voiceover: оригинал {duck_db:+.1} dB ПОД переводом, полный в паузах (динам. огибающая, {} блоков)",
            speech_blocks.len()));
        let new_audio = wd.join("new_audio.wav");
        // Динамическая огибающая на ОРИГИНАЛ по таймингам перевода. Фолбэк — старое плоское приглушение.
        if media::mix_env_db(&dub, &audio_hq, &speech_blocks, duck_db, &new_audio).is_err() {
            emit(progress, "mix", "voiceover: огибающая недоступна -> плоское приглушение",
            );
            let bed = if duck_db.abs() < 0.05 {
                audio_hq.clone()
            } else {
                let ducked = wd.join("orig_ducked.wav");
                match media::gain(&audio_hq, &ducked, duck_db) {
                    Ok(()) => ducked,
                    Err(_) => audio_hq.clone(),
                }
            };
            media::mix(&dub, &bed, &new_audio)?;
            discard_mix(&bed, wd);
        }
        new_audio
    } else if let Some(inst) = instrumental {
        // Детерминированный дакинг (#106): фон приглушается на дефолтные −3 дБ (env DUB_DUCK_DB) по
        // кусочно-линейной ОГИБАЮЩЕЙ из точных таймингов речевых блоков — компрессор (sidechaincompress)
        // реагировал на мгновенную амплитуду TTS и давал «качели» на микропаузах внутри фраз. Требование
        // юзера: «дубляж громче фона, но фон НЕ гробить» (−12 дБ срезали весь фон). Каскад фолбэков:
        // огибающая -> sidechain -> прямой mix.
        let new_audio = wd.join("new_audio.wav");
        // Дакинг фона под дубляжом — ОПЦИЯ (duck_on), ВЫКЛ по умолчанию: не всем нужен, многим фон нужен
        // на полной громкости. Выкл -> прямой mix (фон 1:1). Вкл -> огибающая −3дБ (каскад фолбэков).
        if !crate::models::duck_enabled(&paths.models_root) {
            emit(progress, "mix", "сведение: инструментал + дубль-вокал (дакинг ВЫКЛ — фон полный)",
            );
            media::mix(&dub, &inst, &new_audio)?;
        } else {
            emit(progress, "mix", &format!("сведение: инструментал + дубль-вокал (дакинг ВКЛ, огибающая, {} блоков)", speech_blocks.len()),
            );
            if media::mix_env(&dub, &inst, &speech_blocks, &new_audio).is_err() {
                emit(progress, "mix", "огибающая недоступна -> сайдчейн-дакинг");
                if media::mix_ducked(&dub, &inst, &new_audio).is_err() {
                    emit(progress, "mix", "sidechain недоступен -> прямой mix");
                    media::mix(&dub, &inst, &new_audio)?;
                }
            }
        }
        new_audio
    } else {
        dub
    };
    // 7) финальная нормализация программы EBU R128 + true-peak лимитер (-1 dBTP). РЕШЕНИЕ ЮЗЕРА
    // (best-practice, НЕ питон — приказ 2026-07-12): пофразный normalize_voice выровнял спикеров (и
    // опустил лимитером редкие пики фразы к 0.985), здесь программа приводится к целевой громкости соцсетей
    // (-14 LUFS); финальный true-peak лимитер держит межфразовые суммы и микс с фоном.
    // Все промежуточные стадии — несжатый float WAV (media::lossless_out); единственное кодирование с
    // потерями — в mux (AAC 256k) либо превью dub_audio.m4a.
    emit(progress, "mix", "нормализация громкости (EBU R128, true-peak)",
    );
    let final_audio = wd.join("final_audio.wav");
    let normalized = match media::loudnorm(&mixed, &final_audio, -14.0, -1.0, 11.0) {
        Ok(()) => {
            discard_mix(&mixed, wd);
            final_audio
        }
        Err(e) => {
            emit(progress, "mix", &format!("loudnorm пропущен ({e})"));
            mixed
        }
    };
    for (sid, key) in &fallback_keys {
        ckpts.set(sid, key)?;
    }
    // 8) монтажный гейн всей дорожки (если задан) — наша opt-in фича «усилить всё» поверх нормализации.
    let gain_db = proj.audio.gain_db;
    if gain_db.abs() > 0.05 {
        emit(progress, "mix", &format!("гейн дорожки {gain_db:+.1} dB"));
        let gained = wd.join("gained_audio.wav");
        match media::gain(&normalized, &gained, gain_db) {
            Ok(()) => {
                discard_mix(&normalized, wd);
                Ok(DubPass::Mixed(gained))
            }
            Err(_) => Ok(DubPass::Mixed(normalized)),
        }
    } else {
        Ok(DubPass::Mixed(normalized))
    }
}

/// Записать, где каждая озвученная фраза реально звучит в финальной дорожке (dub_timing.json): субтитры
/// dub/voiceover берут отсюда тайминги событий, а пословные пресеты — слова, услышанные в самом дубле.
/// Цикл укладки кладёт в `placed` ровно одну запись на каждый элемент `segs` в том же порядке, а
/// timeline возвращает спаны в порядке `placed` (onset'ы неубывают, сортировка стабильная).
/// `segs` — фразы вида для синтеза с индексом в полном списке сегментов; `shown` — проект с показанным
/// текстом тех же сегментов в том же порядке. В запись идёт показанный текст: build_ass сверяет свежесть
/// с tgt_text проекта, а не с текстом для синтеза.
#[allow(clippy::too_many_arguments)]
fn record_dub_timing(
    shown: &Project,
    paths: &RenderPaths,
    segs: &[(usize, &dub_core::Segment)],
    placed: &[(f64, PathBuf, f64)],
    fit_recs: &[Option<crate::dub_timing::FitRecord>],
    laid_spans: &[(f64, f64)],
    track_sf: f64,
    progress: &Progress,
) -> Result<(), String> {
    let wd = &paths.work_dir;
    if placed.len() != segs.len() || laid_spans.len() != placed.len() || fit_recs.len() != placed.len() {
        crate::dub_timing::clear(wd)?;
        emit(progress, "mix", &format!(
            "тайминги дубляжа для субтитров не записаны: укладка ({} фраз, {} спанов) не сопоставилась с сегментами ({}) — субтитры по таймингам оригинала",
            placed.len(), laid_spans.len(), segs.len()
        ));
        return Ok(());
    }
    let seg_keep = |s: &dub_core::Segment| {
        s.extra.get("keep_original").and_then(|v| v.as_bool()).unwrap_or(false)
    };
    let mut laid: Vec<crate::dub_timing::Laid> = Vec::with_capacity(segs.len());
    for ((((i, s), p), span), fit) in segs.iter().zip(placed).zip(laid_spans).zip(fit_recs) {
        if seg_keep(s) || s.tgt_text.trim().is_empty() {
            continue;
        }
        let seg = shown.segments.get(*i).filter(|o| o.id == s.id).ok_or_else(|| {
            format!("тайминги дубляжа: фраза {} вида для синтеза не совпала с сегментом проекта №{i}", s.id)
        })?;
        laid.push(crate::dub_timing::Laid { seg, file: &p.1, span: *span, fit: fit.clone(),
        });
    }
    let preset = &shown.captions.preset;
    let caption_style = preset.name.as_deref().filter(|n| *n != "match");
    // Слова дубля нужны только строке перевода: в режиме «оригинал» субтитр — не то, что звучит.
    let need_words = shown.subs.burn
        && matches!(shown.subs.mode.as_str(), "translate" | "bilingual")
        && dub_captions::word_timed_reveal(caption_style, preset.plate.as_deref(), preset.reveal.as_deref(), preset.font.as_deref(),
        );
    if need_words {
        emit(progress, "mix", &format!("пословные тайминги субтитров: распознавание {} фраз дубляжа", laid.len()),
        );
    }
    let warn = |m: String| emit(progress, "mix", &m);
    let asr = need_words.then_some((&paths.asr, shown.tgt_lang.as_str()));
    crate::dub_timing::record(wd, &laid, track_sf, asr, &warn)?;
    Ok(())
}

/// Референс клона на КАЖДОГО спикера: {speaker -> ref_spk{N}.wav}.
/// Порт voices.resolve clone-ветки, УЛУЧШЕННЫЙ (BORROWINGS #2): вместо «абсолютно длиннейшей реплики»
/// (часто крик/оверлап/шумная первая) выбираем СТАБИЛЬНЫЙ identity-реф — чистая реплика 7-12с, ±1с
/// внутренняя обрезка полей, дроп шумной первой реплики у говорливого спикера. Это фолбэк-реф спикера,
/// поверх которого работает per-segment эмоц-реф (emo_ref_of). Спикер None -> ключ "0" (моно-ролик).
type SpkRefs = (
    std::collections::BTreeMap<String, PathBuf>,
    std::collections::BTreeMap<String, String>,
    // альт-рефы (ступени 4-5 лестницы): {спикер -> (wav, ref_text)} — из того же скоринга/REF-QC.
    std::collections::BTreeMap<String, (PathBuf, Option<String>)>,
);

/// Идеальная длина identity-рефа спикера: 7-12с — модель клонирует стабильнее, чем на очень коротком
/// (мало тембра) или очень длинном (крик/оверлап, раздувает prefill-граф Higgs). Верх капится ref_secs.
const REF_IDEAL_LO: f64 = 7.0;
const REF_IDEAL_HI: f64 = 12.0;
/// ±1с внутренняя обрезка полей реф-клипа: края реплики часто с придыханием/захватом соседней речи.
/// Применяем только если после обрезки остаётся достаточно (≥ ~2.5с) — иначе берём клип как есть.
const REF_EDGE_TRIM: f64 = 1.0;
const REF_MIN_AFTER_TRIM: f64 = 2.5;
/// Порог «говорливого» спикера: при >4 репликах ПЕРВАЯ (часто шумный вход/бэкграунд) исключается из
/// кандидатов. У немногословного спикера первую не трогаем — иначе можно остаться без рефа.
const REF_DROP_FIRST_ABOVE: usize = 4;
/// Гейт подсистемы voice-ref+эмоция (#81/#88). При false рендер идёт СТАРЫМ путём (identity-реф =
/// длиннейшая реплика спикера, без per-segment эмоц-рефа) = питон-паритет. Ревью-рой подтвердил 5
/// паритет-брешей при live-включении без гейта → держим OFF до E2E-валидации на реальном длинном
/// контенте + probe DLL (temperature/seed). Включить = сменить на true после валидации.
const EMO_VOICE_REF: bool = false;

/// Реплика спикера «чистая», если НЕ перекрывается по времени репликой ДРУГОГО спикера (BORROWINGS #2
/// `_adjacent_to_other_speaker`): пересечение = захват чужого голоса в реф -> грязный тембр/эмоция.
/// Реплики того же спикера не считаются загрязнением. `all` — весь транскрипт (индекс+сегмент).
fn seg_is_clean(s: &dub_core::Segment, spk: &str, all: &[(usize, &dub_core::Segment)]) -> bool {
    !all.iter().any(|(_, o)| {
        let ospk = o.speaker.clone().unwrap_or_else(|| "0".into());
        ospk != spk && o.start < s.end && o.end > s.start
    })
}

/// Выбранное окно identity-рефа спикера (координаты дорожки, с ±1с обрезкой и капом ref_secs).
struct RefWindow {
    start: f64,
    end: f64,
    /// Текст, который звучит в окне (ref_window); None — неизвестен.
    text: Option<String>,
}

/// Край урезанного окна рефа отступает от слова в паузу не дальше этого (и не дальше середины паузы).
const REF_WORD_PAD: f64 = 0.25;
/// Окно рефа после сдвига к словам не короче этого, иначе остаётся несдвинутым.
const REF_MIN_SNAPPED: f64 = 1.0;
/// Допуск границ слова относительно окна (словные тайминги ASR с шагом кадра).
const REF_WORD_EPS: f64 = 0.01;

/// Сдвинуть края окна [a,b] внутрь, в паузы между словами: начало, попавшее в слово, уходит в паузу
/// после этого слова, конец — в паузу перед своим словом. Возвращает окно и ровно его слова через
/// пробел; None — внутри нет целого слова или окно стало бы короче REF_MIN_SNAPPED.
fn snap_to_words(a: f64, b: f64, words: &[(String, f64, f64)]) -> Option<(f64, f64, String)> {
    let inside = |w: &(String, f64, f64)| w.1 >= a - REF_WORD_EPS && w.2 <= b + REF_WORD_EPS;
    let fi = words.iter().position(inside)?;
    let li = words.iter().rposition(inside)?;
    let (first, last) = (&words[fi], &words[li]);
    let na = match fi.checked_sub(1).map(|p| &words[p]) {
        Some(prev) if prev.2 > a => first.1 - ((first.1 - prev.2) / 2.0).clamp(0.0, REF_WORD_PAD),
        _ => a,
    }
    .max(a);
    let nb = match words.get(li + 1) {
        Some(next) if next.1 < b => last.2 + ((next.1 - last.2) / 2.0).clamp(0.0, REF_WORD_PAD),
        _ => b,
    }
    .min(b);
    if nb - na < REF_MIN_SNAPPED {
        return None;
    }
    let text = words[fi..=li]
        .iter()
        .map(|w| w.0.trim())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then_some((na, nb, text))
}

/// Окно рефа [a,b] из реплики `s` и текст, который в нём звучит. Окно на всю реплику — её src_text;
/// урезанное — краями в паузы между словами (snap_to_words) и ровно его слова; урезанное окно без словных
/// таймингов остаётся как есть и без текста.
fn ref_window(s: &dub_core::Segment, a: f64, b: f64) -> (f64, f64, Option<String>) {
    if a <= s.start + 1e-3 && b >= s.end - 1e-3 {
        let src = s.src_text.trim();
        return (a, b, (!src.is_empty()).then(|| src.to_string()));
    }
    match seg_words(s).and_then(|w| snap_to_words(a, b, &w)) {
        Some((na, nb, text)) => (na, nb, Some(text)),
        None => (a, b, None),
    }
}

/// Почему голос из реплики спикера не сделан. `NoSeparator` пользователь исправляет сам (ставит компонент
/// сепарации), поэтому это отказ, а не сбой; в нём пути, которых нет.
#[derive(Debug)]
pub(crate) enum VoiceClipError {
    NoSeparator(String),
    Separation(String),
    Io(String),
}

impl From<String> for VoiceClipError {
    fn from(e: String) -> Self {
        VoiceClipError::Io(e)
    }
}

/// Голос из реплики спикера `s` («Сделать голос»): окно до `cap` с (ref_window) в `out` тем же форматом,
/// что реф клона (media::trim_ref). Источник — вокал проекта в полной полосе (`wd/stems/vocals.wav`); без
/// стемов реплика вырезается из `input` в 44.1 кГц стерео и сепарируется в `tmp` движком `sep`. Голос с
/// музыкой оригинала за очищенный не выдаётся: без движка — отказ до вырезки, сбой сепарации — ошибка.
/// Возвращает текст окна.
pub(crate) fn speaker_voice_clip(
    s: &dub_core::Segment,
    cap: f64,
    wd: &Path,
    input: &Path,
    sep: (&Path, &Path),
    tmp: &Path,
    out: &Path,
) -> Result<Option<String>, VoiceClipError> {
    let (a, b, text) = ref_window(s, s.start, s.end.min(s.start + cap));
    let stem = wd.join("stems").join("vocals.wav");
    if stem.is_file() && media::stems_current(wd)? {
        media::trim_ref(&stem, out, a, b)?;
        return Ok(text);
    }
    let missing: Vec<String> = [sep.0, sep.1].iter().filter(|p| !p.is_file()).map(|p| p.display().to_string()).collect();
    if !missing.is_empty() {
        return Err(VoiceClipError::NoSeparator(missing.join(", ")));
    }
    std::fs::create_dir_all(tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let clip = tmp.join("cut44.wav");
    media::cut(input, &clip, a, b, 44_100, 2)?;
    let voc = dub_sep::separate(&clip, &tmp.join("stems"), sep.0, sep.1)
        .map_err(|e| VoiceClipError::Separation(e.to_string()))?;
    media::trim_ref(&voc.vocals, out, 0.0, b - a)?;
    Ok(text)
}

/// Выбрать окно identity-рефа спикера из его реплик по BORROWINGS #2. None — у спикера нет реплик.
fn pick_ref_window(
    spk: &str,
    segs: &[(usize, &dub_core::Segment)],
    ref_secs: f64,
) -> Option<RefWindow> {
    // реплики спикера в порядке транскрипта (для дропа первой).
    let mine: Vec<&dub_core::Segment> = segs
        .iter()
        .filter(|(_, s)| s.speaker.clone().unwrap_or_else(|| "0".into()) == spk)
        .map(|(_, s)| *s)
        .collect();
    if mine.is_empty() {
        return None;
    }
    // дроп шумной первой реплики у говорливого спикера (но всегда оставляем хоть одного кандидата).
    let pool: Vec<&dub_core::Segment> = if mine.len() > REF_DROP_FIRST_ABOVE {
        mine[1..].to_vec()
    } else {
        mine.clone()
    };
    // «полезность» кандидата (BORROWINGS #2 — НЕ «абсолютно длиннейшая», она часто крик/оверлап):
    //   1) чистота (нет оверлапа чужого спикера) — важнее всего;
    //   2) попадание В идеальную полосу 7-12с — предпочесть спокойный клип нужной длины;
    //   3) внутри полосы — длиннее лучше; ВНЕ полосы (все короче 7с) — тоже длиннее (максимум тембра),
    //      но такой кандидат всегда проигрывает любому in-band.
    // Ключ сортировки строим так, чтобы max_by брал лучший: (clean, in_band, dur_key).
    let score = |s: &dub_core::Segment| -> (bool, bool, f64) {
        let dur = (s.end - s.start).max(0.0);
        let clean = seg_is_clean(s, spk, segs);
        let in_band = dur >= REF_IDEAL_LO && dur <= REF_IDEAL_HI;
        // в полосе — ближе к верху полосы (больше тембра, но без «крик/оверлап» сверхдлинных); вне
        // полосы — просто длиннее. Отрицательное расстояние до REF_IDEAL_HI даёт «ближе к 12с = лучше».
        let dur_key = if in_band { -(REF_IDEAL_HI - dur) } else { dur };
        (clean, in_band, dur_key)
    };
    let best = pool
        .iter()
        .copied()
        .max_by(|a, b| {
            let (ca, ia, da) = score(a);
            let (cb, ib, db) = score(b);
            (ca, ia)
                .cmp(&(cb, ib))
                .then(da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal))
        })?;
    // окно = отрезок реплики. Идеал: до REF_IDEAL_HI (капается ref_secs), с ±1с обрезкой полей когда
    // после неё остаётся ≥REF_MIN_AFTER_TRIM. Короткую реплику берём целиком (парити коротких).
    let cap = ref_secs.min(REF_IDEAL_HI).max(REF_MIN_AFTER_TRIM);
    let dur = (best.end - best.start).max(0.0);
    let (mut a, mut b) = (best.start, best.end);
    if dur - 2.0 * REF_EDGE_TRIM >= REF_MIN_AFTER_TRIM {
        a += REF_EDGE_TRIM;
        b -= REF_EDGE_TRIM;
    }
    // кап длины сверху (не раздувать prefill-граф Higgs), обрезаем хвост.
    if b - a > cap {
        b = a + cap;
    }
    let (start, end, text) = ref_window(best, a, b);
    Some(RefWindow { start, end, text })
}

/// `ref_src` — дорожка, из которой режутся рефы (вокал в полной полосе, без сепарации — микс).
fn build_speaker_refs(
    segs: &[(usize, &dub_core::Segment)],
    ref_src: &Path,
    wd: &Path,
    ref_secs: f64,
    asr: &mut dyn dub_asr::AsrEngine,
    progress: &Progress,
) -> Result<SpkRefs, String> {
    let mut refs: std::collections::BTreeMap<String, PathBuf> = std::collections::BTreeMap::new();
    let mut texts: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut alts: std::collections::BTreeMap<String, (PathBuf, Option<String>)> =
        std::collections::BTreeMap::new();
    let mut speakers: Vec<String> =
        segs.iter().map(|(_, s)| s.speaker.clone().unwrap_or_else(|| "0".into())).collect();
    speakers.sort();
    speakers.dedup();
    if EMO_VOICE_REF {
        // НОВЫЙ выбор (#81): окно 7-12с, ±1с обрезка, дроп первой реплики. ТОЛЬКО под флагом.
        for spk in speakers {
            let Some(pick) = pick_ref_window(&spk, segs, ref_secs) else { continue;
            };
            let ref_wav = wd.join(format!("ref_spk{spk}.wav"));
            media::trim_ref(ref_src, &ref_wav, pick.start, pick.end.max(pick.start + 0.05),
            )?;
            refs.insert(spk.clone(), ref_wav);
            if let Some(t) = pick.text {
                texts.insert(spk, t);
            }
        }
        return Ok((refs, texts, alts));
    }

    // Скоринг кандидата в identity-рефы: ПЛОТНОСТЬ РЕЧИ (симв/с из готового транскрипта) ×
    // близость к Higgs-оптимуму 5-9с. REF-QC-факт (прогон 2026-07-17): «длиннейшая реплика»
    // выбирала мусор — 9.7с крика с одним «No!», хоровой выкрик интро, клип, где ASR слышит
    // тишину; клоны от таких рефов выли «Ааааа» вместо коротких фраз. Нормальная речь ~12-16
    // симв/с, крик/вой/шум — единицы.
    let cps = |s: &dub_core::Segment| -> f64 {
        s.src_text.trim().chars().count() as f64 / (s.end - s.start).max(0.1)
    };
    let score = |s: &dub_core::Segment| -> f64 {
        let dur = s.end - s.start;
        let cps_score = (1.0 - (cps(s) - 14.0).abs() / 14.0).clamp(0.0, 1.0);
        let dur_score = if (5.0..=9.0).contains(&dur) {
            1.0
        } else if dur < 5.0 {
            0.5 + (dur - 2.5) / 5.0
        } else {
            1.0 - (dur - 9.0) / 6.0
        };
        cps_score * dur_score.clamp(0.3, 1.0)
    };
    // Топ-3 кандидата на спикера + ОДНА пакетная транскрипция всех кандидатов (Whisper = один
    // сабпроцесс на список; Parakeet in-process и так быстр).
    let mut cand_map: std::collections::BTreeMap<String, Vec<&dub_core::Segment>> = Default::default();
    // Окно каждого кандидата: (длина, текст, который в нём звучит; без словных таймингов у урезанного
    // окна — src_text реплики, как было).
    let mut cand_win: std::collections::BTreeMap<String, Vec<(f64, String)>> = Default::default();
    let mut batch: Vec<PathBuf> = Vec::new();
    let mut batch_pos: std::collections::BTreeMap<String, usize> = Default::default();
    for spk in &speakers {
        let mine: Vec<&dub_core::Segment> = segs
            .iter()
            .filter(|(_, s)| s.speaker.clone().unwrap_or_else(|| "0".into()) == *spk)
            .map(|(_, s)| *s)
            .collect();
        let mut good: Vec<&dub_core::Segment> = mine
            .iter()
            .copied()
            .filter(|s| {
                let d = s.end - s.start;
                (2.5..=ref_secs + 0.05).contains(&d) && cps(s) >= 6.0
            })
            .collect();
        good.sort_by(|a, b| {
            score(b).partial_cmp(&score(a)).unwrap_or(std::cmp::Ordering::Equal)
        });
        good.truncate(3);
        if good.is_empty() {
            // Фолбэк (мало данных у спикера): длиннейшая влезающая, затем длиннейшая вообще.
            let by_dur = |a: &&dub_core::Segment, b: &&dub_core::Segment| {
                (a.end - a.start).partial_cmp(&(b.end - b.start)).unwrap_or(std::cmp::Ordering::Equal)
            };
            let fitting = mine
                .iter()
                .copied()
                .filter(|s| (2.5..=ref_secs + 0.05).contains(&(s.end - s.start)))
                .max_by(by_dur);
            if let Some(c) = fitting.or_else(|| mine.iter().copied().max_by(by_dur)) {
                good.push(c);
            }
        }
        batch_pos.insert(spk.clone(), batch.len());
        let mut wins: Vec<(f64, String)> = Vec::with_capacity(good.len());
        for (i, c) in good.iter().enumerate() {
            let p = wd.join(format!("ref_cand_spk{spk}_{i}.wav"));
            let (a, b, text) = ref_window(c, c.start, c.end.min(c.start + ref_secs));
            media::trim_ref(ref_src, &p, a, b.max(a + 0.05))?;
            batch.push(p);
            wins.push((b - a, text.unwrap_or_else(|| c.src_text.trim().to_string())));
        }
        cand_map.insert(spk.clone(), good);
        cand_win.insert(spk.clone(), wins);
    }
    // REF-QC: транскрипт каждого кандидата сверяем с текстом его окна — реф обязан ЗВУЧАТЬ как его
    // текст (кривой реф = кривой ref_text = каскад брака в клоне). Сбой ASR -> None -> кандидат
    // принимается без сверки (не хуже прежнего поведения).
    let heard = asr.transcribe_many(&batch, "auto");
    let unheard = heard.iter().filter(|h| h.is_err()).count();
    if let Some(Err(e)) = heard.iter().find(|h| h.is_err()) {
        emit(progress, "tts", &format!("сверка рефов: {unheard} кандидатов приняты без сверки — распознавание не удалось: {e}"));
    }
    let heard: Vec<Option<String>> = heard.into_iter().map(Result::ok).collect();
    for spk in &speakers {
        let cands = &cand_map[spk];
        if cands.is_empty() {
            continue;
        }
        let wins = &cand_win[spk];
        let base = batch_pos[spk];
        // (кандидат, услышанное, прошёл ли сверку)
        let verdict: Vec<(usize, Option<&str>, bool)> = wins
            .iter()
            .enumerate()
            .map(|(i, (_, expect))| {
                let h = heard.get(base + i).and_then(|o| o.as_deref());
                let ok = match h {
                    Some(t) => qc_similarity(expect, t) >= 0.5,
                    None => true, // не распознан (причина уже в журнале) — доверяем скорингу
                };
                (i, h, ok)
            })
            .collect();
        let passed: Vec<&(usize, Option<&str>, bool)> = verdict.iter().filter(|v| v.2).collect();
        let (main_i, main_heard) = match passed.first() {
            Some((i, h, _)) => (*i, *h),
            None => {
                let h0 = verdict[0].1.unwrap_or("");
                emit(
                    progress,
                    "tts",
                    &format!("⚠ спикер {spk}: все реф-кандидаты не прошли сверку (слышно: «{}») — беру лучший по скору", h0.chars().take(60).collect::<String>()),
                );
                (0, verdict[0].1)
            }
        };
        let ref_wav = wd.join(format!("ref_spk{spk}.wav"));
        std::fs::rename(wd.join(format!("ref_cand_spk{spk}_{main_i}.wav")), &ref_wav)
            .map_err(|e| format!("реф спикера {spk}: {e}"))?;
        refs.insert(spk.clone(), ref_wav);
        // ref_text: прошёл сверку -> УСЛЫШАННОЕ (точно соответствует звуку клипа); иначе текст окна.
        let t = main_heard
            .filter(|h| !h.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| wins[main_i].1.clone());
        if !t.is_empty() {
            texts.insert(spk.clone(), t);
        }
        // Альт-реф (ступени 4-5 лестницы ретраев): следующий ПРОШЕДШИЙ сверку кандидат; фолбэк —
        // просто следующий по скору. Спикер с одним кандидатом остаётся без альтернативы.
        let alt = passed
            .iter()
            .find(|(i, _, _)| *i != main_i)
            .map(|(i, h, _)| (*i, *h))
            .or_else(|| {
                verdict.iter().find(|(i, _, _)| *i != main_i).map(|(i, h, _)| (*i, *h))
            });
        if let Some((ai, ah)) = alt {
            let alt_wav = wd.join(format!("ref_alt_spk{spk}.wav"));
            if std::fs::rename(wd.join(format!("ref_cand_spk{spk}_{ai}.wav")), &alt_wav).is_ok() {
                let at = ah
                    .filter(|h| !h.trim().is_empty())
                    .map(str::to_string)
                    .or_else(|| Some(wins[ai].1.clone()).filter(|s| !s.is_empty()));
                alts.insert(spk.clone(), (alt_wav, at));
            }
        }
        // Прибрать невостребованных кандидатов.
        for (i, _) in cands.iter().enumerate() {
            let _ = std::fs::remove_file(wd.join(format!("ref_cand_spk{spk}_{i}.wav")));
        }
        emit(
            progress,
            "tts",
            &format!(
                "реф спикера {spk}: «{}» ({:.1}с, {} кандидата, сверка {})",
                texts.get(spk).map(|s| s.chars().take(50).collect::<String>()).unwrap_or_default(),
                wins[main_i].0,
                cands.len(),
                if passed.is_empty() { "⚠ не пройдена" } else { "ok" }
            ),
        );
    }
    Ok((refs, texts, alts))
}

/// Пауза между речевыми блоками, короче которой блоки СЛИВАЮТСЯ (музыку в коротких паузах не поднимаем).
const DUCK_BLOCK_GAP: f64 = 1.6;

/// Слить уложенные сегменты в речевые блоки для дакинг-огибающей (#106). Границы — по ФАКТУ: onset +
/// длительность fit-файла (то, что реально легло в таймлайн). Сортируем по onset, объединяем в один блок,
/// если пауза между концом предыдущего и стартом следующего < DUCK_BLOCK_GAP. Сбой чтения длительности —
/// Спаны — ФАКТИЧЕСКАЯ укладка из timeline (единый источник правды, без повторного ffprobe [22]/[5]).
fn build_speech_blocks(spans: &[(f64, f64)]) -> Vec<media::SpeechBlock> {
    let mut spans: Vec<(f64, f64)> = spans.to_vec();
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut blocks: Vec<media::SpeechBlock> = Vec::new();
    for (s, e) in spans {
        match blocks.last_mut() {
            Some(b) if s - b.end < DUCK_BLOCK_GAP => b.end = b.end.max(e),
            _ => blocks.push(media::SpeechBlock { start: s, end: e }),
        }
    }
    blocks
}

/// Ускорить или замедлить дубль под target_dur. factor>1 ускоряет (укорачивает); <1 замедляет
/// (растягивает). `cap` — потолок ускорения (считается у вызова: seg_cap + дрейф-эскалация);
/// `untrimmed` — длительность клипа до обрезки тишины (fit_factor). Возвращает путь уложенного файла И
/// его фактическую длительность.
fn fit_to_slot(
    seg_wav: &Path,
    target_dur: f64,
    work_path: &Path,
    cap: f64,
    untrimmed: f64,
) -> Result<(PathBuf, f64), String> {
    let actual = media::duration(seg_wav)?;
    let Some(factor) = fit_factor(actual, target_dur, cap, untrimmed) else {
        return Ok((seg_wav.to_path_buf(), actual.max(0.0)));
    };
    media::time_stretch(seg_wav, work_path, factor)?;
    let d = media::duration(work_path).unwrap_or(actual / factor);
    Ok((work_path.to_path_buf(), d))
}

/// Текст, который звучит в клипе реплики по записи прошлого рендера.
fn prior_text(prior: &Option<crate::dub_timing::DubTiming>, s: &dub_core::Segment,
) -> Option<String> {
    prior.as_ref()?.segments.get(&s.id).map(|t| t.text.trim().to_string()).filter(|t| !t.is_empty())
}

/// Дорожка, из которой режутся рефы клона, и true, когда это микс: вокал проекта в полной полосе, кто бы
/// его ни посчитал (сепарация этого рендера или анализа); voiceover и рендер без фона сами не
/// сепарируют, у них рефы из микса.
fn ref_source(wd: &Path, audio_hq: &Path) -> (PathBuf, bool) {
    let sep_vocals = wd.join("stems").join("vocals.wav");
    if sep_vocals.is_file() {
        (sep_vocals, false)
    } else {
        (audio_hq.to_path_buf(), true)
    }
}

/// Нужна ли строка в журнал «реф клона из микса»: реф из микса нужен только локальному клону (облачному
/// TTS рефы не нужны, голоса пака свои).
fn mix_ref_noted(from_mix: bool, cloud_tts: bool, clones: bool) -> bool {
    from_mix && !cloud_tts && clones
}

/// Ушло ли ускорение клипа в кап `cap` только благодаря обрезке тишины.
fn trimmed_into_cap(before: f64, after: f64, slot: f64, cap: f64) -> bool {
    slot > 0.05 && before / slot > cap && after / slot <= cap
}

/// Темп atempo для клипа `actual` в слоте `target`; None — клип остаётся как есть. Замедление не глубже
/// MIN_SLOW=0.85 (~15% растяжения) и не глубже, чем замедлился бы клип до обрезки тишины (`untrimmed`):
/// клип, влезавший в слот с тишиной, после обрезки кладётся короче слота, а не растягивается.
fn fit_factor(actual: f64, target: f64, cap: f64, untrimmed: f64) -> Option<f64> {
    const MIN_SLOW: f64 = 0.85;
    if target <= 0.05 || actual <= 0.05 {
        return None;
    }
    let floor = (untrimmed.max(actual) / target).clamp(MIN_SLOW, 1.0);
    let factor = (actual / target).min(cap).max(floor);
    (!(FIT_NOOP_LO..=FIT_NOOP_HI).contains(&factor)).then_some(factor)
}

/// Темп дубля в этих пределах fit_to_slot не трогает: клип до FIT_NOOP_HI × слот ускорять не нужно.
const FIT_NOOP_LO: f64 = 0.98;
const FIT_NOOP_HI: f64 = 1.02;

/// Клип TTS, подготовленный к fit_to_slot: файл, длительность до и после обрезки тишины, снято паузами.
struct TightClip {
    path: PathBuf,
    before: f64,
    after: f64,
    pauses: f64,
}

/// Снять тишину с клипа TTS под слот (tts_trim::tighten): паузы сжимаются, только если без этого клип
/// пришлось бы ускорять. Снятое пишется в `out` без потерь; seg-файл синтеза (кэш) не меняется, поэтому
/// повторный рендер получает тот же клип.
fn tighten_clip(raw: &Path, slot: f64, out: &Path) -> Result<TightClip, String> {
    let (x, sr) = wavio::read_mono_f32(raw)?;
    let before = x.len() as f64 / sr as f64;
    let t = crate::tts_trim::tighten(&x, sr, slot * FIT_NOOP_HI);
    if t.edge_cut == 0 && t.pause_cut == 0 {
        return Ok(TightClip { path: raw.to_path_buf(), before, after: before, pauses: 0.0,
        });
    }
    dub_core::atomic::write_with(out, |tmp| wavio::write_mono_f32(tmp, &t.samples, sr))?;
    Ok(TightClip {
        path: out.to_path_buf(),
        before,
        after: t.samples.len() as f64 / sr as f64,
        pauses: t.pause_cut as f64 / sr as f64,
    })
}

/// Генерирует сэмпл мягкого человеческого вдоха (процедурный легкий вдох ~0.20с).
fn generate_breath_sample(sr: u32, seed: usize) -> Vec<f32> {
    let dur_secs = 0.18 + (seed % 5) as f64 * 0.02; // 0.18 .. 0.26 сек
    let n = (dur_secs * sr as f64) as usize;
    let mut buf = Vec::with_capacity(n);
    let mut state: u32 = (seed as u32).wrapping_add(12345);
    let mut lp = 0.0f32;
    let mut hp = 0.0f32;
    for i in 0..n {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let raw_noise = ((state >> 9) as f32 / 8388608.0) - 1.0;
        lp += 0.35 * (raw_noise - lp);
        hp += 0.12 * (lp - hp);
        let band_noise = lp - hp;
        let progress = i as f32 / n as f32;
        let env = if progress < 0.35 {
            (progress / 0.35).powf(1.5)
        } else {
            ((1.0 - progress) / 0.65).powf(1.2)
        };
        buf.push(band_noise * env * 0.075);
    }
    buf
}

/// Уложить сегменты на полную дорожку по таймкодам, без перекрытия/обрезки. Порт assemble.timeline.
/// Применяет 10 мс crossfade к краям фраз для устранения кликов. При breath_on=true подставляет вдохи.
/// Возвращает фактические спаны укладки и сводку лимитера (фраз с пиками выше полки, таких сэмплов).
fn timeline(placed: &[(f64, PathBuf, f64)], total_dur: f64, out_wav: &Path, breath_on: bool,
) -> Result<TimelineOut, String> {
    if placed.is_empty() {
        // тишина total_dur @ 24000.
        let n = (total_dur * 24000.0) as usize;
        wavio::write_mono_f32(out_wav, &vec![0.0f32; n], 24000)?;
        return Ok((Vec::new(), (0, 0)));
    }
    let mut placed: Vec<(f64, PathBuf, f64)> = placed.to_vec();
    placed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    // sr берём из первого файла.
    let first = wavio::read_mono_f32(&placed[0].1)?;
    let sr = first.1;
    let mut laid: Vec<(f64, Vec<f32>)> = Vec::with_capacity(placed.len());
    let mut spans: Vec<(f64, f64)> = Vec::with_capacity(placed.len());
    let mut cursor = 0.0f64;
    let (mut limited_phrases, mut limited_samples) = (0usize, 0usize);
    for (start, wav, _) in &placed {
        let (mut s, ssr) = if *wav == placed[0].1 {
            (first.0.clone(), first.1)
        } else {
            wavio::read_mono_f32(wav)?
        };
        let over = normalize_voice(&mut s, ssr); // все фразы/спикеры к одной громкости
        if over > 0 {
            limited_phrases += 1;
            limited_samples += over;
        }

        // 10ms Crossfade (fade-in & fade-out) для бесшовного стыка без кликов
        let fade_len = ((sr as f64 * 0.010) as usize).min(s.len() / 2);
        if fade_len > 0 {
            for k in 0..fade_len {
                let f = k as f32 / fade_len as f32;
                s[k] *= f;
                let end_k = s.len() - 1 - k;
                s[end_k] *= f;
            }
        }

        let at = start.max(cursor);
        let end = at + s.len() as f64 / sr as f64;

        // Вставка дыхания в естественную паузу между фразами (0.40..1.80с)
        if breath_on && !spans.is_empty() {
            let prev_end = spans.last().unwrap().1;
            let gap = at - prev_end;
            if (0.40..=1.80).contains(&gap) {
                let b_sample = generate_breath_sample(sr, spans.len());
                let b_dur = b_sample.len() as f64 / sr as f64;
                let b_at = (at - b_dur - 0.04).max(prev_end + 0.04);
                laid.push((b_at, b_sample));
            }
        }

        cursor = end;
        spans.push((at, end));
        laid.push((at, s));
    }
    let len = ((total_dur.max(cursor) + 0.5) * sr as f64) as usize;
    let mut track = vec![0.0f32; len];
    for (at, s) in &laid {
        let i = (at * sr as f64) as usize;
        let end = (i + s.len()).min(track.len());
        for (k, v) in s.iter().take(end - i).enumerate() {
            track[i + k] += *v;
        }
    }
    // НЕ делить всю дорожку на глобальный пик: один громкий сэмпл (крик) ронял громкость ВСЕГО
    // фильма на ~12дБ (жалоба «голос тихий», замер -30.6 LUFS при фразах -18.7). Пики фраз уже
    // опущены лимитером в normalize_voice; здесь лишь страховка от сумм при наложении — локальный клип.
    for x in &mut track {
        *x = x.clamp(-VOICE_CEILING, VOICE_CEILING);
    }
    wavio::write_mono_f32(out_wav, &track, sr)?;
    Ok((spans, (limited_phrases, limited_samples)))
}

/// Выровнять ОДНУ фразу к общей громкости, чтобы все спикеры звучали одинаково громко (dialog-gated
/// нормализация): интегральная громкость BS.1770 к -14 LUFS; короткие/тихие фразы — RMS к -16 dBFS.
/// Цель поднята с -16 (замер 2026-07-17: фон мультика -17.1 LUFS, голос -16 давал зазор всего 1.1 LU —
/// «дубляж не слышно»; вместе с поджимом фона -3 дБ в миксе зазор ~6 LU = нижняя проф-норма).
/// РЕШЕНИЕ ЮЗЕРА (EBU R128 best-practice, НЕ копия питона — «гугли best practices, не повторяй за мной»,
/// приказ 2026-07-12): гейн НЕ клэмпится вниз (тихая фраза дожимается, сани-кап +40 dB от раздувания
/// почти-тишины), а редкие пики результата опускаются к полке 0.985 пофразно — иначе timeline давил всю
/// дорожку глобальным делением на пик одного крика (-12 дБ всему фильму, замер R5). Пики держит лимитер
/// с предпросмотром (limiter::SPEECH): гейн опускается до пика и возвращается за ~80 мс, без хрипа
/// жёсткого клипа. Возвращает число сэмплов, которые без лимитера легли бы выше полки.
fn normalize_voice(x: &mut [f32], sr: u32) -> usize {
    if x.is_empty() {
        return 0;
    }
    let peak = x.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    if peak < 1e-4 {
        return 0; // почти тишина -> не трогаем
    }
    let mut gain: Option<f64> = None;
    if x.len() >= (0.4 * sr as f64) as usize {
        if let Some(li) = integrated_lufs(x, sr) {
            if li.is_finite() && li > -60.0 {
                gain = Some(10f64.powf((-14.0 - li) / 20.0));
            }
        }
    }
    let gain = gain.unwrap_or_else(|| {
        let thr = peak * 0.05;
        let (mut sum, mut cnt) = (0.0f64, 0usize);
        for &v in x.iter() {
            if v.abs() > thr {
                sum += (v as f64) * (v as f64);
                cnt += 1;
            }
        }
        let rms = if cnt > 0 {
            (sum / cnt as f64).sqrt()
        } else {
            (x.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / x.len() as f64).sqrt()
        }
        .max(1e-9);
        10f64.powf(-16.0 / 20.0) / rms
    });
    let gain = gain.min(10f64.powf(40.0 / 20.0)); // сани-кап +40 dB (не раздувать почти-тишину)
    for v in x.iter_mut() {
        *v = (*v as f64 * gain) as f32;
    }
    let over = crate::limiter::limit_mono(x, sr, VOICE_CEILING as f64, &crate::limiter::SPEECH);
    // Лимитер гарантирует полку; clamp остаётся страховкой от погрешности f32 и ничего не срезает.
    for v in x.iter_mut() {
        *v = v.clamp(-VOICE_CEILING, VOICE_CEILING);
    }
    over
}

/// Спаны укладки (начало, конец) и сводка лимитера (фраз, сэмплов выше полки).
type TimelineOut = (Vec<(f64, f64)>, (usize, usize));

/// Полка пика фразы дубляжа (доля полной шкалы): запас под true-peak до финального loudnorm.
const VOICE_CEILING: f32 = 0.985;

/// Интегральная громкость ITU-R BS.1770 (LUFS) моно-сигнала: K-weighting (high-shelf + high-pass
/// биквады как в pyloudnorm) -> блоки 400мс с overlap 75% -> абсолютный гейт -70 + относительный -10.
/// None если блоков не осталось (слишком коротко/тихо).
fn integrated_lufs(x: &[f32], sr: u32) -> Option<f64> {
    let hs = biquad_high_shelf(1681.9744509555319, 0.7071752369554196, 4.0, sr as f64);
    let hp = biquad_high_pass(38.13547087613982, 0.5003270373253953, sr as f64);
    let y = apply_biquad(&apply_biquad(x, &hs), &hp);
    let block = (0.4 * sr as f64) as usize;
    let step = (0.1 * sr as f64) as usize;
    if block == 0 || step == 0 || y.len() < block {
        return None;
    }
    let mut zs: Vec<f64> = Vec::new();
    let mut i = 0;
    while i + block <= y.len() {
        let ms: f64 = y[i..i + block].iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>()
            / block as f64;
        zs.push(ms);
        i += step;
    }
    if zs.is_empty() {
        return None;
    }
    let loud = |z: f64| -0.691 + 10.0 * (z.max(1e-12)).log10();
    // абсолютный гейт -70 LUFS
    let abs_gated: Vec<f64> = zs.iter().copied().filter(|&z| loud(z) >= -70.0).collect();
    if abs_gated.is_empty() {
        return None;
    }
    let mean_abs = abs_gated.iter().sum::<f64>() / abs_gated.len() as f64;
    let rel_thr = loud(mean_abs) - 10.0;
    let rel_gated: Vec<f64> = abs_gated.into_iter().filter(|&z| loud(z) >= rel_thr).collect();
    if rel_gated.is_empty() {
        return None;
    }
    let mean_rel = rel_gated.iter().sum::<f64>() / rel_gated.len() as f64;
    Some(loud(mean_rel))
}

/// Биквад-фильтр прямой формы I (b/a нормированы на a0). Возвращает отфильтрованный сигнал.
fn apply_biquad(x: &[f32], c: &[f64; 5]) -> Vec<f32> {
    let [b0, b1, b2, a1, a2] = *c;
    let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut out = Vec::with_capacity(x.len());
    for &xn in x {
        let xn = xn as f64;
        let yn = b0 * xn + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = xn;
        y2 = y1;
        y1 = yn;
        out.push(yn as f32);
    }
    out
}

/// High-shelf биквад (pyloudnorm K-weighting stage 1). Коэффы [b0,b1,b2,a1,a2] нормированы на a0.
fn biquad_high_shelf(fc: f64, q: f64, gain_db: f64, sr: f64) -> [f64; 5] {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * fc / sr;
    let (cw, sw) = (w0.cos(), w0.sin());
    let alpha = sw / (2.0 * q);
    let am = a - 1.0;
    let ap = a + 1.0;
    let sa = 2.0 * a.sqrt() * alpha;
    let b0 = a * (ap + am * cw + sa);
    let b1 = -2.0 * a * (am + ap * cw);
    let b2 = a * (ap + am * cw - sa);
    let a0 = ap - am * cw + sa;
    let a1 = 2.0 * (am - ap * cw);
    let a2 = ap - am * cw - sa;
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

/// High-pass биквад (pyloudnorm K-weighting stage 2, RLB). Коэффы нормированы на a0.
fn biquad_high_pass(fc: f64, q: f64, sr: f64) -> [f64; 5] {
    let w0 = 2.0 * std::f64::consts::PI * fc / sr;
    let (cw, sw) = (w0.cos(), w0.sin());
    let alpha = sw / (2.0 * q);
    let b0 = (1.0 + cw) / 2.0;
    let b1 = -(1.0 + cw);
    let b2 = (1.0 + cw) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cw;
    let a2 = 1.0 - alpha;
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

/// E2E-верификация капшенов без TTS/аудио: собрать ASS из Project и вжечь блюр+сабы в captioned.mp4.
/// Используется примером verify_captions (реальные кадры порт-vs-эталон). Порт captions.build+burn
/// call-site pipeline.run, но БЕЗ аудио-ветки.
pub(crate) fn build_and_burn_captions(
    proj: &Project,
    input: &Path,
    out_ass: &Path,
    captioned: &Path,
    fonts_dir: &Path,
    vw: i64,
    vh: i64,
    total: f64,
    src_codec: &str,
) -> Result<(), String> {
    dub_captions::set_fonts_dir(fonts_dir);
    let sub_covers = build_ass(proj, out_ass, None, vw, vh, total)?;
    let mut blur_boxes = collect_blur_boxes(proj);
    blur_boxes.extend(sub_covers.iter().map(cover_to_blur));
    dub_captions::burn(
        input,
        out_ass,
        captioned,
        &blur_boxes,
        Some((vw, vh)),
        proj.render.blur,
        true,
        true,
        proj.render.burn_cq,
        Some(src_codec),
        proj.render.blur_sigma,
    )
}

/// Блюр-подложка под нашим субтитром -> BlurBox (fill=None -> gblur). Старые band-боксы не трогаем.
pub(crate) fn cover_to_blur(c: &dub_captions::SubCover) -> BlurBox {
    BlurBox { x: c.x, y: c.y, w: c.w, h: c.h, t0: c.t0, t1: c.t1, fill: None,
    }
}

/// Собрать ASS через dub-captions из Project. Порт captions.build call-site pipeline.run. Возвращает
/// габариты подложек под дублированными субтитрами (для блюр-подложки; см. SubCover).
/// `timing_dir` — каталог проекта с dub_timing.json: в режимах dub/voiceover субтитр стоит там, где
/// фраза дубляжа реально звучит, и подсвечивает слова по услышанному в дубле. None — тайминги оригинала.
pub(crate) fn build_ass(
    proj: &Project,
    out_ass: &Path,
    timing_dir: Option<&Path>,
    vw: i64,
    vh: i64,
    total: f64,
) -> Result<Vec<dub_captions::SubCover>, String> {
    let titles: Vec<CapTitle> = proj.captions.titles.iter().map(map_title).collect();
    let sub_style = proj.captions.sub_style.as_ref().map(map_sub_style);
    // sub_y дефолт vh*0.82 если не задан (как pipeline.py: не затирать edited/pinned sub_y).
    let sub_y = proj.captions.sub_y.unwrap_or((vh as f64 * 0.82) as i64);
    // PER-SEGMENT Y-RIDE (порт pipeline.py 616-631). Каждую дублированную строку кладём на y, где в этот
    // момент была ОРИГИНАЛЬНАЯ полоса сабов, чтобы наш текст/плашка НАКРЫЛИ заблюренный оригинал (а не
    // висели на одной фикс-линии, пока блюр другой строки просвечивает). Источник полосы —
    // persisted blur_boxes. Питон медианит per-segment seg_y (pipeline.py 757-765) ТОЛЬКО по caption_boxes
    // (субтитр-полоса из analyze_layout), а НЕ по всему blur-набору — титры/таглайны/group туда не входят.
    // Порт складывает всё в blur_boxes, поэтому band-подмножество помечено compose.rs маркером extra["band"]
    // (= питоновский caption_boxes-производный band_blur). seg_y едет ТОЛЬКО по нему. Без такого — верхний
    // title/tagline-блюр в нижней половине кадра затягивал медиану вверх и строка садилась мимо полосы
    // Fallback (старые проекты без
    // маркера / ручной blur из редактора): весь blur-набор, как было — иначе потеряли бы band целиком.
    let all_band: Vec<&dub_core::BlurBox> =
        proj.captions.blur_boxes.iter().filter(|b| !b.hidden).collect();
    let tagged: Vec<&dub_core::BlurBox> = all_band
        .iter()
        .copied()
        .filter(|b| {
            b.extra.get("band").and_then(|v| v.as_bool()).unwrap_or(false)
        })
        .collect();
    let band: Vec<&dub_core::BlurBox> = if tagged.is_empty() { all_band } else { tagged };
    let no_band = band.len() < 3; // нет повторяющейся ОРИГИНАЛЬНОЙ полосы -> не на что ехать
    let cap_lo = 0.40 * vw as f64;
    let cap_hi = 0.60 * vw as f64;
    let seg_y = |st: f64, en: f64| -> i64 {
        if proj.captions.sub_y_locked || no_band {
            return sub_y; // editor-pinned или полосы нет -> выбранная band
        }
        // медиана y-центров band-боксов, перекрывающих сегмент по времени, центрированных по X,
        // в нижней половине кадра (ехать на нижнюю оригинальную полосу, не на верхний оверлей).
        let mut ys: Vec<f64> = band
            .iter()
            .filter(|b| {
                let cyb = b.y as f64 + b.h as f64 / 2.0;
                (b.t0 as f64) < en + 0.3
                    && (b.t1 as f64) > st - 0.3
                    && (b.x as f64) < cap_hi
                    && (b.x as f64 + b.w as f64) > cap_lo
                    && cyb >= 0.45 * vh as f64
            })
            .map(|b| b.y as f64 + b.h as f64 / 2.0)
            .collect();
        if ys.is_empty() {
            return (vh as f64 * 0.82) as i64;
        }
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ys[ys.len() / 2] as i64
    };
    // Per-segment CaptionOverride: карта seg_id -> текст-оверрайд (редактор дал свой текст строки).
    // Порт продуктового требования «научить build_ass читать overrides»: питоновский write_artifacts
    // overrides в план НЕ прокидывает (хранит round-trip), поэтому текст-оверрайд — Rust-улучшение поверх
    // источника истины: если для сегмента задан override.text, рисуем ЕГО вместо tgt_text. Стилевые
    // per-seg поля (override.style/x/y/w/fs) сохраняются в Project, но в ASS-строку пока не вплетаются —
    // dub-captions строит субтитр из общего sub_style; это совпадает с питоном (тоже не рендерит их).
    let is_dub = proj.mode == "dub" || proj.mode == "voiceover";
    let dub_timing: Option<crate::dub_timing::DubTiming> = match (is_dub, timing_dir) {
        (true, Some(d)) => Some(crate::dub_timing::DubTiming::load(d)?.unwrap_or_default()),
        (true, None) => Some(crate::dub_timing::DubTiming::default()),
        (false, _) => None,
    };
    let overrides: std::collections::HashMap<&str, &str> = proj
        .captions
        .overrides
        .iter()
        .filter_map(|o| o.text.as_deref().map(|t| (o.seg_id.as_str(), t)))
        .collect();
    // Режим «без субтитров» (subs.mode=none) -> НЕ рисуем строки субтитров вообще. Титры/локализация
    // экранного текста живут отдельно (proj.captions.titles) и не затрагиваются. Раньше build_ass
    // рендерил сегменты безусловно -> в режиме «без субтитров» они всё равно прожигались (баг-репорт).
    let subs: Vec<Sub> = if proj.subs.mode == "none" {
        Vec::new()
    } else { proj
        .segments
        .iter()
        .filter(|s| {   // hidden -> нет субтитра; keep_original -> играет оригинал, субтитра нет (порт write_artifacts)
            !s.extra.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false)
                && !s.extra.get("keep_original").and_then(|v| v.as_bool()).unwrap_or(false)
        })
        .map(|s| {
            let tgt = match overrides.get(s.id.as_str()) {
                Some(t) => t.to_string(),
                None => s.tgt_text.clone(),
            };
            (s, tgt)
        })
        .map(|(s, tgt)| (s, crate::subs_text::lines(&proj.subs.mode, is_dub, s, &tgt)))
        .filter(|(_, l)| !l.primary.is_empty())
        .map(|(s, l)| {
            let (start, end, words) = match dub_timing.as_ref() {
                // Дубляж: где фраза реально легла и слова, услышанные в самом дубле (они — слова перевода,
                // поэтому только у строки перевода). Нет свежей записи — тайминг оригинала без слов.
                Some(t) => match t.fresh(s) {
                    Some(st) => (
                        st.at,
                        st.at + st.dur,
                        (l.primary_is_translation && !st.words.is_empty()).then(|| st.words.clone()),
                    ),
                    None => (s.start, if s.end > 0.0 { s.end } else { total }, None),
                },
                // Без дубляжа звучит оригинал: слова ASR оригинала (word_align сам отбросит их, если
                // текст субтитра — перевод, а не транскрипт).
                None => (s.start, if s.end > 0.0 { s.end } else { total }, seg_words(s),
                    ),
            };
            Sub {
                start,
                end,
                tgt: l.primary,
                y: Some(seg_y(start, end)),
                words,
                secondary: l.secondary,
            }
        })
        .collect() };
    let secondary = (proj.subs.mode == "bilingual").then(|| secondary_look(&proj.subs.bilingual));

    let preset = proj.captions.preset.name.clone();
    let caption_style = preset.as_deref().filter(|n| *n != "match");
    let args = dub_captions::BuildArgs {
        preset: caption_style,
        titles: &titles,
        subs: &subs,
        max_lines: 2,
        sub_y: Some(sub_y),
        sub_style: sub_style.as_ref(),
        caption_style,
        caption_plate: proj.captions.preset.plate.as_deref(),
        caption_reveal: proj.captions.preset.reveal.as_deref(),
        caption_font: proj.captions.preset.font.as_deref(),
        sub_px: proj
            .captions
            .raw_plan
            .get("sub_px")
            .and_then(|v| v.as_i64()),
        secondary: secondary.as_ref(),
    };
    dub_captions::build(vw, vh, out_ass, args)
}

/// Вид второй строки двуязычных субтитров из настроек проекта.
fn secondary_look(b: &dub_core::Bilingual) -> dub_captions::Secondary {
    dub_captions::Secondary {
        below: b.order != dub_core::ORDER_ORIGINAL_TOP,
        size_pct: b.secondary.size_pct,
        color: b.secondary.color.clone(),
        opacity: b.secondary.opacity,
    }
}

/// Словные тайминги ASR оригинала из extra.words ({word,start,end}); нет или пусто — None.
fn seg_words(s: &dub_core::Segment) -> Option<Vec<(String, f64, f64)>> {
    let arr = s.extra.get("words")?.as_array()?;
    let ws: Vec<(String, f64, f64)> = arr
        .iter()
        .filter_map(|w| {
            let word = w.get("word")?.as_str()?.to_string();
            let start = w.get("start")?.as_f64()?;
            let end = w.get("end").and_then(|v| v.as_f64()).unwrap_or(start);
            Some((word, start, end.max(start)))
        })
        .collect();
    (!ws.is_empty()).then_some(ws)
}

/// Blur-боксы из Project (project.captions.blur_boxes, hidden исключаются). Порт caption_plan blur_boxes.
/// В режиме «без субтитров» (subs.mode=none) НЕ блюрим band-полосу (место оригинальных субтитров) — раз
/// мы не накладываем свои субтитры, незачем и закрашивать оригинал (баг-репорт: лишний блюр + прогон).
/// OCR-блюр экранного текста/титров (не-band) остаётся — локализация картинки от субтитров не зависит.
fn collect_blur_boxes(proj: &Project) -> Vec<BlurBox> {
    let drop_band = proj.subs.mode == "none";
    proj.captions
        .blur_boxes
        .iter()
        .filter(|b| !b.hidden)
        .filter(|b| {
            !(drop_band && b.extra.get("band").and_then(|v| v.as_bool()).unwrap_or(false))
        })
        .map(|b| BlurBox { x: b.x, y: b.y, w: b.w, h: b.h, t0: b.t0, t1: b.t1, fill: b.fill.clone(),
        })
        .collect()
}

fn map_title(t: &CoreTitle) -> CapTitle {
    CapTitle {
        text: if !t.tgt.is_empty() { t.tgt.clone() } else { t.text.clone() },
        bbox: t.bbox.clone(),
        color: t.color.clone(),
        bg: t.bg.clone(),
        font: t.font.clone(),
        italic: t.italic,
        align: t.align.clone(),
        start: t.start,
        end: t.end,
        lh: t.lh,
        solid: t.solid,
        bold: t.bold,
        size_px: t.size_px,
        outline: t.outline.clone(),
        outline_w: t.outline_w,
        shadow_dir: t.shadow_dir,
        uppercase: t.uppercase,
    }
}

fn map_sub_style(s: &CoreSubStyle) -> CapSubStyle {
    // background берём из extra (vision кладёт "background"); scene_* тоже могут быть в extra/типизированы.
    let bg = s
        .extra
        .get("background")
        .and_then(|v| v.as_str())
        .map(|x| x.to_string());
    let size_frac = s.extra.get("size_frac").and_then(|v| v.as_f64());
    CapSubStyle {
        color: s.color.clone(),
        background: bg,
        outline: Some(s.outline.clone()),
        outline_w: s.outline_w,
        shadow_dir: s.shadow_dir,
        bold: s.bold,
        italic: s.italic,
        uppercase: s.uppercase,
        align: s.align.clone(),
        font: s.font.clone(),
        n_lines: s.n_lines,
        size_frac,
        size_px: s.size_px,
        scene_color: s.scene_color.clone(),
        scene_flat: s.scene_flat,
        solid: s.extra.get("solid").and_then(|v| v.as_bool()).unwrap_or(false),
        // plate/plate_color из extra (PATCH caption их кладёт).
        plate: s.extra.get("plate").and_then(|v| v.as_bool()),
        plate_color: s
            .extra
            .get("plate_color")
            .and_then(|v| v.as_str())
            .map(|x| x.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dub_core::{BlurBox as CoreBlurBox, Segment};

    fn bb(x: i64, y: i64, w: i64, h: i64, t0: f64, t1: f64, band: bool) -> CoreBlurBox {
        let mut b = CoreBlurBox {
            x, y, w, h, t0, t1, hidden: false, fill: None, extra: Default::default(),
        };
        if band {
            b.extra.insert("band".into(), Value::Bool(true));
        }
        b
    }

    fn seg(id: &str, start: f64, end: f64, tgt: &str) -> Segment {
        // ..Default::default() для полей вне интереса теста (id/тайминги/tgt) — устойчиво к добавлению
        // новых полей Segment смежными подсистемами (напр. ckpt чекпоинтинга).
        Segment {
            id: id.into(),
            start,
            end,
            tgt_text: tgt.into(),
            ..Default::default()
        }
    }

    // Y самого раннего S-субтитра (наш дублированный) в ASS: \pos(cx,cy) -> cy.
    fn first_sub_y(ass: &str) -> i64 {
        for l in ass.lines() {
            if l.starts_with("Dialogue:") && l.contains(",S,") {
                if let Some(i) = l.find("\\pos(") {
                    let rest = &l[i + 5..];
                    let close = rest.find(')').unwrap();
                    let inner = &rest[..close];
                    let cy: i64 = inner.split(',').nth(1).unwrap().trim().parse().unwrap();
                    return cy;
                }
            }
        }
        panic!("нет S-субтитра с \\pos в ASS:\n{ass}");
    }

    fn build_to_string(proj: &Project, vw: i64, vh: i64) -> String {
        let dir = std::env::temp_dir()
            .join(format!("render_segy_{}_{}.ass", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        build_ass(proj, &dir, None, vw, vh, 44.77).unwrap();
        let s = std::fs::read_to_string(&dir).unwrap();
        let _ = std::fs::remove_file(&dir);
        s
    }

    // ducks_ru (ru→en), кадр s0 «SUBSCRIPTION.»: верхний title/tagline-блюр (cy≈438,464)
    // сидит в НИЖНЕЙ половине 824-кадра (>=0.45vh=371) вместе с настоящей полосой (cy≈630-642). Питон медианит
    // seg_y ТОЛЬКО по caption_boxes (полоса). Band-боксы помечены extra["band"], seg_y едет только
    // по ним -> строка на полосе, плашка (BorderStyle=3) обнимает текст там же, блюр оригинала накрыт.
    // Две фразы разной входной громкости после normalize_voice звучат одинаково (RMS сходится) —
    // все спикеры на выходе на одном уровне.
    #[test]
    fn normalize_equalizes_speaker_loudness() {
        let sr = 24000u32;
        let sine = |amp: f32| -> Vec<f32> {
            (0..sr) // 1 c
                .map(|i| {
                    amp * (2.0 * std::f64::consts::PI * 180.0 * i as f64 / sr as f64).sin() as f32
                })
                .collect()
        };
        let rms = |x: &[f32]| (x.iter().map(|&v| (v * v) as f64).sum::<f64>() / x.len() as f64).sqrt();
        let mut loud = sine(0.8);
        let mut quiet = sine(0.08);
        normalize_voice(&mut loud, sr);
        normalize_voice(&mut quiet, sr);
        let (rl, rq) = (rms(&loud), rms(&quiet));
        // после выравнивания уровни должны сойтись (dialog-gated нормализация); пики фразы держит
        // пофразный лимитер, межфразовые суммы и микс с фоном — финальный media::loudnorm.
        assert!((rl - rq).abs() / rl.max(1e-9) < 0.15, "уровни должны сойтись: loud={rl:.4} quiet={rq:.4}");
    }

    #[test]
    fn seg_y_rides_band_not_tagline_ducks() {
        let (vw, vh) = (464i64, 824i64);
        let mut proj = Project::default();
        proj.mode = "dub".into();
        proj.subs.mode = "translate".into(); // включить субтитры (subs=none даёт пустой ASS с фикса #47)
        proj.captions.sub_y = Some(634);
        proj.captions.sub_y_locked = false;
        // sub_style как у ducks: белый Oswald caps на тёмной полосе (vision background -> BorderStyle=3).
        let mut ss = CoreSubStyle::default();
        ss.color = "#FFFFFF".into();
        ss.bold = true;
        ss.uppercase = true;
        ss.font = Some("Oswald".into());
        ss.extra.insert("background".into(), Value::String("#000000".into()));
        proj.captions.sub_style = Some(ss);
        // Покадровый таглайн cy≈437/464 (10 боксов, НЕ band) равен по числу настоящей полосе
        // cy≈630-686 (9 боксов, band): без разделения списков таглайн затянул бы медиану.
        let mut boxes = Vec::new();
        for t in [1.75, 2.0, 2.25, 2.5, 2.75].iter() {
            boxes.push(bb(161, 427, 140, 21, *t, *t + 0.25, false)); // таглайн стр.1 cy=437
            boxes.push(bb(42, 453, 378, 22, *t, *t + 0.25, false));  // таглайн стр.2 cy=464 (широкая)
        }
        for (cy, t) in [(630.0, 1.75), (630.0, 2.0), (641.0, 2.25), (641.0, 2.5),
                        (641.0, 2.75), (641.0, 3.0), (652.0, 1.75), (652.0, 2.0), (686.0, 2.5),
        ]
        {
            let y = cy as i64 - 9;
            boxes.push(bb(120, y, 220, 19, t, t + 0.25, true)); // полоса
        }
        proj.captions.blur_boxes = boxes;
        proj.segments = vec![
            seg("s0", 2.22, 2.80, "SUBSCRIPTION."), // окно 4-й word-группы s0
        ];
        let ass = build_to_string(&proj, vw, vh);
        let y = first_sub_y(&ass);
        assert!(
            (630..=650).contains(&y),
            "строка должна ехать на полосу оригинала (~640), а не на таглайн (~464): y={y}\n{ass}"
        );
        // плашка обнимает текст: S-style несёт BorderStyle=3 (плашка = обводка вокруг текста той же строки).
        let s_style = ass.lines().find(|l| l.starts_with("Style: S,")).unwrap();
        assert!(s_style.contains(",3,11,"), "плашка = BorderStyle=3 в стиле строки: {s_style}");
    }

    #[test]
    fn only_mix_temporaries_of_the_project_are_discarded() {
        let wd = std::env::temp_dir().join(format!("render_discard_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wd);
        std::fs::create_dir_all(&wd).unwrap();
        for n in ["final_audio.wav", "dub_vocals.wav", "source.mp4"] {
            std::fs::write(wd.join(n), b"x").unwrap();
        }
        discard_mix(&wd.join("final_audio.wav"), &wd);
        discard_mix(&wd.join("dub_vocals.wav"), &wd);
        discard_mix(&wd.join("source.mp4"), &wd);
        assert!(!wd.join("final_audio.wav").exists());
        assert!(wd.join("dub_vocals.wav").exists() && wd.join("source.mp4").exists());
        let _ = std::fs::remove_dir_all(&wd);
    }

    #[test]
    fn dub_subtitle_sits_where_the_dub_phrase_sounds() {
        let dir = std::env::temp_dir().join(format!("render_dubtiming_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut timing = crate::dub_timing::DubTiming::default();
        timing.segments.insert(
            "s0".into(),
            crate::dub_timing::SegTiming {
                text: "Привет мир".into(),
                seg_start: 1.0,
                seg_end: 2.0,
                at: 3.0,
                dur: 1.5,
                words: vec![("Привет".into(), 3.1, 3.5), ("мир".into(), 3.6, 4.2)],
                fit: None,
            },
        );
        std::fs::write(dir.join(crate::dub_timing::FILE), serde_json::to_string(&timing).unwrap(),
        ).unwrap();
        let mut proj = Project { mode: "dub".into(), segments: vec![seg("s0", 1.0, 2.0, "Привет мир")], ..Default::default() };
        proj.subs.mode = "translate".into();
        proj.captions.preset.name = Some("karaoke".into());
        let ass_path = dir.join("caps.ass");
        let events = |proj: &Project| -> Vec<String> {
            build_ass(proj, &ass_path, Some(&dir), 1080, 1920, 10.0).unwrap();
            std::fs::read_to_string(&ass_path)
                .unwrap()
                .lines()
                .filter(|l| l.starts_with("Dialogue: 1,") && l.contains(",KT,"))
                .map(|l| l.to_string())
                .collect()
        };
        let ev = events(&proj);
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert!(ev[0].starts_with("Dialogue: 1,0:00:03.00,0:00:04.50,"), "субтитр на месте дубля: {}", ev[0]);
        assert!(ev[0].contains("{\\k10}{\\kf50}"), "пауза до первого слова и его длительность по дублю: {}", ev[0]);
        // Правка текста после сборки дубляжа: запись устарела — тайминг оригинала.
        proj.segments[0].tgt_text = "Пока мир".into();
        let ev = events(&proj);
        assert!(ev[0].starts_with("Dialogue: 1,0:00:01.00,0:00:02.00,"), "{}", ev[0]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_phrase_cleaned_for_synthesis_keeps_its_dub_timing_in_the_subtitles() {
        let dir = std::env::temp_dir().join(format!("render_dubtiming_shown_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut shown = Project { mode: "dub".into(), tgt_lang: "ru".into(), ..Default::default() };
        shown.subs.mode = "translate".into();
        shown.subs.burn = false;
        shown.segments = vec![
            seg("s0", 1.0, 2.0, "«Привет»,  (смеётся) мир"),
            seg("s1", 2.5, 3.0, "[музыка]"),
            seg("s2", 4.0, 5.0, "Всё"),
        ];
        let view = crate::tts_text::synthesis_view(&shown, &|_| {});
        assert_ne!(view.segments[0].tgt_text, shown.segments[0].tgt_text, "текст для синтеза очищен");
        let segs: Vec<(usize, &Segment)> =
            view.segments.iter().enumerate().filter(|(_, s)| !s.tgt_text.trim().is_empty()).collect();
        let wav = dir.join("seg_fit.wav");
        std::fs::write(&wav, b"x").unwrap();
        let placed = vec![(3.0, wav.clone(), 1.5), (6.0, wav.clone(), 0.5)];
        let spans = vec![(3.0, 4.5), (6.0, 6.5)];
        let paths = RenderPaths {
            input: dir.join("in.mp4"),
            work_dir: dir.clone(),
            output: dir.join("out.mp4"),
            bsroformer_cli: PathBuf::new(),
            bsroformer_model: PathBuf::new(),
            higgs_dll: PathBuf::new(),
            higgs_model_root: PathBuf::new(),
            higgs_quant: String::new(),
            fonts_dir: PathBuf::new(),
            higgs_backend: "cpu".into(),
            higgs_device: 0,
            higgs_threads: 1,
            max_stretch: 1.0,
            voices_dir: PathBuf::new(),
            asr: crate::models::AsrChoice::Parakeet(PathBuf::new()),
            bench: false,
            ref_secs: 12.0,
            models_root: PathBuf::new(),
            llama_bin: PathBuf::new(),
            mt_model: PathBuf::new(),
        };
        record_dub_timing(&shown, &paths, &segs, &placed, &[None, None], &spans, 1.0, &|_| {},
        ).unwrap();
        let ass_path = dir.join("caps.ass");
        build_ass(&shown, &ass_path, Some(&dir), 1080, 1920, 10.0).unwrap();
        let ass = std::fs::read_to_string(&ass_path).unwrap();
        let starts: Vec<&str> = ass
            .lines()
            .filter(|l| l.starts_with("Dialogue: 1,"))
            .map(|l| l.split(',').take(3).collect::<Vec<_>>()[1])
            .collect();
        assert!(starts.contains(&"0:00:03.00"), "фраза с кавычками и (смеётся) — на месте дубля: {ass}");
        assert!(starts.contains(&"0:00:06.00"), "{ass}");
        assert!(!starts.contains(&"0:00:01.00"), "не на тайминге оригинала: {ass}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Fallback: проект без маркеров band (ручной blur из редактора) — seg_y работает по всему
    // blur-набору. Здесь только полоса без тегов -> едет на неё.
    #[test]
    fn seg_y_fallback_untagged_boxes() {
        let (vw, vh) = (464i64, 824i64);
        let mut proj = Project::default();
        proj.mode = "dub".into();
        proj.subs.mode = "translate".into(); // включить субтитры (subs=none даёт пустой ASS с фикса #47)
        proj.captions.sub_y = Some(634);
        let mut boxes = Vec::new();
        for t in [0.75, 1.0, 1.25, 1.5].iter() {
            boxes.push(bb(120, 631, 220, 19, *t, *t + 0.25, false)); // НЕ помечены band
        }
        proj.captions.blur_boxes = boxes;
        proj.segments = vec![seg("s0", 1.0, 1.5, "TEXT")];
        let ass = build_to_string(&proj, vw, vh);
        let y = first_sub_y(&ass);
        assert!((630..=650).contains(&y), "fallback: без маркеров едем по всему набору: y={y}");
    }

    #[test]
    fn segment_with_matching_key_is_not_resynthesized() {
        let s = seg("s0", 1.0, 2.5, "Привет");
        let key = seg_key(&s, "clone", "ref_spk0.wav:1000|", "higgs:m:q8_0", "sr1");
        // dirty (правка текста туда-обратно, смена голоса и обратно) не заставляет пересинтезировать.
        assert!(!seg_needs_synth(true, Some(&key), &key, true));
        assert!(seg_needs_synth(true, Some("другой"), &key, false));
        assert!(seg_needs_synth(false, Some(&key), &key, false));
        // Проект без записанных ключей — прежнее правило dirty.
        assert!(!seg_needs_synth(true, None, &key, false));
        assert!(seg_needs_synth(true, None, &key, true));
        // Оригинальная реплика вместо провалившегося синтеза — при продолжении синтез повторяется.
        assert!(seg_needs_synth(true, Some(SEG_FALLBACK), &key, false));
        assert!(!is_synth_key(SEG_FALLBACK) && !is_synth_key(SEG_ORIGINAL) && is_synth_key(&key));
    }

    #[test]
    fn segment_key_tracks_what_changes_the_sound() {
        let a = seg("s0", 1.0, 2.5, "Привет");
        let base = seg_key(&a, "clone", "r", "e", "o");
        let mut dirty = a.clone();
        dirty.dirty = true;
        assert_eq!(seg_key(&dirty, "clone", "r", "e", "o"), base);
        assert_ne!(seg_key(&seg("s0", 1.0, 2.5, "Пока"), "clone", "r", "e", "o"), base);
        assert_ne!(seg_key(&a, "Anna", "r", "e", "o"), base);
        assert_ne!(seg_key(&a, "clone", "r2", "e", "o"), base);
        assert_ne!(seg_key(&a, "clone", "r", "higgs:q4", "o"), base);
        let mut regen = a.clone();
        regen.extra.insert(REGEN_NONCE.into(), serde_json::json!("n1"));
        assert_ne!(seg_key(&regen, "clone", "r", "e", "o"), base);
        let mut moved = a.clone();
        moved.end = 3.0;
        assert_ne!(seg_key(&moved, "clone", "r", "e", "o"), base);
    }

    #[test]
    fn unfinished_tmp_is_not_a_cached_segment() {
        let wd = std::env::temp_dir().join(format!("dub_segckpt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wd);
        std::fs::create_dir_all(&wd).unwrap();
        let raw = wd.join("seg_s0.wav");
        // Обрыв записи: остался только временный файл, ключ записан не был.
        std::fs::write(dub_core::atomic::tmp_path(&raw), b"RIFF-half").unwrap();
        let ck = SegCkpts::load(&wd).unwrap();
        assert!(!raw.is_file());
        assert!(seg_needs_synth(raw.is_file(), ck.get("s0"), "k", false));
        // Полная запись: файл + ключ -> повтор не синтезирует.
        dub_core::atomic::write(&raw, b"RIFF-full").unwrap();
        let mut ck = SegCkpts::load(&wd).unwrap();
        ck.set("s0", "k").unwrap();
        let ck = SegCkpts::load(&wd).unwrap();
        assert!(!seg_needs_synth(raw.is_file(), ck.get("s0"), "k", true));
        let _ = std::fs::remove_dir_all(&wd);
    }

    #[test]
    fn seg_file_id_matches_render_naming() {
        assert_eq!(seg_file_id("s12").as_deref(), Some("s12"));
        assert_eq!(seg_file_id("u-lk3.9").as_deref(), Some("ulk39"));
        assert_eq!(seg_file_id("--"), None);
    }

    fn dialogue_texts(ass: &str, style: &str) -> Vec<String> {
        ass.lines()
            .filter(|l| l.starts_with("Dialogue: 1,") && l.contains(&format!(",{style},,")))
            .map(|l| l.rsplit('}').next().unwrap().to_string())
            .collect()
    }

    fn two_language_project(mode: &str, subs: &str) -> Project {
        let mut s = seg("s0", 1.0, 3.0, "Где ты был?");
        s.src_text = "Where were you?".into();
        Project {
            mode: mode.into(),
            subs: dub_core::Subs { mode: subs.into(), ..Default::default() },
            segments: vec![s],
            ..Default::default()
        }
    }

    #[test]
    fn a_dub_can_carry_subtitles_in_the_original_language() {
        let ass = build_to_string(&two_language_project("dub", "transcribe"), 640, 360);
        assert_eq!(dialogue_texts(&ass, "S"), ["Where were you?"], "{ass}");
        let ass = build_to_string(&two_language_project("voiceover", "translate"), 640, 360);
        assert_eq!(dialogue_texts(&ass, "S"), ["Где ты был?"], "{ass}");
    }

    #[test]
    fn bilingual_burns_the_translation_with_the_original_line() {
        let mut proj = two_language_project("dub", "bilingual");
        proj.subs.bilingual.order = dub_core::ORDER_ORIGINAL_TOP.into();
        let ass = build_to_string(&proj, 640, 360);
        assert_eq!(dialogue_texts(&ass, "S"), ["Где ты был?"], "{ass}");
        assert_eq!(dialogue_texts(&ass, "S2"), ["Where were you?"], "{ass}");
        assert!(ass.contains("Style: S2,"));
        let y = |style: &str| first_y(&ass, style);
        assert!(y("S2") < y("S"), "оригинал сверху: {ass}");
    }

    #[test]
    fn a_translate_project_has_no_second_line() {
        let ass = build_to_string(&two_language_project("dub", "translate"), 640, 360);
        assert!(!ass.contains("S2"), "{ass}");
    }

    fn first_y(ass: &str, style: &str) -> i64 {
        let l = ass.lines().find(|l| l.starts_with("Dialogue: 1,") && l.contains(&format!(",{style},,"))).unwrap();
        let rest = &l[l.find("\\pos(").unwrap() + 5..];
        rest[..rest.find(')').unwrap()].split(',').nth(1).unwrap().trim().parse().unwrap()
    }

    fn with_words(mut s: Segment, src: &str, words: &[(&str, f64, f64)]) -> Segment {
        s.src_text = src.into();
        s.speaker = Some("0".into());
        let ws: Vec<Value> = words.iter().map(|(w, a, b)| json!({ "word": w, "start": a, "end": b })).collect();
        s.extra.insert("words".into(), Value::Array(ws));
        s
    }

    #[test]
    fn cut_ref_window_moves_its_edges_into_pauses_and_carries_exactly_its_words() {
        let words = [
            ("Раз,", 0.10, 0.60),
            ("два", 0.90, 1.60),
            ("три", 1.80, 2.40),
            ("четыре", 2.70, 3.50),
            ("пять", 3.90, 4.80),
            ("шесть.", 5.00, 5.80),
        ];
        let s = with_words(seg("s0", 0.0, 6.0, "x"), "Раз, два три четыре пять шесть.", &words,
        );
        let (a, b, text) = ref_window(&s, 1.3, 4.6);
        assert!(a > 1.60 && a < 1.80, "начало в паузе между «два» и «три»: {a}");
        assert!(b > 3.50 && b < 3.90, "конец в паузе между «четыре» и «пять»: {b}");
        assert_eq!(text.as_deref(), Some("три четыре"));
        let (a, b, text) = ref_window(&s, 0.0, 6.0);
        assert_eq!((a, b), (0.0, 6.0));
        assert_eq!(text.as_deref(), Some("Раз, два три четыре пять шесть."), "окно на всю реплику — её src_text");
        let (a, b, text) = ref_window(&s, 0.7, 5.9);
        assert_eq!((a, b), (0.7, 5.9), "края уже в паузах — окно не двигается");
        assert_eq!(text.as_deref(), Some("два три четыре пять шесть."));
    }

    #[test]
    fn cut_ref_window_without_words_stays_put_and_has_no_text() {
        let s = seg("s0", 0.0, 10.0, "x");
        assert_eq!(ref_window(&s, 1.0, 9.0), (1.0, 9.0, None));
        let short = with_words(seg("s1", 0.0, 3.0, "x"), "да нет", &[("да", 0.1, 1.2), ("нет", 1.3, 2.9)],
        );
        assert_eq!(ref_window(&short, 0.5, 2.5), (0.5, 2.5, None), "внутри окна нет целого слова");
    }

    #[test]
    fn identity_ref_window_is_trimmed_then_snapped_to_words() {
        let words: Vec<(String, f64, f64)> =
            (0..12).map(|k| (format!("w{k}"), k as f64 * 0.8 + 0.05, k as f64 * 0.8 + 0.6)).collect();
        let wref: Vec<(&str, f64, f64)> = words.iter().map(|(w, a, b)| (w.as_str(), *a, *b)).collect();
        let s = with_words(seg("s0", 0.0, 10.0, "x"), "whole line", &wref);
        let segs = vec![(0usize, &s)];
        let pick = pick_ref_window("0", &segs, 12.0).unwrap();
        assert!((pick.start - 1.525).abs() < 1e-9, "{}", pick.start);
        assert!((pick.end - 8.725).abs() < 1e-9, "{}", pick.end);
        let expected: Vec<String> = (2..=10).map(|k| format!("w{k}")).collect();
        assert_eq!(pick.text, Some(expected.join(" ")));
    }

    #[test]
    fn tightened_clip_is_written_beside_and_the_cached_segment_is_untouched() {
        let wd = std::env::temp_dir().join(format!("render_tight_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wd);
        std::fs::create_dir_all(&wd).unwrap();
        let sr = 24_000usize;
        let mut x = vec![0.0f32; sr / 2];
        x.extend((0..sr).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr as f32).sin()),
        );
        x.extend(vec![0.0f32; sr * 8 / 10]);
        let raw = wd.join("seg_s0.wav");
        std::fs::write(&raw, AudiocppEngine::encode_wav(&x, sr as i32, 1)).unwrap();
        let raw_bytes = std::fs::read(&raw).unwrap();
        let out = wd.join("seg_000_tight.wav");
        let t = tighten_clip(&raw, 10.0, &out).unwrap();
        assert_eq!(t.path, out);
        assert!((t.before - 2.3).abs() < 1e-6 && (t.after - 1.12).abs() < 1e-6, "{} -> {}", t.before, t.after);
        assert_eq!(std::fs::read(&raw).unwrap(), raw_bytes, "кэш синтеза не меняется");
        let first = std::fs::read(&out).unwrap();
        tighten_clip(&raw, 10.0, &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), first, "повторный рендер даёт тот же клип");
        let (y, ysr) = wavio::read_mono_f32(&out).unwrap();
        assert_eq!((y.len(), ysr), (sr * 112 / 100, sr as u32));

        let full = wd.join("seg_s1.wav");
        std::fs::write(&full, AudiocppEngine::encode_wav(&x[sr / 2..sr * 3 / 2], sr as i32, 1),
        ).unwrap();
        let t = tighten_clip(&full, 10.0, &wd.join("seg_001_tight.wav")).unwrap();
        assert_eq!(t.path, full, "снимать нечего — клип остаётся своим файлом");
        assert!(!wd.join("seg_001_tight.wav").exists());
        let _ = std::fs::remove_dir_all(&wd);
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("render_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn clone_refs_come_from_the_project_vocals_and_the_mix_is_said_only_for_a_local_clone() {
        let wd = scratch("ref_source");
        let hq = wd.join("audio_hq.wav");
        assert_eq!(ref_source(&wd, &hq), (hq.clone(), true), "без стемов — микс");
        std::fs::create_dir_all(wd.join("stems")).unwrap();
        std::fs::write(wd.join("stems").join("vocals.wav"), b"v").unwrap();
        assert_eq!(ref_source(&wd, &hq), (wd.join("stems").join("vocals.wav"), false));
        assert!(mix_ref_noted(true, false, true));
        assert!(!mix_ref_noted(true, true, true), "облачному TTS рефы не нужны");
        assert!(!mix_ref_noted(true, false, false), "у всех спикеров голоса пака");
        assert!(!mix_ref_noted(false, false, true));
        let _ = std::fs::remove_dir_all(&wd);
    }

    #[test]
    fn a_clip_counts_as_brought_into_the_cap_only_by_its_trim() {
        assert!(trimmed_into_cap(2.8, 2.4, 2.0, 1.25), "1.4 -> 1.2 при капе 1.25");
        assert!(!trimmed_into_cap(2.4, 2.2, 2.0, 1.25), "и до обрезки влезал в кап");
        assert!(!trimmed_into_cap(3.0, 2.8, 2.0, 1.25), "и после обрезки выше капа");
        assert!(!trimmed_into_cap(3.0, 1.0, 0.0, 1.25), "слота нет");
    }

    #[test]
    fn trimming_never_slows_a_clip_more_than_its_untrimmed_self() {
        assert_eq!(fit_factor(1.5, 2.2, 1.25, 1.5), Some(0.85), "необрезанный короткий клип тянется, как раньше");
        let f = fit_factor(1.7, 2.2, 1.25, 2.0).unwrap();
        assert!((f - 2.0 / 2.2).abs() < 1e-12, "обрезанный тянется ровно как тянулся бы целиком: {f}");
        assert_eq!(fit_factor(1.7, 2.2, 1.25, 2.6), None, "целиком его ускоряли бы — обрезанный не замедляется");
        let f = fit_factor(2.5, 2.2, 1.25, 3.0).unwrap();
        assert!((f - 2.5 / 2.2).abs() < 1e-12, "ускорение считается по обрезанному: {f}");
        assert_eq!(fit_factor(4.0, 2.2, 1.25, 4.0), Some(1.25), "кап ускорения");
        assert_eq!(fit_factor(2.2, 2.2, 1.25, 2.2), None);
    }

    #[test]
    fn a_clip_that_fit_with_its_tail_is_not_stretched_after_trimming() {
        let wd = scratch("fit_tail");
        let sr = 24_000usize;
        let mut x: Vec<f32> =
            (0..sr * 18 / 10).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr as f32).sin()).collect();
        x.extend(vec![0.0f32; sr * 4 / 10]);
        let raw = wd.join("seg_s0.wav");
        std::fs::write(&raw, AudiocppEngine::encode_wav(&x, sr as i32, 1)).unwrap();
        let t = tighten_clip(&raw, 2.2, &wd.join("seg_000_tight.wav")).unwrap();
        assert!((t.before - 2.2).abs() < 1e-6 && (t.after - 1.86).abs() < 1e-6, "{} -> {}", t.before, t.after);
        assert_eq!(fit_factor(t.before, 2.2, 1.25, t.before), None, "с хвостом клип ложился без atempo");
        assert_eq!(fit_factor(t.after, 2.2, 1.25, t.after), Some(0.85), "без нижней границы его растянуло бы");
        let work = wd.join("seg_000_fit.wav");
        let (placed, d) = fit_to_slot(&t.path, 2.2, &work, 1.25, t.before).unwrap();
        assert_eq!(placed, t.path);
        assert!((d - 1.86).abs() < 0.01, "ложится короче слота: {d}");
        assert!(!work.exists(), "atempo не звался");
        let _ = std::fs::remove_dir_all(&wd);
    }

    fn stereo_wav(path: &Path, sr: u32, secs: f64) {
        let spec = hound::WavSpec { channels: 2, sample_rate: sr, bits_per_sample: 32, sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(secs * sr as f64) as usize {
            let v = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin();
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn speaker_voice_is_cut_from_the_project_vocals_in_full_band() {
        let wd = scratch("spkvoice_stems");
        std::fs::create_dir_all(wd.join("stems")).unwrap();
        stereo_wav(&wd.join("stems").join("vocals.wav"), 44_100, 16.0);
        media::mark_separation(&wd.join("stems")).unwrap();
        let no_engine = wd.join("no-engine.exe");
        let tmp = wd.join("_voicecut");
        let mut s = seg("s0", 1.0, 4.0, "x");
        s.src_text = "Одна реплика".into();
        let out = wd.join("voice.wav");
        let text = speaker_voice_clip(&s, 12.0, &wd, &wd.join("source.mp4"), (&no_engine, &no_engine), &tmp, &out,
        ).unwrap();
        assert_eq!(text.as_deref(), Some("Одна реплика"), "окно на всю реплику — её текст");
        let r = hound::WavReader::open(&out).unwrap();
        let spec = r.spec();
        assert_eq!(
            (spec.sample_rate, spec.channels, spec.bits_per_sample, spec.sample_format),
            (44_100, 1, 16, hound::SampleFormat::Int),
            "моно PCM16 на частоте вокала"
        );
        assert!((r.duration() as f64 / 44_100.0 - 3.0).abs() < 0.01, "{}", r.duration());
        assert!(!tmp.exists(), "вокал проекта есть — отдельной сепарации нет");

        let long = seg("s1", 0.5, 15.5, "x");
        let text = speaker_voice_clip(&long, 12.0, &wd, &wd.join("source.mp4"), (&no_engine, &no_engine), &tmp, &out,
        ).unwrap();
        assert_eq!(text, None, "урезанное окно без словных таймингов — без текста");
        let r = hound::WavReader::open(&out).unwrap();
        assert!((r.duration() as f64 / 44_100.0 - 12.0).abs() < 0.01, "кап 12 с: {}", r.duration());
        let _ = std::fs::remove_dir_all(&wd);
    }

    #[test]
    fn speaker_voice_without_stems_or_separator_is_refused_before_cutting() {
        let wd = scratch("spkvoice_noengine");
        let input = wd.join("source.wav");
        stereo_wav(&input, 44_100, 5.0);
        let no_engine = wd.join("no-engine.exe");
        let model = wd.join("model.gguf");
        std::fs::write(&model, b"gguf").unwrap();
        let tmp = wd.join("_voicecut");
        let out = wd.join("voice.wav");
        match speaker_voice_clip(&seg("s0", 1.0, 3.0, "x"), 12.0, &wd, &input, (&no_engine, &model), &tmp, &out,
        ) {
            Err(VoiceClipError::NoSeparator(missing)) => {
                assert_eq!(missing, no_engine.display().to_string())
            }
            other => panic!("ждали отказ без движка: {other:?}"),
        }
        assert!(!out.exists() && !tmp.exists(), "без движка реплика не вырезается и голос не пишется");
        let _ = std::fs::remove_dir_all(&wd);
    }

    #[test]
    fn speaker_voice_without_stems_fails_when_the_line_cannot_be_separated() {
        let wd = scratch("spkvoice_nosep");
        let input = wd.join("source.wav");
        stereo_wav(&input, 44_100, 5.0);
        let broken = wd.join("broken-cli.exe");
        std::fs::write(&broken, b"not an executable").unwrap();
        let model = wd.join("model.gguf");
        std::fs::write(&model, b"gguf").unwrap();
        let tmp = wd.join("_voicecut");
        let out = wd.join("voice.wav");
        match speaker_voice_clip(&seg("s0", 1.0, 3.0, "x"), 12.0, &wd, &input, (&broken, &model), &tmp, &out,
        ) {
            Err(VoiceClipError::Separation(e)) => assert!(e.contains("запуск движка"), "{e}"),
            other => panic!("ждали сбой сепарации: {other:?}"),
        }
        assert!(!out.exists(), "голос с музыкой оригинала за очищенный не пишется");
        let r = hound::WavReader::open(tmp.join("cut44.wav")).unwrap();
        assert_eq!((r.spec().sample_rate, r.spec().channels), (44_100, 2), "на сепарацию реплика идёт в полной полосе");
        assert!((r.duration() as f64 / 44_100.0 - 2.0).abs() < 0.01, "{}", r.duration());
        let _ = std::fs::remove_dir_all(&wd);
    }
}
