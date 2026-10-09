//! Порт dubengine/ctx_translate.py run() — единый Gemma-проход: (1) vision layout -> sub_style/sub_y/
//! titles/brands/captions; (2) vision scene-контекст; (3) audio-контекст (окна <=28с); (4) перевод ВСЕГО
//! транскрипта (+тайтлы) С полным vision+audio контекстом. Каждая фаза fail-safe: упавшая фаза даёт пустой
//! контекст, перевод всё равно случается. Промпты TP/AP перенесены ДОСЛОВНО.

use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::Value;

use dub_core::GlossaryEntry;
use dub_llm::{strip_think, ChatClient, Message, Part, Sampling};

use crate::contract::{label, rule as contract_rule, Answer, Contract, Format, LineCheck};
use crate::seg::Seg;
use crate::vision;
use crate::{Note, TranslateError};

/// _LANG из ctx_translate — код -> имя (для vision/перевода). Линейный поиск по срезу (как lang_name
/// в translate.rs) — без построения HashMap на каждый вызов.
fn lang_name(code: &str) -> String {
    let lc = code.to_lowercase();
    crate::WHISPER_LANGS
        .iter()
        .find(|(k, _)| *k == lc.as_str())
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| code.to_string())
}

/// Результат ctx-перевода: заполненные segs + extra (vision/audio/scene) как в питоне.
pub struct CtxResult {
    /// sub_style / sub_y / titles(+tgt) / brands / captions / audio_context / scene_context.
    pub extra: Value,
}

/// Конфиг ctx-прохода — минимальные поля cfg питона, нужные тут.
pub struct CtxConfig {
    pub input: PathBuf,     // исходное видео
    pub work_dir: PathBuf,  // рабочий каталог (_ctx_kf.png)
    pub tgt_lang: String,
    pub vocals16: Option<PathBuf>, // вокал для audio-контекста (может отсутствовать в порту — separation в раунде 4)
    pub vh: f64,            // высота кадра
    pub total: f64,         // длительность
    /// Нужен ли VISION-layout (sub_style/titles/brands). Его выход используется ТОЛЬКО при вжигании
    /// субтитров/титров — в режимах без субтитров это 10-20 лишних vision-вызовов Gemma (минуты на
    /// длинном видео) ради данных, которые никто не прочитает. false -> фаза 1 пропускается целиком.
    pub want_layout: bool,
    /// Стилевая инструкция перевода (#112): доп-указание тона/регистра/лексики. Пусто = без стиля.
    /// Вставляется в инструкционную часть TP-промпта ПЕРЕД форматом-контрактом (он остаётся приоритетным).
    pub style: String,
    /// Глоссарий проекта (и сериала); перевод берёт из него glossary::for_translation.
    pub glossary: Vec<GlossaryEntry>,
}

