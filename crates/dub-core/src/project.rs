//! Project — единый редактируемый документ дубляжа. Зеркало dubengine/project.py (Pydantic).
//!
//! Ключевой инвариант оригинала: model_config = extra="allow" на КАЖДОЙ модели — неизвестные поля
//! переживают round-trip GUI нетронутыми. В Rust это воспроизводится через `#[serde(flatten)] extra`
//! (serde_json::Map) на каждой структуре. Значения по умолчанию и опциональность полей строго
//! повторяют Pydantic, чтобы JSON был совместим с backend/app.py и фронтом без правок.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

type Extra = Map<String, Value>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub video: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub width: i64,
    #[serde(default)]
    pub height: i64,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub src_codec: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voice {
    #[serde(default = "default_clone")]
    pub mode: String, // clone | autocast | auto | voice
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

fn default_clone() -> String {
    "clone".to_string()
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            mode: default_clone(),
            name: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Audio {
    #[serde(default = "default_true")]
    pub keep_music: bool,
    #[serde(default)]
    pub voice: Voice,
    #[serde(default)]
    pub rewrite: Option<String>,
    /// Стилевая инструкция перевода (#112): доп-указание переводчику (тон/регистр/лексика), напр.
    /// «formal», «gen-z slang». Пресеты живут на фронте — бэку приходит только активный текст.
    /// Пусто = без стиля. Дополняет перевод (в отличие от rewrite, который ЗАМЕНЯЕТ содержимое).
    #[serde(default)]
    pub translate_style: String,
    /// Дополнительное усиление всей дорожки на монтаже, dB (0 = без изменений). Применяется на рендере.
    #[serde(default)]
    pub gain_db: f64,
    /// Выравнивание громкости: фразы к одной громкости и программа к -14 LUFS / -1 dBTP. Выкл = микс как есть.
    #[serde(default = "default_true")]
    pub loudness_normalize: bool,
    /// Громкость ОРИГИНАЛЬНОЙ дорожки под переводом в режиме voiceover (закадровый), dB.
    /// 0 = оригинал в полную силу; отрицательное = тише перевода. Регулируется в редакторе/на старте.
    /// Дефолт -12 dB: перевод на ~8-10 LU громче фона (broadcast-практика), оригинал слышно, но приглушённо.
    #[serde(default = "default_voiceover_gain")]
    pub voiceover_gain_db: f64,
    /// Экспортировать ВТОРУЮ звуковую дорожку с оригиналом (дубляж — default 1-я, оригинал — 2-я).
    /// false = один трек как раньше. Только dub/voiceover (в nodub/transcribe оригинал уже основной).
    #[serde(default)]
    pub keep_original_track: bool,
    /// Контейнер выхода при keep_original_track: "mp4" | "mkv" (mkv надёжнее держит мультитрек-метаданные).
    #[serde(default = "default_container")]
    pub container: String,
    /// Тип контента для кастинга (#115), определённый авто-детектом Gemma при content_type="auto":
    /// "real" (реальные лица) | "anime" (рисованные). Пусто = не определяли (юзер задал явно/кастинг выкл).
    #[serde(default)]
    pub content_type: String,
    #[serde(flatten)]
    pub extra: Extra,
}

fn default_true() -> bool {
    true
}

fn default_voiceover_gain() -> f64 {
    -12.0
}

fn default_container() -> String {
    "mp4".to_string()
}

impl Default for Audio {
    fn default() -> Self {
        Audio {
            keep_music: true,
            voice: Voice::default(),
            rewrite: None,
            translate_style: String::new(),
            gain_db: 0.0,
            loudness_normalize: true,
            voiceover_gain_db: default_voiceover_gain(),
            keep_original_track: false,
            container: default_container(),
            content_type: String::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub src_text: String,
    #[serde(default)]
    pub tgt_text: String,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub dirty: bool,
    /// BLAKE3-ключ последнего успешно синтезированного TTS-фрагмента (tgt_text+speaker+voice+ref+quant).
    /// Позволяет решать ре-синтез сравнением ckpt с пересчитанным ключом ВМЕСТО/В ДОПОЛНЕНИЕ к dirty
    /// (dirty остаётся UI-сигналом «показать точку правки»). Option → отсутствует в старых project.json
    /// (serde default None) → полная обратная совместимость, инвариант extra=allow не задет.
    #[serde(default)]
    pub ckpt: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subs {
    #[serde(default = "default_none")]
    pub mode: String, // none | translate | transcribe | bilingual (перевод + оригинал второй строкой)
    /// Вжигать ли субтитры в видео. По умолчанию true (как раньше). false = дубляж/закадр БЕЗ субтитров
    /// на картинке — независимая галочка (композируемость режимов: дубляж без сабов, перевод без дубляжа).
    #[serde(default = "default_true")]
    pub burn: bool,
    /// Двуязычные субтитры (mode = bilingual): порядок строк и вид второй строки.
    #[serde(default)]
    pub bilingual: Bilingual,
    #[serde(flatten)]
    pub extra: Extra,
}

fn default_none() -> String {
    "none".to_string()
}

impl Default for Subs {
    fn default() -> Self {
        Subs {
            mode: default_none(),
            burn: true,
            bilingual: Bilingual::default(),
            extra: Extra::new(),
        }
    }
}

/// Порядок строк двуязычного субтитра: перевод сверху, оригинал под ним.
pub const ORDER_TRANSLATION_TOP: &str = "translation_top";
/// Порядок строк двуязычного субтитра: оригинал сверху, перевод под ним.
pub const ORDER_ORIGINAL_TOP: &str = "original_top";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bilingual {
    /// translation_top | original_top.
    #[serde(default = "default_order")]
    pub order: String,
    /// Вторая строка (оригинал).
    #[serde(default)]
    pub secondary: SecondaryStyle,
    #[serde(flatten)]
    pub extra: Extra,
}

fn default_order() -> String {
    ORDER_TRANSLATION_TOP.to_string()
}

impl Default for Bilingual {
    fn default() -> Self {
        Bilingual { order: default_order(), secondary: SecondaryStyle::default(), extra: Extra::new() }
    }
}

/// Вид второй строки двуязычного субтитра. Поле None — как у основной строки.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SecondaryStyle {
    /// Кегль в процентах основной строки.
    #[serde(default = "default_secondary_size")]
    pub size_pct: i64,
    /// Цвет текста #RRGGBB.
    #[serde(default)]
    pub color: Option<String>,
    /// Непрозрачность, 0..=100.
    #[serde(default)]
    pub opacity: Option<i64>,
    #[serde(flatten)]
    pub extra: Extra,
}

pub const SECONDARY_SIZE_PCT: i64 = 70;

fn default_secondary_size() -> i64 {
    SECONDARY_SIZE_PCT
}

impl Default for SecondaryStyle {
    fn default() -> Self {
        SecondaryStyle { size_pct: SECONDARY_SIZE_PCT, color: None, opacity: None, extra: Extra::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubStyle {
    #[serde(default = "white")]
    pub color: String,
    #[serde(default = "black")]
    pub outline: String,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub uppercase: bool,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub scene_color: Option<String>,
    #[serde(default)]
    pub scene_flat: bool,
    #[serde(default)]
    pub n_lines: Option<i64>,
    #[serde(default = "center")]
    pub align: String,
    #[serde(default)]
    pub size_px: Option<i64>,
    #[serde(default)]
    pub outline_w: Option<i64>,
    #[serde(default)]
    pub shadow_dir: Option<i64>, // направленная тень (градусы, screen-y-down); None = нет тени
    #[serde(flatten)]
    pub extra: Extra,
}

fn white() -> String {
    "#FFFFFF".to_string()
}
fn black() -> String {
    "#000000".to_string()
}
fn center() -> String {
    "center".to_string()
}

impl Default for SubStyle {
    fn default() -> Self {
        SubStyle {
            color: white(),
            outline: black(),
            italic: false,
            bold: false,
            uppercase: false,
            font: None,
            scene_color: None,
            scene_flat: false,
            n_lines: None,
            align: center(),
            size_px: None,
            outline_w: None,
            shadow_dir: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CaptionOverride {
    pub seg_id: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub x: Option<i64>,
    #[serde(default)]
    pub y: Option<i64>,
    #[serde(default)]
    pub w: Option<i64>,
    #[serde(default)]
    pub fs: Option<i64>,
    #[serde(default)]
    pub style: Option<SubStyle>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Title {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub tgt: String,
    #[serde(default)]
    pub bbox: Option<Vec<i64>>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub italic: bool,
    #[serde(default = "center")]
    pub align: String,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub lh: Option<i64>,
    #[serde(default)]
    pub solid: bool,
    #[serde(default = "default_true")]
    pub bold: bool,
    #[serde(default)]
    pub size_px: Option<i64>,
    #[serde(default)]
    pub outline: Option<String>,
    #[serde(default)]
    pub outline_w: Option<i64>,
    #[serde(default)]
    pub shadow_dir: Option<i64>, // направленная тень (градусы, screen-y-down); None = нет тени
    #[serde(default)]
    pub uppercase: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Title {
    fn default() -> Self {
        Title {
            text: String::new(),
            tgt: String::new(),
            bbox: None,
            color: None,
            bg: None,
            font: None,
            italic: false,
            align: center(),
            start: 0.0,
            end: 0.0,
            lh: None,
            solid: false,
            bold: true,
            size_px: None,
            outline: None,
            outline_w: None,
            shadow_dir: None,
            uppercase: false,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Brand {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub bbox: Option<Vec<i64>>,
    #[serde(default)]
    pub y_frac: Option<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlurBox {
    #[serde(default)]
    pub x: i64,
    #[serde(default)]
    pub y: i64,
    #[serde(default)]
    pub w: i64,
    #[serde(default)]
    pub h: i64,
    #[serde(default)]
    pub t0: f64,
    #[serde(default)]
    pub t1: f64,
    #[serde(default)]
    pub hidden: bool,
    /// None = gblur (текстурный фон); "#rrggbb" = сплошная заливка этим цветом (плоский фон).
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Preset {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub plate: Option<String>,
    #[serde(default)]
    pub reveal: Option<String>,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Captions {
    #[serde(default)]
    pub sub_style: Option<SubStyle>,
    #[serde(default)]
    pub sub_y: Option<i64>,
    #[serde(default)]
    pub sub_y_locked: bool,
    #[serde(default)]
    pub overrides: Vec<CaptionOverride>,
    #[serde(default)]
    pub titles: Vec<Title>,
    #[serde(default)]
    pub brands: Vec<Brand>,
    #[serde(default)]
    pub blur_boxes: Vec<BlurBox>,
    #[serde(default)]
    pub preset: Preset,
    /// Verbatim engine caption_plan.json — сохраняется для байт-идентичного re-burn при resume.
    #[serde(default)]
    pub raw_plan: Map<String, Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Render {
    #[serde(default = "cq24")]
    pub burn_cq: i64,
    #[serde(default = "sigma60")]
    pub blur_sigma: i64,
    #[serde(default = "default_true")]
    pub blur: bool,
    #[serde(default = "hevc")]
    pub codec: String,
    #[serde(flatten)]
    pub extra: Extra,
}

fn cq24() -> i64 {
    24
}
fn sigma60() -> i64 {
    60
}
fn hevc() -> String {
    "hevc".to_string()
}

impl Default for Render {
    fn default() -> Self {
        Render {
            burn_cq: cq24(),
            blur_sigma: sigma60(),
            blur: true,
            codec: hevc(),
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub meta: Meta,
    #[serde(default = "dub")]
    pub mode: String, // dub | nodub | transcribe
    #[serde(default = "en")]
    pub tgt_lang: String,
    #[serde(default)]
    pub audio: Audio,
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub subs: Subs,
    #[serde(default)]
    pub captions: Captions,
    #[serde(default)]
    pub render: Render,
    #[serde(default)]
    pub work_dir: Option<String>,
    /// Verbatim ctx_extra.json passthrough (audio_context/scene_context/…).
    #[serde(default)]
    pub raw_ctx: Map<String, Value>,
    /// Зеркало param-ключей крупных стадий analyze (extract/diarize/asr/translate/ocr) прямо в project.json,
    /// чтобы resume работал даже если каталог work_dir/cache.json потеряли, но project.json сохранён (он
    /// автосейвится каждый PATCH). Аддитивно, дефолтно, не нарушает инвариант «extra=allow». Формат:
    /// {"extract":"<blake3-hex>", "diarize":"…", …}.
    #[serde(default)]
    pub stage_ckpts: Map<String, Value>,
    /// Кастинг персонажей (#115): при true analyze сканирует кадры (SCRFD+LVFace), кластеризует лица в
    /// персонажей и связывает их с голосами-спикерами (casting.json). ВЫКЛ по умолчанию — при false НИ
    /// ОДНОГО кадра не сканируется, ноль оверхеда (как detect_text/OCR). Аддитивно, не нарушает extra=allow.
    #[serde(default)]
    pub casting_enabled: bool,
    /// Глоссарий проекта: термины для перевода, распознавания и озвучки (см. glossary.rs).
    #[serde(default)]
    pub glossary: Vec<crate::GlossaryEntry>,
    /// Отпечаток глоссария, с которым сделан текущий перевод: не совпал с нынешним — перевод устарел.
    #[serde(default)]
    pub glossary_fp: String,
    /// Профиль сериала из библиотеки кастингов, применённый при анализе (casting_ref); пусто — нет.
    #[serde(default)]
    pub casting_ref: String,
    #[serde(flatten)]
    pub extra: Extra,
}

fn dub() -> String {
    "dub".to_string()
}
fn en() -> String {
    "en".to_string()
}

impl Default for Project {
    fn default() -> Self {
        Project {
            meta: Meta::default(),
            mode: dub(),
            tgt_lang: en(),
            audio: Audio::default(),
            segments: Vec::new(),
            subs: Subs::default(),
            captions: Captions::default(),
            render: Render::default(),
            work_dir: None,
            raw_ctx: Map::new(),
            stage_ckpts: Map::new(),
            casting_enabled: false,
            glossary: Vec::new(),
            glossary_fp: String::new(),
            casting_ref: String::new(),
            extra: Extra::new(),
        }
    }
}

impl Project {
    /// Разобрать Project из JSON-строки (зеркало Project.load / model_validate_json).
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }

    /// Сериализовать в JSON (indent=1, как model_dump_json(indent=1) в оригинале).
    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        // Pydantic использует отступ в 1 пробел; serde_json умеет только фиксированный форматтер,
        // потому берём стандартный pretty (2 пробела) — семантически идентично, фронт парсит одинаково.
        serde_json::to_string_pretty(self)
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}
