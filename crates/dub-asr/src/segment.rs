//! Сегментация словного потока в реплики дубляжа. Порт _segment из dubengine/asr.py:
//! разрыв на паузах > max_gap, конце предложения (.!?…) или превышении max_dur. Точка после
//! сокращения, инициала или в десятичном числе предложение не кончает. Реплика, на которой сменился
//! спикер диаризации, режется на слове смены ([`split_at_speaker_turns`]).

use serde::Serialize;

use crate::{DiarIndex, Turn};

/// Дефолтные параметры сегментации (порт из dubengine/asr.py): разрыв на паузе > SEG_MAX_GAP сек и
/// жёсткий кап длины реплики SEG_MAX_DUR сек. Единый источник для всех ASR-движков (Parakeet/Whisper).
pub const SEG_MAX_GAP: f64 = 0.6;
pub const SEG_MAX_DUR: f64 = 8.0;

/// Слово со временем (секунды).
#[derive(Debug, Clone, Serialize)]
pub struct Word {
    pub word: String,
    pub start: f64,
    pub end: f64,
}

/// Сегмент дубляжа: [start,end] + текст + список слов. Тот же контракт, что в Python-движке.
#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<Word>,
}

/// Сокращения, за которыми фраза всегда продолжается (титул перед именем, «т.е.», «z.B.»). Нижний
/// регистр, с точкой.
const PREFIX_ABBREVIATIONS: &[&str] = &[
    // en
    "mr.", "mrs.", "ms.", "dr.", "st.", "sr.", "prof.", "vs.", "e.g.", "i.e.",
    // ru
    "т.е.", "ул.",
    // de
    "z.b.", "bzw.", "hr.", "fr.",
    // fr
    "mme.",
    // es, pt
    "sra.", "dra.", "srta.",
];

/// Сокращения, которыми предложение может и закончиться («… и т.д.», «at 5 p.m.»): конец предложения
/// только перед словом с заглавной буквы или в конце потока. «им.» здесь, а не среди титулов: «им.»
/// с точкой чаще местоимение в конце фразы («Скажи им.»), чем «имени».
const TERMINAL_ABBREVIATIONS: &[&str] = &[
    // en
    "etc.", "a.m.", "p.m.", "jr.", "no.",
    // ru
    "т.д.", "т.п.", "г.", "гг.", "им.", "др.", "стр.", "рис.", "см.",
    // de
    "u.a.", "usw.", "nr.",
];

/// Открывающие кавычки и скобки перед словом — не часть сокращения.
const OPENING: &[char] = &['"', '\'', '«', '„', '“', '‘', '(', '[', '¿', '¡'];

/// Одна заглавная буква с точкой — инициал («J.», «А.»). «I.» — английское местоимение в конце фразы.
fn is_initial(core: &str) -> bool {
    let mut chars = core.chars();
    matches!((chars.next(), chars.next(), chars.next()), (Some(c), Some('.'), None) if c.is_uppercase() && c != 'I')
}

/// Буквы через точку, не меньше двух: «U.S.», «a.m.».
fn is_dotted_letters(core: &str) -> bool {
    let chars: Vec<char> = core.chars().collect();
    chars.len() >= 4
        && chars.len().is_multiple_of(2)
        && chars.iter().enumerate().all(|(i, c)| if i.is_multiple_of(2) { c.is_alphabetic() } else { *c == '.' })
}

/// Кончается ли на слове `word` предложение; `next` — следующее слово потока.
fn ends_sentence(word: &str, next: Option<&str>) -> bool {
    if !word.ends_with(['.', '!', '?', '…']) {
        return false;
    }
    if !word.ends_with('.') || word.ends_with("..") {
        return true;
    }
    let core = word.trim_start_matches(OPENING);
    let lower = core.to_lowercase();
    let next_first = next.and_then(|n| n.trim_start_matches(OPENING).chars().next());
    let next_capital = next_first.is_none_or(char::is_uppercase);
    let next_digit = next_first.is_some_and(|c| c.is_ascii_digit());
    if PREFIX_ABBREVIATIONS.contains(&lower.as_str()) || is_initial(core) {
        return false;
    }
    if TERMINAL_ABBREVIATIONS.contains(&lower.as_str()) || is_dotted_letters(core) {
        return next_capital;
    }
    let before_dot = core.trim_end_matches('.').chars().last();
    if before_dot.is_some_and(|c| c.is_ascii_digit()) && next_digit {
        return false;
    }
    true
}

