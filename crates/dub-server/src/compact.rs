//! Сжатие тишины перед анализом (`compact_silence`). Паузы тише −60 дБFS длиннее 20 с сжимаются до 20 с
//! (по 10 с запаса у звука); разделение, диаризация и распознавание идут по короткой записи, а карта
//! кусков переводит их времена обратно на шкалу исходной записи.
//!
//! Сжатые файлы лежат только в `<work_dir>/compact/`; общие `stems/` и `audio_hq.wav` не пишутся.

use std::path::{Path, PathBuf};

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Метка версии правил сжатия: входит в ключи кэша диаризации и распознавания.
pub const VER: &str = "compact-v1";
const FRAME_SEC: f64 = 0.032;
const SILENCE_DBFS: f64 = -60.0;
const MIN_PAUSE: f64 = 20.0;
const PAD: f64 = 10.0;
/// Меньше этого вырезать — сжатие не применяется.
pub const MIN_SAVING: f64 = 60.0;
/// Паузы длиннее этого сохраняются в проекте.
const LONG_PAUSE: f64 = 60.0;
/// Границы кусков — на сетке 10 мс: целое число отсчётов и на 16 кГц, и на 44,1 кГц, поэтому сжатые
/// файлы разных частот режутся в одних и тех же точках без накопления сдвига.
const GRID: f64 = 100.0;
/// Отсчётов на блок потокового чтения: кратно кадру огибающей (512 отсчётов на 16 кГц).
const BLOCK_FRAMES: usize = 512 * 2048;

/// Параметр анализа из запроса: включён только явным `1`.
pub fn flag(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Вход ключа кэша диаризации или распознавания: без сжатия — прежний отпечаток, байт в байт.
pub fn key_input(input_hash: &str, applied: bool) -> String {
    if applied {
        format!("{input_hash}|{VER}")
    } else {
        input_hash.to_string()
    }
}

/// Кусок записи, который остаётся после сжатия: где он стоял в исходной записи и где начинается в сжатой.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub orig_start: f64,
    pub orig_end: f64,
    pub compact_start: f64,
}

impl Piece {
    fn len(&self) -> f64 {
        self.orig_end - self.orig_start
    }
    fn compact_end(&self) -> f64 {
        self.compact_start + self.len()
    }
}

/// Карта сжатия: упорядоченные куски без пересечений и паузы исходной записи длиннее минуты.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Map {
    pub pieces: Vec<Piece>,
    pub pauses: Vec<(f64, f64)>,
    pub orig_len: f64,
}

impl Map {
    pub fn compact_len(&self) -> f64 {
        self.pieces.last().map_or(0.0, Piece::compact_end)
    }

    pub fn cut_seconds(&self) -> f64 {
        self.orig_len - self.compact_len()
    }

    /// Время сжатой записи -> время исходной. Точка на стыке двух кусков — конец предыдущего.
    pub fn to_original(&self, t: f64) -> f64 {
        if self.pieces.is_empty() {
            return t;
        }
        let t = t.clamp(0.0, self.compact_len());
        // последний кусок, начавшийся строго раньше t: стык уходит в предыдущий
        let k = self.pieces.partition_point(|p| p.compact_start < t).saturating_sub(1);
        let p = &self.pieces[k];
        p.orig_start + (t - p.compact_start)
    }

    /// Время исходной записи -> время сжатой; None — точка в вырезанной тишине.
    #[cfg(test)]
    pub fn to_compact(&self, t: f64) -> Option<f64> {
        self.pieces
            .iter()
            .find(|p| t >= p.orig_start && t <= p.orig_end)
            .map(|p| p.compact_start + (t - p.orig_start))
    }

