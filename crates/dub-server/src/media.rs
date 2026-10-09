//! ffmpeg/ffprobe-обёртки для analyze. Порт нужных кусков dubengine/media.py: probe (длительность,
//! видеопоток, fps, кодек) и extract_audio -> wav 16k mono. Тяжёлого ничего: только вызовы бинарей.

use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
const FFMPEG: &str = "ffmpeg.exe";
#[cfg(windows)]
const FFPROBE: &str = "ffprobe.exe";
#[cfg(not(windows))]
const FFMPEG: &str = "ffmpeg";
#[cfg(not(windows))]
const FFPROBE: &str = "ffprobe";

/// Сводка probe: длительность/размер/fps/кодек первого видеопотока. Зеркало api._meta().
#[derive(Debug, Clone, Default)]
pub struct MediaMeta {
    pub duration: f64,
    pub width: i64,
    pub height: i64,
    pub fps: f64,
    pub src_codec: String,
}

fn parse_fps(r: &str) -> f64 {
    // r_frame_rate вида "30000/1001".
    let mut it = r.split('/');
    match (it.next(), it.next()) {
        (Some(n), Some(d)) => {
            let n: f64 = n.parse().unwrap_or(0.0);
            let d: f64 = d.parse().unwrap_or(0.0);
            if d != 0.0 {
                n / d
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// ffprobe -show_format -show_streams (json) -> MediaMeta. Ошибка если нет видеопотока/длительности.
pub fn probe(input: &Path) -> Result<MediaMeta, String> {
    let out = Command::new(FFPROBE)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(input)
        .output()
        .map_err(|e| t!("media-ffprobe-start", error = e.to_string()))?;
    if !out.status.success() {
        return Err(t!(
            "media-ffprobe-exit",
            code = format!("{:?}", out.status.code()),
            stderr = String::from_utf8_lossy(&out.stderr).into_owned()
        ));
    }
    let v: Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe json: {e}"))?;
    let streams = v
        .get("streams")
        .and_then(|s| s.as_array())
        .ok_or_else(|| t!("media-ffprobe-no-streams"))?;
    // Видеопоток ОПЦИОНАЛЕН: чистый аудио-вход (WAV/mp3/…) поддерживается в аудио-режиме (без видео).
    // Нет видео -> width/height/fps=0 (сигнал audio-only), кодек берём из аудиопотока.
    let vstream = streams
        .iter()
        .find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("video"));
    let astream = streams
        .iter()
        .find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("audio"));
    if vstream.is_none() && astream.is_none() {
        return Err(t!("media-no-streams"));
    }
    let duration = v
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse::<f64>().ok())
        .or_else(|| {
            // некоторые WAV не имеют format.duration -> берём из аудиопотока
            astream
                .and_then(|s| s.get("duration"))
                .and_then(|d| d.as_str())
                .and_then(|d| d.parse::<f64>().ok())
        })
        .ok_or_else(|| t!("media-no-duration"))?;
    let width = vstream.and_then(|s| s.get("width")).and_then(|w| w.as_i64()).unwrap_or(0);
    let height = vstream.and_then(|s| s.get("height")).and_then(|h| h.as_i64()).unwrap_or(0);
    let fps = vstream
        .and_then(|s| s.get("r_frame_rate"))
        .and_then(|r| r.as_str())
        .map(parse_fps)
        .unwrap_or(0.0);
    let src_codec = vstream
        .or(astream)
        .and_then(|s| s.get("codec_name"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    Ok(MediaMeta {
        duration,
        width,
        height,
        fps,
        src_codec,
    })
}

/// Первый фильтр каждого извлечения звука, которое идёт в синхронные стадии (ASR, диаризация, сепарация,
/// микс, рендер, волна таймлайна): сэмплы стоят по своим меткам времени (дыра в pts заполнена тишиной,
/// наложение срезано), и поток начинается с t=0 контейнера, как картинка.
pub const SYNC_AF: &str = "aresample=async=1:first_pts=0";

fn os_args(parts: &[&OsStr]) -> Vec<OsString> {
    parts.iter().map(|p| p.to_os_string()).collect()
}

fn extract_wav_16k_mono_args(input: &Path, out_wav: &Path) -> Vec<OsString> {
    os_args(&[
        OsStr::new("-y"), OsStr::new("-i"), input.as_os_str(),
        OsStr::new("-vn"), OsStr::new("-af"), OsStr::new(SYNC_AF),
        OsStr::new("-ac"), OsStr::new("1"), OsStr::new("-ar"), OsStr::new("16000"),
        OsStr::new("-c:a"), OsStr::new("pcm_s16le"), OsStr::new("-f"), OsStr::new("wav"), out_wav.as_os_str(),
    ])
}

/// Извлечь аудиодорожку в WAV 16 кГц mono (pcm_s16le) — вход ASR. Порт media.to_16k_mono/extract_audio
/// (объединённо: сразу 16k/mono, т.к. дальше в порту нет separation-стадии). Если у видео нет аудио —
/// ffmpeg вернёт ошибку, которую пробрасываем.
pub fn extract_wav_16k_mono(input: &Path, out_wav: &Path) -> Result<(), String> {
    dub_core::atomic::write_with(out_wav, |tmp| {
        let mut cmd = Command::new(FFMPEG);
        cmd.args(extract_wav_16k_mono_args(input, tmp));
        let status = dub_core::proc::output(&mut cmd).map_err(|e| t!("common-ffmpeg-start", error = e.to_string()))?;
        if !status.status.success() {
            return Err(t!(
                "common-ffmpeg-exit",
                code = format!("{:?}", status.status.code()),
                tail = String::from_utf8_lossy(&status.stderr).into_owned()
            ));
        }
        if !tmp.is_file() {
            return Err(t!("media-no-wav"));
        }
        Ok(())
    })
}

// ─── Рендер-хелперы (порт media.py: extract_audio/duration/time_stretch/mix/mux/trim) ─────────

fn run_ff(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = dub_core::proc::output(Command::new(FFMPEG).args(args)).map_err(|e| t!("common-ffmpeg-start", error = e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.chars().rev().take(1500).collect::<String>().chars().rev().collect();
        return Err(t!("common-ffmpeg-exit", code = format!("{:?}", out.status.code()), tail = tail));
    }
    Ok(())
}

/// run_ff с ЖЁСТКИМ таймаутом — для дорогих графов (mix_env volume:eval=frame на длинном файле мог
/// зависнуть навечно и заблокировать единственный воркер джоб, как burn #105). Drain-потоки с дедлайном
/// (паттерн dub-captions::burn::output_with_timeout).
fn run_ff_timeout(args: &[&std::ffi::OsStr], secs: u64) -> Result<(), String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = Command::new(FFMPEG)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| t!("common-ffmpeg-start", error = e.to_string()))?;
    let _tracked = dub_core::proc::track(child.id());
    let mut se = child.stderr.take().expect("piped stderr");
    let th_err = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = se.read_to_end(&mut b);
        b
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(t!("media-ffmpeg-hung", seconds = secs));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(250)),
            Err(e) => return Err(format!("ffmpeg wait: {e}")),
        }
    };
    if !status.success() {
        let err = th_err.join().unwrap_or_default();
        let s = String::from_utf8_lossy(&err);
        let tail: String = s.chars().rev().take(1500).collect::<String>().chars().rev().collect();
        return Err(t!("common-ffmpeg-exit", code = format!("{:?}", status.code()), tail = tail));
    }
    Ok(())
}

