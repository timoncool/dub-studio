//! dub-sep — вокал/инструментал сепарация (порт dubengine/separate.py).
//! Движок: **Mel-Band Roformer voc_fv6-Q8_0** через
//! нативный сайдкар **BSRoformer.cpp** (C++/ggml, CUDA). Эталон интеграции — voiceclean.rs из
//! Higgs-Ultimate (CLI `<model.gguf> <in.wav> <out.wav>`, вход 44.1кГц, CREATE_NO_WINDOW, cwd=папка
//! движка ради ggml-DLL).
//!
//! CLI отдаёт ТОЛЬКО вокал-стем (в модели num_stems=1). Инструментал получаем как разницу
//! `mix − vocals` во временной области — выход сепаратора выровнен по семплам со входом (та же длина,
//! 44.1кГц), потому вычитание сэмпл-в-сэмпл корректно (стандартная практика для vocal-моделей).
//!
//! Интерфейс: `separate(mix_wav, out_dir) -> SepResult { vocals, instrumental }`. Обе дорожки —
//! WAV float32 44.1кГц, как выдаёт движок; downstream (analyze audio-context / рендер-фон) читает их
//! напрямую.

use std::path::{Path, PathBuf};
use std::process::Command;

mod wav;

#[derive(Debug, thiserror::Error)]
pub enum SepError {
    #[error("separation engine not found: {0}")]
    EngineMissing(PathBuf),
    #[error("separation model not found: {0}")]
    ModelMissing(PathBuf),
    #[error("engine start: {0}")]
    Spawn(String),
    /// Движок завершился с ошибкой: код выхода и хвост его вывода.
    #[error("the engine failed (exit code {code:?}): {tail}")]
    EngineFailed { code: Option<i32>, tail: String },
    #[error("the engine made no vocal stem: {0}")]
    NoOutput(PathBuf),
    #[error("audio I/O: {0}")]
    Wav(String),
}

impl SepError {
    /// Стабильный код ошибки (аргументы — поля варианта).
    pub fn code(&self) -> &'static str {
        match self {
            SepError::EngineMissing(_) => "sep_engine_missing",
            SepError::ModelMissing(_) => "sep_model_missing",
            SepError::Spawn(_) => "sep_spawn",
            SepError::EngineFailed { .. } => "sep_engine_failed",
            SepError::NoOutput(_) => "sep_no_output",
            SepError::Wav(_) => "sep_audio_io",
        }
    }
}

/// Результат сепарации: пути к вокалу и инструменталу (оба WAV, 44.1кГц).
#[derive(Debug, Clone)]
pub struct SepResult {
    pub vocals: PathBuf,
    pub instrumental: PathBuf,
}

/// Сайдкар-CLI движка. Рядом лежат библиотеки ggml: на Windows их подхватывает загрузчик из cwd движка, на
/// Linux — LD_LIBRARY_PATH запуска (RUNPATH сборки указывает на каталог её CI).
#[cfg(windows)]
pub const ENGINE_CLI_FILE: &str = "bs_roformer-cli.exe";
#[cfg(not(windows))]
pub const ENGINE_CLI_FILE: &str = "bs_roformer-cli";
/// GGUF-модель вокал-сепарации по умолчанию (Mel-Band Roformer voc_fv6, Q8_0).
pub const MODEL_FILE: &str = "voc_fv6-Q8_0.gguf";