    /// Реплики и их слова (`extra["words"]`) — на исходную шкалу; концы переводятся отдельно, поэтому
    /// реплика через стык накрывает вырезанную тишину.
    pub fn segments_to_original(&self, segs: &mut [dub_core::Segment]) {
        for s in segs {
            s.start = self.to_original(s.start);
            s.end = self.to_original(s.end);
            if let Some(Value::Array(words)) = s.extra.get_mut("words") {
                for w in words {
                    for key in ["start", "end"] {
                        if let Some(t) = w.get(key).and_then(Value::as_f64) {
                            w[key] = json!(self.to_original(t));
                        }
                    }
                }
            }
        }
    }

    pub fn diar_to_original(&self, d: &dub_asr::DiarTurns) -> dub_asr::DiarTurns {
        dub_asr::DiarTurns {
            turns: d
                .turns
                .iter()
                .map(|t| dub_asr::Turn { start: self.to_original(t.start), end: self.to_original(t.end), speaker: t.speaker })
                .collect(),
            n_speakers: d.n_speakers,
            ref_windows: d.ref_windows.iter().map(|(k, w)| (*k, (self.to_original(w.0), self.to_original(w.1)))).collect(),
        }
    }

    /// Запись в `meta.extra.compact_silence` проекта.
    pub fn meta(&self) -> Value {
        json!({
            "ver": VER,
            "applied": true,
            "pieces": self.pieces.iter().map(|p| [p.orig_start, p.orig_end, p.compact_start]).collect::<Vec<_>>(),
            "pauses": self.pauses.iter().map(|p| [p.0, p.1]).collect::<Vec<_>>(),
        })
    }
}

/// `meta.extra.compact_silence` анализа, который сжатие не применил.
pub fn skipped_meta(reason: &str) -> Value {
    json!({ "ver": VER, "applied": false, "reason": reason })
}

/// Карта по огибающей 16 кГц записи: кадр тише −60 дБFS — тишина, непрерывная тишина длиннее 20 с — пауза.
pub fn plan(env: &[f32], frame_sec: f64, orig_len: f64) -> Map {
    let threshold = 10f64.powf(SILENCE_DBFS / 20.0) as f32;
    let mut cuts: Vec<(f64, f64)> = Vec::new();
    let mut pauses = Vec::new();
    let mut i = 0;
    while i < env.len() {
        if env[i] >= threshold {
            i += 1;
            continue;
        }
        let first = i;
        while i < env.len() && env[i] < threshold {
            i += 1;
        }
        let a = first as f64 * frame_sec;
        let b = (i as f64 * frame_sec).min(orig_len);
        if b - a <= MIN_PAUSE {
            continue;
        }
        if b - a > LONG_PAUSE {
            // до миллисекунды: карта в памяти совпадает с прочитанной из map.json
            pauses.push(((a * 1000.0).round() / 1000.0, (b * 1000.0).round() / 1000.0));
        }
        let (lead, trail) = (first == 0, i == env.len());
        if lead && trail {
            // звука в записи нет
            continue;
        }
        // Запас округляется наружу от звука, чтобы на сетке его осталось не меньше 10 с.
        let from = if lead { 0.0 } else { ((a + PAD) * GRID).ceil() / GRID };
        let to = if trail { orig_len } else { ((b - PAD) * GRID).floor() / GRID };
        if to > from {
            cuts.push((from, to));
        }
    }
    let mut pieces = Vec::with_capacity(cuts.len() + 1);
    let (mut at, mut compact) = (0.0f64, 0.0f64);
    for (from, to) in cuts.into_iter().chain(std::iter::once((orig_len, orig_len))) {
        if from > at {
            pieces.push(Piece { orig_start: at, orig_end: from, compact_start: compact });
            compact = ((compact + (from - at)) * GRID).round() / GRID;
        }
        at = to;
    }
    if pieces.is_empty() && orig_len > 0.0 {
        pieces.push(Piece { orig_start: 0.0, orig_end: orig_len, compact_start: 0.0 });
    }
    Map { pieces, pauses, orig_len }
}

fn frame_of(t: f64, sr: u32) -> u64 {
    (t * sr as f64).round().max(0.0) as u64
}

