//! Ударения, «ё» и омографы русского текста — RUAccent (`ruaccent/accentuator`, MIT): словарь 3,2 млн словоформ,
//! символьная сеть для слов вне словаря (int8) и сеть омографов tiny2 (int8). Шаги и их порядок — RUAccent
//! `process_all` без морфологического теггера (так же делает stressonnx): «ё» по словарю, омографы по контексту
//! предложения, ударения словом из словаря или сетью. Сеть «ё»-омографов в RUAccent есть только в fp32, поэтому
//! такие слова («все/всё») не меняются, а помечаются для ручного выбора.
//!
//! Ручные `+` и `ё` в тексте не перезаписываются; омограф определяется по тексту без ручных меток соседей
//! (урок issue #6 Silero Stress). Разметка синтеза (`<|…|>`, `[pause …]`, `[[…]]`, `<[…]>`) не трогается.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use ort::session::Session;
use ort::value::TensorRef;
use regex::Regex;
use serde::Serialize;

pub const DIR: &str = "ruaccent";
pub const OMOGRAPH_BIG: &str = "nn/nn_omograph/big_poetry";
const VOWELS: &str = "аеёиоуыэюя";
/// Сеть ударений обучена на словах до 40 знаков с учётом служебных.
const ACCENT_MAX_CHARS: usize = 38;
const ACCENT_MIN_SCORE: f32 = 0.55;
/// Омограф спорный, когда сеть отдала лучшему варианту меньше этого отрыва от второго.
const DOUBT_MARGIN: f32 = 0.2;

pub(crate) static MARKUP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\|[^|\n]*\|>|\[\[[^\[\]]*\]\]|\[\s*pause[^\]]*\]|\[\s*rate\s*:[^\]]*\]|sil\s*<\[[^\]]*\]>|<\[[^\]]*\]>").expect("markup")
});
static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\w]+|[^\w\s]+").expect("token"));

fn is_cyr(c: char) -> bool {
    matches!(c, 'а'..='я' | 'А'..='Я' | 'ё' | 'Ё')
}

fn is_vowel(c: char) -> bool {
    VOWELS.contains(c.to_lowercase().next().unwrap_or(c))
}

fn lower(s: &str) -> String {
    s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect()
}

/// Как пословно решён текст: для подсветки в окне и трассировки подготовки.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct WordNote {
    /// Границы слова в результате, в символах.
    pub start: usize,
    pub end: usize,
    pub word: String,
    /// `manual`, `dictionary`, `model`, `omograph`, `yo`.
    pub source: &'static str,
    /// Варианты омографа с уверенностью сети (у `omograph`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<(String, f32)>,
    /// Спорный омограф: стоит проверить на слух и выбрать вручную.
    pub doubt: bool,
    /// «ё»-омограф, который не решается автоматически: вариант с «ё» для ручного выбора.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yo_choice: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Stressed {
    pub text: String,
    pub words: Vec<WordNote>,
}

pub struct Stresser {
    accents: fst::Map<Vec<u8>>,
    omographs: HashMap<String, Vec<String>>,
    yo_words: HashMap<String, String>,
    yo_homographs: HashMap<String, String>,
    chars: HashMap<char, i64>,
    accent: Mutex<Session>,
    omograph: Mutex<Session>,
    omo_tok: tokenizers::Tokenizer,
    /// The homograph model's folder, and whether the model takes `token_type_ids`.
    omo_dir: PathBuf,
    omo_types: bool,
}

pub fn dir(models_root: &Path) -> PathBuf {
    models_root.join(DIR)
}

/// The homograph model to use: big_poetry (weight-only int8, nerualdreming/ruaccent-omograph-int8) once it is
/// downloaded, else the tiny2 of the base component.
fn omograph_dir(d: &Path) -> PathBuf {
    let big = d.join(OMOGRAPH_BIG);
    if big.join("model.onnx").is_file() && big.join("tokenizer.json").is_file() { big } else { d.join("nn/nn_omograph/tiny2") }
}