fn run_ff_args(args: &[OsString]) -> Result<(), String> {
    let refs: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
    run_ff(&refs)
}

fn extract_audio_args(video: &Path, out_wav: &Path, sr: u32, ac: u32) -> Vec<OsString> {
    let (sr, ac) = (sr.to_string(), ac.to_string());
    os_args(&[
        OsStr::new("-y"), OsStr::new("-i"), video.as_os_str(),
        OsStr::new("-vn"), OsStr::new("-af"), OsStr::new(SYNC_AF), OsStr::new("-ac"), OsStr::new(&ac),
        OsStr::new("-ar"), OsStr::new(&sr), out_wav.as_os_str(),
    ])
}

/// Извлечь аудио в WAV sr/ac (порт media.extract_audio). Для сепарации: sr=44100, ac=2.
/// Пишет атомарно: недописанный файл не появится под целевым именем.
pub fn extract_audio(video: &Path, out_wav: &Path, sr: u32, ac: u32) -> Result<(), String> {
    dub_core::atomic::write_with(out_wav, |tmp| run_ff_args(&extract_audio_args(video, tmp, sr, ac)))
}

fn to_16k_mono_args(src: &Path, dst: &Path) -> Vec<OsString> {
    os_args(&[
        OsStr::new("-y"), OsStr::new("-i"), src.as_os_str(),
        OsStr::new("-vn"), OsStr::new("-af"), OsStr::new(SYNC_AF), OsStr::new("-ac"), OsStr::new("1"),
        OsStr::new("-ar"), OsStr::new("16000"), dst.as_os_str(),
    ])
}

/// WAV/медиа -> 16k mono (порт media.to_16k_mono). Пишет атомарно.
pub fn to_16k_mono(src: &Path, dst: &Path) -> Result<(), String> {
    dub_core::atomic::write_with(dst, |tmp| run_ff_args(&to_16k_mono_args(src, tmp)))
}

/// Длительность файла в секундах (ffprobe format.duration). Порт media.duration.
pub fn duration(path: &Path) -> Result<f64, String> {
    let out = Command::new(FFPROBE)
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
        .arg(path)
        .output()
        .map_err(|e| t!("media-ffprobe-start", error = e.to_string()))?;
    if !out.status.success() {
        return Err(t!("media-ffprobe-duration-exit", code = format!("{:?}", out.status.code())));
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .map_err(|e| format!("duration parse: {e}"))
}

/// atempo-цепочка для factor вне [0.5,2.0] (порт media._atempo_chain).
fn atempo_chain(factor: f64) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut f = factor;
    while f > 2.0 {
        parts.push("atempo=2.0".into());
        f /= 2.0;
    }
    while f < 0.5 {
        parts.push("atempo=0.5".into());
        f /= 0.5;
    }
    parts.push(format!("atempo={:.6}", f));
    parts.join(",")
}

/// factor>1 ускоряет (укорачивает); <1 замедляет. Порт media.time_stretch.
pub fn time_stretch(src: &Path, dst: &Path, factor: f64) -> Result<(), String> {
    let chain = atempo_chain(factor);
    run_ff(&[
        OsStr::new("-y"), OsStr::new("-i"), src.as_os_str(),
        OsStr::new("-filter:a"), OsStr::new(&chain), dst.as_os_str(),
    ])
}

/// Частота промежуточных стадий микса: частота audio_hq/стемов сепарации, чтобы по цепочке
/// mix -> loudnorm -> gain не было лишних пересчётов частоты. loudnorm внутри работает на 192 кГц,
/// поэтому частота выхода задаётся явно.
const MIX_SR: &str = "44100";

/// Кодек и частота промежуточного файла микса: float32 WAV без потерь (сумма голоса и фона может
/// выйти за 0 dBFS — float хранит это до loudnorm без клипа). Сжатие с потерями одно — в mux.
/// `-rf64 auto`: многочасовой стерео-float переходит границу RIFF в 4 ГБ.
fn lossless_out() -> [&'static OsStr; 6] {
    [
        OsStr::new("-c:a"), OsStr::new("pcm_f32le"),
        OsStr::new("-ar"), OsStr::new(MIX_SR),
        OsStr::new("-rf64"), OsStr::new("auto"),
    ]
}

/// Свести дубль-вокал поверх фона. МУЗЫКУ НЕ ГЛУШИМ (прямой приказ юзера, многократно): вокал уже вырезан
/// сепарацией, поэтому инструментал = чистый реальный фон и звучит в ПОЛНЫЙ уровень (1.0) — дублированный
/// голос заменяет вырезанный вокал, фон остаётся как в оригинале. amix normalize=0 НЕ делит входы пополам
/// (питон-дефолт=1 занизил бы фон ~в 0.5). aformat=cl=stereo снимает нестандартный mono layout hound-дубляжа
/// (уровень не меняет). Пики/итоговую громкость держит финальный loudnorm на смиксованной дорожке.
pub fn mix(voice: &Path, music: &Path, out: &Path) -> Result<(), String> {
    let fc = "[0:a]aformat=channel_layouts=stereo[v];\
              [1:a]aformat=channel_layouts=stereo[m];\
              [v][m]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0[a]";
    let mut args: Vec<&OsStr> = vec![
        OsStr::new("-y"), OsStr::new("-i"), voice.as_os_str(), OsStr::new("-i"), music.as_os_str(),
        OsStr::new("-filter_complex"), OsStr::new(fc), OsStr::new("-map"), OsStr::new("[a]"),
    ];
    args.extend(lossless_out());
    args.push(out.as_os_str());
    run_ff(&args)
}

/// Сведение диалог+фон с САЙДЧЕЙН-ДАКИНГОМ — проф. практика дубляжа: компрессор приглушает фон
/// ТОЛЬКО пока звучит голос (attack 150мс / release 600мс — плавные подныривания), в паузах фон
/// живёт ровно 1:1 — атмосфера НЕ гробится статичным резом (требование юзера: «дубляж громче фона,
/// но фон не гробить»). Замер: фон мультика -17.1 LUFS ≈ уровню голоса — без дакинга речь тонет.
/// threshold 0.02 / ratio 8 ≈ -6..-9 дБ фону под фразой. Порядок sidechaincompress: [фон][ключ-голос].
pub fn mix_ducked(voice: &Path, music: &Path, out: &Path) -> Result<(), String> {
    let fc = "[0:a]aformat=channel_layouts=stereo,asplit=2[v][vkey];\
              [1:a]aformat=channel_layouts=stereo[m];\
              [m][vkey]sidechaincompress=threshold=0.02:ratio=8:attack=150:release=600:makeup=1[bg];\
              [v][bg]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0[a]";
    let mut args: Vec<&OsStr> = vec![
        OsStr::new("-y"), OsStr::new("-i"), voice.as_os_str(), OsStr::new("-i"), music.as_os_str(),
        OsStr::new("-filter_complex"), OsStr::new(fc), OsStr::new("-map"), OsStr::new("[a]"),
    ];
    args.extend(lossless_out());
    args.push(out.as_os_str());
    run_ff(&args)
}

/// Речевой блок на таймлайне [start,end] — слитые по паузе <1.6с реплики дубляжа. Границы берутся из
/// ФАКТИЧЕСКИ уложенных сегментов (onset + длительность fit-файла), а не из сырых proj.segments.
#[derive(Clone, Copy, Debug)]
pub struct SpeechBlock {
    pub start: f64,
    pub end: f64,
}