fn open(path: &Path) -> Result<WavReader<std::io::BufReader<std::fs::File>>, String> {
    WavReader::open(path).map_err(|e| format!("open {}: {e}", path.display()))
}

/// Карта по `vocals16.wav` без чтения всей записи в память.
pub fn plan_wav(vocals16: &Path) -> Result<Map, String> {
    let mut r = open(vocals16)?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let total = r.duration() as usize;
    let mut env = Vec::new();
    let mut frame_sec = FRAME_SEC;
    let mut done = 0usize;
    while done < total {
        let n = BLOCK_FRAMES.min(total - done);
        let block = read_mono(&mut r, spec, n * ch)?;
        if block.is_empty() {
            break;
        }
        let (e, f) = dub_asr::speech_envelope(&block, spec.sample_rate, FRAME_SEC);
        env.extend(e);
        frame_sec = f;
        done += block.len();
    }
    Ok(plan(&env, frame_sec, total as f64 / spec.sample_rate.max(1) as f64))
}

fn read_mono(r: &mut WavReader<std::io::BufReader<std::fs::File>>, spec: WavSpec, n: usize) -> Result<Vec<f32>, String> {
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => r.samples::<f32>().take(n).collect::<Result<_, _>>().map_err(|e| format!("read f32: {e}"))?,
        SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().take(n).map(|s| s.map(|v| v as f32 / max)).collect::<Result<_, _>>().map_err(|e| format!("read int: {e}"))?
        }
    };
    Ok(if ch <= 1 { interleaved } else { interleaved.chunks(ch).map(|fr| fr.iter().sum::<f32>() / ch as f32).collect() })
}

type Writer = WavWriter<std::io::BufWriter<std::fs::File>>;

/// Переписать `n` кадров с кадра `from` источника; вернуть, сколько кадров в нём нашлось.
fn copy_frames(r: &mut WavReader<std::io::BufReader<std::fs::File>>, w: &mut Writer, from: u64, n: u64) -> Result<u64, String> {
    let spec = r.spec();
    let ch = spec.channels.max(1) as u64;
    let total = r.duration() as u64;
    if from >= total || n == 0 {
        return Ok(0);
    }
    let n = n.min(total - from);
    r.seek(from as u32).map_err(|e| format!("seek: {e}"))?;
    let samples = (n * ch) as usize;
    match spec.sample_format {
        SampleFormat::Float => {
            for s in r.samples::<f32>().take(samples) {
                w.write_sample(s.map_err(|e| format!("read: {e}"))?).map_err(|e| format!("write: {e}"))?;
            }
        }
        SampleFormat::Int => {
            for s in r.samples::<i32>().take(samples) {
                w.write_sample(s.map_err(|e| format!("read: {e}"))?).map_err(|e| format!("write: {e}"))?;
            }
        }
    }
    Ok(n)
}

fn zeros(w: &mut Writer, spec: WavSpec, n: u64) -> Result<(), String> {
    let samples = n * spec.channels.max(1) as u64;
    for _ in 0..samples {
        match spec.sample_format {
            SampleFormat::Float => w.write_sample(0.0f32),
            SampleFormat::Int => w.write_sample(0i32),
        }
        .map_err(|e| format!("write: {e}"))?;
    }
    Ok(())
}

/// Сжать запись любой частоты и числа каналов по карте: остаются только куски.
pub fn cut_wav(src: &Path, dst: &Path, map: &Map) -> Result<(), String> {
    let mut r = open(src)?;
    let spec = r.spec();
    dub_core::atomic::write_with(dst, |tmp| {
        let mut w = WavWriter::create(tmp, spec).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        let mut written = 0u64;
        for p in &map.pieces {
            let (from, to) = (frame_of(p.orig_start, spec.sample_rate), frame_of(p.orig_end, spec.sample_rate));
            let at = frame_of(p.compact_start, spec.sample_rate);
            // стык на сетке совпадает точно; конец файла может разойтись на отсчёт — выравниваем нулями
            if at > written {
                zeros(&mut w, spec, at - written)?;
                written = at;
            }
            written += copy_frames(&mut r, &mut w, from, to.saturating_sub(from))?;
        }
        w.finalize().map_err(|e| format!("finalize {}: {e}", tmp.display()))
    })
    .map_err(|e| format!("{} -> {}: {e}", src.display(), dst.display()))
}