/// Словари и сети базового компонента на месте.
pub fn installed(models_root: &Path) -> bool {
    let d = dir(models_root);
    [
        "dictionary/accents.json.gz",
        "dictionary/omographs.json.gz",
        "dictionary/yo_words.json.gz",
        "dictionary/yo_homographs.json.gz",
        "nn/nn_accent/model.onnx",
        "nn/nn_accent/vocab.txt",
        "nn/nn_omograph/tiny2/model.onnx",
        "nn/nn_omograph/tiny2/tokenizer.json",
    ]
    .iter()
    .all(|f| d.join(f).is_file())
}

static LOADED: LazyLock<Mutex<Option<Arc<Stresser>>>> = LazyLock::new(|| Mutex::new(None));

/// Общий на процесс разметчик: грузится при первом обращении (словарь собирается в память один раз).
pub fn shared(models_root: &Path) -> Result<Arc<Stresser>, String> {
    // паника загрузки (чужая onnxruntime.dll) не должна валить на замке каждый следующий блок
    let mut slot = LOADED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(s) = slot.as_ref().filter(|s| s.omo_dir == omograph_dir(&dir(models_root))) {
        return Ok(s.clone());
    }
    let root = dir(models_root);
    let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Stresser::load(&root))).map_err(|panic| {
        let text = panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_default();
        format!("RUAccent не загрузился: {text}")
    })?;
    let s = Arc::new(loaded?);
    *slot = Some(s.clone());
    Ok(s)
}

fn read_gz(path: &Path) -> Result<Vec<u8>, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(f).read_to_end(&mut out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(out)
}

fn json_gz<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&read_gz(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// Позиция ударной гласной (в символах слова без `+`) из записи вида `молок+о`.
fn plus_pos(stressed: &str) -> Option<usize> {
    stressed.chars().position(|c| c == '+')
}

/// Словарь ударений как FST: слово → номер ударного символа. 3,2 млн пар в HashMap заняли бы сотни мегабайт.
fn build_accents(path: &Path) -> Result<fst::Map<Vec<u8>>, String> {
    let raw: HashMap<String, String> = json_gz(path)?;
    let mut pairs: Vec<(String, u64)> = raw.into_iter().filter_map(|(k, v)| plus_pos(&v).map(|p| (k, p as u64))).collect();
    pairs.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    pairs.dedup_by(|a, b| a.0 == b.0);
    let mut b = fst::MapBuilder::memory();
    for (k, v) in pairs {
        b.insert(k.as_bytes(), v).map_err(|e| format!("accents fst: {e}"))?;
    }
    let bytes = b.into_inner().map_err(|e| format!("accents fst: {e}"))?;
    fst::Map::new(bytes).map_err(|e| format!("accents fst: {e}"))
}

fn session(path: &Path, threads: usize) -> Result<Session, String> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    Session::builder()
        .map_err(|e| err(&e))?
        .with_intra_threads(threads)
        .map_err(|e| err(&e))?
        .commit_from_file(path)
        .map_err(|e| err(&e))
}

impl Stresser {
    pub fn load(d: &Path) -> Result<Stresser, String> {
        let dict = d.join("dictionary");
        let mut omographs: HashMap<String, Vec<String>> = json_gz(&dict.join("omographs.json.gz"))?;
        omographs.insert("коса".into(), vec!["к+оса".into(), "кос+а".into()]);
        let vocab = std::fs::read_to_string(d.join("nn/nn_accent/vocab.txt")).map_err(|e| e.to_string())?;
        let chars = vocab
            .lines()
            .enumerate()
            .filter_map(|(i, l)| {
                let mut it = l.chars();
                match (it.next(), it.next()) {
                    (Some(c), None) => Some((c, i as i64)),
                    _ => None,
                }
            })
            .collect();
        let omo_dir = omograph_dir(d);
        let omograph = session(&omo_dir.join("model.onnx"), if omo_dir.ends_with("tiny2") { 2 } else { 4 })?;
        let mut omo_tok = tokenizers::Tokenizer::from_file(omo_dir.join("tokenizer.json")).map_err(|e| e.to_string())?;
        omo_tok
            .with_truncation(Some(tokenizers::TruncationParams { max_length: 512, ..Default::default() }))
            .map_err(|e| e.to_string())?;
        omo_tok.with_padding(None);
        Ok(Stresser {
            accents: build_accents(&dict.join("accents.json.gz"))?,
            omographs,
            yo_words: json_gz(&dict.join("yo_words.json.gz"))?,
            yo_homographs: json_gz(&dict.join("yo_homographs.json.gz"))?,
            chars,
            accent: Mutex::new(session(&d.join("nn/nn_accent/model.onnx"), 2)?),
            omo_types: omograph.inputs().iter().any(|i| i.name() == "token_type_ids"),
            omograph: Mutex::new(omograph),
            omo_tok,
            omo_dir,
        })
    }

