//! burn/burn_frame — ffmpeg + libass. Дословный порт captions.burn/burn_frame: полнокадровый gblur,
//! композит обратно ТОЛЬКО внутри каждого (плотного) текст-бокса, затем оверлей ASS. NVENC h264/hevc,
//! GPU-декод. burn_frame — ОДИН кадр в PNG (input-seek, без ре-энкода) для превью.

use crate::ass;
use crate::types::{BlurBox, CaptionsError};
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
const FFMPEG: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const FFMPEG: &str = "ffmpeg";

/// Ключ передачи filtergraph ФАЙЛОМ. ffmpeg 8.0 УДАЛИЛ `-filter_complex_script` (был deprecated с 7.0):
/// на свежих сборках burn падал с «Unrecognized option 'filter_complex_script'» — issue #1. Универсальная
/// замена `-/filter_complex` (общий префикс `/` = «значение опции лежит в файле») есть с 7.0, поэтому:
/// major >= 7 -> новый ключ, старее -> прежний. Версию спрашиваем у самого ffmpeg ОДИН раз за процесс.
pub fn filter_script_flag() -> &'static str {
    static FLAG: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    FLAG.get_or_init(|| {
        let major = Command::new(FFMPEG)
            .arg("-version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .and_then(|s| {
                // "ffmpeg version 7.1-essentials_build-..." / "ffmpeg version n8.0" / "... version 2026-07-21-git-..."
                let v = s.split("version").nth(1)?.trim_start().trim_start_matches('n');
                v.split(['.', '-', ' ']).next()?.parse::<u32>().ok()
            });
        // Неизвестная версия (git-сборки с датой вместо номера) -> новый ключ: он и есть актуальный,
        // а старый в свежих сборках уже удалён.
        match major {
            Some(m) if m < 7 => "-filter_complex_script",
            _ => "-/filter_complex",
        }
    })
}

/// Последние ~1000 символов stderr ffmpeg (в исходном порядке) — для лаконичных сообщений об ошибке.
fn stderr_tail(stderr: &[u8]) -> String {
    let err = String::from_utf8_lossy(stderr);
    err.chars().rev().take(1000).collect::<String>().chars().rev().collect()
}