/// Развернуть сжатую запись обратно на исходную шкалу: длина `orig_frames`, нули на месте вырезанного.
pub fn expand_wav(compact_src: &Path, dst: &Path, map: &Map, orig_frames: u64) -> Result<(), String> {
    let mut r = open(compact_src)?;
    let spec = r.spec();
    dub_core::atomic::write_with(dst, |tmp| {
        let mut w = WavWriter::create(tmp, spec).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        let mut written = 0u64;
        for p in &map.pieces {
            let from = frame_of(p.orig_start, spec.sample_rate).min(orig_frames);
            let to = frame_of(p.orig_end, spec.sample_rate).min(orig_frames);
            if from > written {
                zeros(&mut w, spec, from - written)?;
                written = from;
            }
            let want = to.saturating_sub(written);
            let got = copy_frames(&mut r, &mut w, frame_of(p.compact_start, spec.sample_rate), want)?;
            zeros(&mut w, spec, want - got)?;
            written += want;
        }
        zeros(&mut w, spec, orig_frames.saturating_sub(written))?;
        w.finalize().map_err(|e| format!("finalize {}: {e}", tmp.display()))
    })
    .map_err(|e| format!("{} -> {}: {e}", compact_src.display(), dst.display()))
}

/// Число кадров WAV.
pub fn frames(path: &Path) -> Result<u64, String> {
    Ok(open(path)?.duration() as u64)
}

/// Файлы анализа со сжатием. Всё сжатое — под `<work_dir>/compact/`.
pub struct Files {
    pub dir: PathBuf,
    pub vocals16: PathBuf,
    pub map: PathBuf,
    pub audio_hq: PathBuf,
    /// Временная полная 44,1 кГц запись, когда общей `audio_hq.wav` нет.
    pub audio_hq_full: PathBuf,
    pub stems: PathBuf,
    pub stems_mark: PathBuf,
    pub clean16: PathBuf,
    /// Чистый голос на исходной шкале для кастинга, `voice_slots` и выравнивания — единственный файл
    /// вне `compact/`.
    pub clean16_expanded: PathBuf,
}

impl Files {
    pub fn under(work_dir: &Path) -> Self {
        let dir = work_dir.join("compact");
        let stems = dir.join("stems");
        Files {
            vocals16: dir.join("vocals16.wav"),
            map: dir.join("map.json"),
            audio_hq: dir.join("audio_hq.wav"),
            audio_hq_full: dir.join("audio_hq_full.wav"),
            stems_mark: stems.join("compact.mark"),
            clean16: dir.join("vocals16_clean.wav"),
            clean16_expanded: work_dir.join("vocals16_clean.wav"),
            stems,
            dir,
        }
    }

    /// Отпечаток карты: сжатые стемы посчитаны по этой карте.
    pub fn map_tag(map: &Map) -> Result<String, String> {
        let body = serde_json::to_vec(map).map_err(|e| format!("compact map: {e}"))?;
        Ok(blake3::hash(&body).to_hex().to_string())
    }

    pub fn stems_current(&self, map: &Map) -> Result<bool, String> {
        match std::fs::read_to_string(&self.stems_mark) {
            Ok(tag) => Ok(tag == Self::map_tag(map)? && self.stems.join("vocals.wav").is_file()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("{}: {e}", self.stems_mark.display())),
        }
    }