// Параметры детерминированной огибающей дакинга.
// ВАЖНО: в ДУБЛЯЖЕ фон = сепарированный инструментал (= M&E, оригинального голоса там НЕТ). Проф.практика
// дубляжа (Netflix/Deepdub M&E): M&E микшируется на ПОЛНОМ уровне с новым дубляжом — его НЕ душат, дубляж
// просто занимает место оригинального диалога. Поэтому дакинг мягкий (−3 дБ, лёгкий провал для разборчивости
// синтет-голоса), фон остаётся слышен. Env DUB_DUCK_DB (dB, 0 = совсем не душить). Прошлые −12 дБ «срезали
// весь фон» (подкаст/закадр-техника, ошибочно на дубляж). [[reference_parakeet_rs_ort_gotchas]] — «фон не глушить».
const DUCK_GAIN: f64 = 0.708; // −3 дБ под речью (дефолт); env DUB_DUCK_DB override
const DUCK_PREROLL: f64 = 0.08; // fade-down стартует за 0.08с ДО блока
const DUCK_FADE_DOWN: f64 = 0.10; // длина спуска
const DUCK_HOLD: f64 = 0.30; // держим приглушение 0.30с ПОСЛЕ конца блока
const DUCK_FADE_UP: f64 = 0.40; // длина подъёма

/// Глубина дакинга фона (линейный gain 0..1) под речью в ДУБЛЯЖЕ. Env DUB_DUCK_DB (дБ; 0 = без дакинга,
/// отрицательное = приглушать). Дефолт DUCK_GAIN (−3 дБ). Клэмп в [0.02, 1.0].
fn duck_gain() -> f64 {
    match std::env::var("DUB_DUCK_DB").ok().and_then(|s| s.trim().parse::<f64>().ok()) {
        Some(db) => 10f64.powf(db / 20.0).clamp(0.02, 1.0),
        None => DUCK_GAIN,
    }
}

/// Собрать выражение gain(t) для ffmpeg-фильтра `volume` из речевых блоков: 1.0 вне блоков, DUCK_GAIN
/// внутри, линейные фейды на краях (down за DUCK_PREROLL до start, up через DUCK_HOLD после end).
/// Детерминированно и с точными dB — компрессор (sidechaincompress) реагировал на мгновенную амплитуду
/// TTS и давал «качели» на микропаузах; здесь огибающая задана таймингами, а не сигналом.
/// Форма: gain(t) = 1 − (1−g)·Σ trap_i(t), где trap_i — трапеция блока i (clip(min(рампа-вниз,
/// рампа-вверх),0,1)). Блоки разведены паузой ≥1.6с > preroll+fade+hold+fade — трапеции не пересекаются,
/// сумма ≤ 1. Длина выражения O(N) по блокам (вложенные if давали O(2^N) — дубль prev в обеих ветках).
fn duck_volume_expr(blocks: &[SpeechBlock], g: f64) -> String {
    if blocks.is_empty() {
        return "1".into();
    }
    // Трапеция блока: (t-ds)/fd растёт 0->1 на спуске, (ue-t)/fu убывает 1->0 на подъёме; между ними
    // обе ≥1 -> clip даёт полку 1 (полное приглушение). Вне [ds,ue] одна из рамп ≤0 -> clip даёт 0.
    let traps: Vec<String> = blocks
        .iter()
        .map(|b| {
            let ds = (b.start - DUCK_PREROLL).max(0.0); // старт спуска
            let us = b.end + DUCK_HOLD; // старт подъёма (после hold)
            let ue = us + DUCK_FADE_UP; // конец подъёма = снова 1.0
            format!(
                "clip(min((t-{ds:.3})/{fd:.3},({ue:.3}-t)/{fu:.3}),0,1)",
                fd = DUCK_FADE_DOWN,
                fu = DUCK_FADE_UP
            )
        })
        .collect();
    // clip суммы в [0,1] (#116, находка [0]): при большом tempo-fit границы блоков делятся, а фейд-константы
    // нет — соседние трапеции могут пересечься, sum>1 дало бы gain<g и даже <0 (инверсия фазы). clip держит
    // gain в [g,1].
    format!("1-{d:.4}*clip({sum},0,1)", d = 1.0 - g, sum = traps.join("+"))
}

/// Сведение диалог+фон с ДЕТЕРМИНИРОВАННОЙ ОГИБАЮЩЕЙ дакинга (#106). Фон приглушается по кусочно-линейной
/// огибающей громкости, построенной из ТОЧНЫХ таймингов речевых блоков (а не по мгновенной амплитуде
/// TTS, как sidechaincompress — тот давал «качели» на микропаузах внутри фраз). volume с выражением от t
/// (eval=frame). На длинных видео сотни блоков -> выражение большое: filtergraph пишем в файл через
/// `-filter_complex_script` (лимит CreateProcess 32767, паттерн из dub-captions/burn.rs). Порядок как в
/// mix: голос полным уровнем + фон по огибающей, amix normalize=0. Пусто блоков -> фон ровно 1.0.
pub fn mix_env(voice: &Path, music: &Path, blocks: &[SpeechBlock], out: &Path) -> Result<(), String> {
    // Дубляж: глубина дакинга фона = duck_gain() (−3 дБ дефолт, env DUB_DUCK_DB).
    mix_env_g(voice, music, blocks, duck_gain(), out)
}

/// Динамический дакинг с ЗАДАННОЙ глубиной (дБ) — для ЗАКАДРА (UN-style voice-over): оригинал звучит
/// ПОЛНЫМ между репликами перевода (слышно исходного спикера/эмоцию) и приглушается на `duck_db` ПОД
/// переводом, восстанавливаясь после. `bed` — весь оригинал, `blocks` — тайминги переведённой речи.
/// Прежде оригинал давился ПЛОСКО на всю дорожку (−12 дБ навсегда, в т.ч. в паузах) — не по практике.
pub fn mix_env_db(voice: &Path, bed: &Path, blocks: &[SpeechBlock], duck_db: f64, out: &Path) -> Result<(), String> {
    let g = 10f64.powf(duck_db / 20.0).clamp(0.02, 1.0);
    mix_env_g(voice, bed, blocks, g, out)
}

fn mix_env_g(voice: &Path, music: &Path, blocks: &[SpeechBlock], g: f64, out: &Path) -> Result<(), String> {
    let vol = duck_volume_expr(blocks, g);
    let fc = format!(
        "[0:a]aformat=channel_layouts=stereo[v];\
         [1:a]aformat=channel_layouts=stereo,volume='{vol}':eval=frame[bg];\
         [v][bg]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0[a]"
    );
    // Граф — в файл: выражение огибающей на сотнях блоков раздувает cmdline за лимит CreateProcess.
    let script = out.with_extension("envfilter");
    std::fs::write(&script, &fc).map_err(|e| t!("media-env-filter-script", error = e.to_string()))?;
    // Таймаут пропорционален длине музыки (eval=frame дорог на многочасовом): max(600с, 2×длит.).
    let secs = (duration(music).unwrap_or(0.0) * 2.0).max(600.0) as u64;
    let mut args: Vec<&OsStr> = vec![
        OsStr::new("-y"), OsStr::new("-i"), voice.as_os_str(), OsStr::new("-i"), music.as_os_str(),
        OsStr::new(dub_captions::burn::filter_script_flag()), script.as_os_str(),
        OsStr::new("-map"), OsStr::new("[a]"),
    ];
    args.extend(lossless_out());
    args.push(out.as_os_str());
    let r = run_ff_timeout(&args, secs);
    let _ = std::fs::remove_file(&script); // прибрать временный filter-скрипт (в т.ч. при ошибке)
    r
}