/// Склейка слов сегмента в текст (см. [`join_text`]).
fn join_words(ws: &[Word]) -> String {
    join_text(ws.iter().map(|w| w.word.as_str()))
}

/// Pieces of recognised text joined as the language writes them: a space between words, none where the script puts
/// none (Chinese, Japanese, Thai, Lao, Khmer, Burmese — between two of their characters, or one of them and a digit) and
/// none before closing punctuation.
pub fn join_text<'a>(pieces: impl IntoIterator<Item = &'a str>) -> String {
    let mut s = String::new();
    for piece in pieces {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if let (Some(a), Some(b)) = (s.chars().last(), piece.chars().next()) {
            if spaced(a, b) {
                s.push(' ');
            }
        }
        s.push_str(piece);
    }
    s
}

fn spaced(a: char, b: char) -> bool {
    if is_closing(b) {
        return false;
    }
    let (sa, sb) = (unspaced_script(a), unspaced_script(b));
    !((sa && (sb || b.is_ascii_digit())) || (sb && (a.is_ascii_digit() || a.is_ascii_punctuation())))
}

/// Characters of scripts written without spaces between words.
fn unspaced_script(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF       // hiragana, katakana
        | 0x31F0..=0x31FF     // katakana phonetic extensions
        | 0x3000..=0x303F     // CJK symbols and punctuation
        | 0x3400..=0x4DBF     // CJK extension A
        | 0x4E00..=0x9FFF     // CJK unified ideographs
        | 0xF900..=0xFAFF     // CJK compatibility ideographs
        | 0xFF00..=0xFFEF     // halfwidth and fullwidth forms
        | 0x20000..=0x2FA1F   // CJK extensions B-F and compatibility supplement
        | 0x0E00..=0x0EFF     // Thai, Lao
        | 0x1780..=0x17FF     // Khmer
        | 0x1000..=0x109F     // Myanmar
    )
}

/// Punctuation that closes what is before it and takes no space before itself.
fn is_closing(c: char) -> bool {
    matches!(c, ',' | '.' | '!' | '?' | ';' | ':' | ')' | ']' | '}' | '%' | '、' | '。' | '，' | '！' | '？' | '；' | '：' | '）' | '」' | '』' | '…')
}

fn segment_of(ws: Vec<Word>) -> Segment {
    Segment {
        start: ws.first().map_or(0.0, |w| w.start),
        end: ws.last().map_or(0.0, |w| w.end),
        text: join_words(&ws),
        words: ws,
    }
}