/// run — единый проход. rewrite=Some(instr) -> творческий ре-дубляж; None -> точный перевод.
/// llm — клиент перевода; vision — клиент мультимодальной модели для кадров и аудио-контекста (у локальной
/// Gemma это тот же сервер с mmproj). vision=None — фазы 1-3 пропускаются, перевод идёт без них.
/// Пишет segs[i].tgt и возвращает extra. contract — формат ответа джобы (общий для всех её проходов).
pub fn run(
    llm: &ChatClient,
    vision: Option<&ChatClient>,
    cfg: &CtxConfig,
    contract: &Contract,
    segs: &mut [Seg],
    rewrite: Option<&str>,
    mut log: impl FnMut(&Note),
) -> Result<CtxResult, TranslateError> {
    let tgt = lang_name(&cfg.tgt_lang);
    let tmp = cfg.work_dir.join("_ctx_kf.png");

    let mut extra = serde_json::json!({
        "sub_style": Value::Null, "sub_y": Value::Null, "titles": [], "captions": [],
        "brands": [], "audio_context": "", "scene_context": ""
    });

    // ── фаза 1: VISION layout — ТОЛЬКО если субтитры/титры будут вжигаться ─
    // (гейт по режиму: в «без субтитров»/burn=off выход layout никем не используется, а это 2 vision-
    // вызова на каждый из 5-10 кейфреймов = минуты Gemma на длинном видео впустую).
    if cfg.want_layout && vision.is_none() {
        log(&Note::LayoutNoVision);
    } else if let (true, Some(vision_llm)) = (cfg.want_layout, vision) {
        match vision::analyze_layout(vision_llm, &cfg.input, &tmp, cfg.total, cfg.vh) {
            Ok(layout) => {
                extra["sub_style"] = layout.sub_style.unwrap_or(Value::Null);
                extra["sub_y"] = layout.sub_y.map(|y| Value::from(y)).unwrap_or(Value::Null);
                extra["titles"] = Value::Array(layout.titles.clone());
                extra["captions"] = Value::Array(layout.captions);
                extra["brands"] = Value::Array(layout.brands.clone());
                let tnames: Vec<String> = layout.titles.iter().filter_map(|t| t.get("text").and_then(|x| x.as_str()).map(String::from)).collect();
                let bnames: Vec<String> = layout.brands.iter().filter_map(|b| b.get("text").and_then(|x| x.as_str()).map(String::from)).collect();
                log(&Note::Layout { sub_style: &extra["sub_style"], titles: &tnames, brands: &bnames });
            }
            Err(e) => log(&Note::LayoutFailed { error: &e }),
        }
    } else {
        log(&Note::LayoutNotNeeded);
    }

    // ── фаза 2: VISION scene-контекст ──────────────────────────────────────
    match vision {
        Some(vision_llm) => match vision::scene_context(vision_llm, &cfg.input, &tmp, cfg.total, &tgt) {
            Ok(sc) => extra["scene_context"] = Value::from(sc),
            Err(e) => log(&Note::SceneFailed { error: &e }),
        },
        None => log(&Note::SceneNoVision),
    }

    // ── фаза 3: AUDIO-контекст (окна <=28с). Fail-safe: нет вокала / модель не умеет audio -> пусто ──
    if let (Some(vocals), Some(vision_llm)) = (&cfg.vocals16, vision) {
        match audio_context(vision_llm, vocals, &tgt) {
            Ok(ac) if !ac.is_empty() => extra["audio_context"] = Value::from(ac),
            Ok(_) => {}
            Err(e) => log(&Note::AudioFailed { error: &e }),
        }
    }

    // ── фаза 4: TRANSLATE весь транскрипт (+тайтлы) С контекстом ───────────
    let n_seg = segs.len();
    let title_texts: Vec<String> = extra["titles"].as_array().map(|a| {
        a.iter().filter_map(|t| t.get("text").and_then(|x| x.as_str()).map(String::from)).collect()
    }).unwrap_or_default();

    // Единый список исходных строк (речь + тайтлы) в ГЛОБАЛЬНОЙ нумерации 1..N (тайтлы после речи), как
    // раньше. Хранится без "N. " префикса — нумеруем локально внутри пакета при отправке.
    let mut line_texts: Vec<String> = segs.iter().map(|s| s.text.trim().to_string()).collect();
    line_texts.extend(title_texts.iter().cloned());

    // Бюджет символов на строку (#107): темп голоса × длительность сегмента — мягкий лимит для укладки
    // перевода в тайминг слота. У тайтлов длительности нет (None -> без лимита в промпте).
    let mut budgets: Vec<Option<usize>> = segs.iter().map(|s| s.budget(&cfg.tgt_lang)).collect();
    budgets.extend(std::iter::repeat_n(None, title_texts.len()));

    let mut ctx = String::new();
    if let Some(sc) = extra["scene_context"].as_str() {
        if !sc.is_empty() {
            ctx += &format!("=== VISUAL SCENE ===\n{sc}\n\n");
        }
    }
    if let Some(ac) = extra["audio_context"].as_str() {
        if !ac.is_empty() {
            ctx += &format!("=== AUDIO (tone/slang/speakers) ===\n{ac}\n\n");
        }
    }

    // Батч-перевод длинного скрипта (#82): чанки по бюджету + скользящий контекст + глоссарий.
    // Возвращает by_n = {глобальный_N -> перевод} — тот же контракт, что раньше давал единый вызов.
    let glossary = dub_core::glossary::for_translation(&cfg.glossary, &cfg.tgt_lang);
    let job = Job { llm, contract, tgt: &tgt, tgt_code: &cfg.tgt_lang, rewrite, style: &cfg.style, glossary };
    let by_n = translate_lines(&job, &line_texts, &budgets, &ctx, &mut log)?;

    for (i, s) in segs.iter_mut().enumerate() {
        let t = by_n.get(&(i + 1)).cloned().unwrap_or_default();
        s.tgt = if t.is_empty() { s.text.trim().to_string() } else { crate::fix_translation(&t, &cfg.tgt_lang) };
    }
    // переводы тайтлов идут после речевых строк.
    if let Some(arr) = extra["titles"].as_array_mut() {
        for (j, ttl) in arr.iter_mut().enumerate() {
            let default = ttl.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let tr = by_n.get(&(n_seg + j + 1)).map(|t| crate::fix_translation(t, &cfg.tgt_lang)).unwrap_or(default);
            ttl.as_object_mut().map(|o| o.insert("tgt".into(), Value::from(tr)));
        }
    }

    let _ = std::fs::remove_file(&tmp);
    Ok(CtxResult { extra })
}