    pub fn mark_stems(&self, map: &Map) -> Result<(), String> {
        std::fs::write(&self.stems_mark, Self::map_tag(map)?).map_err(|e| format!("{}: {e}", self.stems_mark.display()))
    }

    /// Сжатые стемы другой карты — удалить вместе с вырезанной 44,1 кГц записью.
    pub fn drop_stale_stems(&self, map: &Map) -> Result<(), String> {
        if self.stems_current(map)? {
            return Ok(());
        }
        if self.stems.is_dir() {
            std::fs::remove_dir_all(&self.stems).map_err(|e| format!("{}: {e}", self.stems.display()))?;
        }
        for p in [&self.audio_hq, &self.clean16] {
            if p.is_file() {
                std::fs::remove_file(p).map_err(|e| format!("{}: {e}", p.display()))?;
            }
        }
        Ok(())
    }
}

/// Итог подготовки: карта и сжатая запись, либо сколько секунд вырезалось бы (меньше минуты).
pub enum Prepared {
    Applied(Map),
    Skipped { cut: f64 },
}

/// Построить карту по `vocals16.wav` и записать `compact/vocals16.wav` и `compact/map.json`.
/// Вырезается меньше минуты — ничего не пишется.
pub fn prepare(vocals16: &Path, files: &Files) -> Result<Prepared, String> {
    let map = plan_wav(vocals16)?;
    let cut = map.cut_seconds();
    if cut < MIN_SAVING {
        return Ok(Prepared::Skipped { cut });
    }
    std::fs::create_dir_all(&files.dir).map_err(|e| format!("{}: {e}", files.dir.display()))?;
    cut_wav(vocals16, &files.vocals16, &map)?;
    let body = serde_json::to_vec(&map).map_err(|e| format!("compact map: {e}"))?;
    dub_core::atomic::write(&files.map, &body)?;
    Ok(Prepared::Applied(map))
}

/// Карта из `compact/map.json` (повтор анализа с тем же извлечением).
pub fn load_map(files: &Files) -> Result<Map, String> {
    let body = std::fs::read(&files.map).map_err(|e| format!("{}: {e}", files.map.display()))?;
    serde_json::from_slice(&body).map_err(|e| format!("{}: {e}", files.map.display()))
}

