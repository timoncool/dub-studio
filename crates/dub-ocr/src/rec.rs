//! Распознавание текста (PP-OCR CRNN + CTC). Дефолты config.yaml: rec_img_shape=[3,48,320].
//! Словарь символов берётся из метаданных ONNX (ключ "character") — как RapidOCR v3
//! (get_character_list). CTC-декод: argmax по кадрам, схлопывание повторов, пропуск blank(0).
//!
//! Словарь ONNX (ключ "character") УЖЕ содержит полный набор символов PaddleOCR (для cyrillic-модели:
//! space ПЕРВЫМ токеном, пустой токен ПОСЛЕДНИМ); его длина + 1(blank) == размер выхода softmax (C=165).
//! Поэтому НЕ добавляем/не переписываем токены — только префиксуем blank в позицию 0. Любая правка
//! (доп. пробел в конце / замена пустого токена) сдвигает индексы и портит CTC-декод — это и был
//! корневой шум («КОРОЧЕ» -> «kрhеh»). Fallback на .dict.txt даёт то же: blank + строки файла как есть.

use crate::OcrError;
use crate::ort_engine::OnnxModel;
use image::RgbImage;
use ndarray::Array4;

/// Высота входа rec (стандарт PP-OCR).
const REC_H: usize = 48;

/// Словарь распознавания: blank в позиции 0, далее символы из метаданных ONNX (или .dict.txt).
#[derive(Clone)]
pub struct RecDict {
    chars: Vec<String>, // chars[0] = blank
}

impl RecDict {
    /// Прочитать словарь из метаданных сессии (ключ "character") — источник истины RapidOCR v3.
    pub fn from_session_meta(model: &OnnxModel) -> Result<Self, OcrError> {
        let raw = model
            .metadata_character()?
            .ok_or(OcrError::NoDictionary)?;
        Ok(Self::from_dict_str(&raw))
    }

    /// Прочитать словарь из текста метаданных (по одному токену на строку). Токены берутся КАК ЕСТЬ,
    /// blank префиксуется в позицию 0. Хвостовой пустой токен НЕ отбрасывается — он значим (класс C-1).
    pub fn from_dict_str(raw: &str) -> Self {
        let mut chars: Vec<String> = vec![String::new()]; // blank index 0
        chars.extend(raw.split('\n').map(str::to_string));
        Self { chars }
    }

    /// Fallback: загрузить словарь из .dict.txt (по строке на символ). blank префиксуется. Убираем лишь
    /// \r; порядок = порядок классов модели с индекса 1. Предпочтителен путь from_session_meta.
    pub fn load(path: &std::path::Path) -> Result<Self, OcrError> {
        let txt = std::fs::read_to_string(path).map_err(|e| OcrError::Io(format!("dict {}: {e}", path.display())))?;
        // нормализуем перевод строк, не трогая содержимое токенов (первая строка часто пробел).
        let normalized = txt.replace("\r\n", "\n").replace('\r', "");
        // отбрасываем единственный хвостовой перевод строки от финального \n (но не значимый пустой
        // токен внутри метаданных — тут файл, где хвостовой \n — просто конец файла).
        let normalized = normalized.strip_suffix('\n').unwrap_or(&normalized);
        Ok(Self::from_dict_str(normalized))
    }

    /// class index -> символ. 0=blank(""), далее токены словаря.
    fn ch(&self, idx: usize) -> &str {
        self.chars.get(idx).map_or("", |s| s.as_str())
    }
}

/// Ресайз кропа к высоте REC_H (ширина пропорц., мин.16), нормализация [-1,1]. Возвращает (rw, канал-планы
/// [3][REC_H*rw]-flat в порядке CHW). Билинейный ресемпл.
fn preprocess(img: &RgbImage) -> Option<(usize, Vec<f32>)> {
    let (cw, ch) = (img.width() as usize, img.height() as usize);
    if cw == 0 || ch == 0 {
        return None;
    }
    let ratio = cw as f32 / ch as f32;
    let rw = ((REC_H as f32 * ratio).ceil() as usize).max(16);
    let mut plane = vec![0.0f32; 3 * REC_H * rw];
    let sx = cw as f32 / rw as f32;
    let sy = ch as f32 / REC_H as f32;
    for oy in 0..REC_H {
        let fy = ((oy as f32 + 0.5) * sy - 0.5).clamp(0.0, ch as f32 - 1.0);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(ch - 1);
        let wy = fy - y0 as f32;
        for ox in 0..rw {
            let fx = ((ox as f32 + 0.5) * sx - 0.5).clamp(0.0, cw as f32 - 1.0);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(cw - 1);
            let wx = fx - x0 as f32;
            for c in 0..3 {
                let s = |xx: usize, yy: usize| img.get_pixel(xx as u32, yy as u32)[c] as f32;
                let top = s(x0, y0) * (1.0 - wx) + s(x1, y0) * wx;
                let bot = s(x0, y1) * (1.0 - wx) + s(x1, y1) * wx;
                let v = (top * (1.0 - wy) + bot * wy) / 255.0;
                plane[c * REC_H * rw + oy * rw + ox] = (v - 0.5) / 0.5;
            }
        }
    }
    Some((rw, plane))
}