// ── Батч-перевод длинного скрипта (#82) ────────────────────────────────────
// Порог «короткого» транскрипта: до и включая — один чанк (ПАРИТЕТ со старым единым Gemma-вызовом);
// выше — режем на чанки по бюджету строк/символов со скользящим контекстом.
const SHORT_LINES: usize = 30;
// Бюджет чанка при длинном скрипте: не больше MAX_LINES строк И не больше ~MAX_CHARS символов исходника.
const MAX_LINES: usize = 10;
const MAX_CHARS: usize = 600;
// Скользящий контекст: сколько уже-переведённых строк прошлого чанка и сырых строк следующего показать.
const CTX_BEFORE: usize = 3;
const CTX_AFTER: usize = 2;

/// Границы чанков [start,end) над line_texts. len<=SHORT_LINES -> один чанк (паритет). Иначе жадно
/// пакуем по MAX_LINES строк / MAX_CHARS символов (минимум 1 строка на чанк, даже если она длиннее бюджета).
fn chunk_bounds(line_texts: &[String]) -> Vec<(usize, usize)> {
    let n = line_texts.len();
    if n <= SHORT_LINES {
        return if n == 0 { vec![] } else { vec![(0, n)] };
    }
    let mut bounds = Vec::new();
    let mut start = 0;
    while start < n {
        let mut end = start;
        let mut chars = 0usize;
        while end < n {
            let add = line_texts[end].chars().count();
            // всегда берём хотя бы одну строку; далее — пока в оба бюджета влезаем
            if end > start && (end - start >= MAX_LINES || chars + add > MAX_CHARS) {
                break;
            }
            chars += add;
            end += 1;
        }
        bounds.push((start, end));
        start = end;
    }
    bounds
}

/// Что постоянно на всю джобу перевода.
struct Job<'a> {
    llm: &'a ChatClient,
    contract: &'a Contract,
    /// Имя целевого языка для промпта.
    tgt: &'a str,
    tgt_code: &'a str,
    rewrite: Option<&'a str>,
    style: &'a str,
    /// Глоссарий цели, ручные записи раньше.
    glossary: Vec<GlossaryEntry>,
}

