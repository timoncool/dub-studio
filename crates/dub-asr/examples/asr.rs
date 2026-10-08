//! Пример asr: транскрибировать WAV со словными таймстемпами, опционально с диаризацией. JSON в stdout.
//!
//!   asr --wav <in.wav> --tdt <каталог_TDT> [--diarize --diar <nemotron3_diar_v3.onnx>] [--lang auto] [--whole]
//!
//! Без --whole транскрипция идёт тем же путём, что в приложении (окнами по паузам); --whole — одним прогоном.
//!
//! Без --diarize: печатает {"segments":[{start,end,text,words:[{word,start,end}]}]}.
//! С --diarize: печатает {"turns":[...],"n_speakers":N,"segments":[{start,end,text,speaker}]} — сегменты по
//! сырым репликам модели; n_speakers — после свёртки, как в analyze.
//! Время загрузки+прогона каждой стадии — в stderr.

use dub_asr::{diarize, merge_turns, Asr, AsrEngine};

fn main() {
    let mut wav = None;
    let mut tdt = None;
    let mut diar = None;
    let mut do_diarize = false;
    let mut whole = false;
    let mut lang = "auto".to_string();

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wav" => wav = it.next(),
            "--tdt" => tdt = it.next(),
            "--diar" => diar = it.next(),
            "--diarize" => do_diarize = true,
            "--whole" => whole = true,
            "--lang" => lang = it.next().unwrap_or(lang),
            other => {
                eprintln!("неизвестный флаг: {other}");
                std::process::exit(2);
            }
        }
    }

    let wav = wav.unwrap_or_else(|| die("нужен --wav"));
    let tdt = tdt.unwrap_or_else(|| die("нужен --tdt <каталог TDT-модели>"));

    let mut asr = Asr::new(&tdt);

    if do_diarize {
        let diar = diar.unwrap_or_else(|| die("--diarize требует --diar <nemotron3_diar_v3.onnx>"));
        eprintln!("[diar] модель {diar}");
        let t0 = std::time::Instant::now();
        let turns = match diarize(&wav, &diar) {
            Ok(t) => t,
            Err(e) => die(&format!("диаризация не удалась: {e}")),
        };
        let mut raw_ids: Vec<i32> = turns.iter().map(|t| t.speaker).collect();
        raw_ids.sort_unstable();
        raw_ids.dedup();
        eprintln!(
            "[diar] готово за {:.2}с: реплик {}, спикеров {}",
            t0.elapsed().as_secs_f32(),
            turns.len(),
            raw_ids.len()
        );
        let merged = merge_turns(&turns, 0.8, 2.5);
        eprintln!(
            "[diar] как в analyze (merge_gap=0.8, min_spk=2.5): реплик {}, спикеров {}",
            merged.turns.len(),
            merged.n_speakers
        );
        let t1 = std::time::Instant::now();
        let segs = match asr.transcribe_turns(&wav, &turns) {
            Ok(s) => s,
            Err(e) => die(&format!("транскрипция не удалась: {e}")),
        };
        eprintln!("[asr] транскрипция реплик за {:.2}с, сегментов: {}", t1.elapsed().as_secs_f32(), segs.len());
        let out = serde_json::json!({ "turns": turns, "n_speakers": merged.n_speakers, "segments": segs });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        eprintln!("[asr] загрузка модели TDT из {tdt} + транскрипция {wav} ...");
        let t0 = std::time::Instant::now();
        let segs = match if whole { asr.transcribe(&wav, &lang) } else { AsrEngine::transcribe(&mut asr, std::path::Path::new(&wav), &lang) } {
            Ok(s) => s,
            Err(e) => die(&format!("транскрипция не удалась: {e}")),
        };
        let words: usize = segs.iter().map(|s| s.words.len()).sum();
        eprintln!(
            "[asr] готово за {:.2}с, сегментов: {}, слов: {words}",
            t0.elapsed().as_secs_f32(),
            segs.len()
        );
        let out = serde_json::json!({ "segments": segs });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    }
}

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}