/// Финальная нормализация программы по EBU R128 (ffmpeg loudnorm): интегральная громкость к I LUFS и
/// true-peak лимитер к TP dBTP. Решение юзера (best-practice, НЕ питон): ставится последним шагом на
/// смиксованную дорожку — держит целевую громкость соцсетей и ловит межфразовые суммы и микс с фоном
/// (пики отдельной фразы до этого опускает пофразный лимитер normalize_voice). Выход без потерь: после
/// loudnorm уже нет перекодирования, которое сдвинуло бы true-peak.
pub fn loudnorm(src: &Path, dst: &Path, i: f64, tp: f64, lra: f64) -> Result<(), String> {
    let af = format!("loudnorm=I={i}:TP={tp}:LRA={lra}");
    let mut args: Vec<&OsStr> = vec![
        OsStr::new("-y"), OsStr::new("-i"), src.as_os_str(),
        OsStr::new("-af"), OsStr::new(&af),
    ];
    args.extend(lossless_out());
    args.push(dst.as_os_str());
    run_ff(&args)
}

/// Усилить всю дорожку на `gain_db` dB (монтажный гейн, наша opt-in фича). Выход без потерь.
pub fn gain(src: &Path, dst: &Path, gain_db: f64) -> Result<(), String> {
    let af = format!("volume={gain_db}dB");
    let mut args: Vec<&OsStr> = vec![
        OsStr::new("-y"), OsStr::new("-i"), src.as_os_str(),
        OsStr::new("-af"), OsStr::new(&af),
    ];
    args.extend(lossless_out());
    args.push(dst.as_os_str());
    run_ff(&args)
}

fn preview_aac_args(src: &Path, dst: &Path) -> Vec<OsString> {
    os_args(&[
        OsStr::new("-y"), OsStr::new("-i"), src.as_os_str(),
        OsStr::new("-vn"), OsStr::new("-af"), OsStr::new(SYNC_AF),
        OsStr::new("-ac"), OsStr::new("2"), OsStr::new("-ar"), OsStr::new(MIX_SR),
        OsStr::new("-c:a"), OsStr::new("aac"), OsStr::new("-b:a"), OsStr::new("192k"), dst.as_os_str(),
    ])
}

/// Дорожка для прослушивания в редакторе (<audio> WebView): одно кодирование AAC из несжатого
/// микса. Источник может быть и видео (nodub: звук оригинала), поэтому -vn.
pub fn encode_preview_aac(src: &Path, dst: &Path) -> Result<(), String> {
    run_ff_args(&preview_aac_args(src, dst))
}

/// Смуксить видео (copy) + аудио (aac). БЕЗ -shortest (выход по длиннейшему потоку). Порт media.mux.
/// -af aformat=cl=stereo нормализует channel layout (mono-дубляж от hound = "1 channels (FL)", AAC
/// его отвергает -22); на уже-стерео входе это no-op. +faststart двигает moov в начало (мгновенный
/// старт воспроизведения по сети/в webview) — на mp4 no-op для локального файла, но полезен при раздаче.
pub fn mux(video: &Path, audio: &Path, out: &Path) -> Result<(), String> {
    run_ff(&[
        OsStr::new("-y"), OsStr::new("-i"), video.as_os_str(), OsStr::new("-i"), audio.as_os_str(),
        OsStr::new("-map"), OsStr::new("0:v:0"), OsStr::new("-map"), OsStr::new("1:a:0?"),
        OsStr::new("-af"), OsStr::new("aformat=channel_layouts=stereo"),
        // Дубляж-микс перекодируется в AAC — держим высокий битрейт (256k), чтобы не терять качество (дефолт
        // ffmpeg ~128k занижал звук). Это ТОЛЬКО для сгенерированного дубляжа; оригинал идёт через mux_keep_audio.
        OsStr::new("-c:v"), OsStr::new("copy"), OsStr::new("-c:a"), OsStr::new("aac"),
        OsStr::new("-b:a"), OsStr::new("256k"),
        OsStr::new("-movflags"), OsStr::new("+faststart"), out.as_os_str(),
    ])
}

/// Смуксить видео (copy) + ОРИГИНАЛЬНУЮ аудиодорожку БЕЗ перекодирования (`-c:a copy`). Для режимов, где
/// звук не трогаем (субтитры/транскрипт): сохраняем оригинал байт-в-байт — каналы (5.1/стерео), частоту,
/// битрейт, кодек. НИКАКОГО aformat/downmix (он рушил 6ch→stereo) и никакого переэнкода (терял качество).
pub fn mux_keep_audio(video: &Path, source_with_audio: &Path, out: &Path) -> Result<(), String> {
    run_ff(&[
        OsStr::new("-y"), OsStr::new("-i"), video.as_os_str(), OsStr::new("-i"), source_with_audio.as_os_str(),
        OsStr::new("-map"), OsStr::new("0:v:0"), OsStr::new("-map"), OsStr::new("1:a:0?"),
        OsStr::new("-c:v"), OsStr::new("copy"), OsStr::new("-c:a"), OsStr::new("copy"),
        OsStr::new("-movflags"), OsStr::new("+faststart"), out.as_os_str(),
    ])
}

/// ISO 639-1 (2-буквенный код Whisper/UI) -> ISO 639-2/B (3-буквенный код дорожки контейнера, `language`
/// в metadata:s:a). Покрывает весь WHISPER_LANGS (99 языков) + алиасы. Незнакомый код -> "und" (undefined).
pub fn iso639_1_to_2(code: &str) -> &'static str {
    match code.trim().to_lowercase().as_str() {
        "en" => "eng", "zh" => "zho", "de" => "deu", "es" => "spa", "ru" => "rus",
        "ko" => "kor", "fr" => "fra", "ja" => "jpn", "pt" => "por", "tr" => "tur",
        "pl" => "pol", "ca" => "cat", "nl" => "nld", "ar" => "ara", "sv" => "swe",
        "it" => "ita", "id" => "ind", "hi" => "hin", "fi" => "fin", "vi" => "vie",
        "he" => "heb", "uk" => "ukr", "el" => "ell", "ms" => "msa", "cs" => "ces",
        "ro" => "ron", "da" => "dan", "hu" => "hun", "ta" => "tam", "no" => "nor",
        "th" => "tha", "ur" => "urd", "hr" => "hrv", "bg" => "bul", "lt" => "lit",
        "la" => "lat", "mi" => "mri", "ml" => "mal", "cy" => "cym", "sk" => "slk",
        "te" => "tel", "fa" => "fas", "lv" => "lav", "bn" => "ben", "sr" => "srp",
        "az" => "aze", "sl" => "slv", "kn" => "kan", "et" => "est", "mk" => "mkd",
        "br" => "bre", "eu" => "eus", "is" => "isl", "hy" => "hye", "ne" => "nep",
        "mn" => "mon", "bs" => "bos", "kk" => "kaz", "sq" => "sqi", "sw" => "swa",
        "gl" => "glg", "mr" => "mar", "pa" => "pan", "si" => "sin", "km" => "khm",
        "sn" => "sna", "yo" => "yor", "so" => "som", "af" => "afr", "oc" => "oci",
        "ka" => "kat", "be" => "bel", "tg" => "tgk", "sd" => "snd", "gu" => "guj",
        "am" => "amh", "yi" => "yid", "lo" => "lao", "uz" => "uzb", "fo" => "fao",
        "ht" => "hat", "ps" => "pus", "tk" => "tuk", "nn" => "nno", "mt" => "mlt",
        "sa" => "san", "lb" => "ltz", "my" => "mya", "bo" => "bod", "tl" => "tgl",
        "mg" => "mlg", "as" => "asm", "tt" => "tat", "haw" => "haw", "ln" => "lin",
        "ha" => "hau", "ba" => "bak", "jw" => "jav", "su" => "sun", "yue" => "yue",
        _ => "und",
    }
}