    fn dict_pos(&self, word: &str) -> Option<usize> {
        self.accents.get(word.as_bytes()).map(|v| v as usize)
    }

    /// Ударная гласная слова символьной сетью; `None` — сеть не уверена.
    fn model_pos(&self, word: &str) -> Result<Option<usize>, String> {
        let chars: Vec<char> = word.chars().collect();
        if chars.len() > ACCENT_MAX_CHARS {
            return Ok(None);
        }
        let mut ids: Vec<i64> = vec![2];
        ids.extend(chars.iter().map(|c| *self.chars.get(c).unwrap_or(&1)));
        ids.push(3);
        let n = ids.len();
        let mask = vec![1i64; n];
        let types = vec![0i64; n];
        let shape = vec![1i64, n as i64];
        let mut s = self.accent.lock().map_err(|_| "accent lock".to_string())?;
        let out = s
            .run(ort::inputs![
                "input_ids" => TensorRef::from_array_view((shape.clone(), ids.as_slice())).map_err(|e| e.to_string())?,
                "attention_mask" => TensorRef::from_array_view((shape.clone(), mask.as_slice())).map_err(|e| e.to_string())?,
                "token_type_ids" => TensorRef::from_array_view((shape, types.as_slice())).map_err(|e| e.to_string())?,
            ])
            .map_err(|e| format!("accent: {e}"))?;
        let (_, logits) = out["logits"].try_extract_tensor::<f32>().map_err(|e| format!("accent: {e}"))?;
        let labels = logits.len() / n;
        let mut best: Option<(usize, f32)> = None;
        for (i, c) in chars.iter().enumerate() {
            let row = &logits[(i + 1) * labels..(i + 2) * labels];
            let p = softmax(row);
            let (arg, score) = p.iter().enumerate().fold((0, f32::MIN), |m, (k, v)| if *v > m.1 { (k, *v) } else { m });
            if arg == 1 && score >= ACCENT_MIN_SCORE && is_vowel(*c) && best.is_none_or(|b| score > b.1) {
                best = Some((i, score));
            }
        }
        Ok(best.map(|b| b.0))
    }

    /// Вероятность того, что `hypothesis` — верное прочтение помеченного `<w>…</w>` слова в `context`.
    fn omograph_prob(&self, context: &str, hypothesis: &str) -> Result<f32, String> {
        let enc = self.omo_tok.encode((context, hypothesis), true).map_err(|e| e.to_string())?;
        let ids: Vec<i64> = enc.get_ids().iter().map(|v| *v as i64).collect();
        let mask: Vec<i64> = enc.get_attention_mask().iter().map(|v| *v as i64).collect();
        let types: Vec<i64> = enc.get_type_ids().iter().map(|v| *v as i64).collect();
        let shape = vec![1i64, ids.len() as i64];
        let mut s = self.omograph.lock().map_err(|_| "omograph lock".to_string())?;
        let mut inputs = ort::inputs![
            "input_ids" => TensorRef::from_array_view((shape.clone(), ids.as_slice())).map_err(|e| e.to_string())?,
            "attention_mask" => TensorRef::from_array_view((shape.clone(), mask.as_slice())).map_err(|e| e.to_string())?,
        ];
        if self.omo_types {
            inputs.push(("token_type_ids".into(), TensorRef::from_array_view((shape, types.as_slice())).map_err(|e| e.to_string())?.into()));
        }
        let out = s.run(inputs).map_err(|e| format!("omograph: {e}"))?;
        let (_, logits) = out["logits"].try_extract_tensor::<f32>().map_err(|e| format!("omograph: {e}"))?;
        Ok(softmax(&logits[..2])[1])
    }

