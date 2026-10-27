//! WAV I/O сепарации: чтение в интерливнутый f32 и запись IEEE-float32. Вход — микс от ffmpeg (PCM16, у файла
//! длиннее 4 ГиБ — RF64), выход движка BSRoformer (dr_wav, float32) и свои стемы.
//!
//! Файл больше 4 ГиБ не помещает размер в 32-битное поле: такой пишется RF64 (размеры в чанке ds64), а размер
//! data 0xFFFFFFFF без ds64 (ffmpeg и dr_wav без RF64) значит «до конца файла». Неполный последний кадр не читается.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Интерливнутый PCM во float32 + метаданные.
pub struct Audio {
    pub sample_rate: u32,
    pub channels: u16,
    pub data: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding {
    Int(u16),
    Float32,
}

/// Где в файле лежат кадры и как они записаны.
#[derive(Clone, Copy, Debug)]
struct Layout {
    sample_rate: u32,
    channels: u16,
    encoding: Encoding,
    data_offset: u64,
    frames: u64,
}

impl Layout {
    fn block_align(&self) -> u64 {
        let bytes = match self.encoding {
            Encoding::Int(bits) => (bits as u64).div_ceil(8),
            Encoding::Float32 => 4,
        };
        bytes * self.channels as u64
    }
}

const UNKNOWN_32: u32 = 0xFFFF_FFFF;
const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

fn layout(path: &Path) -> Result<Layout, String> {
    let bad = |what: &str| format!("open {}: {what}", path.display());
    let mut f = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let file_len = f.metadata().map_err(|e| format!("open {}: {e}", path.display()))?.len();
    let mut head = [0u8; 12];
    f.read_exact(&mut head).map_err(|_| bad("not a WAV file: shorter than its header"))?;
    let rf64 = match &head[0..4] {
        b"RIFF" => false,
        b"RF64" => true,
        _ => return Err(bad("not a WAV file: no RIFF or RF64 tag")),
    };
    if &head[8..12] != b"WAVE" {
        return Err(bad("not a WAV file: no WAVE tag"));
    }
    let mut ds64_data: Option<u64> = None;
    let mut fmt: Option<(u32, u16, Encoding)> = None;
    let mut pos: u64 = 12;
    loop {
        let mut ch = [0u8; 8];
        f.seek(SeekFrom::Start(pos)).map_err(|e| bad(&e.to_string()))?;
        if f.read_exact(&mut ch).is_err() {
            return Err(bad("no data chunk"));
        }
        let id = [ch[0], ch[1], ch[2], ch[3]];
        let size32 = u32_at(&ch, 4);
        let body = pos + 8;
        match &id {
            b"ds64" => {
                let mut d = [0u8; 24];
                f.read_exact(&mut d).map_err(|_| bad("ds64 chunk cut short"))?;
                ds64_data = Some(u64_at(&d, 8));
            }
            b"fmt " => {
                let mut d = vec![0u8; (size32 as usize).min(64)];
                if d.len() < 16 {
                    return Err(bad("fmt chunk shorter than 16 bytes"));
                }
                f.read_exact(&mut d).map_err(|_| bad("fmt chunk cut short"))?;
                let mut tag = u16_at(&d, 0);
                let channels = u16_at(&d, 2);
                let sample_rate = u32_at(&d, 4);
                let bits = u16_at(&d, 14);
                if tag == WAVE_FORMAT_EXTENSIBLE {
                    if d.len() < 26 {
                        return Err(bad("WAVE_FORMAT_EXTENSIBLE without its subformat"));
                    }
                    tag = u16_at(&d, 24);
                }
                let encoding = match (tag, bits) {
                    (WAVE_FORMAT_PCM, 8 | 16 | 24 | 32) => Encoding::Int(bits),
                    (WAVE_FORMAT_IEEE_FLOAT, 32) => Encoding::Float32,
                    _ => return Err(bad(&format!("unsupported sample format {tag} with {bits} bits"))),
                };
                if channels == 0 || sample_rate == 0 {
                    return Err(bad("no channels or no sample rate"));
                }
                fmt = Some((sample_rate, channels, encoding));
            }
            b"data" => {
                let (sample_rate, channels, encoding) = fmt.ok_or_else(|| bad("data chunk before fmt chunk"))?;
                let rest = file_len.saturating_sub(body);
                let size = match (rf64, size32, ds64_data) {
                    (true, UNKNOWN_32, Some(n)) => n,
                    (true, UNKNOWN_32, None) => return Err(bad("RF64 without its ds64 chunk")),
                    (false, UNKNOWN_32, _) => rest,
                    (_, n, _) => n as u64,
                };
                if size > rest {
                    return Err(bad(&format!("the data chunk names {size} bytes and the file has {rest}: it is cut short")));
                }
                let mut l = Layout { sample_rate, channels, encoding, data_offset: body, frames: 0 };
                l.frames = size / l.block_align();
                return Ok(l);
            }
            _ => {}
        }
        if size32 == UNKNOWN_32 {
            return Err(bad("a chunk of unknown size before the data chunk"));
        }
        let size = size32 as u64;
        pos = body + size + (size & 1);
    }
}

fn decode(bytes: &[u8], encoding: Encoding, out: &mut Vec<f32>) {
    match encoding {
        Encoding::Float32 => out.extend(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
        Encoding::Int(8) => out.extend(bytes.iter().map(|&b| (b as f32 - 128.0) / 128.0)),
        Encoding::Int(16) => out.extend(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)),
        Encoding::Int(24) => out.extend(bytes.chunks_exact(3).map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0)),
        Encoding::Int(_) => out.extend(bytes.chunks_exact(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0)),
    }
}