/// Перевести все строки пакетами со скользящим контекстом и глоссарием; каждая строка ответа проверяется
/// (contract::LineCheck), непрошедшие переспрашиваются (batch::drive).
/// Ключи результата — ГЛОБАЛЬНЫЕ номера строк 1..line_texts.len() (как by_n у старого единого вызова).
fn translate_lines(
    job: &Job,
    line_texts: &[String],
    budgets: &[Option<usize>],
    ctx: &str,
    log: &mut impl FnMut(&Note),
) -> Result<std::collections::HashMap<usize, String>, TranslateError> {
    if line_texts.is_empty() {
        return Ok(Default::default());
    }
    let llm = job.llm;
    let tgt = job.tgt;

    // Страховка от переполнения n_ctx: блок контекста (scene+audio) приклеивается к КАЖДОМУ чанку.
    // Если он раздулся (любой будущий источник) — обрезаем по бюджету символов, а не роняем перевод.
    const CTX_CHAR_BUDGET: usize = 6000; // ≈1.5-2К токенов; n_ctx=12288 остаётся с запасом под строки+ответ
    let ctx: String = if ctx.chars().count() > CTX_CHAR_BUDGET {
        log(&Note::ContextTrimmed { chars: ctx.chars().count(), budget: CTX_CHAR_BUDGET });
        format!("{}\n[context truncated]\n\n", ctx.chars().take(CTX_CHAR_BUDGET).collect::<String>())
    } else {
        ctx.to_string()
    };

    let bounds = chunk_bounds(line_texts);
    // Авто-пары имён — ТОЛЬКО на длинном скрипте (>1 чанка), как в HEAD: на коротком единый вызов без них
    // (питон-паритет). Их сбой перевод не валит — строка в журнал. Ремикс пишет новый текст: ни имён, ни
    // глоссария (как плоский rewrite).
    let names = if bounds.len() > 1 && job.rewrite.is_none() {
        let texts = line_texts.iter().map(|s| s.as_str());
        match crate::translate::glossary_pairs(llm, texts, &crate::translate::name_src(""), tgt, Some(6), &job.glossary) {
            Ok(pairs) => pairs,
            Err(e) => {
                log(&Note::NamesSkipped { error: &e });
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let glossary: &[GlossaryEntry] = if job.rewrite.is_some() { &[] } else { &job.glossary };
    if bounds.len() > 1 {
        log(&Note::Chunks { lines: line_texts.len(), chunks: bounds.len(), terms: glossary.len(), names: names.len() });
    }

    job.contract.announce(llm, log);

    // Мягкий бюджет длины (#107): если после номера в скобках стоит «(≤NN)» — уложиться в NN символов;
    // при нехватке места убирать вводные слова и дубли, НЕ выдумывать факты. Скобку в ответ не писать.
    let budget_rule = " After each number, a parenthesis like (\u{2264}45) gives a soft character limit for that \
line — stay within it: if it doesn't fit, drop filler words and repetitions, keep the meaning, invent nothing. \
Do NOT copy the (\u{2264}NN) marker into your output.";
    let style_c = crate::translate::style_clause(job.style);
    let gloss_rule = if glossary.is_empty() { "" } else { crate::gloss::RULE };
    let names_c = crate::gloss::names_clause(&names);
    // Неизменная часть — инструкция, стиль, правило глоссария, формат и контекст сцены: одинакова для
    // всех пакетов джобы, llama-server переиспользует её KV-кэш. Меняется только сообщение со строками.
    let system = |fmt: Format| -> String {
        let rule = contract_rule(fmt, tgt);
        match job.rewrite {
            Some(instr) => format!(
                "You are a creative scriptwriter writing a BRAND-NEW voice-over script in {tgt} for this video. \
IGNORE the literal meaning of the source lines — they are ONLY a rhythm/length template. Write a completely NEW \
script whose CONTENT follows this instruction: \"{instr}\". Every line must fit the instruction, NOT translate the \
source. Keep the SAME number of lines and each line about the SAME LENGTH (it will be dubbed to fit the timing).{budget_rule}{style_c}{names_c}{gloss_rule} \
{rule} Use the scene/audio context below for tone.\n\n{ctx}"
            ),
            None => format!(
                "Translate EACH numbered line into natural, spoken {tgt} for dubbing — keep the order and the \
numbering, match tone/slang/intent.{budget_rule}{style_c}{names_c}{gloss_rule} {rule} Use ALL the context below and with the lines \
(what the words alone don't convey).\n\n{ctx}"
            ),
        }
    };
    let lines_title = if job.rewrite.is_some() { "=== LINES (rhythm template) ===" } else { "=== LINES ===" };

    let log_cell = std::cell::RefCell::new(log);
    let mut ask = |idx: &[usize], done: &std::collections::HashMap<usize, String>| -> Result<Answer, TranslateError> {
        // Локальная нумерация 1..len (маленькие номера надёжнее больших; label формата). После номера — мягкий
        // лимит символов «(≤NN)» из бюджета строки (#107); у строк без бюджета (тайтлы) лимита нет.
        let numbered = |fmt: Format| {
            idx.iter()
                .enumerate()
                .map(|(k, &gi)| match budgets.get(gi).copied().flatten() {
                    Some(lim) => format!("{}. (\u{2264}{lim}) {}", label(fmt, k + 1, idx.len()), line_texts[gi]),
                    None => format!("{}. {}", label(fmt, k + 1, idx.len()), line_texts[gi]),
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        // Скользящий контекст: CTX_BEFORE уже-переведённых строк перед пакетом (src -> tgt) + CTX_AFTER сырых
        // строк после него. Только как СПРАВКА для связности, НЕ переводить их.
        let (lo, hi) = (idx[0], idx[idx.len() - 1]);
        let mut ctx_block = String::new();
        let prev: Vec<String> = (lo.saturating_sub(CTX_BEFORE)..lo)
            .map(|gi| format!("{} => {}", line_texts[gi], done.get(&gi).map(String::as_str).unwrap_or("")))
            .collect();
        if !prev.is_empty() {
            ctx_block += &format!("=== PREVIOUS LINES (already translated, for continuity — do NOT re-output) ===\n{}\n\n", prev.join("\n"));
        }
        let after: Vec<&str> = line_texts[(hi + 1).min(line_texts.len())..(hi + 1 + CTX_AFTER).min(line_texts.len())]
            .iter()
            .map(String::as_str)
            .collect();
        if !after.is_empty() {
            ctx_block += &format!("=== UPCOMING LINES (context only — do NOT translate) ===\n{}\n\n", after.join("\n"));
        }
        let texts: Vec<&str> = idx.iter().map(|&gi| line_texts[gi].as_str()).collect();
        let gloss_block = crate::gloss::block(glossary, &texts);
        let messages = |fmt: Format| {
            vec![
                Message::system(system(fmt)),
                Message::user_text(format!("{gloss_block}{ctx_block}{lines_title}\n{}\n\n{}", numbered(fmt), contract_rule(fmt, tgt))),
            ]
        };
        // mt (макс. выход) капим — не резервировать гигантский n_predict из контекста на большой пакет.
        let mt = (96 + 52 * idx.len()).min(2560) as u32;
        let s = Sampling::new(0.2, 0.95, mt).top_k(64);
        let mut answer = job.contract.ask(llm, &messages, &s, idx.len(), &mut |m: &Note| (log_cell.borrow_mut())(m))?;
        for line in answer.lines.iter_mut().flatten() {
            *line = crate::gloss::term_lock(line, glossary, &names);
        }
        Ok(answer)
    };
    let check = |gi: usize, line: Option<&str>, cut: bool| {
        LineCheck { src: &line_texts[gi], budget: budgets[gi], tgt_lang: job.tgt_code, glossary, rewrite: job.rewrite.is_some() }
            .check(line, cut)
    };
    let chunks: Vec<Vec<usize>> = bounds.iter().map(|&(a, b)| (a..b).collect()).collect();
    let out = crate::batch::drive(chunks, &mut ask, &check, &|gi| gi + 1, &mut |m: &Note| (log_cell.borrow_mut())(m))?;
    if line_texts.len() > SHORT_LINES || !out.failed.is_empty() || !out.flawed.is_empty() {
        (log_cell.borrow_mut())(&Note::Done { translated: out.accepted.len(), flawed: out.flawed.len(), untranslated: out.failed.len() });
    }
    Ok(out.accepted.into_iter().map(|(gi, t)| (gi + 1, t)).collect())
}

/// AUDIO-контекст — нарезка вокала на окна <=28с и запрос input_audio. Fail-safe вызывающим.
/// AP — ДОСЛОВНО из ctx_translate.py.
fn audio_context(llm: &ChatClient, vocals: &Path, tgt: &str) -> Result<String, TranslateError> {
    let mut reader = hound::WavReader::open(vocals).map_err(|e| TranslateError::Audio(e.to_string()))?;
    let spec = reader.spec();
    let sr = spec.sample_rate as usize;
    // читаем как f32 mono (микшируем каналы усреднением, как d.mean(axis=1) в питоне)
    let ch = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / max).collect()
        }
    };
    let mono: Vec<f32> = if ch > 1 {
        samples.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect()
    } else {
        samples
    };

    let ap = format!(
        "Helping a translator dub to {tgt}. Listen; give context the transcript MISSES (do NOT \
transcribe): situation, tone/register (slang/sarcasm/anger/flirt/...), each speaker gender+vibe, \
slang/idioms and their real meaning here. 4-7 bullets."
    );
    let win = 28 * sr;
    // КАП числа окон: старый `while` шёл по ВСЕМУ файлу (22 мин = 49 окон × до 320 ток. ответа ≈ 15.7К
    // токенов) — этот текст приклеивался к КАЖДОМУ чанку перевода и переполнял n_ctx=12288 (реальный
    // фейл: «request 16946 tokens exceeds 12288» → весь перевод пустой). Теперь окон максимум
    // AC_MAX_WIN, равномерно по файлу. Файл ≤ AC_MAX_WIN окон (≈2.3 мин) — окна подряд, как раньше
    // (паритет коротких). Длиннее — сэмплируем: тон/сленг/вайб спикеров не требуют каждой секунды.
    const AC_MAX_WIN: usize = 5;
    let n_total = mono.len().div_ceil(win).max(1);
    let starts: Vec<usize> = if n_total <= AC_MAX_WIN {
        (0..n_total).map(|k| k * win).collect()
    } else {
        (0..AC_MAX_WIN)
            .map(|k| {
                let fr = k as f64 / (AC_MAX_WIN - 1) as f64; // 0.0 .. 1.0
                (((n_total - 1) as f64 * fr).round() as usize) * win
            })
            .collect()
    };
    let mut notes: Vec<String> = vec![];
    for &i in &starts {
        let end = (i + win).min(mono.len());
        if end <= i {
            continue;
        }
        let chunk = &mono[i..end];
        if chunk.len() < 3 * sr {
            continue;
        }
        let wav_b64 = encode_wav_b64(chunk, sr as u32)?;
        let parts = vec![
            Part::AudioB64 { data: wav_b64, format: "wav".into() },
            Part::Text(ap.clone()),
        ];
        let s = Sampling::new(0.2, 0.95, 320).top_k(64);
        let ans = strip_think(&llm.chat(&[Message::user_parts(parts)], &s)?);
        notes.push(format!("[{}s+] {}", i / sr, ans));
    }
    Ok(notes.join("\n\n"))
}

/// Собрать WAV (PCM16) из f32-моно в память и base64-кодировать (как sf.write(buf, ...) в питоне).
fn encode_wav_b64(samples: &[f32], sr: u32) -> Result<String, TranslateError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut w = hound::WavWriter::new(cursor, spec).map_err(|e| TranslateError::Audio(e.to_string()))?;
        for &x in samples {
            let v = (x.clamp(-1.0, 1.0) * 32767.0) as i16;
            w.write_sample(v).map_err(|e| TranslateError::Audio(e.to_string()))?;
        }
        w.finalize().map_err(|e| TranslateError::Audio(e.to_string()))?;
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(&buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: usize, ch: usize) -> Vec<String> {
        (0..n).map(|_| "x".repeat(ch)).collect()
    }

    #[test]
    fn short_script_is_single_chunk() {
        // <= SHORT_LINES -> ровно один чанк (паритет со старым единым вызовом), даже если суммарно
        // символов больше MAX_CHARS.
        assert_eq!(chunk_bounds(&v(1, 5)), vec![(0, 1)]);
        assert_eq!(chunk_bounds(&v(SHORT_LINES, 100)), vec![(0, SHORT_LINES)]);
        assert!(chunk_bounds(&[]).is_empty());
    }

    #[test]
    fn long_script_splits_by_line_budget() {
        // 31 короткая строка -> чанки по MAX_LINES строк.
        let b = chunk_bounds(&v(31, 3));
        assert_eq!(b, vec![(0, 10), (10, 20), (20, 30), (30, 31)]);
        // покрытие полное и без дыр
        assert_eq!(b.first().unwrap().0, 0);
        assert_eq!(b.last().unwrap().1, 31);
        for w in b.windows(2) {
            assert_eq!(w[0].1, w[1].0);
        }
    }

    #[test]
    fn long_script_splits_by_char_budget() {
        // строки по 200 симв, MAX_CHARS=600 -> 3 строки на чанк (до превышения бюджета).
        let b = chunk_bounds(&v(40, 200));
        assert_eq!(b[0], (0, 3));
        assert_eq!(b[1], (3, 6));
    }

    #[test]
    fn oversized_single_line_still_progresses() {
        // одна строка длиннее MAX_CHARS не должна зациклить — берётся одна и идём дальше.
        let mut lines = v(31, 1);
        lines[0] = "y".repeat(MAX_CHARS + 500);
        let b = chunk_bounds(&lines);
        assert_eq!(b[0], (0, 1));
        assert_eq!(b.last().unwrap().1, lines.len());
    }

    #[test]
    fn budget_by_voice_rate_and_marker_strip() {
        let mut s = Seg::new("x", 0);
        s.end = 2.0;
        assert_eq!(s.budget("en"), Some(30));
        s.cps = Some(14.0);
        assert_eq!(s.budget("en"), Some(28));
        s.end = 0.0;
        assert_eq!(s.budget("en"), None);
        let strip = crate::translate::strip_budget_marker;
        assert_eq!(strip("(≤45) перевод"), "перевод");
        assert_eq!(strip("(<=12)  x"), "x");
        assert_eq!(strip("без маркера"), "без маркера");
    }

    #[test]
    fn each_line_is_checked_and_the_instruction_stays_the_same_across_batches() {
        use dub_llm::test_http::{body_json, serve, Reply};
        let reply = |content: &str| Reply::json(200, &serde_json::json!({ "choices": [{ "message": { "content": content }, "finish_reason": "stop" }] }).to_string());
        let server = serve(vec![reply(r#"{"1":"Привет, Гарри","2":"Hello there"}"#), reply(r#"{"1":"Ну привет"}"#)]);
        let llm = ChatClient::new(server.base()).unwrap();
        let glossary = vec![GlossaryEntry { term: "Harry".into(), translation: "Гарри".into(), ..GlossaryEntry::default() }];
        let job = Job { llm: &llm, contract: &Contract::for_client(&llm), tgt: "Russian", tgt_code: "ru", rewrite: None, style: "", glossary };
        let lines: Vec<String> = vec!["Hi, Harry".into(), "Hello there".into()];
        let mut log: Vec<String> = Vec::new();
        let by_n = translate_lines(&job, &lines, &[Some(14), Some(14)], "=== VISUAL SCENE ===\nA castle\n\n", &mut |m: &Note| log.push(m.to_string())).unwrap();
        assert_eq!(by_n[&1], "Привет, Гарри");
        assert_eq!(by_n[&2], "Ну привет");
        let (a, b) = (body_json(&server.request(0)), body_json(&server.request(1)));
        assert_eq!(a["messages"][0], b["messages"][0], "a stable prefix for the KV cache");
        assert!(a["messages"][0]["content"].as_str().unwrap().contains("A castle"));
        assert!(a["messages"][1]["content"].as_str().unwrap().contains("Harry → Гарри"));
        assert!(b["messages"][1]["content"].as_str().unwrap().contains("Hi, Harry => Привет, Гарри"), "the retry sees what is done");
        assert!(log.iter().any(|l| l.contains("repeats the source")), "{log:?}");
    }

    #[test]
    fn a_remix_takes_no_glossary() {
        use dub_llm::test_http::{body_json, serve, Reply};
        let reply = |content: &str| Reply::json(200, &serde_json::json!({ "choices": [{ "message": { "content": content }, "finish_reason": "stop" }] }).to_string());
        let server = serve(vec![reply(r#"{"1":"Harry ест пиццу"}"#)]);
        let llm = ChatClient::new(server.base()).unwrap();
        let glossary = vec![GlossaryEntry { term: "Harry".into(), translation: "Гарри".into(), ..GlossaryEntry::default() }];
        let job = Job { llm: &llm, contract: &Contract::for_client(&llm), tgt: "Russian", tgt_code: "ru", rewrite: Some("about pizza"), style: "", glossary };
        let by_n = translate_lines(&job, &["Hi, Harry".into()], &[Some(30)], "", &mut |_: &Note| {}).unwrap();
        assert_eq!(by_n[&1], "Harry ест пиццу", "no term lock");
        let body = body_json(&server.request(0));
        let all = format!("{}{}", body["messages"][0]["content"], body["messages"][1]["content"]);
        assert!(!all.contains("GLOSSARY") && !all.contains("Гарри"), "{all}");
    }
}