    /// Расставить ударения и «ё» в тексте блока.
    pub fn annotate(&self, text: &str) -> Result<Stressed, String> {
        let plan = Plan::new(text);
        let mut words: Vec<Decision> = Vec::new();
        for sentence in plan.sentences() {
            self.decide_sentence(&plan, sentence, &mut words)?;
        }
        Ok(plan.render(&words))
    }

    fn decide_sentence(&self, plan: &Plan, sentence: (usize, usize), out: &mut Vec<Decision>) -> Result<(), String> {
        let tokens = plan.tokens(sentence);
        let mut forms: Vec<String> = tokens.iter().map(|t| plan.clean_str(t.0, t.1)).collect();
        let mut decisions: Vec<Option<Decision>> = vec![None; tokens.len()];
        for (i, &(s, e)) in tokens.iter().enumerate() {
            if !plan.processable(s, e) {
                continue;
            }
            let lw = lower(&forms[i]);
            if plan.manual(s, e) {
                decisions[i] = Some(Decision::new(s, e, "manual"));
                continue;
            }
            if lw.contains('ё') {
                continue;
            }
            if let Some(yo) = self.yo_words.get(&lw) {
                forms[i] = fix_capital(&forms[i], yo);
                let mut d = Decision::new(s, e, "yo");
                d.yo = yo_positions(&lw, yo);
                decisions[i] = Some(d);
            } else if let Some(yo) = self.yo_homographs.get(&lw) {
                let mut d = Decision::new(s, e, "dictionary");
                d.yo_choice = Some(fix_capital(&forms[i], yo));
                decisions[i] = Some(d);
            }
        }
        for (i, &(s, e)) in tokens.iter().enumerate() {
            if !plan.processable(s, e) || plan.manual(s, e) {
                continue;
            }
            let lw = lower(&forms[i]);
            let Some(variants) = self.omographs.get(&lw) else { continue };
            let mut scored: Vec<(String, f32)> = Vec::new();
            if variants.len() == 1 {
                scored.push((variants[0].clone(), 1.0));
            } else {
                let context = omograph_context(&forms, i);
                for v in variants {
                    scored.push((v.clone(), self.omograph_prob(&context, v)?));
                }
            }
            let best = scored.iter().cloned().fold(None::<(String, f32)>, |m, v| match m {
                Some(b) if b.1 >= v.1 => Some(b),
                _ => Some(v),
            });
            let Some((chosen, p)) = best else { continue };
            let second = scored.iter().filter(|v| v.0 != chosen).map(|v| v.1).fold(0.0f32, f32::max);
            let bare: String = chosen.chars().filter(|c| *c != '+').collect();
            let mut d = decisions[i].take().unwrap_or_else(|| Decision::new(s, e, "omograph"));
            d.source = "omograph";
            d.stress = plus_pos(&chosen);
            d.yo.extend(yo_positions(&lw, &bare));
            d.doubt = scored.len() > 1 && p - second < DOUBT_MARGIN;
            d.variants = scored;
            decisions[i] = Some(d);
        }
        for (i, &(s, e)) in tokens.iter().enumerate() {
            if !plan.processable(s, e) || plan.manual(s, e) {
                continue;
            }
            if decisions[i].as_ref().is_some_and(|d| d.source == "omograph") {
                continue;
            }
            let lw = lower(&forms[i]);
            if lw.contains('ё') || lw.chars().filter(|c| is_vowel(*c)).count() < 2 {
                if let Some(d) = decisions[i].take() {
                    out.push(d);
                }
                continue;
            }
            let (pos, source) = match self.dict_pos(&lw) {
                Some(p) => (Some(p), "dictionary"),
                None => (self.model_pos(&lw)?, "model"),
            };
            let mut d = decisions[i].take().unwrap_or_else(|| Decision::new(s, e, source));
            if d.source != "yo" {
                d.source = source;
            }
            d.stress = pos.filter(|p| lw.chars().nth(*p).is_some_and(is_vowel));
            decisions[i] = Some(d);
        }
        out.extend(decisions.into_iter().flatten());
        Ok(())
    }
}