/// CTC-greedy декод одного выхода [T,C] (blank=0, пропуск повторов). Возвращает (текст, ср.скор).
fn ctc_decode(flat: &[f32], t: usize, c: usize, dict: &RecDict) -> (String, f32) {
    let mut text = String::new();
    let mut scores: Vec<f32> = Vec::new();
    let mut prev_idx = usize::MAX;
    for ti in 0..t {
        let base = ti * c;
        let mut best = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for ci in 0..c {
            let v = flat[base + ci];
            if v > best_v {
                best_v = v;
                best = ci;
            }
        }
        if best != 0 && best != prev_idx {
            text.push_str(dict.ch(best));
            scores.push(best_v);
        }
        prev_idx = best;
    }
    let score = if scores.is_empty() { 0.0 } else { scores.iter().sum::<f32>() / scores.len() as f32 };
    (text, score)
}

/// Распознать текст в одном кропе (PP-OCR rec + CTC greedy). Билинейный ресайз к высоте 48, [-1,1].
#[allow(dead_code)] // сохранено для single-crop fallback
pub fn recognize(model: &mut OnnxModel, dict: &RecDict, img: &RgbImage) -> Result<(String, f32), OcrError> {
    let Some((rw, plane)) = preprocess(img) else { return Ok((String::new(), 0.0)) };
    // plane уже уложен в C-order (c, oy, ox) — строим тензор напрямую, без поэлементного копирования.
    let input = Array4::from_shape_vec((1, 3, REC_H, rw), plane)
        .map_err(|e| OcrError::Internal(format!("rec shape: {e}")))?;
    let (shape, flat) = model.run(input)?;
    if shape.len() != 3 {
        return Ok((String::new(), 0.0));
    }
    Ok(ctc_decode(&flat, shape[1], shape[2], dict))
}

/// Распознать ПАЧКУ кропов за один инференс: [N,3,48,Wmax] (правый zero-pad до Wmax; лишние столбцы
/// декодятся как blank). Один вызов ORT вместо N — экономит оверхед на кадр. Пустой вход -> пусто.
pub fn recognize_batch(
    model: &mut OnnxModel,
    dict: &RecDict,
    imgs: &[RgbImage],
) -> Result<Vec<(String, f32)>, OcrError> {
    if imgs.is_empty() {
        return Ok(Vec::new());
    }
    let pre: Vec<Option<(usize, Vec<f32>)>> = imgs.iter().map(preprocess).collect();
    let w_max = pre.iter().flatten().map(|(w, _)| *w).max().unwrap_or(0).max(16);
    let n = imgs.len();
    let mut input = Array4::<f32>::zeros((n, 3, REC_H, w_max));
    for (i, p) in pre.iter().enumerate() {
        let Some((rw, plane)) = p else { continue };
        for c in 0..3 {
            for oy in 0..REC_H {
                let src = c * REC_H * rw + oy * rw;
                for ox in 0..*rw {
                    input[[i, c, oy, ox]] = plane[src + ox];
                }
            }
        }
    }
    let (shape, flat) = model.run(input)?;
    if shape.len() != 3 {
        return Ok(vec![(String::new(), 0.0); n]);
    }
    let (t, c) = (shape[1], shape[2]);
    let per = t * c;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if pre[i].is_none() {
            out.push((String::new(), 0.0));
        } else {
            out.push(ctc_decode(&flat[i * per..(i + 1) * per], t, c, dict));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ключевой фикс качества B: словарь берётся из метадаты ONNX КАК ЕСТЬ (space первым токеном,
    /// пустой токен последним), blank префиксуется в 0. Лишний пробел/замена пустого сдвигали индексы
    /// и портили CTC-декод («КОРОЧЕ» -> «kрhеh»). Проверяем точное отображение классов.
    #[test]
    fn dict_metadata_indexing_exact() {
        // модель cyrillic: 164 токена (space, ..., пустой), + blank = 165 классов (C выхода).
        let raw = " \n!\n#\nА\nБ\nВ\n"; // split -> [" ","!","#","А","Б","В",""] = 7 токенов
        let d = RecDict::from_dict_str(raw);
        // 7 токенов (последний "" хвостовой) + blank = 8 классов
        assert_eq!(d.chars.len(), 8);
        assert_eq!(d.ch(0), ""); // blank
        assert_eq!(d.ch(1), " "); // space ПЕРВЫМ (как в cyrillic-мете)
        assert_eq!(d.ch(2), "!");
        assert_eq!(d.ch(3), "#");
        assert_eq!(d.ch(4), "А");
        assert_eq!(d.ch(5), "Б");
        assert_eq!(d.ch(6), "В");
        assert_eq!(d.ch(7), ""); // хвостовой пустой токен = класс C-1
        // хвостовой пустой токен на месте (не съеден) — иначе сдвиг индексов
        let raw2 = "А\nБ\n"; // "А","Б",""  -> split даёт ["А","Б",""]
        let d2 = RecDict::from_dict_str(raw2);
        assert_eq!(d2.chars.len(), 4); // blank + А + Б + ""
        assert_eq!(d2.ch(3), "");
    }

    #[test]
    fn dict_file_fallback_strips_only_trailing_newline() {
        let tmp = std::env::temp_dir().join("dub_ocr_dict_test.txt");
        std::fs::write(&tmp, " \r\n!\r\nА\r\n").unwrap(); // CRLF, space первым
        let d = RecDict::load(&tmp).unwrap();
        // blank + [space, !, А] = 4 (хвостовой \n съеден, пустого токена нет)
        assert_eq!(d.chars.len(), 4);
        assert_eq!(d.ch(1), " ");
        assert_eq!(d.ch(2), "!");
        assert_eq!(d.ch(3), "А");
        let _ = std::fs::remove_file(&tmp);
    }
}
