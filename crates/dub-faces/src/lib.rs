//! dub-faces — кастинг персонажей: лица + голоса, персистентная база актёров на сериал (#115).
//!
//! Пайплайн: sample_frames (ffmpeg, ~1.5 fps) -> detect (SCRFD, лица+5 точек) -> align 112x112 ->
//! embed (LVFace-L, 512-d, L2) -> cluster (косинус-порог, детерминированно, через dub_asr) -> медоид
//! на кластер = персонаж -> связка лицо↔голос (со-встречаемость «лицо в кадре во время реплик спикера»
//! + опц. LR-ASD speaking-скор; argmax) -> аватарка (самый фронтальный/чёткий кадр) -> casting.json.
//!
//! Стек ЗАПЕРТ: SCRFD-10G-KPS, LVFace-L (ICCV 2025), LR-ASD (IJCV 2025). Модели через ort (load-dynamic,
//! onnxruntime.dll 1.28.2 в рантайме — как dub-asr/dub-ocr). LR-ASD ОПЦИОНАЛЕН: без экспортированного
//! ONNX связка работает только на со-встречаемости (см. asd.rs, документированный фолбэк).

mod anime_detect;
mod asd;
mod cast;
mod ccip;
mod detect;
mod embed;
mod frames;
mod link;
mod occluder;
mod ort_engine;
mod voice;

pub use anime_detect::AnimeFaceDetector;
pub use asd::AsdScorer;
pub use cast::{
    load_casting, match_cross_episode, save_casting, Casting, Character, CASTING_VERSION,
};
pub use ccip::{ccip_path, ccip_threshold, character_crop_bbox, l2_distance, CcipEmbedder, CCIP_DIM};
pub use detect::{Face, Scrfd};
pub use embed::{align_112, l2_normalize, LvFace};
pub use frames::{
    crop_sharpness, sample_frames, save_face_crop, stream_frames, FrameDisposition, SampledFrame,
    DEFAULT_FPS,
};
pub use link::{
    assign, cooccurrence_matrix, discriminative_prominence_score, Linkage, SpeakerTurn,
};
pub use occluder::{occluder_path, FaceOccluder};
pub use voice::{voice_cos_threshold, wespeaker_path, VoiceEmbedder, VOICE_DIM};

use std::path::Path;