fn softmax(x: &[f32]) -> Vec<f32> {
    let m = x.iter().cloned().fold(f32::MIN, f32::max);
    let e: Vec<f32> = x.iter().map(|v| (v - m).exp()).collect();
    let s: f32 = e.iter().sum();
    e.into_iter().map(|v| v / s).collect()
}

/// Регистр исходного слова на форму из словаря той же длины.
fn fix_capital(source: &str, target: &str) -> String {
    let s: Vec<char> = source.chars().collect();
    let t: Vec<char> = target.chars().collect();
    if s.len() != t.len() {
        return target.to_string();
    }
    s.iter()
        .zip(t)
        .map(|(a, b)| if a.is_uppercase() { b.to_uppercase().next().unwrap_or(b) } else { b.to_lowercase().next().unwrap_or(b) })
        .collect()
}

/// Где в `with_yo` стоит «ё» на месте «е» исходного слова.
fn yo_positions(word: &str, with_yo: &str) -> Vec<usize> {
    word.chars()
        .zip(with_yo.chars())
        .enumerate()
        .filter(|(_, (a, b))| matches!(a, 'е' | 'Е') && matches!(b, 'ё' | 'Ё'))
        .map(|(i, _)| i)
        .collect()
}

/// RUAccent `delete_spaces_before_punc`.
fn delete_spaces_before_punc(text: &str) -> String {
    let mut t = text.to_string();
    for c in "!\"#$%&'()*,./:;<=>?@[\\]^_`{|}-".chars() {
        if c == '-' {
            t = t.replace(" -", "-").replace("- ", "-");
        }
        t = t.replace(&format!(" {c}"), &c.to_string());
    }
    t.replace('~', "-")
}

static SPACE_BEFORE_PUNCT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+([,.?!:;…])").expect("punct"));

/// Предложение с помеченным словом, как его готовит RUAccent для сети омографов.
fn omograph_context(forms: &[String], at: usize) -> String {
    let joined = forms
        .iter()
        .enumerate()
        .map(|(i, w)| if i == at { format!(" <w>{w}</w> ") } else { w.clone() })
        .collect::<Vec<_>>()
        .join(" ");
    SPACE_BEFORE_PUNCT.replace_all(&delete_spaces_before_punc(&joined), "$1").into_owned()
}

#[derive(Clone, Debug)]
struct Decision {
    /// Границы слова в очищенном тексте.
    start: usize,
    end: usize,
    source: &'static str,
    stress: Option<usize>,
    yo: Vec<usize>,
    doubt: bool,
    variants: Vec<(String, f32)>,
    yo_choice: Option<String>,
}

impl Decision {
    fn new(start: usize, end: usize, source: &'static str) -> Self {
        Decision { start, end, source, stress: None, yo: Vec::new(), doubt: false, variants: Vec::new(), yo_choice: None }
    }
}

/// Текст без ручных `+`: где они стояли, что из текста — разметка синтеза.
struct Plan {
    clean: Vec<char>,
    manual_stress: Vec<bool>,
    markup: Vec<bool>,
}

impl Plan {
    fn new(text: &str) -> Plan {
        let mut markup_bytes = vec![false; text.len()];
        for m in MARKUP.find_iter(text) {
            markup_bytes[m.start()..m.end()].iter_mut().for_each(|b| *b = true);
        }
        let src: Vec<(usize, char)> = text.char_indices().collect();
        let (mut clean, mut manual_stress, mut markup) = (Vec::new(), Vec::new(), Vec::new());
        let mut pending = false;
        for (k, &(b, c)) in src.iter().enumerate() {
            let in_markup = markup_bytes[b];
            let next_is_vowel = src.get(k + 1).is_some_and(|(_, n)| is_cyr(*n) && is_vowel(*n));
            if c == '+' && !in_markup && next_is_vowel {
                pending = true;
                continue;
            }
            clean.push(c);
            manual_stress.push(pending);
            markup.push(in_markup);
            pending = false;
        }
        Plan { clean, manual_stress, markup }
    }