/// Смуксить видео (copy) + ДВЕ звуковые дорожки: дубляж (default, 1-я) + оригинал (2-я). MP4/MKV по
/// расширению `out`. Дубляж перекодируется в AAC 256k (сгенерированный микс), а оригинал КОПИРУЕТСЯ без
/// перекода (`-c:a copy`) — сохраняем каналы (5.1)/частоту/битрейт/кодек как есть, никакой деградации.
/// БЕЗ -shortest. Метки языка (ISO 639-2) и человекочитаемые title'ы кладутся в metadata дорожек;
/// disposition:a:0 default помечает дубляж дорожкой по умолчанию, оригинал — недефолтной. +faststart на mp4.
#[allow(clippy::too_many_arguments)]
pub fn mux_multitrack(
    video: &Path,
    dub_audio: &Path,
    orig_source: &Path,
    out: &Path,
    dub_lang: &str,
    orig_lang: &str,
    dub_title: &str,
    orig_title: &str,
) -> Result<(), String> {
    let dl = format!("language={dub_lang}");
    let dt = format!("title={dub_title}");
    let ol = format!("language={orig_lang}");
    let ot = format!("title={orig_title}");
    let is_mp4 = out
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mp4"))
        .unwrap_or(true);
    let mut args: Vec<&OsStr> = vec![
        OsStr::new("-y"),
        OsStr::new("-i"), video.as_os_str(),
        OsStr::new("-i"), dub_audio.as_os_str(),
        OsStr::new("-i"), orig_source.as_os_str(),
        OsStr::new("-map"), OsStr::new("0:v:0"),
        OsStr::new("-map"), OsStr::new("1:a:0"),
        // без `?`: вызов гарантирует аудио в источнике (has_audio) — иначе -disposition:a:1 валит команду.
        OsStr::new("-map"), OsStr::new("2:a:0"),
        OsStr::new("-c:v"), OsStr::new("copy"),
        // дубляж (a:0) — сгенерированный микс, перекод в AAC на высоком битрейте (256k)
        OsStr::new("-c:a:0"), OsStr::new("aac"), OsStr::new("-b:a:0"), OsStr::new("256k"),
        OsStr::new("-metadata:s:a:0"), OsStr::new(&dl),
        OsStr::new("-metadata:s:a:0"), OsStr::new(&dt),
        // оригинал (a:1) — КОПИЯ без перекода: сохраняем каналы (5.1)/частоту/битрейт/кодек как в источнике
        OsStr::new("-c:a:1"), OsStr::new("copy"),
        OsStr::new("-metadata:s:a:1"), OsStr::new(&ol),
        OsStr::new("-metadata:s:a:1"), OsStr::new(&ot),
        OsStr::new("-disposition:a:0"), OsStr::new("default"),
        OsStr::new("-disposition:a:1"), OsStr::new("0"),
    ];
    if is_mp4 {
        args.push(OsStr::new("-movflags"));
        args.push(OsStr::new("+faststart"));
    }
    args.push(out.as_os_str());
    run_ff(&args)
}

/// Ремукс лёгкого playable output.mp4 из мультитрек-mkv (#116): видео + ПЕРВАЯ (дубляж) дорожка, copy
/// без перекодирования (доли секунды) + faststart. Плеер редактора (WebView2 не играет Matroska) тянет
/// этот mp4, а «Сохранить» отдаёт полный mkv.
pub fn remux_playable_mp4(mkv: &Path, out_mp4: &Path) -> Result<(), String> {
    run_ff(&[
        OsStr::new("-y"), OsStr::new("-i"), mkv.as_os_str(),
        OsStr::new("-map"), OsStr::new("0:v:0"), OsStr::new("-map"), OsStr::new("0:a:0"),
        OsStr::new("-c"), OsStr::new("copy"),
        OsStr::new("-movflags"), OsStr::new("+faststart"), out_mp4.as_os_str(),
    ])
}