/// Порог косинуса для кластеризации лиц в персонажей (LVFace-L L2-эмбеддинги). ~0.5 по ТЗ. Env
/// DUB_FACES_CLUSTER_COS для тюнинга.
pub fn cluster_cos_threshold() -> f32 {
    std::env::var("DUB_FACES_CLUSTER_COS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.5)
}

/// Порог косинуса для матчинга персонажей между эпизодами (лицо+голос). ~0.5 по ТЗ. Env
/// DUB_FACES_MATCH_COS.
pub fn match_cos_threshold() -> f32 {
    std::env::var("DUB_FACES_MATCH_COS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.5)
}

/// Ошибки кастинг-пайплайна.
#[derive(thiserror::Error, Debug)]
pub enum FacesError {
    #[error("model not found: {0}")]
    ModelMissing(String),
    #[error("ffmpeg: {0}")]
    Ffmpeg(String),
    /// ffmpeg завершился с ошибкой: код выхода и хвост stderr.
    #[error("ffmpeg exit code {code:?}: {tail}")]
    FfmpegExit { code: Option<i32>, tail: String },
    #[error("ort: {0}")]
    Ort(String),
    #[error("io: {0}")]
    Io(String),
    #[error("the model returned no outputs")]
    NoOutputs,
    /// Выход модели `model` не той формы.
    #[error("{model}: unexpected output shape {shape:?}")]
    OutputShape { model: &'static str, shape: Vec<usize> },
    #[error("{model}: expected {expected} outputs, got {got}")]
    OutputCount { model: &'static str, expected: usize, got: usize },
    /// Эмбеддинг лица ждёт выровненный кроп 112x112.
    #[error("the face embedding expects a 112x112 aligned crop, got {width}x{height}")]
    CropSize { width: u32, height: u32 },
    #[error("voice: expected {expected} Hz, got {got}")]
    SampleRate { expected: u32, got: u32 },
    #[error("voice: the clip is shorter than one fbank frame")]
    ClipTooShort,
}

impl FacesError {
    /// Стабильный код ошибки (аргументы — поля варианта).
    pub fn code(&self) -> &'static str {
        match self {
            FacesError::ModelMissing(_) => "faces_model_missing",
            FacesError::Ffmpeg(_) => "faces_ffmpeg",
            FacesError::FfmpegExit { .. } => "faces_ffmpeg_exit",
            FacesError::Ort(_) => "faces_ort",
            FacesError::Io(_) => "faces_io",
            FacesError::NoOutputs => "faces_no_outputs",
            FacesError::OutputShape { .. } => "faces_output_shape",
            FacesError::OutputCount { .. } => "faces_output_count",
            FacesError::CropSize { .. } => "faces_crop_size",
            FacesError::SampleRate { .. } => "faces_sample_rate",
            FacesError::ClipTooShort => "faces_clip_too_short",
        }
    }
}

/// Пути к весам кастинга.
pub struct FacesModels {
    /// SCRFD det_10g.onnx (детект лиц).
    pub scrfd: std::path::PathBuf,
    /// LVFace-L_Glint360K.onnx (эмбеддинг).
    pub lvface: std::path::PathBuf,
    /// LR-ASD ONNX (active-speaker), опционально — None/несуществующий = фолбэк на со-встречаемость.
    pub lr_asd: Option<std::path::PathBuf>,
}

impl FacesModels {
    /// Резолв путей относительно models_root (<root>/faces/…). LR-ASD ищем как экспортированный .onnx
    /// (сам чекпоинт .model экспортируется отдельно; если .onnx нет — LR-ASD выключен, см. asd.rs).
    pub fn resolve(models_root: &Path) -> Self {
        let faces = models_root.join("faces");
        let lr_onnx = faces.join("lr-asd").join("lr_asd.onnx");
        FacesModels {
            scrfd: faces.join("det_10g.onnx"),
            lvface: faces.join("LVFace-L_Glint360K.onnx"),
            lr_asd: if lr_onnx.is_file() { Some(lr_onnx) } else { None },
        }
    }

    pub fn available(&self) -> bool {
        self.scrfd.is_file() && self.lvface.is_file()
    }
}

/// Одно детектированное+эмбеддированное лицо на кадре (промежуточный результат пайплайна).
#[derive(Clone, Debug)]
pub struct FrameFace {
    /// Время кадра в секундах.
    pub t: f64,
    /// bbox в исходных координатах кадра.
    pub bbox: (f32, f32, f32, f32),
    pub score: f32,
    /// Резкость кропа лица (variance of Laplacian) — для выбора аватарки.
    pub sharpness: f32,
    /// Фронтальность 0..1 (симметрия глаз относительно носа) — для выбора аватарки.
    pub frontality: f32,
    /// L2-нормированный 512-d эмбеддинг лица.
    pub embedding: Vec<f32>,
}

/// Индекс кадра (путь к PNG аватарки) относительно каталога кадров.
#[derive(Clone, Debug)]
pub struct FaceCluster {
    /// Индексы FrameFace, попавших в кластер.
    pub members: Vec<usize>,
    /// Индекс медоида (самый центральный по косинусу внутри кластера).
    pub medoid: usize,
    /// Медоидный эмбеддинг (512-d, L2).
    pub embedding: Vec<f32>,
    /// Индекс кадра-аватарки (самый фронтальный/чёткий).
    pub avatar: usize,
}

/// Мин. score лица для попадания в кластеризацию (мелкие/фоновые/ложные детекты отбрасываем ДО O(N²)).
/// Env DUB_FACES_MIN_SCORE.
pub fn min_face_score() -> f32 {
    std::env::var("DUB_FACES_MIN_SCORE").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0.6)
}

/// Мин. сторона bbox лица (px) — фоновые/мелкие лица не дают устойчивого эмбеддинга. Env DUB_FACES_MIN_BBOX.
pub fn min_face_bbox() -> f32 {
    std::env::var("DUB_FACES_MIN_BBOX").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(40.0)
}

/// Косинус-порог дедупа почти-идентичных лиц соседних кадров (>порога -> тот же ракурс, пропускаем).
/// Env DUB_FACES_DEDUP_COS.
pub fn dedup_cos() -> f32 {
    std::env::var("DUB_FACES_DEDUP_COS").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0.9)
}

/// Хард-кап числа лиц перед кластеризацией (O(N²)): защита от квадратичного взрыва на длинном видео.
/// Env DUB_FACES_MAX.
pub fn max_faces_cap() -> usize {
    std::env::var("DUB_FACES_MAX").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(4000)
}

/// Мин. число кадров-лиц в кластере, чтобы он стал персонажем. Отсекает разовые ложняки (случайный
/// «лицеподобный» детект в одном кадре) и статичные фон-каракули (дедуп схлопывает их почти в один
/// кадр -> кластер мельче порога -> отброшен). Живой персонаж мелькает в нескольких кадрах. Env
/// DUB_FACES_MIN_CLUSTER.
pub fn min_cluster_members() -> usize {
    std::env::var("DUB_FACES_MIN_CLUSTER").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(2)
}

/// Мин. суммарное время НА ЭКРАНЕ (сек), чтобы НЕговорящий кластер стал персонажем. Требование юзера:
/// «не вставлять лица, которые появляются на долю секунды и не говорят». Кластер, привязанный к спикеру
/// (говорит), проходит ВСЕГДА, независимо от этого порога. Время = число кадров кластера / casting_fps.
/// Env DUB_FACES_MIN_ONSCREEN_SEC. 0 -> фильтр по времени выключен (только по «говорит»).
pub fn min_onscreen_secs() -> f64 {
    std::env::var("DUB_FACES_MIN_ONSCREEN_SEC").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(3.0)
}

/// Статистика отбраковки лиц (для лога — правило «no silent caps»).
#[derive(Clone, Debug, Default)]
pub struct FaceFilterStats {
    /// Отброшено по низкому score.
    pub dropped_score: usize,
    /// Отброшено по мелкому bbox.
    pub dropped_bbox: usize,
    /// Отброшено дедупом (почти-идентичное соседнему кадру).
    pub dropped_dedup: usize,
    /// Отброшено хард-капом (сверх max).
    pub dropped_cap: usize,
    /// Принято в кластеризацию.
    pub kept: usize,
}

/// Потоковый аккумулятор лиц (#115 перф): фильтрует+дедупит лица ПО ХОДУ детекта (без накопления всех
/// декодированных кадров). Держит только принятые FrameFace + кольцо последних эмбеддингов для дедупа.
/// Порядок вызова: на каждый кадр — offer(...) на каждый детект; в конце — finish() -> (faces, idx, stats).
pub struct FaceAccumulator {
    min_score: f32,
    min_bbox: f32,
    dedup_cos: f32,
    cap: usize,
    faces: Vec<FrameFace>,
    /// Индекс кадра-источника аватарки, синхронный с faces (вызывающий кладёт свой путь по этому индексу).
    avatar_frame_idx: Vec<usize>,
    /// Кольцо последних принятых эмбеддингов (для дедупа соседних кадров) — маленькое, O(1) на кадр.
    recent: std::collections::VecDeque<Vec<f32>>,
    stats: FaceFilterStats,
}

/// Сколько последних эмбеддингов держать для дедупа (несколько лиц в кадре + пара кадров назад).
const DEDUP_RING: usize = 16;

impl FaceAccumulator {
    pub fn new() -> Self {
        FaceAccumulator {
            min_score: min_face_score(),
            min_bbox: min_face_bbox(),
            dedup_cos: dedup_cos(),
            cap: max_faces_cap(),
            faces: Vec::new(),
            avatar_frame_idx: Vec::new(),
            recent: std::collections::VecDeque::with_capacity(DEDUP_RING),
            stats: FaceFilterStats::default(),
        }
    }

    /// Ёмкость исчерпана (достигнут хард-кап) — вызывающий может прекратить детект досрочно.
    pub fn is_full(&self) -> bool {
        self.faces.len() >= self.cap
    }

    /// Предложить одно детектированное+эмбеддированное лицо. `avatar_frame`: индекс кадра-источника
    /// (для последующего сохранения аватарки). Возвращает true, если лицо принято.
    pub fn offer(&mut self, face: FrameFace, avatar_frame: usize) -> bool {
        if self.faces.len() >= self.cap {
            self.stats.dropped_cap += 1;
            return false;
        }
        if face.score < self.min_score {
            self.stats.dropped_score += 1;
            return false;
        }
        let (x1, y1, x2, y2) = face.bbox;
        if (x2 - x1).max(0.0) < self.min_bbox || (y2 - y1).max(0.0) < self.min_bbox {
            self.stats.dropped_bbox += 1;
            return false;
        }
        // Дедуп: почти-идентичное лицо в одном из последних кадров (соседние кадры @1.5fps почти равны).
        if self.recent.iter().any(|e| dub_asr::cosine(e, &face.embedding) > self.dedup_cos) {
            self.stats.dropped_dedup += 1;
            return false;
        }
        if self.recent.len() >= DEDUP_RING {
            self.recent.pop_front();
        }
        self.recent.push_back(face.embedding.clone());
        self.faces.push(face);
        self.avatar_frame_idx.push(avatar_frame);
        self.stats.kept += 1;
        true
    }

    /// Завершить: вернуть принятые лица, их кадры-источники аватарок (синхронно) и статистику отбраковки.
    pub fn finish(self) -> (Vec<FrameFace>, Vec<usize>, FaceFilterStats) {
        (self.faces, self.avatar_frame_idx, self.stats)
    }
}

impl Default for FaceAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Кластеризовать лица в персонажей: косинус-порог через dub_asr::cluster_embeddings (детерминированно),
/// на кластер — медоид (макс. суммарный косинус к своим) + аватарка (макс. фронтальность*резкость*размер).
pub fn cluster_faces(faces: &[FrameFace], cos_threshold: f32) -> Vec<FaceCluster> {
    if faces.is_empty() {
        return Vec::new();
    }
    let embs: Vec<Vec<f32>> = faces.iter().map(|f| f.embedding.clone()).collect();
    let labels = dub_asr::cluster_embeddings(&embs, cos_threshold);
    let k = labels.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    let min_members = min_cluster_members();
    let mut clusters: Vec<FaceCluster> = Vec::new();
    for c in 0..k {
        let members: Vec<usize> = (0..faces.len()).filter(|&i| labels[i] == c).collect();
        // Разовые ложняки / статичные фон-каракули (после дедупа их 1 кадр) — не персонажи.
        if members.len() < min_members {
            continue;
        }
        // медоид: макс. суммарный косинус к остальным членам.
        let mut best_medoid = members[0];
        let mut best_sum = f32::MIN;
        for &i in &members {
            let mut sum = 0.0f32;
            for &j in &members {
                if i != j {
                    sum += dub_asr::cosine(&faces[i].embedding, &faces[j].embedding);
                }
            }
            if sum > best_sum || (sum == best_sum && i < best_medoid) {
                best_sum = sum;
                best_medoid = i;
            }
        }
        // аватарка: ЧЁТКОЕ НЕПЕРЕКРЫТОЕ фронтальное лицо. Ключевой сигнал против перекрытия/бэклита —
        // score детекции SCRFD (лицо за объектом/в контровом свете детектится с низкой уверенностью) в
        // паре с резкостью (перекрытый кадр обычно ещё и мутный/расфокус). Площадь — НЕ линейный бонус
        // (иначе крупный перекрытый бэклит-кадр забивал чёткий фронтальный — #115-баг), а насыщающий
        // гейт area/(area+K): мелкие лица давит, крупным бонуса не даёт.
        let mut best_av = members[0];
        let mut best_q = f32::MIN;
        for &i in &members {
            let (x1, y1, x2, y2) = faces[i].bbox;
            let area = ((x2 - x1).max(0.0) * (y2 - y1).max(0.0)).sqrt();
            let area_gate = area / (area + 120.0); // ~0.5 при 120px, ~0.76 при 380px — насыщается
            // score² — сильный штраф перекрытым/неуверенным детектам; резкость — против расфокуса/мыла.
            let q = faces[i].score * faces[i].score
                * (faces[i].sharpness + 1.0)
                * (faces[i].frontality + 0.05)
                * area_gate;
            if q > best_q || (q == best_q && i < best_av) {
                best_q = q;
                best_av = i;
            }
        }
        clusters.push(FaceCluster {
            members,
            medoid: best_medoid,
            embedding: faces[best_medoid].embedding.clone(),
            avatar: best_av,
        });
    }
    clusters
}

/// Фронтальность лица 0..1 по 5 точкам: симметрия глаз относительно линии нос↔центр рта. Профиль (нос
/// смещён к одному глазу) -> ниже. Грубая, но детерминированная метрика для выбора аватарки.
pub fn frontality(face: &Face) -> f32 {
    let (le, re, nose) = (face.kps[0], face.kps[1], face.kps[2]);
    let eye_dx = re.0 - le.0;
    if eye_dx.abs() < 1e-3 {
        return 0.0;
    }
    // положение носа между глазами по X, 0.5 = ровно посередине (фронтально).
    let t = (nose.0 - le.0) / eye_dx;
    // штраф за отклонение от центра.
    (1.0 - (t - 0.5).abs() * 2.0).clamp(0.0, 1.0)
}

/// Один прогон кастинга: кадры (SCRFD detect + LVFace embed) уже собраны в `faces`. Здесь — кластеризация
/// в персонажей, связка с голосами (со-встречаемость), ФИЛЬТР «только говорящие/существенные», сборка
/// Casting БЕЗ аватарок на диске (avatar-путь проставляет вызывающий, сохраняя кадр). voice_embeddings:
/// speaker_id -> эмбеддинг голоса, может быть пуст. `fps` — частота семплинга кадров кастинга (для перевода
/// «кадров кластера» -> «секунд на экране» в фильтре). Возвращает (Casting, кластеры, отброшено) —
/// кластеры синхронны characters по индексу (нужны вызывающему для аватарок); отброшено — для лога.
pub fn build_casting(
    faces: &[FrameFace],
    turns: &[SpeakerTurn],
    speakers: &[String],
    voice_embeddings: &std::collections::HashMap<String, Vec<f32>>,
    genders: &std::collections::HashMap<String, String>,
    cos_threshold: f32,
    fps: f64,
    min_onscreen_sec: f64,
) -> (Casting, Vec<FaceCluster>, usize) {
    let all_clusters = cluster_faces(faces, cos_threshold);
    // времена кадров каждого кластера (для со-встречаемости).
    let frame_times: Vec<Vec<f64>> = all_clusters
        .iter()
        .map(|c| {
            let mut ts: Vec<f64> = c.members.iter().map(|&i| faces[i].t).collect();
            ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            ts
        })
        .collect();
    // Дискриминативная связка (как прод-путь casting.rs::faces_to_speakers): эксклюзивность лица важнее
    // числа кадров, иначе «слушатель» в кадре у всех перетягивает спикера. prominence здесь единичный.
    let linkage = assign(
        &discriminative_prominence_score(&cooccurrence_matrix(&frame_times, turns, speakers), &[]),
        speakers,
    );

    // ФИЛЬТР (требование юзера): персонаж = кластер, который ЛИБО говорит (привязан к спикеру), ЛИБО
    // суммарно на экране >= порога. Не говорит И мелькнул на долю секунды -> отбросить (не персонаж).
    // Фильтруем ДО сборки Character, чтобы casting.characters и возвращаемые clusters оставались синхронны
    // по индексу (вызывающий берёт clusters[ci] для аватарки character[ci]). Порог/fps передаёт вызывающий
    // (env читается на уровне casting.rs, не в этой функции — детерминизм тестов).
    let min_secs = min_onscreen_sec;
    let fps = if fps > 0.0 { fps } else { DEFAULT_FPS };
    let mut clusters: Vec<FaceCluster> = Vec::with_capacity(all_clusters.len());
    let mut linked: Vec<Option<String>> = Vec::with_capacity(all_clusters.len());
    let mut dropped = 0usize;
    for (ci, cl) in all_clusters.into_iter().enumerate() {
        let speaker = linkage.get(&ci).cloned();
        let onscreen = cl.members.len() as f64 / fps;
        let keep = speaker.is_some() || min_secs <= 0.0 || onscreen >= min_secs;
        if keep {
            linked.push(speaker);
            clusters.push(cl);
        } else {
            dropped += 1;
        }
    }

    // id переприсваиваем ПО ОТФИЛЬТРОВАННОМУ порядку (char_0..char_n без дыр) — единый источник для
    // аватарок char_<i>.jpg и фронтовых ссылок.
    let mut casting = Casting::default();
    for (ci, cl) in clusters.iter().enumerate() {
        let speaker = linked[ci].clone();
        let voice_emb = speaker
            .as_ref()
            .and_then(|s| voice_embeddings.get(s))
            .cloned()
            .unwrap_or_default();
        let gender = speaker
            .as_ref()
            .and_then(|s| genders.get(s))
            .cloned()
            .unwrap_or_default();
        casting.characters.push(Character {
            id: format!("char_{ci}"),
            name: String::new(),
            gender,
            speech_note: String::new(),
            dub_voice: "clone".to_string(),
            speaker_ids: speaker.into_iter().collect(),
            face_embedding: cl.embedding.clone(),
            voice_embedding: voice_emb,
            sample_frame: String::new(), // проставит вызывающий после сохранения кадра
            voice_sample: String::new(),
        });
    }
    (casting, clusters, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ff(t: f64, emb: Vec<f32>) -> FrameFace {
        FrameFace {
            t,
            bbox: (0.0, 0.0, 10.0, 10.0),
            score: 0.9,
            sharpness: 1.0,
            frontality: 1.0,
            embedding: l2_normalize(emb),
        }
    }

    #[test]
    fn cluster_two_distinct_faces() {
        let faces = vec![
            ff(0.0, vec![1.0, 0.0, 0.0]),
            ff(1.0, vec![0.98, 0.05, 0.0]),
            ff(2.0, vec![0.0, 1.0, 0.0]),
            ff(3.0, vec![0.02, 0.99, 0.0]),
        ];
        let cl = cluster_faces(&faces, 0.5);
        assert_eq!(cl.len(), 2, "два персонажа");
        // каждый кластер по 2 члена
        assert!(cl.iter().all(|c| c.members.len() == 2));
    }

    #[test]
    fn empty_faces_no_clusters() {
        assert!(cluster_faces(&[], 0.5).is_empty());
    }

    #[test]
    fn models_available_false_when_missing() {
        let m = FacesModels {
            scrfd: std::path::PathBuf::from("no_such_scrfd.onnx"),
            lvface: std::path::PathBuf::from("no_such_lvface.onnx"),
            lr_asd: None,
        };
        assert!(!m.available());
    }

    #[test]
    fn build_casting_links_and_gender() {
        // Один персонаж (кластер лица A) в кадре во время реплик спикера "0" (мужчина).
        let faces = vec![
            ff(0.5, vec![1.0, 0.0, 0.0]),
            ff(1.0, vec![0.99, 0.02, 0.0]),
        ];
        let turns = vec![SpeakerTurn { start: 0.0, end: 2.0, speaker: "0".into() }];
        let speakers = vec!["0".to_string()];
        let mut voice_emb = std::collections::HashMap::new();
        voice_emb.insert("0".to_string(), l2_normalize(vec![0.0, 1.0]));
        let mut genders = std::collections::HashMap::new();
        genders.insert("0".to_string(), "male".to_string());
        let (casting, clusters, dropped) =
            build_casting(&faces, &turns, &speakers, &voice_emb, &genders, 0.5, DEFAULT_FPS, 3.0);
        assert_eq!(clusters.len(), 1);
        assert_eq!(casting.characters.len(), 1);
        assert_eq!(dropped, 0, "говорящий кластер не отбрасывается");
        let c = &casting.characters[0];
        assert_eq!(c.speaker_ids, vec!["0".to_string()], "лицо связано со спикером 0");
        assert_eq!(c.gender, "male");
        assert_eq!(c.face_embedding.len(), 3);
        assert_eq!(c.voice_embedding.len(), 2, "voice_embedding перенесён от спикера");
        assert_eq!(c.dub_voice, "clone", "дефолт — клонирование");
    }

    #[test]
    fn filter_drops_silent_brief_cluster() {
        // Порог 3с (передаём параметром — без env, детерминизм в parallel-тестах); fps=1.
        // Кластер A — 4 кадра (4с на экране), НЕ говорит -> проходит по времени.
        // Кластер B — 2 кадра (2с < 3с), НЕ говорит -> отброшен. (2 члена, чтобы пережить min_cluster=2.)
        // Спикеров/реплик нет — оба кластера молчат, решает только время на экране.
        let faces = vec![
            ff(0.0, vec![1.0, 0.0, 0.0]),
            ff(1.0, vec![0.99, 0.02, 0.0]),
            ff(2.0, vec![0.98, 0.03, 0.0]),
            ff(3.0, vec![0.99, 0.01, 0.0]),
            ff(4.0, vec![0.0, 1.0, 0.0]), // кластер B
            ff(5.0, vec![0.02, 0.99, 0.0]), // кластер B
        ];
        let turns: Vec<SpeakerTurn> = vec![];
        let speakers: Vec<String> = vec![];
        let voice_emb = std::collections::HashMap::new();
        let genders = std::collections::HashMap::new();
        // fps=1 -> A=4с (>=3, kept), B=2с (<3, не говорит -> dropped).
        let (casting, clusters, dropped) =
            build_casting(&faces, &turns, &speakers, &voice_emb, &genders, 0.5, 1.0, 3.0);
        assert_eq!(dropped, 1, "молчащий кратковременный кластер отброшен");
        assert_eq!(casting.characters.len(), 1, "остался только существенный");
        assert_eq!(clusters.len(), 1, "clusters синхронны characters");
    }

    #[test]
    fn filter_keeps_speaking_even_if_brief() {
        // Кластер говорит (со-встречаемость со спикером "0") — проходит ВСЕГДА, хотя на экране <порога 10с.
        let faces = vec![ff(0.5, vec![1.0, 0.0, 0.0]), ff(1.0, vec![0.99, 0.01, 0.0])];
        let turns = vec![SpeakerTurn { start: 0.0, end: 2.0, speaker: "0".into() }];
        let speakers = vec!["0".to_string()];
        let voice_emb = std::collections::HashMap::new();
        let genders = std::collections::HashMap::new();
        let (casting, _clusters, dropped) =
            build_casting(&faces, &turns, &speakers, &voice_emb, &genders, 0.5, 1.0, 10.0);
        assert_eq!(dropped, 0, "говорящий не отброшен даже при малом времени на экране");
        assert_eq!(casting.characters.len(), 1);
    }

    #[test]
    fn filter_disabled_when_threshold_zero() {
        // Порог 0 -> фильтр по времени выключен: молчащий кластер тоже остаётся персонажем.
        let faces = vec![ff(0.0, vec![1.0, 0.0, 0.0]), ff(1.0, vec![0.99, 0.01, 0.0])];
        let turns: Vec<SpeakerTurn> = vec![];
        let speakers: Vec<String> = vec![];
        let voice_emb = std::collections::HashMap::new();
        let genders = std::collections::HashMap::new();
        let (casting, _clusters, dropped) =
            build_casting(&faces, &turns, &speakers, &voice_emb, &genders, 0.5, 1.0, 0.0);
        assert_eq!(dropped, 0);
        assert_eq!(casting.characters.len(), 1);
    }

    #[test]
    fn frontality_symmetric_high() {
        // Нос ровно между глазами -> фронтальность ~1.
        let f = Face {
            x1: 0.0, y1: 0.0, x2: 10.0, y2: 10.0, score: 0.9,
            kps: [(2.0, 3.0), (8.0, 3.0), (5.0, 5.0), (3.0, 8.0), (7.0, 8.0)],
        };
        assert!(frontality(&f) > 0.95, "фронтально");
        // Нос смещён к левому глазу -> ниже.
        let p = Face {
            x1: 0.0, y1: 0.0, x2: 10.0, y2: 10.0, score: 0.9,
            kps: [(2.0, 3.0), (8.0, 3.0), (2.5, 5.0), (3.0, 8.0), (7.0, 8.0)],
        };
        assert!(frontality(&p) < frontality(&f), "профиль ниже фронтали");
    }
}