    fn clean_str(&self, s: usize, e: usize) -> String {
        self.clean[s..e].iter().collect()
    }

    /// Предложения очищенного текста (границы в символах), разметка — пробел.
    fn sentences(&self) -> Vec<(usize, usize)> {
        let n = self.clean.len();
        let mut out = Vec::new();
        let mut start = 0;
        let mut i = 0;
        while i < n {
            let c = self.clean[i];
            if !self.markup[i] && matches!(c, '.' | '!' | '?' | '…' | '\n') {
                let mut j = i + 1;
                while j < n && !self.markup[j] && matches!(self.clean[j], '.' | '!' | '?' | '…' | '"' | '»') {
                    j += 1;
                }
                if j >= n || self.clean[j].is_whitespace() {
                    out.push((start, j));
                    start = j;
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
        if start < n {
            out.push((start, n));
        }
        out
    }

    /// Слова и знаки предложения (как в RUAccent), без разметки синтеза.
    fn tokens(&self, (s, e): (usize, usize)) -> Vec<(usize, usize)> {
        let text: String = self.clean[s..e].iter().enumerate().map(|(k, c)| if self.markup[s + k] { ' ' } else { *c }).collect();
        let offsets: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        let to_char = |b: usize| offsets.partition_point(|x| *x < b);
        TOKEN.find_iter(&text).map(|m| (s + to_char(m.start()), s + to_char(m.end()))).collect()
    }

    fn processable(&self, s: usize, e: usize) -> bool {
        self.clean[s..e].iter().all(|c| is_cyr(*c))
    }

    fn manual(&self, s: usize, e: usize) -> bool {
        self.manual_stress[s..e].iter().any(|m| *m)
    }

    fn render(&self, decisions: &[Decision]) -> Stressed {
        let mut stress_at = vec![false; self.clean.len()];
        let mut yo_at = vec![false; self.clean.len()];
        for d in decisions {
            if let Some(p) = d.stress {
                if d.start + p < d.end {
                    stress_at[d.start + p] = true;
                }
            }
            for p in &d.yo {
                yo_at[d.start + p] = true;
            }
        }
        let mut text = String::new();
        let mut out_pos = vec![0usize; self.clean.len() + 1];
        let mut n = 0;
        for (i, c) in self.clean.iter().enumerate() {
            out_pos[i] = n;
            let stressed = self.manual_stress[i] || stress_at[i];
            let c = if yo_at[i] { if c.is_uppercase() { 'Ё' } else { 'ё' } } else { *c };
            if stressed && !(matches!(c, 'ё' | 'Ё') && !self.manual_stress[i]) {
                text.push('+');
                n += 1;
            }
            text.push(c);
            n += 1;
        }
        out_pos[self.clean.len()] = n;
        let chars: Vec<char> = text.chars().collect();
        let mut ordered: Vec<&Decision> = decisions.iter().collect();
        ordered.sort_by_key(|d| d.start);
        let words = ordered
            .into_iter()
            .map(|d| {
                let (start, end) = (out_pos[d.start], out_pos[d.end]);
                WordNote {
                    start,
                    end,
                    word: chars[start..end].iter().collect(),
                    source: d.source,
                    variants: d.variants.clone(),
                    doubt: d.doubt,
                    yo_choice: d.yo_choice.clone(),
                }
            })
            .collect();
        Stressed { text, words }
    }
}

/// Русская ли озвучка: язык перевода проекта.
pub fn is_russian(lang: &str) -> bool {
    lang.to_ascii_lowercase().starts_with("ru")
}

/// Какая сеть омографов сейчас стоит: в ключ кэша озвучки, чтобы большая сеть переозвучила фразы.
pub fn omograph_tag(models_root: &Path) -> &'static str {
    if omograph_dir(&dir(models_root)).ends_with("tiny2") { "tiny2" } else { "big" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_under_the_lock_does_not_fail_every_later_line() {
        let _ = std::thread::spawn(|| {
            let _held = LOADED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            panic!("a load that panicked");
        })
        .join();
        assert!(LOADED.is_poisoned());
        let missing = std::env::temp_dir().join(format!("no-ruaccent-{}", std::process::id()));
        let error = shared(&missing).err().unwrap_or_default();
        assert_ne!(error, "stress lock", "the next line names its own cause");
    }

    #[test]
    fn manual_pluses_leave_the_clean_text_and_come_back_in_place() {
        let plan = Plan::new("Это з+амок, а не <|emotion:awe|> зам+ок.");
        let clean: String = plan.clean.iter().collect();
        assert_eq!(clean, "Это замок, а не <|emotion:awe|> замок.");
        assert_eq!(plan.render(&[]).text, "Это з+амок, а не <|emotion:awe|> зам+ок.");
    }

    #[test]
    fn markup_is_neither_a_word_nor_a_sentence_end() {
        let plan = Plan::new("Раз. [pause 1.5s] Два <|emotion:anger|> три! [[каша]] [rate:0,8]");
        let words: Vec<String> = plan
            .sentences()
            .into_iter()
            .flat_map(|s| plan.tokens(s))
            .map(|(s, e)| plan.clean_str(s, e))
            .collect();
        assert_eq!(words, vec!["Раз", ".", "Два", "три", "!"]);
    }

    #[test]
    fn decisions_put_plus_before_the_vowel_and_yo_keeps_its_case() {
        let plan = Plan::new("Елка и молоко");
        let mut a = Decision::new(0, 4, "yo");
        a.yo = vec![0];
        let mut b = Decision::new(7, 13, "dictionary");
        b.stress = Some(5);
        let s = plan.render(&[a, b]);
        assert_eq!(s.text, "Ёлка и молок+о");
        assert_eq!((s.words[1].start, s.words[1].end, s.words[1].word.as_str()), (7, 14, "молок+о"));
    }

    #[test]
    fn the_omograph_context_is_the_ruaccent_one() {
        let forms: Vec<String> = ["Старинный", "замок", "стоял", "на", "холме", "."].iter().map(|s| s.to_string()).collect();
        assert_eq!(omograph_context(&forms, 1), "Старинный <w>замок</w>  стоял на холме.");
    }

    #[test]
    fn russian_is_the_target_language() {
        assert!(is_russian("ru") && is_russian("RU") && !is_russian("en"));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Прогон на настоящих моделях: `STRESS_DIR` — папка RUAccent, `ORT_DYLIB_PATH` — onnxruntime 1.28.
    #[test]
    #[ignore]
    fn real_models_read_context_and_dictionary() {
        let dir = std::env::var("STRESS_DIR").expect("STRESS_DIR");
        let t0 = std::time::Instant::now();
        let s = Stresser::load(Path::new(&dir)).expect("load");
        eprintln!("load {:.2}s", t0.elapsed().as_secs_f64());
        for text in [
            "Старинный замок стоял на холме.",
            "Закрой дверь на замок, пожалуйста.",
            "Из этой муки пекут хлеб. Адские муки грешников.",
            "Елка стояла в углу, все дети пели.",
            "Он пришел домой и включил телевизор.",
            "Это з+амок, а не <|emotion:awe|> замок. [pause 300ms] Белки бегали по деревьям.",
            "Кто-то сказал: «Вышлите документы в среду».",
        ] {
            let t = std::time::Instant::now();
            let r = s.annotate(text).expect("annotate");
            eprintln!("{:.0}ms | {}", t.elapsed().as_secs_f64() * 1000.0, r.text);
            for w in r.words.iter().filter(|w| w.doubt || w.yo_choice.is_some() || !w.variants.is_empty()) {
                eprintln!("    {} {} {:?} doubt={} yo={:?}", w.word, w.source, w.variants, w.doubt, w.yo_choice);
            }
        }
    }
}