/// Есть ли в файле аудиопоток (ffprobe). Для мультитрек-mux: нет аудио в источнике -> вторую дорожку
/// не добавляем (иначе -disposition:a:1 по несуществующему потоку валит ffmpeg). Ошибка probe -> false.
pub fn has_audio(input: &Path) -> bool {
    Command::new(FFPROBE)
        .args(["-v", "error", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0"])
        .arg(input)
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Сконвертировать аудио в PCM 16-bit WAV (стерео). Финальный формат аудио-режима (вход без видео):
/// пачка WAV -> пачка озвученных WAV. Микс приходит несжатым float, здесь только квантование в 16 бит.
pub fn to_wav(src: &Path, dst: &Path) -> Result<(), String> {
    run_ff(&[
        OsStr::new("-y"), OsStr::new("-i"), src.as_os_str(),
        OsStr::new("-af"), OsStr::new("aformat=channel_layouts=stereo"),
        OsStr::new("-c:a"), OsStr::new("pcm_s16le"), dst.as_os_str(),
    ])
}

fn cut_args(src: &Path, dst: &Path, start: f64, end: f64, sr: u32, ac: u32) -> Vec<OsString> {
    let (ss, to, ar, ac) = (format!("{start:.3}"), format!("{end:.3}"), sr.to_string(), ac.to_string());
    os_args(&[
        OsStr::new("-y"), OsStr::new("-ss"), OsStr::new(&ss), OsStr::new("-to"), OsStr::new(&to),
        OsStr::new("-i"), src.as_os_str(), OsStr::new("-af"), OsStr::new(SYNC_AF), OsStr::new("-ac"), OsStr::new(&ac),
        OsStr::new("-ar"), OsStr::new(&ar), dst.as_os_str(),
    ])
}

/// Вырезать [start,end] в mono @ sr Гц. Порт media.trim(..., sr=16000): keep-сплайс 24к, клипы голоса 16к.
/// Пишет атомарно (seg-файлы — кэш по существованию).
pub fn trim(src: &Path, dst: &Path, start: f64, end: f64, sr: u32) -> Result<(), String> {
    cut(src, dst, start, end, sr, 1)
}

/// Вырезать [start,end] в sr Гц с ac каналами (вход сепарации — 44.1 кГц стерео). Пишет атомарно.
pub fn cut(src: &Path, dst: &Path, start: f64, end: f64, sr: u32, ac: u32) -> Result<(), String> {
    dub_core::atomic::write_with(dst, |tmp| run_ff_args(&cut_args(src, tmp, start, end, sr, ac)))
}

/// Фейды 15 мс на обоих краях рефа: разворот клипа даёт фейду в конце ту же точку отсчёта, что в начале,
/// без знания длины.
const REF_FADES: &str = "afade=t=in:d=0.015:curve=hsin,areverse,afade=t=in:d=0.015:curve=hsin,areverse";

/// Метка стемов проекта: из звука какого извлечения они посчитаны.
const STEMS_MARK: &str = "source.ver";
/// Версия извлечения звука, из которого сепарируются стемы проекта: поднимать вместе с изменением
/// извлечения (SYNC_AF). Стемы прежнего извлечения на файле с дырами в метках расходятся с картинкой.
pub const STEMS_SOURCE_VER: &str = "sync-v2";

/// Посчитаны ли стемы проекта (`wd/stems`) из звука текущего извлечения.
pub fn stems_current(wd: &Path) -> Result<bool, String> {
    let mark = wd.join("stems").join(STEMS_MARK);
    match std::fs::read_to_string(&mark) {
        Ok(v) => Ok(v.trim() == STEMS_SOURCE_VER),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", mark.display())),
    }
}

/// Стемы проекта из звука прежнего извлечения удаляются вместе с audio_hq.wav и чистым 16k-вокалом
/// (analyze берёт их по существованию): их посчитают заново. true — было что удалить.
pub fn drop_stale_separation(wd: &Path) -> Result<bool, String> {
    let stems = wd.join("stems");
    if !stems.is_dir() || stems_current(wd)? {
        return Ok(false);
    }
    std::fs::remove_dir_all(&stems).map_err(|e| format!("{}: {e}", stems.display()))?;
    for name in ["audio_hq.wav", "vocals16_clean.wav"] {
        let path = wd.join(name);
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(true)
}

/// Отметить стемы `stems` как посчитанные из звука текущего извлечения.
pub fn mark_separation(stems: &Path) -> Result<(), String> {
    let mark = stems.join(STEMS_MARK);
    std::fs::write(&mark, STEMS_SOURCE_VER).map_err(|e| format!("{}: {e}", mark.display()))
}

/// Самая высокая частота рефа: выше неё wav-мюксер ffmpeg пишет заголовок EXTENSIBLE, который Higgs не
/// читает (голос, записанный на 96 или 192 кГц).
const REF_MAX_RATE: u32 = 48_000;

fn trim_ref_args(src: &Path, dst: &Path, start: f64, end: f64, rate: Option<u32>) -> Vec<OsString> {
    let (ss, to, af) = (format!("{start:.3}"), format!("{end:.3}"), format!("{SYNC_AF},{REF_FADES}"));
    let mut args = os_args(&[
        OsStr::new("-y"), OsStr::new("-ss"), OsStr::new(&ss), OsStr::new("-to"), OsStr::new(&to),
        OsStr::new("-i"), src.as_os_str(), OsStr::new("-vn"), OsStr::new("-af"), OsStr::new(&af),
        OsStr::new("-ac"), OsStr::new("1"),
    ]);
    if let Some(rate) = rate {
        args.extend([OsString::from("-ar"), OsString::from(rate.to_string())]);
    }
    args.extend([OsString::from("-c:a"), OsString::from("pcm_s16le"), dst.as_os_str().to_os_string()]);
    args
}

/// Частота первого звукового потока файла (ffprobe).
pub fn audio_rate(src: &Path) -> Result<u32, String> {
    let out = Command::new(FFPROBE)
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=sample_rate", "-of", "csv=p=0"])
        .arg(src)
        .output()
        .map_err(|e| t!("media-ffprobe-start", error = e.to_string()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    match (out.status.success(), text.trim().parse::<u32>()) {
        (true, Ok(rate)) if rate > 0 => Ok(rate),
        _ => Err(t!("media-no-sample-rate", path = src.display().to_string(), stderr = String::from_utf8_lossy(&out.stderr).trim().to_string())),
    }
}

/// Реф клона: [start,end] источника моно на его собственной частоте (не выше 48 кГц), PCM16 с простым
/// заголовком (float WAV и заголовок EXTENSIBLE Higgs не читает), фейды 15 мс на краях. Частоту движок
/// приводит к своей сам. Пишет атомарно.
pub fn trim_ref(src: &Path, dst: &Path, start: f64, end: f64) -> Result<(), String> {
    let rate = audio_rate(src)?;
    let out_rate = (rate > REF_MAX_RATE).then_some(REF_MAX_RATE);
    dub_core::atomic::write_with(dst, |tmp| run_ff_args(&trim_ref_args(src, tmp, start, end, out_rate)))
}

// ─── Оконная нарезка для полнометражного пайплайна (#79) ──────────────────────────────────────────
// Новые хелперы (не трогают существующие): вырезать ОДНО окно вокала в отдельный WAV (RAM O(окна)),
// либо нарезать весь вокал segment-muxer'ом одним проходом. `-reset_timestamps 1` даёт локальный t=0 в
// каждом окне (чистый клип для BSRoformer/Sortformer/ASR) — обратный сдвиг window_offset делает
// вызывающий при сшивке диаризации/ASR (dub_asr::Window::offset). Хардненные флаги для длинных входов
// (BORROWINGS #19): +discardcorrupt / avoid_negative_ts / max_muxing_queue_size — против timestamp-drift
// и 'Too many packets buffered' на часовых файлах.

/// Вырезать окно [start,end) вокала в отдельный WAV @ sr Гц (mono). Локальный t=0 (accurate seek:
/// -ss ПОСЛЕ -i для точного реза по сэмплу). Для стадийной обработки одного окна — RAM O(окна).
// allow(dead_code): вызывается оркестратором оконного пайплайна (интеграция #79 идёт отдельно).
#[allow(dead_code)]
pub fn slice_window(src: &Path, dst: &Path, start: f64, end: f64, sr: u32) -> Result<(), String> {
    let ss = format!("{:.3}", start);
    let to = format!("{:.3}", end);
    let ar = sr.to_string();
    run_ff(&[
        OsStr::new("-y"),
        OsStr::new("-fflags"), OsStr::new("+discardcorrupt"),
        OsStr::new("-i"), src.as_os_str(),
        // accurate seek внутри уже открытого потока (после -i) — точная граница окна
        OsStr::new("-ss"), OsStr::new(&ss), OsStr::new("-to"), OsStr::new(&to),
        OsStr::new("-ac"), OsStr::new("1"), OsStr::new("-ar"), OsStr::new(&ar),
        OsStr::new("-avoid_negative_ts"), OsStr::new("make_zero"),
        OsStr::new("-c:a"), OsStr::new("pcm_s16le"),
        dst.as_os_str(),
    ])
}

/// Нарезать весь вокал на окна фикс. длины `win_sec` segment-muxer'ом за ОДИН проход (RAM O(окна),
/// не O(файла)). Файлы пишутся по шаблону `pattern` с `%03d` (например `win_%03d.wav`).
/// `-reset_timestamps 1` -> каждый сегмент стартует с t=0; window_offset = idx*win_sec прибавляет
/// вызывающий при ре-базинге. Дешёвый фолбэк к min-cut нарезке, когда важна только RAM-локальность.
// allow(dead_code): вызывается оркестратором оконного пайплайна (интеграция #79 идёт отдельно).
#[allow(dead_code)]
pub fn segment_wav(src: &Path, out_dir: &Path, pattern: &str, win_sec: f64, sr: u32) -> Result<(), String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    let seg_time = format!("{:.3}", win_sec.max(0.1));
    let ar = sr.to_string();
    let out_tpl = out_dir.join(pattern);
    run_ff(&[
        OsStr::new("-y"),
        OsStr::new("-fflags"), OsStr::new("+discardcorrupt"),
        OsStr::new("-i"), src.as_os_str(),
        OsStr::new("-ac"), OsStr::new("1"), OsStr::new("-ar"), OsStr::new(&ar),
        OsStr::new("-c:a"), OsStr::new("pcm_s16le"),
        OsStr::new("-f"), OsStr::new("segment"),
        OsStr::new("-segment_time"), OsStr::new(&seg_time),
        OsStr::new("-reset_timestamps"), OsStr::new("1"),
        OsStr::new("-max_muxing_queue_size"), OsStr::new("2048"),
        out_tpl.as_os_str(),
    ])
}

#[cfg(test)]
mod env_tests {
    use super::*;

    /// Референс-огибающая: та же кусочно-линейная функция, что генерит duck_volume_expr, но на Rust —
    /// проверяем ключевые точки (вне блока 1.0, в центре блока DUCK_GAIN, середины фейдов).
    // Референс gain(t): та же формула, что в duck_volume_expr — СУММА трапеций с clip суммы в [0,1]
    // (не return по первой трапеции), чтобы тест ловил пересечение блоков.
    fn gain_at(t: f64, blocks: &[SpeechBlock]) -> f64 {
        let g = DUCK_GAIN;
        let mut sum = 0.0f64;
        for b in blocks {
            let ds = (b.start - DUCK_PREROLL).max(0.0);
            let us = b.end + DUCK_HOLD;
            let ue = us + DUCK_FADE_UP;
            let trap = ((t - ds) / DUCK_FADE_DOWN).min((ue - t) / DUCK_FADE_UP).clamp(0.0, 1.0);
            sum += trap;
        }
        1.0 - (1.0 - g) * sum.clamp(0.0, 1.0)
    }

    #[test]
    fn envelope_key_points() {
        let blocks = [SpeechBlock { start: 5.0, end: 8.0 }, SpeechBlock { start: 20.0, end: 22.0 }];
        // вне любого блока -> 1.0
        assert!((gain_at(0.0, &blocks) - 1.0).abs() < 1e-9);
        assert!((gain_at(15.0, &blocks) - 1.0).abs() < 1e-9);
        // центр блока -> полное приглушение
        assert!((gain_at(6.5, &blocks) - DUCK_GAIN).abs() < 1e-9);
        // до начала спуска (за preroll) ещё 1.0
        assert!((gain_at(5.0 - DUCK_PREROLL - 0.01, &blocks) - 1.0).abs() < 1e-9);
        // середина fade-down: между 1.0 и g
        let mid_down = gain_at(5.0 - DUCK_PREROLL + DUCK_FADE_DOWN / 2.0, &blocks);
        assert!(mid_down < 1.0 && mid_down > DUCK_GAIN);
        // hold после конца блока ещё приглушено
        assert!((gain_at(8.0 + DUCK_HOLD - 0.01, &blocks) - DUCK_GAIN).abs() < 1e-9);
    }

    #[test]
    fn empty_blocks_is_flat_one() {
        assert_eq!(duck_volume_expr(&[], DUCK_GAIN), "1");
    }

    #[test]
    fn expr_mentions_each_block_boundaries() {
        let blocks = [SpeechBlock { start: 1.0, end: 2.0 }];
        let e = duck_volume_expr(&blocks, DUCK_GAIN);
        // спуск стартует за preroll (0.920), глубина 1-g присутствует
        assert!(e.contains("0.920"), "{e}");
        assert!(e.contains(&format!("{:.4}", 1.0 - DUCK_GAIN)), "{e}");
    }

    #[test]
    fn expr_length_linear_in_blocks() {
        // Длина выражения растёт линейно по блокам (вложенные if давали O(2^N) и взрывали память).
        let blocks: Vec<SpeechBlock> = (0..300)
            .map(|i| SpeechBlock { start: i as f64 * 10.0, end: i as f64 * 10.0 + 5.0 })
            .collect();
        let e = duck_volume_expr(&blocks, DUCK_GAIN);
        assert!(e.len() < 60 * 300 + 64, "len={}", e.len());
    }

    #[test]
    fn expr_has_clip_wrapper() {
        // clip(...,0,1) держит sum трапеций в [0,1] -> gain не ниже g и не отрицательный (#116).
        assert!(duck_volume_expr(&[SpeechBlock { start: 1.0, end: 2.0 }], DUCK_GAIN).contains("clip("));
    }

    #[test]
    fn overlapping_blocks_gain_never_below_g() {
        // Два блока ВПЛОТНУЮ (пауза 0.05с < суммы фейдов) — трапеции пересекаются, sum>1. Без clip gain
        // ушёл бы ниже g и в минус (инверсия фазы). С clip — держится в [g,1] на всей шкале.
        let blocks = [SpeechBlock { start: 1.0, end: 2.0 }, SpeechBlock { start: 2.05, end: 3.0 }];
        let mut t = 0.0;
        while t < 4.0 {
            let gv = gain_at(t, &blocks);
            assert!(gv >= DUCK_GAIN - 1e-9 && gv <= 1.0 + 1e-9, "t={t} gain={gv}");
            t += 0.01;
        }
        // в зоне стыка приглушение полное (обе трапеции=1, clip=1 -> g)
        assert!((gain_at(2.02, &blocks) - DUCK_GAIN).abs() < 1e-9);
    }
}

#[cfg(test)]
mod iso639_tests {
    use super::iso639_1_to_2;

    #[test]
    fn known_codes_map_to_iso639_2() {
        assert_eq!(iso639_1_to_2("ru"), "rus");
        assert_eq!(iso639_1_to_2("en"), "eng");
        assert_eq!(iso639_1_to_2("de"), "deu");
        assert_eq!(iso639_1_to_2("ja"), "jpn");
        assert_eq!(iso639_1_to_2("zh"), "zho");
        assert_eq!(iso639_1_to_2("uk"), "ukr");
        // 3-буквенные входы Whisper (haw/yue) тоже покрыты
        assert_eq!(iso639_1_to_2("haw"), "haw");
        assert_eq!(iso639_1_to_2("yue"), "yue");
    }

    #[test]
    fn case_and_whitespace_insensitive() {
        assert_eq!(iso639_1_to_2("RU"), "rus");
        assert_eq!(iso639_1_to_2("  Fr  "), "fra");
    }

    #[test]
    fn unknown_or_auto_is_und() {
        assert_eq!(iso639_1_to_2("auto"), "und");
        assert_eq!(iso639_1_to_2(""), "und");
        assert_eq!(iso639_1_to_2("xx"), "und");
    }

    #[test]
    fn every_whisper_code_has_mapping() {
        // Незнакомый код -> "und". Все коды из WHISPER_LANGS должны маппиться в НЕ-"und".
        for (code, _) in dub_translate::WHISPER_LANGS {
            assert_ne!(iso639_1_to_2(code), "und", "no ISO 639-2 mapping for {code}");
        }
    }
}

#[cfg(test)]
mod lossless_tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dub_media_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sine_wav(path: &Path, sr: u32, channels: u16, amp: f32, secs: f32) {
        let spec = hound::WavSpec { channels, sample_rate: sr, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(sr as f32 * secs) as usize {
            let v = amp * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr as f32).sin();
            for _ in 0..channels {
                w.write_sample(v).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    fn read(path: &Path) -> (hound::WavSpec, Vec<f32>) {
        let mut r = hound::WavReader::open(path).unwrap();
        let spec = r.spec();
        let s: Vec<f32> = r.samples::<f32>().map(|x| x.unwrap()).collect();
        (spec, s)
    }

    #[test]
    fn mix_and_loudnorm_stay_lossless_float_at_the_mix_rate() {
        let d = tmp("mix");
        let voice = d.join("dub_vocals.wav");
        let music = d.join("instrumental.wav");
        sine_wav(&voice, 24_000, 1, 0.8, 1.0);
        sine_wav(&music, 44_100, 2, 0.8, 1.0);
        let mixed = d.join("new_audio.wav");
        mix(&voice, &music, &mixed).unwrap();
        let (spec, s) = read(&mixed);
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        assert_eq!(spec.sample_rate, 44_100);
        let peak = s.iter().fold(0.0f32, |p, v| p.max(v.abs()));
        assert!(peak > 1.2, "сумма голоса и фона выше 0 dBFS должна дожить до loudnorm без клипа: {peak}");

        let normed = d.join("final_audio.wav");
        loudnorm(&mixed, &normed, -14.0, -1.0, 11.0).unwrap();
        let (spec, s) = read(&normed);
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        assert_eq!(spec.sample_rate, 44_100, "loudnorm без явной частоты отдаёт 192 кГц");
        assert!(s.iter().all(|v| v.abs() <= 1.0));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sync_filter_leads_every_audio_extraction() {
        assert_eq!(SYNC_AF, "aresample=async=1:first_pts=0");
        let (i, o) = (Path::new("in.mp4"), Path::new("out.wav"));
        let all = [
            ("extract_wav_16k_mono", extract_wav_16k_mono_args(i, o)),
            ("extract_audio", extract_audio_args(i, o, 44_100, 2)),
            ("to_16k_mono", to_16k_mono_args(i, o)),
            ("trim", cut_args(i, o, 1.0, 2.0, 24_000, 1)),
            ("cut", cut_args(i, o, 1.0, 2.0, 44_100, 2)),
            ("trim_ref", trim_ref_args(i, o, 1.0, 2.0, None)),
            ("trim_ref 96k", trim_ref_args(i, o, 1.0, 2.0, Some(REF_MAX_RATE))),
            ("encode_preview_aac", preview_aac_args(i, o)),
        ];
        for (name, args) in all {
            let at: Vec<usize> = args.iter().enumerate().filter(|(_, a)| *a == "-af").map(|(k, _)| k).collect();
            assert_eq!(at.len(), 1, "{name}: одна цепочка -af {args:?}");
            let chain = args[at[0] + 1].to_string_lossy();
            assert!(chain.starts_with(SYNC_AF), "{name}: aresample первым в цепочке: {chain}");
        }
    }

    fn rms_of(s: &[f32]) -> f32 {
        (s.iter().map(|v| v * v).sum::<f32>() / s.len().max(1) as f32).sqrt()
    }

    #[test]
    fn extracted_audio_follows_timestamps_across_a_gap() {
        let d = tmp("gap");
        let src = d.join("gap.mkv");
        // 2.5 с звука, с первой секунды его метки сдвинуты на 0.5 с вперёд (дыра в pts); картинка 3 с.
        run_ff(&[
            OsStr::new("-v"), OsStr::new("error"), OsStr::new("-y"),
            OsStr::new("-f"), OsStr::new("lavfi"), OsStr::new("-i"), OsStr::new("sine=f=440:d=2.5:r=48000"),
            OsStr::new("-f"), OsStr::new("lavfi"), OsStr::new("-i"), OsStr::new("color=c=black:s=64x64:r=25:d=3"),
            OsStr::new("-filter_complex"), OsStr::new("[0:a]asetpts='if(gte(T,1),PTS+0.5/TB,PTS)'[a]"),
            OsStr::new("-map"), OsStr::new("1:v"), OsStr::new("-map"), OsStr::new("[a]"),
            OsStr::new("-c:v"), OsStr::new("rawvideo"), OsStr::new("-c:a"), OsStr::new("pcm_s16le"),
            src.as_os_str(),
        ])
        .unwrap();
        let container = probe(&src).unwrap().duration;
        let frame = 1.0 / 25.0;
        assert!((container - 3.0).abs() <= frame, "контейнер {container}");

        let v16 = d.join("v16.wav");
        extract_wav_16k_mono(&src, &v16).unwrap();
        let hq = d.join("hq.wav");
        extract_audio(&src, &hq, 44_100, 2).unwrap();
        let m16 = d.join("m16.wav");
        to_16k_mono(&src, &m16).unwrap();
        for w in [&v16, &hq, &m16] {
            let got = duration(w).unwrap();
            assert!((got - container).abs() <= frame, "{}: {got} против {container}", w.display());
        }
        // Дыра стоит на своём месте: 1.0–1.5 с тишина, звук после неё начинается с 1.5 с.
        let (s, sr) = crate::wavio::read_mono_f32(&v16).unwrap();
        let at = |t: f64| (t * sr as f64) as usize;
        assert!(rms_of(&s[at(1.1)..at(1.4)]) < 1e-3);
        assert!(rms_of(&s[at(1.6)..at(1.9)]) > 0.05);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn stems_of_an_earlier_extraction_are_separated_again() {
        let d = tmp("stale_stems");
        let stems = d.join("stems");
        std::fs::create_dir_all(&stems).unwrap();
        for name in ["vocals.wav", "instrumental.wav"] {
            std::fs::write(stems.join(name), b"w").unwrap();
        }
        std::fs::write(d.join("audio_hq.wav"), b"hq").unwrap();
        std::fs::write(d.join("vocals16_clean.wav"), b"c").unwrap();
        std::fs::write(d.join("vocals16.wav"), b"v").unwrap();
        assert!(!stems_current(&d).unwrap(), "стемы без метки — прежнего извлечения");
        assert!(drop_stale_separation(&d).unwrap());
        assert!(!stems.exists() && !d.join("audio_hq.wav").exists() && !d.join("vocals16_clean.wav").exists());
        assert!(d.join("vocals16.wav").exists(), "16k-вход ASR ключуется своим EXTRACT_VER");
        std::fs::create_dir_all(&stems).unwrap();
        mark_separation(&stems).unwrap();
        assert!(stems_current(&d).unwrap());
        assert!(!drop_stale_separation(&d).unwrap(), "стемы текущего извлечения остаются");
        assert!(stems.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn clone_ref_is_mono_pcm16_at_the_source_rate_with_edge_fades() {
        let d = tmp("ref");
        for (sr, want) in [(44_100u32, 44_100u32), (16_000, 16_000), (96_000, 48_000)] {
            let src = d.join(format!("vocals_{sr}.wav"));
            sine_wav(&src, sr, 2, 0.5, 3.0);
            let out = d.join(format!("ref_{sr}.wav"));
            trim_ref(&src, &out, 1.0, 2.5).unwrap();
            let bytes = std::fs::read(&out).unwrap();
            assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1, "WAVE_FORMAT_PCM, а не EXTENSIBLE ({sr})");
            let sr = want;
            let mut r = hound::WavReader::open(&out).unwrap();
            let spec = r.spec();
            assert_eq!(spec.sample_rate, sr, "частота источника, не выше 48 кГц");
            assert_eq!((spec.channels, spec.bits_per_sample, spec.sample_format), (1, 16, hound::SampleFormat::Int));
            let s: Vec<f32> = r.samples::<i16>().map(|v| v.unwrap() as f32 / 32768.0).collect();
            assert!((s.len() as f64 / sr as f64 - 1.5).abs() < 0.01, "длина {}", s.len());
            let ms = |t: f64| (t * sr as f64 / 1000.0) as usize;
            let peak = |a: &[f32]| a.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let body = peak(&s[ms(20.0)..ms(40.0)]);
            assert!(body > 0.2, "за фейдом полный уровень: {body}");
            assert!(peak(&s[..ms(2.0)]) < 0.1 * body, "фейд на входе");
            assert!(peak(&s[s.len() - ms(2.0)..]) < 0.1 * body, "фейд на выходе");
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