/// Экранировать путь ASS для filtergraph: \ -> /, : -> \: .
fn ass_escape(p: &Path) -> String {
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy().to_string();
    // canonicalize на Windows даёт \\?\ префикс — убираем его, libass его не понимает.
    let s = s.strip_prefix(r"\\?\").map(|x| x.to_string()).unwrap_or(s);
    s.replace('\\', "/").replace(':', "\\:")
}

/// Собрать `ass='...':fontsdir='...'` фильтр (fontsdir указывает libass на bundled-шрифты).
fn ass_filter(ass_path: &Path) -> String {
    let ass = ass_escape(ass_path);
    let fd_dir = ass::fonts_dir();
    if fd_dir.exists() {
        let fd = ass_escape(&fd_dir);
        format!("ass='{ass}':fontsdir='{fd}'")
    } else {
        format!("ass='{ass}'")
    }
}

fn en(t0: f64, t1: f64) -> String {
    format!("enable='between(t\\,{:.2}\\,{:.2})'", t0, t1)
}

/// '#RRGGBB' -> цвет ffmpeg drawbox '0xrrggbb' (кривое значение -> чёрный). Порт _ff_color.
fn ff_color(hexc: Option<&str>) -> String {
    let c = hexc.unwrap_or("").trim_start_matches('#');
    if c.len() >= 6 {
        format!("0x{}", c[..6].to_lowercase())
    } else {
        "0x000000".to_string()
    }
}

/// Собрать filter_complex графа накрытий+ASS (общий для burn/burn_frame) — порт _cover_parts.
/// Per-box взаимоисключающе: fill="#hex" -> СПЛОШНОЙ drawbox этого цвета (плоский фон, чистый край,
/// на фоне своего цвета невидим); fill=None -> бокс идёт в ОДИН полнокадровый gblur и композитится
/// обратно строго внутри бокса (текстурная сцена). `lead_parts` — начальные узлы (burn_frame: select).
fn blur_graph(
    ass_f: &str,
    blur_boxes: &[BlurBox],
    frame_w: i64,
    frame_h: i64,
    blur_sigma: i64,
    lead_parts: &[String],
    base_label: &str,
) -> String {
    let pad = |bx: &BlurBox| {
        let bx0 = (bx.x - 2).max(0);
        let by0 = (bx.y - 2).max(0);
        let bw = (bx.w + 4).min(frame_w - bx0);
        let bh = (bx.h + 4).min(frame_h - by0);
        (bx0, by0, bw, bh, (bx.t0 - 0.6).max(0.0), bx.t1 + 0.4)
    };
    let fills: Vec<&BlurBox> = blur_boxes.iter().filter(|b| b.fill.is_some()).collect();
    let blurs: Vec<&BlurBox> = blur_boxes.iter().filter(|b| b.fill.is_none()).collect();
    let mut parts: Vec<String> = lead_parts.to_vec();
    let mut cur = base_label.to_string();
    if !fills.is_empty() {
        // сплошные накрытия: цепочка drawbox прямо на стриме.
        let dbs: Vec<String> = fills
            .iter()
            .map(|bx| {
                let (bx0, by0, bw, bh, t0b, t1b) = pad(bx);
                format!(
                    "drawbox=x={bx0}:y={by0}:w={bw}:h={bh}:color={}@1:t=fill:{}",
                    ff_color(bx.fill.as_deref()),
                    en(t0b, t1b)
                )
            })
            .collect();
        parts.push(format!("[{cur}]{}[filled]", dbs.join(",")));
        cur = "filled".to_string();
    }
    if !blurs.is_empty() {
        let n = blurs.len();
        parts.push(format!("[{cur}]split=2[cbase][bsrc]"));
        parts.push(format!("[bsrc]gblur=sigma={}[blr]", blur_sigma));
        let srcs: Vec<String>;
        if n > 1 {
            let mut s = format!("[blr]split={n}");
            for i in 0..n {
                s.push_str(&format!("[s{i}]"));
            }
            parts.push(s);
            srcs = (0..n).map(|i| format!("s{i}")).collect();
        } else {
            srcs = vec!["blr".to_string()];
        }
        cur = "cbase".to_string();
        for (i, bx) in blurs.iter().enumerate() {
            let (bx0, by0, bw, bh, t0b, t1b) = pad(bx);
            parts.push(format!("[{}]crop={bw}:{bh}:{bx0}:{by0}[c{i}]", srcs[i]));
            parts.push(format!("[{cur}][c{i}]overlay={bx0}:{by0}:{}[v{i}]", en(t0b, t1b)));
            cur = format!("v{i}");
        }
    }
    format!("{};[{cur}]{ass_f}[outv]", parts.join(";"))
}

#[cfg(test)]
mod graph_tests {
    use super::*;

    fn bx(fill: Option<&str>) -> BlurBox {
        BlurBox { x: 100, y: 900, w: 300, h: 60, t0: 1.0, t1: 3.0, fill: fill.map(|s| s.into()) }
    }

    // fill-боксы -> drawbox сплошным цветом, БЕЗ gblur; blur-боксы -> gblur-ветка. Порт _cover_parts.
    #[test]
    fn fills_use_drawbox_not_gblur() {
        let g = blur_graph("ass='x.ass'", &[bx(Some("#FFFFFF"))], 720, 1280, 60, &[], "0:v");
        assert!(g.contains("drawbox=") && g.contains("color=0xffffff@1:t=fill"), "{g}");
        assert!(!g.contains("gblur"), "все боксы fill -> gblur не нужен: {g}");
    }

    #[test]
    fn mixed_boxes_split_between_drawbox_and_gblur() {
        let g = blur_graph(
            "ass='x.ass'",
            &[bx(Some("#FFFFFF")), bx(None)],
            720,
            1280,
            60,
            &[],
            "0:v",
        );
        assert!(g.contains("drawbox=") && g.contains("gblur"), "{g}");
        // drawbox-цепь идёт ПЕРЕД блюром (питон: fills -> filled -> split для блюра).
        assert!(g.find("drawbox").unwrap() < g.find("gblur").unwrap(), "{g}");
    }
}

/// Вжечь блюр боксов оригинала + оверлей ASS. blur_boxes: [(x,y,w,h,t0,t1)]. Без аудио (муксится
/// отдельно). NVENC only (нет тихого CPU-фолбэка). Порт captions.burn.
#[allow(clippy::too_many_arguments)]
pub fn burn(
    video: &Path,
    ass_path: &Path,
    out: &Path,
    blur_boxes: &[BlurBox],
    frame_size: Option<(i64, i64)>,
    blur: bool,
    gpu_encode: bool,
    gpu_decode: bool,
    cq: i64,
    src_codec: Option<&str>,
    blur_sigma: i64,
) -> Result<(), CaptionsError> {
    let ass_f = ass_filter(ass_path);
    let (w, h) = frame_size.unwrap_or((1_000_000_000, 1_000_000_000));
    let hevc = matches!(src_codec.map(|c| c.to_lowercase()).as_deref(), Some("hevc") | Some("h265"));
    let nv: Vec<String> = vec![
        "-c:v".into(),
        (if hevc { "hevc_nvenc" } else { "h264_nvenc" }).into(),
        "-preset".into(),
        "p4".into(),
        "-cq".into(),
        cq.to_string(),
    ];
    let sw: Vec<String> = vec![
        "-c:v".into(),
        (if hevc { "libx265" } else { "libx264" }).into(),
        "-preset".into(),
        "medium".into(),
        "-crf".into(),
        (cq - 2).max(0).to_string(),
    ];

    let vargs: Vec<String> = if !blur_boxes.is_empty() && blur {
        let graph = blur_graph(&ass_f, blur_boxes, w, h, blur_sigma, &[], "0:v");
        // Граф — В ФАЙЛ (-filter_complex_script): блюр-подложка на каждый субтитр даёт сотни цепочек
        // (репро: 342 бокса = 37КБ аргументов), а CreateProcess ограничен 32767 символами —
        // spawn падал с ENAMETOOLONG, и burn умирал мгновенно.
        let graph_file = out.with_extension("filter");
        std::fs::write(&graph_file, &graph).map_err(|e| CaptionsError::FilterScript(e.to_string()))?;
        vec![
            filter_script_flag().into(),
            graph_file.to_string_lossy().into_owned(),
            "-map".into(),
            "[outv]".into(),
        ]
    } else {
        vec!["-vf".into(), ass_f.clone()]
    };

    let enc = if gpu_encode { nv } else { sw };
    // Таймаут пропорционален длительности: фикс-30мин зарезал бы легитимный burn многочасового
    // фильма (NVENC ~5-10x риалтайма, softwarе медленнее). 4x длительность + 10 мин запас, минимум 30 мин.
    let dur_secs = probe_duration_secs(video).unwrap_or(0.0);
    let timeout = BURN_TIMEOUT_SECS.max((dur_secs * 4.0) as u64 + 600);
    run_ffmpeg(video, &vargs, &enc, out, gpu_decode, timeout)
}

/// Длительность видео в секундах через ffprobe (для пропорционального таймаута burn). Ошибка -> None.
fn probe_duration_secs(video: &Path) -> Option<f64> {
    let out = Command::new(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" })
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(video)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok()
}

fn run_ffmpeg(
    video: &Path,
    vargs: &[String],
    enc: &[String],
    out: &Path,
    gpu_decode: bool,
    timeout_secs: u64,
) -> Result<(), CaptionsError> {
    let mut cmd = Command::new(FFMPEG);
    cmd.arg("-y");
    if gpu_decode {
        cmd.args(["-hwaccel", "cuda"]);
    }
    cmd.arg("-i").arg(video).arg("-an");
    for a in vargs {
        cmd.arg(a);
    }
    for a in enc {
        cmd.arg(a);
    }
    cmd.arg(out);
    let o = output_with_timeout(cmd, timeout_secs, true)?;
    if !o.status.success() {
        let tail = stderr_tail(&o.stderr);
        return Err(CaptionsError::BurnFailed { tail });
    }
    Ok(())
}

/// Потолок бёрна: NVENC жуёт часовое видео за минуты; 30 мин не хватает только когда ffmpeg
/// мёртво завис (наблюдалось: cmd.output() без таймаута вешал единственный воркер джоб навсегда).
const BURN_TIMEOUT_SECS: u64 = 30 * 60;

/// Command::output() с жёстким таймаутом. stdout/stderr читаются фоновыми потоками (иначе полный
/// пайп блокирует ffmpeg и это превращается в вечное взаимное ожидание); по истечении — kill +
/// ошибка с хвостом stderr. Никакой внешней зависимости: try_wait в цикле с шагом 250мс.
/// `log_cmd` — печатать cmdline в stderr: полный burn да (диагностика долгих джоб), превью-кадр
/// нет (иначе каждый тик плеера спамит лог строкой на 37КБ).
fn output_with_timeout(mut cmd: Command, secs: u64, log_cmd: bool) -> Result<std::process::Output, CaptionsError> {
    use std::io::Read;
    use std::process::Stdio;
    if log_cmd {
        eprintln!("[burn] ffmpeg: {cmd:?}");
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| CaptionsError::Spawn(e.to_string()))?;
    let _tracked = dub_core::proc::track(child.id());
    let mut so = child.stdout.take().expect("piped stdout");
    let mut se = child.stderr.take().expect("piped stderr");
    let th_out = std::thread::spawn(move || { let mut b = Vec::new(); let _ = so.read_to_end(&mut b); b });
    let th_err = std::thread::spawn(move || { let mut b = Vec::new(); let _ = se.read_to_end(&mut b); b });
    // join c дедлайном: EOF пайпа может не прийти, если write-хэндл унаследован пережившим ffmpeg
    // процессом — не даём читателям заблокировать единственный воркер джоб (ревью: и success-, и
    // kill-путь висли бы на join). Не дождались — поток остаётся detached, пустой буфер.
    let drain = |th: std::thread::JoinHandle<Vec<u8>>, secs: u64| -> Vec<u8> {
        let dl = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        while !th.is_finished() {
            if std::time::Instant::now() >= dl {
                return Vec::new();
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        th.join().unwrap_or_default()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let tail = stderr_tail(&drain(th_err, 10));
                drain(th_out, 1);
                return Err(CaptionsError::Timeout { secs, tail });
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(250)),
            Err(e) => return Err(CaptionsError::Wait(e.to_string())),
        }
    };
    let stdout = drain(th_out, 15);
    let stderr = drain(th_err, 15);
    Ok(std::process::Output { status, stdout, stderr })
}

/// ОДИН превью-кадр в момент t: тот же блюр+ASS граф, но input-seek + декод одного кадра в PNG. Порт
/// captions.burn_frame.
#[allow(clippy::too_many_arguments)]
pub fn burn_frame(
    video: &Path,
    ass_path: &Path,
    out_png: &Path,
    t: f64,
    blur_boxes: &[BlurBox],
    frame_size: Option<(i64, i64)>,
    blur: bool,
    blur_sigma: i64,
    scale_w: Option<i64>,
) -> Result<(), CaptionsError> {
    let t = t.max(0.0);
    let ass_f = ass_filter(ass_path);
    let (mut w, mut h) = frame_size.unwrap_or((1_000_000_000, 1_000_000_000));
    // Опциональный даунскейл для быстрого превью больших видео (плей): масштабируем видео и КООРДИНАТЫ
    // blur-боксов на k. ASS-фильтр авто-масштабируется к размеру кадра (PlayRes), поэтому субтитры/титры
    // остаются на своих местах — только рендерятся в низком разрешении (мягче, но 1-в-1 по геометрии).
    let mut scaled_boxes: Vec<BlurBox> = Vec::new();
    let scale_step: Option<String> = match scale_w {
        Some(sw) if sw > 0 && w < 1_000_000_000 && sw < w => {
            let k = sw as f64 / w as f64;
            scaled_boxes = blur_boxes
                .iter()
                .map(|b| BlurBox {
                    x: (b.x as f64 * k).round() as i64,
                    y: (b.y as f64 * k).round() as i64,
                    w: (b.w as f64 * k).round() as i64,
                    h: (b.h as f64 * k).round() as i64,
                    t0: b.t0,
                    t1: b.t1,
                    fill: b.fill.clone(),
                })
                .collect();
            // scale={sw}:-2 -> ширина ровно sw, высота = ближайшая ЧЁТНАЯ (как трактует -2 ffmpeg). Считаем
            // так же, иначе frame_h разойдётся с реальным кадром на 1px и кламп нижних blur-боксов упадёт.
            w = sw;
            h = ((h as f64 * k / 2.0).round() as i64 * 2).max(2);
            Some(format!("scale={sw}:-2"))
        }
        _ => None,
    };
    let boxes: &[BlurBox] = if scale_step.is_some() { &scaled_boxes } else { blur_boxes };
    let sel = format!("select='gte(t\\,{:.3})'", t);
    // Префикс кадра: select [+ scale]. Одинаков для blur- и не-blur веток (ASS/боксы идут ПОСЛЕ).
    let pre = match &scale_step {
        Some(s) => format!("{sel},{s}"),
        None => sel.clone(),
    };

    let vargs: Vec<String> = if !boxes.is_empty() && blur {
        // lead: select[,scale] -> [sel]; base label = sel.
        let lead = vec![format!("[0:v]{pre}[sel]")];
        let graph = blur_graph(&ass_f, boxes, w, h, blur_sigma, &lead, "sel");
        // Граф в файл — тот же 32767-символьный лимит CreateProcess, что и у полного burn.
        let graph_file = out_png.with_extension("filter");
        std::fs::write(&graph_file, &graph).map_err(|e| CaptionsError::FilterScript(e.to_string()))?;
        vec![
            filter_script_flag().into(),
            graph_file.to_string_lossy().into_owned(),
            "-map".into(),
            "[outv]".into(),
        ]
    } else {
        vec!["-vf".into(), format!("{pre},{ass_f}")]
    };

    let mut cmd = Command::new(FFMPEG);
    cmd.arg("-y")
        .arg("-ss")
        .arg(format!("{:.3}", (t - 1.0).max(0.0)))
        .arg("-copyts")
        .arg("-i")
        .arg(video)
        .arg("-an");
    for a in &vargs {
        cmd.arg(a);
    }
    cmd.args(["-frames:v", "1", "-update", "1"]).arg(out_png);
    // Кадр превью обязан отдаваться за секунды; 120с — только против мёртвого зависания ffmpeg,
    // которое иначе навечно занимает единственный воркер джоб (превью всех проектов -> 504).
    // log_cmd=false: кадр дёргается на каждый тик плеера — cmdline-строка спамила бы лог.
    let o = output_with_timeout(cmd, 120, false)?;
    if !o.status.success() {
        let tail = stderr_tail(&o.stderr);
        return Err(CaptionsError::FrameFailed { tail });
    }
    Ok(())
}

#[cfg(test)]
mod filter_flag_tests {
    use super::*;

    /// Гард issue #1: ключ filtergraph-файла должен приниматься УСТАНОВЛЕННЫМ ffmpeg (на 8.x старый
    /// `-filter_complex_script` удалён). Нет ffmpeg в PATH -> тест пропускается.
    #[test]
    fn filter_script_flag_accepted_by_local_ffmpeg() {
        let flag = filter_script_flag();
        assert!(flag == "-/filter_complex" || flag == "-filter_complex_script", "{flag}");
        let dir = std::env::temp_dir().join("dubcap_flag_test");
        let _ = std::fs::create_dir_all(&dir);
        let script = dir.join("g.txt");
        if std::fs::write(&script, "color=c=red:s=32x32:d=0.1[outv]").is_err() {
            return;
        }
        let out = dir.join("o.png");
        let res = Command::new(FFMPEG)
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .arg(flag)
            .arg(&script)
            .args(["-map", "[outv]", "-frames:v", "1"])
            .arg(&out)
            .output();
        let Ok(o) = res else { return }; // нет ffmpeg — пропускаем
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!err.contains("Unrecognized option"), "ffmpeg не принял {flag}: {err}");
    }
}