/// Кадры [from_frame, from_frame + n_frames) файла по его раскладке (за концом — сколько есть).
fn read_frames(path: &Path, l: &Layout, from_frame: u64, n_frames: u64) -> Result<Vec<f32>, String> {
    let from = from_frame.min(l.frames);
    let n = n_frames.min(l.frames - from);
    let align = l.block_align();
    let mut f = BufReader::with_capacity(1 << 20, File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?);
    f.seek(SeekFrom::Start(l.data_offset + from * align)).map_err(|e| format!("seek {}: {e}", path.display()))?;
    let mut out = Vec::with_capacity((n * l.channels as u64) as usize);
    let mut left = n * align;
    let step = (align * 65_536) as usize;
    let mut buf = vec![0u8; step];
    while left > 0 {
        let take = (left as usize).min(step);
        f.read_exact(&mut buf[..take]).map_err(|e| format!("read {}: {e}", path.display()))?;
        decode(&buf[..take], l.encoding, &mut out);
        left -= take as u64;
    }
    Ok(out)
}

/// Прочитать WAV в интерливнутый f32 (int-сэмплы нормализуются в [-1,1]).
pub fn read_f32(path: &Path) -> Result<Audio, String> {
    let l = layout(path)?;
    let data = read_frames(path, &l, 0, l.frames)?;
    Ok(Audio { sample_rate: l.sample_rate, channels: l.channels, data })
}

/// Записать интерливнутый f32 в WAV IEEE-float32 (тот же формат, что отдаёт движок).
pub fn write_f32(path: &Path, a: &Audio) -> Result<(), String> {
    let mut w = StreamWriter::create(path, a.sample_rate, a.channels)?;
    w.write(&a.data)?;
    w.finalize()
}

/// Метаданные WAV без чтения данных: (sample_rate, channels, длительность в СЕКУНДАХ).
pub fn probe(path: &Path) -> Result<(u32, u16, f64), String> {
    let l = layout(path)?;
    Ok((l.sample_rate, l.channels, l.frames as f64 / l.sample_rate as f64))
}

/// Прочитать ДИАПАЗОН фреймов [from_frame, from_frame+n_frames) в интерливнутый f32 — окно длинного
/// файла без загрузки целиком (4ч 44.1к stereo f32 = ~5ГБ, целиком нельзя).
pub fn read_f32_range(path: &Path, from_frame: u32, n_frames: u32) -> Result<Audio, String> {
    let l = layout(path)?;
    let data = read_frames(path, &l, from_frame as u64, n_frames as u64)?;
    Ok(Audio { sample_rate: l.sample_rate, channels: l.channels, data })
}

/// Длина заголовка своих файлов: RIFF(12) + JUNK под ds64 (8+28) + fmt (8+16) + заголовок data (8).
const HEADER_LEN: u64 = 12 + 36 + 24 + 8;
const JUNK_AT: u64 = 12;
const DATA_SIZE_AT: u64 = HEADER_LEN - 4;

/// Потоковый писатель WAV IEEE-float32: держит файл открытым, дописывает чанки — итоговые stems длинного
/// файла собираются без буферизации всего трека в RAM. Место под ds64 занято чанком JUNK: файл больше
/// 4 ГиБ при закрытии становится RF64, меньший остаётся обычным RIFF.
pub struct StreamWriter {
    file: BufWriter<File>,
    data_bytes: u64,
    path: std::path::PathBuf,
    channels: u16,
}