/// Разбить поток слов на сегменты. max_gap=0.6с, max_dur=8.0с (как в _segment).
pub fn segment_words(words: &[Word], max_gap: f64, max_dur: f64) -> Vec<Segment> {
    let mut segs: Vec<Vec<Word>> = Vec::new();
    let mut cur: Vec<Word> = Vec::new();

    for (i, w) in words.iter().enumerate() {
        if let (Some(last), Some(first)) = (cur.last(), cur.first()) {
            let gap = w.start - last.end;
            let dur = last.end - first.start;
            if gap > max_gap || dur > max_dur {
                segs.push(std::mem::take(&mut cur));
            }
        }
        cur.push(w.clone());
        if ends_sentence(&w.word, words.get(i + 1).map(|n| n.word.as_str())) {
            segs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        segs.push(cur);
    }

    segs.into_iter().map(segment_of).collect()
}

/// Спикер слова: реплика диаризации с наибольшим перекрытием; при равенстве — спикер предыдущего слова
/// (иначе меньший id), без перекрытия — спикер предыдущего слова, у первого — ближайшая реплика.
fn word_speaker(idx: &DiarIndex, w: &Word, prev: Option<i32>) -> i32 {
    let end = if w.end > w.start { w.end } else { w.start + 1e-3 };
    let overlaps = idx.overlaps(w.start, end);
    let best = overlaps.iter().map(|(_, o)| *o).fold(0.0, f64::max);
    if best > 0.0 {
        let top = |spk: i32| overlaps.iter().any(|(s, o)| *s == spk && best - o <= 1e-9);
        if let Some(p) = prev.filter(|p| top(*p)) {
            return p;
        }
        if let Some((spk, _)) = overlaps.iter().find(|(_, o)| best - o <= 1e-9) {
            return *spk;
        }
    }
    prev.or_else(|| idx.nearest(w.start, end)).unwrap_or(0)
}

/// Перекрывается ли слово с репликой спикера `spk`.
fn heard_by(idx: &DiarIndex, w: &Word, spk: i32) -> bool {
    let end = if w.end > w.start { w.end } else { w.start + 1e-3 };
    idx.overlaps(w.start, end).iter().any(|(s, o)| *s == spk && *o > 0.0)
}

/// Спикеры реплик по диаризации: каждое слово получает спикера ([`word_speaker`]), реплика режется
/// на слове, где спикер сменился. Одно слово другого спикера между словами одного и того же — дрожание
/// границы диаризации, его не отрезаем. Пустые `turns` — один спикер 0, без резки. Реплика без слов
/// получает спикера по перекрытию целиком.
pub fn split_at_speaker_turns(segs: Vec<Segment>, turns: &[Turn]) -> Vec<(Segment, i32)> {
    if turns.is_empty() {
        return segs.into_iter().map(|s| (s, 0)).collect();
    }
    let idx = DiarIndex::new(turns);
    let mut out = Vec::with_capacity(segs.len());
    for seg in segs {
        if seg.words.is_empty() {
            let spk = idx.assign(seg.start, seg.end);
            out.push((seg, spk));
            continue;
        }
        let mut labels: Vec<i32> = Vec::with_capacity(seg.words.len());
        for w in &seg.words {
            let prev = labels.last().copied();
            labels.push(word_speaker(&idx, w, prev));
        }
        for i in 1..labels.len().saturating_sub(1) {
            if labels[i - 1] == labels[i + 1] && labels[i] != labels[i - 1] {
                labels[i] = labels[i - 1];
            }
        }
        // Крайнее слово, которое заходит и в реплику спикера соседнего куска, лежит на неточной границе
        // диаризации: оно остаётся с соседом, а не становится репликой из одного слова.
        let n = labels.len();
        if n >= 2 {
            if labels[0] != labels[1] && heard_by(&idx, &seg.words[0], labels[1]) {
                labels[0] = labels[1];
            }
            if labels[n - 1] != labels[n - 2] && heard_by(&idx, &seg.words[n - 1], labels[n - 2]) {
                labels[n - 1] = labels[n - 2];
            }
        }
        if labels.iter().all(|l| *l == labels[0]) {
            out.push((seg, labels[0]));
            continue;
        }
        let mut from = 0usize;
        for i in 1..=labels.len() {
            if i == labels.len() || labels[i] != labels[from] {
                out.push((segment_of(seg.words[from..i].to_vec()), labels[from]));
                from = i;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(word: &str, start: f64, end: f64) -> Word {
        Word { word: word.into(), start, end }
    }

    /// Слова подряд по 0.3 с без пауз.
    fn stream(text: &str) -> Vec<Word> {
        text.split_whitespace()
            .enumerate()
            .map(|(i, t)| w(t, i as f64 * 0.3, i as f64 * 0.3 + 0.25))
            .collect()
    }

    fn texts(text: &str) -> Vec<String> {
        segment_words(&stream(text), SEG_MAX_GAP, SEG_MAX_DUR).into_iter().map(|s| s.text).collect()
    }

    #[test]
    fn splits_on_sentence_end() {
        let words = vec![w("Hello", 0.0, 0.4), w("world.", 0.4, 0.8), w("Next", 0.9, 1.2)];
        let segs = segment_words(&words, 0.6, 8.0);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "Hello world.");
        assert_eq!(segs[1].text, "Next");
    }

    #[test]
    fn splits_on_pause() {
        let words = vec![w("a", 0.0, 0.2), w("b", 2.0, 2.2)]; // пауза 1.8с > 0.6
        let segs = segment_words(&words, 0.6, 8.0);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn splits_on_max_dur() {
        let words = vec![w("a", 0.0, 0.2), w("b", 5.0, 5.2), w("c", 9.0, 9.2)];
        // при добавлении c dur (5.0-0.0)=5.0 <8, но добавление b: dur=0 ok; c: dur=5.2-0=... проверяем разрыв
        let segs = segment_words(&words, 100.0, 4.0);
        assert!(segs.len() >= 2);
    }

    #[test]
    fn english_titles_and_latin_abbreviations_do_not_end_a_sentence() {
        assert_eq!(texts("Mr. Smith met Dr. Jones. They talked."), ["Mr. Smith met Dr. Jones.", "They talked."]);
        assert_eq!(texts("Bring fruit, e.g. apples, i.e. red ones. Then go."), ["Bring fruit, e.g. apples, i.e. red ones.", "Then go."]);
        assert_eq!(texts("Mrs. Brown vs. Ms. Green at St. Louis. Done."), ["Mrs. Brown vs. Ms. Green at St. Louis.", "Done."]);
    }

    #[test]
    fn english_terminal_abbreviations_end_only_before_a_capital() {
        assert_eq!(texts("We met at 5 p.m. and left. Fine."), ["We met at 5 p.m. and left.", "Fine."]);
        assert_eq!(texts("Bring pens, paper etc. Then sit."), ["Bring pens, paper etc.", "Then sit."]);
        assert_eq!(texts("Room No. 5 is free. Go."), ["Room No. 5 is free.", "Go."]);
        assert_eq!(texts("No. I will not."), ["No.", "I will not."]);
        assert_eq!(texts("He moved to the U.S. Then he left."), ["He moved to the U.S.", "Then he left."]);
    }

    #[test]
    fn initials_and_decimals_do_not_end_a_sentence() {
        assert_eq!(texts("J. K. Rowling wrote it. Yes."), ["J. K. Rowling wrote it.", "Yes."]);
        assert_eq!(texts("It costs 3. 5 dollars. Ok."), ["It costs 3. 5 dollars.", "Ok."]);
        assert_eq!(texts("So do I. Then we go."), ["So do I.", "Then we go."]);
    }

    #[test]
    fn russian_abbreviations() {
        assert_eq!(texts("Это т.е. главное. Дальше."), ["Это т.е. главное.", "Дальше."]);
        assert_eq!(texts("Столы, стулья и т.д. и всё. Конец."), ["Столы, стулья и т.д. и всё.", "Конец."]);
        assert_eq!(texts("Столы, стулья и т.д. Потом ушли."), ["Столы, стулья и т.д.", "Потом ушли."]);
        assert_eq!(texts("Смотри см. рис. 3 на стр. 5 внизу. Всё."), ["Смотри см. рис. 3 на стр. 5 внизу.", "Всё."]);
        assert_eq!(texts("Парк на ул. Ленина открыт. Да."), ["Парк на ул. Ленина открыт.", "Да."]);
        assert_eq!(texts("Это было в 1999 г. и всё. Точно."), ["Это было в 1999 г. и всё.", "Точно."]);
        assert_eq!(texts("Скажи им. Они ждут."), ["Скажи им.", "Они ждут."]);
        assert_eq!(texts("Пришёл А. С. Пушкин. Ура."), ["Пришёл А. С. Пушкин.", "Ура."]);
    }

    #[test]
    fn german_abbreviations() {
        assert_eq!(texts("Obst, z.B. Äpfel, schmeckt. Gut."), ["Obst, z.B. Äpfel, schmeckt.", "Gut."]);
        assert_eq!(texts("Äpfel, Birnen usw. und mehr. Ende."), ["Äpfel, Birnen usw. und mehr.", "Ende."]);
        assert_eq!(texts("Äpfel, Birnen usw. Dann gehen wir."), ["Äpfel, Birnen usw.", "Dann gehen wir."]);
        assert_eq!(texts("Haus Nr. 5 ist frei. Ja."), ["Haus Nr. 5 ist frei.", "Ja."]);
        assert_eq!(texts("Hr. Müller bzw. Fr. Weber kommt. Gut."), ["Hr. Müller bzw. Fr. Weber kommt.", "Gut."]);
    }

    #[test]
    fn other_sentence_marks_always_end() {
        assert_eq!(texts("Really? Yes! Well… ok."), ["Really?", "Yes!", "Well…", "ok."]);
        assert_eq!(texts("(Mr. Bean) laughs. Ok."), ["(Mr. Bean) laughs.", "Ok."]);
    }

    fn t(start: f64, end: f64, speaker: i32) -> Turn {
        Turn { start, end, speaker }
    }

    #[test]
    fn a_line_over_two_speakers_is_cut_at_the_word_where_the_speaker_changes() {
        // en: «Where are you going I am going home» без паузы и точки: A до 1.2 с, дальше B.
        let words = stream("Where are you going I am going home");
        let segs = segment_words(&words, SEG_MAX_GAP, SEG_MAX_DUR);
        assert_eq!(segs.len(), 1);
        let turns = [t(0.0, 1.15, 0), t(1.15, 3.0, 1)];
        let out = split_at_speaker_turns(segs, &turns);
        let got: Vec<(String, i32)> = out.iter().map(|(s, spk)| (s.text.clone(), *spk)).collect();
        assert_eq!(got, [("Where are you going".to_string(), 0), ("I am going home".to_string(), 1)]);
        assert!((out[1].0.start - 1.2).abs() < 1e-9, "{:?}", out[1].0);
        assert!((out[0].0.end - 1.15).abs() < 1e-9, "{:?}", out[0].0);
        assert_eq!(out[1].0.words.len(), 4);
    }

    #[test]
    fn russian_and_german_lines_split_by_speaker() {
        let ru = segment_words(&stream("Ты куда Домой пойду"), SEG_MAX_GAP, SEG_MAX_DUR);
        let out = split_at_speaker_turns(ru, &[t(0.0, 0.55, 1), t(0.55, 2.0, 0)]);
        let got: Vec<(&str, i32)> = out.iter().map(|(s, spk)| (s.text.as_str(), *spk)).collect();
        assert_eq!(got, [("Ты куда", 1), ("Домой пойду", 0)]);

        let de = segment_words(&stream("Kommst du mit ja gerne"), SEG_MAX_GAP, SEG_MAX_DUR);
        let out = split_at_speaker_turns(de, &[t(0.0, 0.85, 0), t(0.85, 2.0, 1)]);
        let got: Vec<(&str, i32)> = out.iter().map(|(s, spk)| (s.text.as_str(), *spk)).collect();
        assert_eq!(got, [("Kommst du mit", 0), ("ja gerne", 1)]);
    }

    #[test]
    fn one_word_of_another_speaker_inside_a_line_is_jitter() {
        // Реплика B на одно слово внутри речи A: A B A -> одна реплика A.
        let segs = segment_words(&stream("one two three four five"), SEG_MAX_GAP, SEG_MAX_DUR);
        let out = split_at_speaker_turns(segs, &[t(0.0, 0.55, 0), t(0.58, 0.87, 1), t(0.88, 2.0, 0)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, 0);
        assert_eq!(out[0].0.text, "one two three four five");
    }

    #[test]
    fn an_edge_word_on_the_turn_boundary_stays_with_its_line() {
        // «e» (1.2..1.45) заходит и в реплику A (до 1.3), и в B: граница неточна, слово остаётся у A.
        let segs = segment_words(&stream("a b c d e"), SEG_MAX_GAP, SEG_MAX_DUR);
        let out = split_at_speaker_turns(segs, &[t(0.0, 1.3, 0), t(1.3, 3.0, 1)]);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].1, 0);
        // Слово целиком внутри реплики B — это уже B.
        let segs = segment_words(&stream("a b c d e"), SEG_MAX_GAP, SEG_MAX_DUR);
        let out = split_at_speaker_turns(segs, &[t(0.0, 1.15, 0), t(1.15, 3.0, 1)]);
        let got: Vec<(&str, i32)> = out.iter().map(|(s, spk)| (s.text.as_str(), *spk)).collect();
        assert_eq!(got, [("a b c d", 0), ("e", 1)]);
    }

    #[test]
    fn overlapped_speech_keeps_the_speaker_who_was_talking() {
        // A говорит 0..3 с, B вставляет реплику, полностью накрывающую слова A (перекрытие равное):
        // слово остаётся за A, реплика не рвётся.
        let segs = segment_words(&stream("I think that we should go"), SEG_MAX_GAP, SEG_MAX_DUR);
        let out = split_at_speaker_turns(segs, &[t(0.0, 3.0, 1), t(0.55, 1.2, 0)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, 1);
    }

    #[test]
    fn without_diarization_every_line_is_speaker_zero() {
        let segs = segment_words(&stream("a b. c d."), SEG_MAX_GAP, SEG_MAX_DUR);
        let out = split_at_speaker_turns(segs, &[]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(_, spk)| *spk == 0));
    }

    #[test]
    fn languages_without_spaces_join_without_spaces() {
        let zh = ["今天", "天", "气", "很好", ",", "我们", "一起", "去", "公", "园"];
        assert_eq!(join_text(zh), "今天天气很好,我们一起去公园");
        assert_eq!(join_text(["1945", "年", "の", "春", "。"]), "1945年の春。");
        assert_eq!(join_text(["สวัสดี", "ครับ"]), "สวัสดีครับ");
        assert_eq!(join_text(["我", "用", "Python", "写", "代码"]), "我用 Python 写代码");
        assert_eq!(join_text(["Where's", "your", "uniform?", "Hair", "five", "inches"]), "Where's your uniform? Hair five inches");
        assert_eq!(join_text(["Победа", "пришла", "весной", "1945", "года", "."]), "Победа пришла весной 1945 года.");
        assert_eq!(texts(&zh.join(" ")), vec!["今天天气很好,我们一起去公园".to_string()]);
    }
}