/// Строки `— пауза N мин —` между репликами текстовой выдачи. `lines` — строки текста, `times` — начало
/// и конец каждой из них, `pauses` — паузы исходной записи длиннее минуты.
pub fn with_pauses(lines: &[&str], times: &[(f64, f64)], pauses: &[(f64, f64)]) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len() + pauses.len());
    let mut next = 0;
    for (i, line) in lines.iter().enumerate() {
        let start = times[i].0;
        // границы реплик распознаватель ставит неточно: допуск — запас тишины у звука
        while next < pauses.len() && pauses[next].1 <= start + PAD {
            if i > 0 && pauses[next].0 + PAD >= times[i - 1].1 {
                let minutes = ((pauses[next].1 - pauses[next].0) / 60.0).round() as i64;
                out.push(t!("transcript-pause", minutes = minutes));
            }
            next += 1;
        }
        out.push(line.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 16_000;

    fn tone(sec: f64) -> Vec<f32> {
        (0..(sec * SR as f64) as usize).map(|i| 0.5 * (i as f32 * 0.2).sin()).collect()
    }

    fn silence(sec: f64) -> Vec<f32> {
        vec![0.0; (sec * SR as f64) as usize]
    }

    fn concat(parts: &[Vec<f32>]) -> Vec<f32> {
        parts.concat()
    }

    fn map_of(samples: &[f32]) -> Map {
        let (env, f) = dub_asr::speech_envelope(samples, SR, FRAME_SEC);
        plan(&env, f, samples.len() as f64 / SR as f64)
    }

    #[test]
    fn compact_plan_keeps_padding_and_cuts_long_pauses() {
        let samples = concat(&[tone(2.0), silence(38.0), tone(1.0), silence(15.0), tone(1.0)]);
        let map = map_of(&samples);
        let frame = FRAME_SEC;
        // пауза 38 с стала 20 с: от конца первого тона до начала второго по сжатой шкале
        let end1 = map.to_compact(2.0).expect("тон в куске");
        let start2 = map.to_compact(40.0).expect("тон в куске");
        assert!(((start2 - end1) - 20.0).abs() <= frame, "{}", start2 - end1);
        // пауза 15 с не тронута
        let end2 = map.to_compact(41.0).unwrap();
        let start3 = map.to_compact(56.0).unwrap();
        assert!(((start3 - end2) - 15.0).abs() < 1e-9);
        // каждый отсчёт тона — в куске
        for t in (0..=200).map(|k| k as f64 * 0.01).chain((4000..=4100).map(|k| k as f64 * 0.01)).chain((5600..=5700).map(|k| k as f64 * 0.01)) {
            assert!(map.to_compact(t).is_some(), "{t}");
        }
        assert!(map.to_compact(22.0).is_none());
        assert!((map.cut_seconds() - 18.0).abs() <= frame);
        assert!(map.pauses.is_empty());
    }

    #[test]
    fn compact_to_original_roundtrips_and_is_monotonic() {
        let samples = concat(&[silence(5.0), tone(2.0), silence(40.0), tone(1.0), silence(100.0), tone(1.0), silence(4.0)]);
        let map = map_of(&samples);
        assert!(map.pieces.len() >= 3);
        let one = 1.0 / SR as f64;
        for (k, p) in map.pieces.iter().enumerate() {
            // начало куска после стыка — та же точка сжатой шкалы, что конец предыдущего
            let mut t = if k == 0 { p.orig_start } else { p.orig_start + 0.01 };
            while t <= p.orig_end {
                let c = map.to_compact(t).unwrap();
                assert!((map.to_original(c) - t).abs() <= one, "{t}");
                t += 0.05;
            }
        }
        let mut prev = f64::NEG_INFINITY;
        let mut t = 0.0;
        while t <= map.compact_len() + 1.0 {
            let o = map.to_original(t);
            assert!(o >= prev, "{t}");
            prev = o;
            t += 0.01;
        }
        for w in map.pieces.windows(2) {
            let joint = w[1].compact_start;
            assert!((map.to_original(joint) - w[0].orig_end).abs() <= one);
        }
        assert_eq!(map.pauses.len(), 1);
    }

    fn two_piece_map() -> Map {
        Map {
            pieces: vec![
                Piece { orig_start: 0.0, orig_end: 10.0, compact_start: 0.0 },
                Piece { orig_start: 100.0, orig_end: 110.0, compact_start: 10.0 },
            ],
            pauses: vec![(8.5, 101.5)],
            orig_len: 110.0,
        }
    }

    #[test]
    fn compact_maps_segments_words_and_turns_back() {
        let map = two_piece_map();
        let mut extra = serde_json::Map::new();
        extra.insert(
            "words".into(),
            json!([{ "word": "a", "start": 9.0, "end": 9.8 }, { "word": "b", "start": 10.0, "end": 10.5 }, { "word": "c", "start": 10.5, "end": 11.0 }]),
        );
        let mut segs = vec![dub_core::Segment { id: "s0".into(), start: 9.0, end: 11.0, extra, ..Default::default() }];
        map.segments_to_original(&mut segs);
        assert_eq!((segs[0].start, segs[0].end), (9.0, 101.0));
        let words: Vec<(f64, f64)> = segs[0].extra["words"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| (w["start"].as_f64().unwrap(), w["end"].as_f64().unwrap()))
            .collect();
        assert_eq!(words, vec![(9.0, 9.8), (10.0, 100.5), (100.5, 101.0)]);
        let flat: Vec<f64> = words.iter().flat_map(|w| [w.0, w.1]).collect();
        assert!(flat.windows(2).all(|p| p[0] <= p[1]));

        let mut ref_windows = std::collections::HashMap::new();
        ref_windows.insert(1, (12.0, 14.0));
        let diar = dub_asr::DiarTurns {
            turns: vec![dub_asr::Turn { start: 2.0, end: 12.0, speaker: 0 }, dub_asr::Turn { start: 12.0, end: 15.0, speaker: 1 }],
            n_speakers: 2,
            ref_windows,
        };
        let back = map.diar_to_original(&diar);
        let turns: Vec<(f64, f64, i32)> = back.turns.iter().map(|t| (t.start, t.end, t.speaker)).collect();
        assert_eq!(turns, vec![(2.0, 102.0, 0), (102.0, 105.0, 1)]);
        assert_eq!(back.ref_windows[&1], (102.0, 104.0));
        assert_eq!(back.n_speakers, 2);
    }

    fn write16(path: &Path, samples: &[f32]) {
        crate::wavio::write_mono_f32(path, samples, SR).unwrap();
    }

    #[test]
    fn compact_skips_when_saving_under_a_minute() {
        let dir = tempfile::tempdir().unwrap();
        let v = dir.path().join("vocals16.wav");
        write16(&v, &concat(&[tone(2.0), silence(50.0), tone(1.0)]));
        let files = Files::under(dir.path());
        match prepare(&v, &files).unwrap() {
            Prepared::Skipped { cut } => assert!((cut - 30.0).abs() < 0.1, "{cut}"),
            Prepared::Applied(_) => panic!("вырезается меньше минуты"),
        }
        assert!(!files.dir.exists());
        assert_eq!(skipped_meta("x")["applied"], false);

        write16(&v, &concat(&[tone(2.0), silence(100.0), tone(1.0)]));
        let Prepared::Applied(map) = prepare(&v, &files).unwrap() else { panic!("вырезается больше минуты") };
        assert!(files.vocals16.is_file() && files.map.is_file());
        assert_eq!(load_map(&files).unwrap(), map);
        let meta = map.meta();
        assert_eq!(meta["applied"], true);
        assert_eq!(meta["pauses"].as_array().unwrap().len(), 1);
        let (cut, _) = crate::wavio::read_mono_f32(&files.vocals16).unwrap();
        assert_eq!(cut.len() as u64, frame_of(map.compact_len(), SR));
    }

    #[test]
    fn expanded_clean_vocals_match_original_length() {
        let dir = tempfile::tempdir().unwrap();
        let v = dir.path().join("vocals16.wav");
        let original = concat(&[tone(2.0), silence(100.0), tone(1.0), silence(0.3)]);
        write16(&v, &original);
        let files = Files::under(dir.path());
        let Prepared::Applied(map) = prepare(&v, &files).unwrap() else { panic!() };
        // «чистый голос» — та же сжатая запись: развёртка обязана вернуть исходную
        expand_wav(&files.vocals16, &files.clean16_expanded, &map, frames(&v).unwrap()).unwrap();
        let (back, sr) = crate::wavio::read_mono_f32(&files.clean16_expanded).unwrap();
        assert_eq!(sr, SR);
        assert_eq!(back.len(), original.len());
        for (i, (&a, &b)) in original.iter().zip(&back).enumerate() {
            let t = i as f64 / SR as f64;
            if map.to_compact(t).is_some() {
                assert!((a - b).abs() < 1e-6, "голос на месте {t}");
            } else {
                assert_eq!(b, 0.0, "нули на вырезанном {t}");
            }
        }
    }

    #[test]
    fn compacted_files_stay_out_of_shared_stems() {
        let dir = tempfile::tempdir().unwrap();
        let wd = dir.path();
        let f = Files::under(wd);
        let compact = wd.join("compact");
        for p in [&f.vocals16, &f.map, &f.audio_hq, &f.audio_hq_full, &f.stems, &f.stems_mark, &f.clean16] {
            assert!(p.starts_with(&compact), "{}", p.display());
            assert!(!p.starts_with(wd.join("stems")), "{}", p.display());
            assert_ne!(p, &wd.join("audio_hq.wav"));
            assert_ne!(p, &wd.join("vocals16.wav"));
        }
        assert_eq!(f.clean16_expanded, wd.join("vocals16_clean.wav"));
        let v = wd.join("vocals16.wav");
        write16(&v, &concat(&[tone(1.0), silence(100.0), tone(1.0)]));
        let Prepared::Applied(map) = prepare(&v, &f).unwrap() else { panic!() };
        std::fs::create_dir_all(&f.stems).unwrap();
        std::fs::write(f.stems.join("vocals.wav"), b"x").unwrap();
        f.mark_stems(&map).unwrap();
        assert!(f.stems_current(&map).unwrap());
        let mut other = map.clone();
        other.pieces[0].orig_end += 0.01;
        f.drop_stale_stems(&other).unwrap();
        assert!(!f.stems.exists());
        assert!(!wd.join("stems").exists() && !wd.join("audio_hq.wav").exists());
    }

    #[test]
    fn cut_keeps_multichannel_44k_on_the_same_grid() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("hq.wav");
        let spec = WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 16, sample_format: SampleFormat::Int };
        let mut w = WavWriter::create(&src, spec).unwrap();
        for i in 0..(44_100 * 4) {
            w.write_sample((i % 30000) as i16).unwrap();
            w.write_sample(-((i % 30000) as i16)).unwrap();
        }
        w.finalize().unwrap();
        let map = Map {
            pieces: vec![
                Piece { orig_start: 0.0, orig_end: 1.0, compact_start: 0.0 },
                Piece { orig_start: 3.0, orig_end: 4.0, compact_start: 1.0 },
            ],
            pauses: Vec::new(),
            orig_len: 4.0,
        };
        let dst = dir.path().join("cut.wav");
        cut_wav(&src, &dst, &map).unwrap();
        let r = WavReader::open(&dst).unwrap();
        assert_eq!((r.spec().channels, r.spec().sample_rate, r.duration()), (2, 44_100, 88_200));
        let s: Vec<i32> = WavReader::open(&dst).unwrap().samples::<i32>().map(Result::unwrap).collect();
        assert_eq!(s[44_100 * 2], 44_100 * 3 % 30000);
        assert_eq!(s[44_100 * 2 + 1], -(44_100 * 3 % 30000));
    }

    #[test]
    fn pause_lines_go_between_the_lines_around_a_long_pause() {
        let _language = crate::i18n::test_language("ru");
        let out = with_pauses(&["a", "b", "c"], &[(0.0, 1.0), (200.0, 201.0), (202.0, 203.0)], &[(2.5, 198.5)]);
        assert_eq!(out, vec!["a", "— пауза 3 мин —", "b", "c"]);
    }

    #[test]
    fn compaction_messages_read_in_the_window_language() {
        let _language = crate::i18n::test_language("ru");
        assert_eq!(
            t!("analyze-compact-skipped-short", cut = "47", min = "60"),
            "сжатие тишины пропущено: вырезалось бы только 47 с, меньше 60 с — анализ по исходной записи"
        );
        assert_eq!(t!("analyze-compact-done", left = "70.3", total = "318.0", pauses = 4), "тишина сжата: осталось 70.3 мин из 318.0 мин; пауз длиннее минуты: 4");
        crate::i18n::set_language("en").unwrap();
        assert_eq!(t!("analyze-compact-skipped-subs"), "silence compaction skipped: the imported subtitles are timed on the original recording");
        assert_eq!(t!("analyze-compact-searching"), "looking for long silence to compact");
    }

    #[test]
    fn flag_and_key_input() {
        assert!(flag(Some("1")) && !flag(Some("0")) && !flag(None));
        assert_eq!(key_input("h", false), "h");
        assert_ne!(key_input("h", true), "h");
    }
}