impl StreamWriter {
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> Result<Self, String> {
        let channels = channels.max(1);
        let f = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
        let mut file = BufWriter::with_capacity(1 << 20, f);
        let block = 4 * channels as u32;
        let mut h = Vec::with_capacity(HEADER_LEN as usize);
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&0u32.to_le_bytes());
        h.extend_from_slice(b"WAVE");
        h.extend_from_slice(b"JUNK");
        h.extend_from_slice(&28u32.to_le_bytes());
        h.extend_from_slice(&[0u8; 28]);
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&WAVE_FORMAT_IEEE_FLOAT.to_le_bytes());
        h.extend_from_slice(&channels.to_le_bytes());
        h.extend_from_slice(&sample_rate.to_le_bytes());
        h.extend_from_slice(&(sample_rate * block).to_le_bytes());
        h.extend_from_slice(&(block as u16).to_le_bytes());
        h.extend_from_slice(&32u16.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&0u32.to_le_bytes());
        file.write_all(&h).map_err(|e| format!("write {}: {e}", path.display()))?;
        Ok(Self { file, data_bytes: 0, path: path.to_path_buf(), channels })
    }

    pub fn write(&mut self, chunk: &[f32]) -> Result<(), String> {
        let mut bytes = Vec::with_capacity(chunk.len() * 4);
        for s in chunk {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.file.write_all(&bytes).map_err(|e| format!("write {}: {e}", self.path.display()))?;
        self.data_bytes += bytes.len() as u64;
        Ok(())
    }

    pub fn finalize(mut self) -> Result<(), String> {
        let err = |e: std::io::Error| format!("finalize {}: {e}", self.path.display());
        let riff = HEADER_LEN - 8 + self.data_bytes;
        fn put(file: &mut BufWriter<File>, at: u64, bytes: &[u8]) -> std::io::Result<()> {
            file.seek(SeekFrom::Start(at))?;
            file.write_all(bytes)
        }
        if riff <= UNKNOWN_32 as u64 - 1 {
            put(&mut self.file, 4, &(riff as u32).to_le_bytes()).map_err(err)?;
            put(&mut self.file, DATA_SIZE_AT, &(self.data_bytes as u32).to_le_bytes()).map_err(err)?;
        } else {
            let mut ds64 = Vec::with_capacity(36);
            ds64.extend_from_slice(b"ds64");
            ds64.extend_from_slice(&28u32.to_le_bytes());
            ds64.extend_from_slice(&riff.to_le_bytes());
            ds64.extend_from_slice(&self.data_bytes.to_le_bytes());
            ds64.extend_from_slice(&(self.data_bytes / (4 * self.channels as u64)).to_le_bytes());
            ds64.extend_from_slice(&0u32.to_le_bytes());
            put(&mut self.file, 0, b"RF64").map_err(err)?;
            put(&mut self.file, 4, &UNKNOWN_32.to_le_bytes()).map_err(err)?;
            put(&mut self.file, JUNK_AT, &ds64).map_err(err)?;
            put(&mut self.file, DATA_SIZE_AT, &UNKNOWN_32.to_le_bytes()).map_err(err)?;
        }
        self.file.flush().map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm16_header(channels: u16, rate: u32, data_size: u32) -> Vec<u8> {
        let block = 2 * channels;
        let mut h = Vec::new();
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&UNKNOWN_32.to_le_bytes());
        h.extend_from_slice(b"WAVE");
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
        h.extend_from_slice(&channels.to_le_bytes());
        h.extend_from_slice(&rate.to_le_bytes());
        h.extend_from_slice(&(rate * block as u32).to_le_bytes());
        h.extend_from_slice(&block.to_le_bytes());
        h.extend_from_slice(&16u16.to_le_bytes());
        h.extend_from_slice(b"LIST");
        h.extend_from_slice(&3u32.to_le_bytes());
        h.extend_from_slice(b"abc\0");
        h.extend_from_slice(b"data");
        h.extend_from_slice(&data_size.to_le_bytes());
        h
    }

    /// ffmpeg за 4 ГиБ без RF64 оставляет размер data 0xFFFFFFFF: кадры читаются до конца файла, неполный — нет.
    #[test]
    fn an_unknown_data_size_reads_to_the_end_of_the_file() {
        let dir = std::env::temp_dir().join(format!("dub-sep-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ffmpeg-over-4gib.wav");
        let mut b = pcm16_header(2, 44_100, UNKNOWN_32);
        for v in [0i16, 16_384, -16_384, 32_767, 1, 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(7);
        std::fs::write(&p, &b).unwrap();
        let (sr, ch, dur) = probe(&p).unwrap();
        assert_eq!((sr, ch), (44_100, 2));
        assert!((dur - 3.0 / 44_100.0).abs() < 1e-12);
        let a = read_f32(&p).unwrap();
        assert_eq!(a.data.len(), 6);
        assert!((a.data[1] - 0.5).abs() < 1e-6 && (a.data[2] + 0.5).abs() < 1e-6);
        assert_eq!(read_f32_range(&p, 1, 5).unwrap().data.len(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Свой файл: обычный RIFF, который читает и hound; тот же писатель за 4 ГиБ даёт RF64 с размерами в ds64.
    #[test]
    fn the_stream_writer_writes_riff_and_rf64_reads_back() {
        let dir = std::env::temp_dir().join(format!("dub-sep-wav-w-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("stem.wav");
        let mut w = StreamWriter::create(&p, 44_100, 2).unwrap();
        w.write(&[0.25, -0.25, 0.5, -0.5]).unwrap();
        w.finalize().unwrap();
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.duration(), 2);
        assert_eq!(read_f32(&p).unwrap().data, vec![0.25, -0.25, 0.5, -0.5]);

        let mut b = std::fs::read(&p).unwrap();
        let data = 16u64;
        let mut ds64 = Vec::new();
        ds64.extend_from_slice(b"ds64");
        ds64.extend_from_slice(&28u32.to_le_bytes());
        ds64.extend_from_slice(&(HEADER_LEN - 8 + data).to_le_bytes());
        ds64.extend_from_slice(&data.to_le_bytes());
        ds64.extend_from_slice(&2u64.to_le_bytes());
        ds64.extend_from_slice(&0u32.to_le_bytes());
        b[0..4].copy_from_slice(b"RF64");
        b[4..8].copy_from_slice(&UNKNOWN_32.to_le_bytes());
        b[JUNK_AT as usize..JUNK_AT as usize + 36].copy_from_slice(&ds64);
        b[DATA_SIZE_AT as usize..DATA_SIZE_AT as usize + 4].copy_from_slice(&UNKNOWN_32.to_le_bytes());
        b.extend_from_slice(&[0u8; 8]);
        std::fs::write(&p, &b).unwrap();
        assert_eq!(read_f32(&p).unwrap().data, vec![0.25, -0.25, 0.5, -0.5], "ds64 names the size, not the end of the file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Файлы больше 4 ГиБ на диске (DUB_SEP_BIG_DIR, около 14 ГБ места; файлы остаются там): микс ffmpeg с `-rf64 auto` и без него
    /// (размер data 0xFFFFFFFF) и свой стем RF64 читаются до последнего кадра, а стем читает и ffmpeg.
    #[test]
    #[ignore]
    fn files_over_4_gib_read_to_their_last_frame() {
        let dir = std::path::PathBuf::from(std::env::var("DUB_SEP_BIG_DIR").expect("DUB_SEP_BIG_DIR"));
        let secs = 6.0 * 3600.0 + 50.0 * 60.0;
        let frames = (secs * 44_100.0) as u64;
        for (name, rf64) in [("mix-rf64.wav", true), ("mix-ffff.wav", false)] {
            let p = dir.join(name);
            let mut cmd = dub_core::proc::host_command(dub_core::proc::ffmpeg());
            cmd.args(["-y", "-v", "error", "-f", "lavfi", "-i", &format!("sine=frequency=440:sample_rate=44100:duration={secs}"), "-ac", "2"]);
            if rf64 {
                cmd.args(["-rf64", "auto"]);
            }
            assert!(cmd.arg(&p).status().unwrap().success());
            assert!(std::fs::metadata(&p).unwrap().len() > u32::MAX as u64);
            let (sr, ch, dur) = probe(&p).unwrap();
            assert_eq!((sr, ch), (44_100, 2), "{name}");
            assert!((dur - secs).abs() < 0.01, "{name}: {dur}");
            let tail = read_f32_range(&p, (frames - 44_100) as u32, 44_100).unwrap();
            assert_eq!(tail.data.len(), 2 * 44_100, "{name}");
            assert!(tail.data.iter().any(|v| v.abs() > 0.05), "{name}: the tail is the sine, not zeros");
        }
        let stem = dir.join("stem-rf64.wav");
        let mut w = StreamWriter::create(&stem, 44_100, 2).unwrap();
        let second: Vec<f32> = (0..2 * 44_100).map(|k| ((k / 2) as f32 * 0.0627).sin() * 0.5).collect();
        let n = (4.0 * 3600.0) as usize;
        for _ in 0..n {
            w.write(&second).unwrap();
        }
        w.finalize().unwrap();
        assert!(std::fs::metadata(&stem).unwrap().len() > u32::MAX as u64);
        let (_, _, dur) = probe(&stem).unwrap();
        assert_eq!(dur, n as f64);
        assert_eq!(read_f32_range(&stem, ((n - 1) * 44_100) as u32, 44_100).unwrap().data, second);
        let out = dub_core::proc::host_command(dub_core::proc::ffprobe())
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
            .arg(&stem)
            .output()
            .unwrap();
        let ff: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        assert!((ff - n as f64).abs() < 0.01, "ffprobe reads the RF64 stem: {ff}");
    }
}