/// Резолв каталога движка: env DUB_STUDIO_BSROFORMER_DIR, иначе <repo>/tools/bsroformer.
pub fn engine_dir(repo_root: &Path) -> PathBuf {
    std::env::var("DUB_STUDIO_BSROFORMER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("tools").join("bsroformer"))
}

/// Каталог CPU-сборки движка (без CUDA): <repo>/tools/bsroformer-cpu.
pub fn engine_dir_cpu(repo_root: &Path) -> PathBuf {
    repo_root.join("tools").join("bsroformer-cpu")
}

/// Путь к CLI движка под backend: "cpu" -> bsroformer-cpu, иначе (gpu) -> bsroformer. БЕЗ тихой
/// подмены backend — возвращаем ровно путь выбранной сборки; отсутствие ловит пре-флайт и показывает
/// уведомление (а не молча считает без фона).
pub fn engine_cli(repo_root: &Path, backend: &str) -> PathBuf {
    let dir = if backend == "cpu" { engine_dir_cpu(repo_root) } else { engine_dir(repo_root) };
    dir.join(ENGINE_CLI_FILE)
}

/// Резолв GGUF-модели: env DUB_STUDIO_BSROFORMER_MODEL, иначе <repo>/models/bsroformer/<MODEL_FILE>.
pub fn model_path(repo_root: &Path) -> PathBuf {
    std::env::var("DUB_STUDIO_BSROFORMER_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("models").join("bsroformer").join(MODEL_FILE))
}

/// Установлены ли обе части (движок + модель) — для UI/фейл-сейф в пайплайне.
pub fn is_installed(repo_root: &Path) -> bool {
    engine_dir(repo_root).join(ENGINE_CLI_FILE).is_file() && model_path(repo_root).is_file()
}

/// Каталог недописанной сепарации рядом с `out_dir`: `<out_dir>.part`. Готовый результат появляется
/// в `out_dir` только целиком (rename каталога), поэтому кэш по существованию стемов не видит обрыв.
pub fn part_dir(out_dir: &Path) -> PathBuf {
    let mut name = out_dir.file_name().map(|s| s.to_os_string()).unwrap_or_default();
    name.push(".part");
    out_dir.with_file_name(name)
}

/// Разделить mix (WAV 44.1кГц) на вокал + инструментал. `out_dir` — куда положить `vocals.wav` и
/// `instrumental.wav` (порт `separate.split` контракта). `cli` — путь к bs_roformer-cli.exe,
/// `model` — GGUF. Движок пишет вокал; инструментал считается как mix − vocals во времени.
pub fn separate(
    mix_wav: &Path,
    out_dir: &Path,
    cli: &Path,
    model: &Path,
) -> Result<SepResult, SepError> {
    if !cli.is_file() {
        return Err(SepError::EngineMissing(cli.to_path_buf()));
    }
    if !model.is_file() {
        return Err(SepError::ModelMissing(model.to_path_buf()));
    }
    let part = part_dir(out_dir);
    if part.exists() {
        std::fs::remove_dir_all(&part)
            .map_err(|e| SepError::Wav(format!("remove {}: {e}", part.display())))?;
    }
    std::fs::create_dir_all(&part).map_err(|e| SepError::Wav(e.to_string()))?;
    if let Err(e) = separate_into(mix_wav, &part, cli, model) {
        let _ = std::fs::remove_dir_all(&part);
        return Err(e);
    }
    if out_dir.exists() {
        std::fs::remove_dir_all(out_dir)
            .map_err(|e| SepError::Wav(format!("remove {}: {e}", out_dir.display())))?;
    }
    std::fs::rename(&part, out_dir).map_err(|e| {
        SepError::Wav(format!("rename {} -> {}: {e}", part.display(), out_dir.display()))
    })?;
    Ok(SepResult {
        vocals: out_dir.join("vocals.wav"),
        instrumental: out_dir.join("instrumental.wav"),
    })
}

/// Сепарация в готовый пустой каталог `out_dir` (без атомарности — её даёт `separate`).
fn separate_into(
    mix_wav: &Path,
    out_dir: &Path,
    cli: &Path,
    model: &Path,
) -> Result<SepResult, SepError> {
    let vocals = out_dir.join("vocals.wav");
    let instrumental = out_dir.join("instrumental.wav");

    // ДЛИННЫЙ файл (> SEP_WINDOW_GATE_SECS) — оконная сепарация: чтение целиком раздуло бы RAM
    // (4ч 44.1к stereo f32 ≈ 5ГБ на дорожку, а их тут три), окна с кроссфейдной сшивкой держат
    // память O(окна) при том же качестве (стыки в перекрытии, шов заглажен фейдом 1с).
    let (_, _, dur) = wav::probe(mix_wav).map_err(SepError::Wav)?;
    if dur > SEP_WINDOW_GATE_SECS {
        return separate_windowed(mix_wav, out_dir, cli, model, dur);
    }

    run_cli(cli, model, mix_wav, &vocals)?;
    if !vocals.is_file() {
        return Err(SepError::NoOutput(vocals));
    }

    // Инструментал = mix − vocals во временной области. Читаем оба, вычитаем сэмпл-в-сэмпл
    // (движок гарантирует ту же длину/частоту), пишем WAV float32.
    let mix = wav::read_f32(mix_wav).map_err(SepError::Wav)?;
    let voc = wav::read_f32(&vocals).map_err(SepError::Wav)?;
    let inst = subtract(mix, &voc);
    wav::write_f32(&instrumental, &inst).map_err(SepError::Wav)?;

    Ok(SepResult { vocals, instrumental })
}

/// Гейт оконной сепарации: до 30 мин whole-file проверен (22-мин эпизод многократно), длиннее — окна.
const SEP_WINDOW_GATE_SECS: f64 = 30.0 * 60.0;
/// Размер окна сепарации (сек) и перекрытие соседних окон; кроссфейд шва — в середине перекрытия.
const SEP_WIN_SECS: f64 = 20.0 * 60.0;
const SEP_OVERLAP_SECS: f64 = 6.0;
const SEP_CROSSFADE_SECS: f64 = 1.0;

/// Оконная сепарация длинного файла: окна SEP_WIN c перекрытием, каждый через CLI, стемы сшиваются
/// потоково (StreamWriter) с линейным кроссфейдом в середине перекрытия. Инструментал — вторым
/// оконным проходом mix − собранный vocals (память O(окна)).
fn separate_windowed(
    mix_wav: &Path,
    out_dir: &Path,
    cli: &Path,
    model: &Path,
    dur: f64,
) -> Result<SepResult, SepError> {
    let (sr, ch, _) = wav::probe(mix_wav).map_err(SepError::Wav)?;
    let ch = ch.max(1);
    let vocals = out_dir.join("vocals.wav");
    let instrumental = out_dir.join("instrumental.wav");
    let f = |secs: f64| -> u32 { (secs * sr as f64).round() as u32 }; // сек -> фреймы
    let total_frames = f(dur);
    let n_win = ((dur / SEP_WIN_SECS).ceil() as usize).max(1);
    let cf = f(SEP_CROSSFADE_SECS) as usize;

    let mut out_voc = wav::StreamWriter::create(&vocals, sr, ch).map_err(SepError::Wav)?;
    // Хвост предыдущего окна для кроссфейда: интерливнутые сэмплы от точки (cut - CF/2) до конца окна.
    let mut carry: Vec<f32> = Vec::new();
    for i in 0..n_win {
        // Границы окна в фреймах: [g0, g1); первые окна тянут перекрытие влево.
        let grid0 = f(i as f64 * SEP_WIN_SECS);
        let g0 = if i == 0 { 0 } else { grid0.saturating_sub(f(SEP_OVERLAP_SECS)) };
        let g1 = f(((i + 1) as f64 * SEP_WIN_SECS).min(dur)).min(total_frames);
        let win_in = out_dir.join(format!("sep_win_{i}.wav"));
        let win_voc = out_dir.join(format!("sep_win_{i}_voc.wav"));
        let chunk = wav::read_f32_range(mix_wav, g0, g1 - g0).map_err(SepError::Wav)?;
        wav::write_f32(&win_in, &chunk).map_err(SepError::Wav)?;
        run_cli(cli, model, &win_in, &win_voc)?;
        let voc = wav::read_f32(&win_voc).map_err(SepError::Wav)?;
        let _ = std::fs::remove_file(&win_in);
        let _ = std::fs::remove_file(&win_voc);
        let s = voc.data; // интерливнуто, локальный фрейм 0 == глобальный g0

        // Точка шва с ПРЕДЫДУЩИМ окном: cut_prev = grid0 - OV/2 (глобально). Пишем: кроссфейд
        // [cut-CF/2, cut+CF/2) из carry и текущего окна, затем тело до следующего шва (хвост -> carry).
        let to_local = |gframe: u32| -> usize { (gframe.saturating_sub(g0)) as usize * ch as usize };
        let body_start_local = if i == 0 {
            0
        } else {
            let cut = grid0 - f(SEP_OVERLAP_SECS / 2.0);
            let cf_start = to_local(cut).saturating_sub(cf * ch as usize / 2);
            // линейный кроссфейд carry (fade-out) x текущее окно (fade-in), длина = len(carry)
            let n = carry.len().min(s.len().saturating_sub(cf_start));
            let mut mixed = Vec::with_capacity(n);
            for k in 0..n {
                let t = k as f32 / n.max(1) as f32;
                mixed.push(carry[k] * (1.0 - t) + s[cf_start + k] * t);
            }
            out_voc.write(&mixed).map_err(SepError::Wav)?;
            cf_start + n
        };
        if i + 1 < n_win {
            // Тело до начала кроссфейд-зоны следующего шва; хвост от неё — в carry.
            let next_cut = f((i + 1) as f64 * SEP_WIN_SECS) - f(SEP_OVERLAP_SECS / 2.0);
            let tail_start = to_local(next_cut).saturating_sub(cf * ch as usize / 2).max(body_start_local);
            out_voc.write(&s[body_start_local..tail_start]).map_err(SepError::Wav)?;
            carry = s[tail_start..(tail_start + cf * ch as usize).min(s.len())].to_vec();
        } else {
            out_voc.write(&s[body_start_local..]).map_err(SepError::Wav)?;
        }
    }
    out_voc.finalize().map_err(SepError::Wav)?;

    // Инструментал = mix − vocals, окнами по 10 мин (без перекрытий — оба файла уже выровнены).
    let mut out_inst = wav::StreamWriter::create(&instrumental, sr, ch).map_err(SepError::Wav)?;
    let step = f(600.0);
    let mut pos: u32 = 0;
    while pos < total_frames {
        let n = step.min(total_frames - pos);
        let mut m = wav::read_f32_range(mix_wav, pos, n).map_err(SepError::Wav)?;
        let v = wav::read_f32_range(&vocals, pos, n).map_err(SepError::Wav)?;
        let k = m.data.len().min(v.data.len());
        for (o, vv) in m.data[..k].iter_mut().zip(&v.data[..k]) {
            *o -= *vv;
        }
        out_inst.write(&m.data).map_err(SepError::Wav)?;
        pos += n;
    }
    out_inst.finalize().map_err(SepError::Wav)?;
    Ok(SepResult { vocals, instrumental })
}

/// Разница двух дорожек: instrumental[n] = mix[n] − vocals[n]. Каналы/частота берутся из mix; если
/// длины расходятся (крайний случай), выравниваем по минимуму (хвост-остаток mix оставляем как есть).
fn subtract(mut mix: wav::Audio, voc: &wav::Audio) -> wav::Audio {
    let ch = mix.channels.max(1);
    let n = mix.data.len().min(voc.data.len());
    // Переиспользуем буфер mix на месте (был лишний mix.data.clone() на весь трек).
    for (o, v) in mix.data[..n].iter_mut().zip(&voc.data[..n]) {
        *o -= *v;
    }
    wav::Audio {
        sample_rate: mix.sample_rate,
        channels: ch,
        data: mix.data,
    }
}

/// Запустить сайдкар без окна консоли, cwd=папка движка (ради ggml-DLL), дождаться, проверить код.
/// Эталон — voiceclean.rs::run_cli из Higgs-Ultimate.
fn run_cli(cli: &Path, model: &Path, input: &Path, output: &Path) -> Result<(), SepError> {
    // cwd меняем на папку движка (ради ggml-DLL), потому ВСЕ пути аргументов делаем абсолютными —
    // иначе относительный путь модели/входа резолвится от чужого cwd и «файл не найден».
    let abs = |p: &Path| -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|c| c.join(p))
                .unwrap_or_else(|_| p.to_path_buf())
        }
    };
    let mut cmd = Command::new(abs(cli));
    cmd.arg(abs(model)).arg(abs(input)).arg(abs(output));
    if let Some(dir) = cli.parent() {
        cmd.current_dir(abs(dir));
        dub_core::proc::libraries_beside(&mut cmd, &abs(dir));
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let out = dub_core::proc::output(&mut cmd).map_err(|e| SepError::Spawn(e.to_string()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut tail: Vec<&str> = stderr.lines().chain(stdout.lines()).rev().take(8).collect();
        tail.reverse();
        return Err(SepError::EngineFailed { code: out.status.code(), tail: tail.join(" | ") });
    }
    Ok(())
}
