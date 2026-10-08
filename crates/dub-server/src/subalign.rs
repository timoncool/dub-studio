//! Выравнивание импортированных SRT/ASS по речи: текст реплик остаётся из файла, тайминги берутся у
//! распознанных слов. Основа — align_lyrics студий (YuE2-Studio lyrics_sync.rs): все реплики ставятся
//! разом монотонным DP по максимуму суммарного совпадения (окна длиной span-1..span+2, порог 0.45,
//! полный зачёт от 0.7, при равенстве — раньше, донастройка ±2 слова), совпадение — посимвольный Dice
//! студии (normalise дословно, dice/similarity — эталон в тестах), поэтому недослышанные слова не рушат
//! реплику, а повторы («Да!») расходуются по порядку.
//!
//! Адаптация под эпизод: студийный DP O(L·W²) и матрица оценок O(L·W) на серии в 400 реплик и 4000 слов
//! неподъёмны, поэтому кандидаты — только слова в окне ±`WINDOW_SECS` вокруг начала реплики со сдвигом,
//! а DP идёт по префиксному максимуму. Сдвиг не один на файл: дрейф частоты кадров (25 против 23.976 —
//! 4.3 %, к концу серии минуты) и вырезанные или вставленные куски (рекап, реклама) меняют его по ходу.
//! Первый проход центрирует окно на местном сдвиге — пике голосов редких слов в окрестности реплики;
//! второй — на кривой сдвига по репликам, уверенно совпавшим в первом (`Curve`). Конец реплики — конец последнего
//! совпавшего слова (у студии слово без конца), не дальше начала следующей. Реплика без совпадения
//! сдвигается по той же кривой с сохранением длительности, а не растягивается.

use std::collections::HashMap;
use std::ops::Range;

/// Окно поиска кандидатов вокруг начала реплики со сдвигом, сек.
const WINDOW_SECS: f64 = 30.0;
/// Окрестность реплики по времени файла, голоса из которой дают её местный сдвиг, сек.
const LOCAL_SECS: f64 = 60.0;
/// Полуширина пика голосов, сек: дрейф внутри окрестности и неточность ожидаемого времени слова.
const PEAK_SECS: f64 = 4.0;
/// Предельный дрейф сдвига, с на секунду файла (25/23.976 — 0.043).
const MAX_DRIFT: f64 = 0.1;
/// Расхождение сдвига совпавшей реплики с соседями (сверх MAX_DRIFT на расстояние до них), после
/// которого она не входит в кривую, сек.
const OUTLIER_SECS: f64 = 2.0;
/// Сколько совпавших соседей с каждой стороны сверяет фильтр выбросов кривой.
const NEIGHBOURS: usize = 4;
/// Наклон за краем кривой берётся по её крайнему непрерывному куску не длиннее этого, сек.
const EDGE_SECS: f64 = 600.0;
/// Крайний кусок короче этого наклона не даёт: разброс начал реплик в нём больше дрейфа, сек.
const MIN_SLOPE_SECS: f64 = 60.0;
/// Ниже этой доли сопоставленных реплик субтитры не про эту речь (другой релиз с другим текстом, другой
/// язык) — тайминги файла оставляем как есть.
pub const MIN_ALIGNED_SHARE: f64 = 0.3;
/// Минимальная длительность реплики после выравнивания, сек.
const MIN_DUR: f64 = 0.3;
/// Совпадение от этой оценки — полный зачёт в DP и опора кривой сдвига.
const FULL_CREDIT: f64 = 0.7;

/// Услышанное слово: текст, начало, конец (сек).
pub type Heard = (String, f64, f64);

/// Как реплика получила тайминг.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// Совпала с распознанной речью: начало первого и конец последнего совпавшего слова.
    Aligned,
    /// Не совпала: сдвинута по кривой сдвига совпавших реплик (между ними — интерполяция, за краями —
    /// продолжение с наклоном), длительность сохранена.
    Shifted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub start: f64,
    pub end: f64,
    pub how: How,
}

/// Итог выравнивания файла.
#[derive(Debug, Clone)]
pub struct Alignment {
    /// По реплике на каждый cue, в исходном порядке.
    pub cues: Vec<Placed>,
    /// Сдвиг файла (речь минус субтитры) — медиана по совпавшим репликам, сек; при дрейфе и вырезках
    /// сдвиг меняется по ходу, это его середина.
    pub offset: f64,
    /// Доля сопоставленных реплик.
    pub share: f64,
}

fn normalise(word: &str) -> String {
    word.chars().filter(|character| character.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Лучший Dice реплики `written` против окон heard[start..start+len], len = span-1..span+2; возвращает
/// (оценка, длина окна). Dice и посимвольный LCS — студийные dice/similarity (их дословная копия в
/// тестах — эталон); один проход LCS по самому длинному окну даёт LCS для всех более коротких (строка
/// DP по префиксу услышанного), поэтому окна не пересчитываются по отдельности.
fn best_window(written: &[char], heard: &[(String, f64, f64)], start: usize, span: usize) -> (f64, usize) {
    let lo = span.saturating_sub(1).max(1);
    let hi = (span + 2).min(heard.len() - start);
    if hi < lo {
        return (0.0, 0);
    }
    let mut spoken: Vec<char> = Vec::new();
    let mut bounds: Vec<usize> = Vec::with_capacity(hi);
    for (w, _, _) in &heard[start..start + hi] {
        spoken.extend(w.chars());
        bounds.push(spoken.len());
    }
    let mut previous = vec![0usize; spoken.len() + 1];
    let mut current = vec![0usize; spoken.len() + 1];
    for &l in written {
        for r in 0..spoken.len() {
            current[r + 1] = if l == spoken[r] { previous[r] + 1 } else { current[r].max(previous[r + 1]) };
        }
        std::mem::swap(&mut previous, &mut current);
        current.iter_mut().for_each(|v| *v = 0);
    }
    let left = written.len();
    let mut best = (0.0f64, 0usize);
    for len in lo..=hi {
        let right = bounds[len - 1];
        if left + right == 0 {
            continue;
        }
        let d = 2.0 * previous[right] as f64 / (left + right) as f64;
        if d > best.0 {
            best = (d, len);
        }
    }
    best
}

/// Голоса редких слов за сдвиг: слово реплики длиной от 4 букв, встреченное в речи не больше 3 раз,
/// голосует каждым вхождением парой (ожидаемое время слова в файле по доле символов реплики, время в
/// речи минус ожидаемое). По возрастанию времени в файле.
fn votes(cues: &[(f64, f64, Vec<String>)], heard: &[(String, f64, f64)]) -> Vec<(f64, f64)> {
    let mut index: HashMap<&str, Vec<f64>> = HashMap::new();
    for (w, s, _) in heard {
        if w.chars().count() >= 4 {
            index.entry(w.as_str()).or_default().push(*s);
        }
    }
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (start, end, tokens) in cues {
        let total: usize = tokens.iter().map(|t| t.chars().count()).sum::<usize>().max(1);
        let mut before = 0usize;
        for t in tokens {
            let expected = start + (end - start).max(0.0) * before as f64 / total as f64;
            before += t.chars().count();
            if t.chars().count() < 4 {
                continue;
            }
            let Some(times) = index.get(t.as_str()) else { continue };
            if times.len() > 3 {
                continue;
            }
            out.extend(times.iter().map(|&at| (expected, at - expected)));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Пик сдвигов: самый населённый отрезок длиной 2·PEAK_SECS (при равенстве — ближе к нулю), значение —
/// медиана попавших в него. Меньше двух голосов в пике — пика нет: одиночное совпадение редкого слова
/// бывает случайным.
fn peak(mut deltas: Vec<f64>) -> Option<f64> {
    deltas.sort_by(f64::total_cmp);
    let centre = |from: usize, to: usize| (deltas[from] + deltas[to - 1]) / 2.0;
    let mut best: Option<(usize, usize)> = None;
    let mut to = 0;
    for from in 0..deltas.len() {
        while to < deltas.len() && deltas[to] <= deltas[from] + 2.0 * PEAK_SECS {
            to += 1;
        }
        let wins = match best {
            None => true,
            Some((a, b)) => to - from > b - a || (to - from == b - a && centre(from, to).abs() < centre(a, b).abs()),
        };
        if wins {
            best = Some((from, to));
        }
    }
    let (from, to) = best.filter(|(from, to)| to - from >= 2)?;
    Some(deltas[(from + to) / 2])
}

/// Местный сдвиг каждой реплики: пик голосов из окрестности ±LOCAL_SECS её начала. Реплике без пика —
/// сдвиг ближайшей по времени реплики с пиком; пиков нет вовсе — 0 (окно на таймингах файла).
fn local_offsets(starts: &[f64], votes: &[(f64, f64)]) -> Vec<f64> {
    let peaks: Vec<Option<f64>> = starts
        .iter()
        .map(|&t| {
            let from = votes.partition_point(|v| v.0 < t - LOCAL_SECS);
            let to = votes.partition_point(|v| v.0 <= t + LOCAL_SECS);
            peak(votes[from..to].iter().map(|v| v.1).collect())
        })
        .collect();
    let mut known: Vec<(f64, f64)> = starts.iter().zip(&peaks).filter_map(|(&t, p)| p.map(|p| (t, p))).collect();
    known.sort_by(|a, b| a.0.total_cmp(&b.0));
    starts
        .iter()
        .zip(peaks)
        .map(|(&t, p)| {
            p.or_else(|| {
                let k = known.partition_point(|q| q.0 < t);
                [k.checked_sub(1), (k < known.len()).then_some(k)]
                    .into_iter()
                    .flatten()
                    .min_by(|&a, &b| (known[a].0 - t).abs().total_cmp(&(known[b].0 - t).abs()))
                    .map(|k| known[k].1)
            })
            .unwrap_or(0.0)
        })
        .collect()
}

/// Реплика в работе: начало в файле, число слов, текст без пробелов и знаков, кандидаты (первое слово ->
/// (Dice, длина окна), только прошедшие порог) и уже оценённые диапазоны слов.
struct Line {
    start: f64,
    span: usize,
    written: Vec<char>,
    scores: HashMap<usize, (f64, usize)>,
    scanned: Vec<Range<usize>>,
}

impl Line {
    /// Оценить кандидатов — слова, начинающиеся в ±WINDOW_SECS от начала реплики со сдвигом `shift`;
    /// оценённые прошлым проходом не пересчитываются.
    fn scan(&mut self, heard: &[(String, f64, f64)], shift: f64) {
        if self.span == 0 {
            return;
        }
        let centre = self.start + shift;
        let first = heard.partition_point(|(_, s, _)| *s < centre - WINDOW_SECS);
        let end = heard.partition_point(|(_, s, _)| *s <= centre + WINDOW_SECS);
        for j in first..end {
            if self.scanned.iter().any(|done| done.contains(&j)) {
                continue;
            }
            let (score, len) = best_window(&self.written, heard, j, self.span);
            if score >= 0.45 {
                self.scores.insert(j, (score, len));
            }
        }
        self.scanned.push(first..end);
    }
}

/// Все реплики разом по оценённым кандидатам: индекс первого слова каждой реплики, None — не встала.
fn solve(lines: &[Line], count: usize) -> Vec<Option<usize>> {
    // DP студии: best[p] — лучшее (сумма зачётов, сумма стартов) при следующей реплике не раньше слова
    // p; реплика либо ставится на старт ≥ p, либо пропускается. Префиксный максимум вместо перебора всех p.
    let credit = |score: f64| if score >= FULL_CREDIT { 1.0 } else { score };
    let better = |left: (f64, usize), right: (f64, usize)| left.0 > right.0 + 1e-9 || ((left.0 - right.0).abs() <= 1e-9 && left.1 < right.1);
    let mut best: Vec<Option<(f64, usize)>> = vec![None; count + 1];
    best[0] = Some((0.0, 0));
    // Разреженные обратные ссылки: позиция -> (откуда, старт). Нет записи — реплика пропущена.
    let mut back: Vec<HashMap<usize, (usize, usize)>> = Vec::with_capacity(lines.len());
    for line in lines {
        let mut prefix: Vec<Option<((f64, usize), usize)>> = vec![None; count + 1];
        let mut run: Option<((f64, usize), usize)> = None;
        for p in 0..=count {
            if let Some(v) = best[p] {
                if run.is_none_or(|(kept, _)| better(v, kept)) {
                    run = Some((v, p));
                }
            }
            prefix[p] = run;
        }
        let mut next = best.clone();
        let mut choice: HashMap<usize, (usize, usize)> = HashMap::new();
        let mut starts: Vec<(&usize, &(f64, usize))> = line.scores.iter().collect();
        starts.sort_by_key(|(j, _)| **j);
        for (&start, &(score, _)) in starts {
            let Some((so_far, from)) = prefix[start] else { continue };
            let after = (start + line.span).min(count);
            let total = (so_far.0 + credit(score), so_far.1 + start);
            if next[after].is_none_or(|kept| better(total, kept)) {
                next[after] = Some(total);
                choice.insert(after, (from, start));
            }
        }
        best = next;
        back.push(choice);
    }
    let mut position = (0..=count).fold(0, |kept, candidate| match (best[candidate], best[kept]) {
        (Some(offered), Some(held)) if better(offered, held) => candidate,
        (Some(_), None) => candidate,
        _ => kept,
    });
    let mut starts: Vec<Option<usize>> = vec![None; lines.len()];
    for index in (0..lines.len()).rev() {
        if let Some(&(from, start)) = back[index].get(&position) {
            starts[index] = Some(start);
            position = from;
        }
    }

    // A clearly recognised line was credited in full, so its start may sit a
    // word early; settle it where it matches best, between its neighbours.
    for index in 0..lines.len() {
        let Some(start) = starts[index] else { continue };
        let floor = starts[..index].iter().rev().flatten().next().map(|previous| previous + 1).unwrap_or(0);
        let ceiling = starts[index + 1..].iter().flatten().next().copied().unwrap_or(count);
        let low = start.saturating_sub(2).max(floor);
        let high = (start + 2).min(ceiling.saturating_sub(1)).min(count.saturating_sub(1));
        let score_at = |j: usize| lines[index].scores.get(&j).map(|v| v.0).unwrap_or(0.0);
        let settled = (low..=high).fold(start, |kept, candidate| if score_at(candidate) > score_at(kept) { candidate } else { kept });
        starts[index] = Some(settled);
    }
    starts
}

/// (начало в файле, сдвиг) реплик, совпавших с полным зачётом. Слабое совпадение там, где настоящее
/// место реплики за окном, — чужая реплика, а цепочка таких согласована между собой и увела бы кривую.
fn anchors(starts: &[Option<usize>], lines: &[Line], heard: &[(String, f64, f64)]) -> Vec<(f64, f64)> {
    starts
        .iter()
        .zip(lines)
        .filter_map(|(start, line)| {
            let j = (*start)?;
            (line.scores.get(&j)?.0 >= FULL_CREDIT).then(|| (line.start, heard[j].1 - line.start))
        })
        .collect()
}

/// Сдвиг (речь минус файл) как функция времени в файле по совпавшим репликам: ломаная через них, за
/// краями — продолжение с наклоном крайнего непрерывного куска (дрейф частоты кадров). Совпадение, не
/// сходящееся ни с соседями слева, ни с соседями справа, в кривую не входит; сверки с одной стороной
/// достаточно, поэтому настоящий скачок сдвига на вырезанном куске сохраняется.
struct Curve {
    points: Vec<(f64, f64)>,
    head: f64,
    tail: f64,
}

impl Curve {
    fn fit(mut anchors: Vec<(f64, f64)>) -> Option<Curve> {
        anchors.sort_by(|a, b| a.0.total_cmp(&b.0));
        let agrees = |k: usize, side: &[(f64, f64)]| {
            if side.is_empty() {
                return false;
            }
            let mut deltas: Vec<f64> = side.iter().map(|a| a.1).collect();
            deltas.sort_by(f64::total_cmp);
            let reach = side.iter().map(|a| (a.0 - anchors[k].0).abs()).fold(0.0, f64::max);
            (anchors[k].1 - deltas[deltas.len() / 2]).abs() <= OUTLIER_SECS + MAX_DRIFT * reach
        };
        let points: Vec<(f64, f64)> = (0..anchors.len())
            .filter(|&k| {
                let left = &anchors[k.saturating_sub(NEIGHBOURS)..k];
                let right = &anchors[k + 1..(k + 1 + NEIGHBOURS).min(anchors.len())];
                (left.is_empty() && right.is_empty()) || agrees(k, left) || agrees(k, right)
            })
            .map(|k| anchors[k])
            .collect();
        if points.is_empty() {
            return None;
        }
        let head = edge_slope(points.iter().copied());
        let tail = edge_slope(points.iter().rev().copied());
        Some(Curve { points, head, tail })
    }

    fn at(&self, t: f64) -> f64 {
        let p = &self.points;
        let k = p.partition_point(|q| q.0 <= t);
        if k == 0 {
            p[0].1 + self.head * (t - p[0].0)
        } else if k == p.len() {
            p[k - 1].1 + self.tail * (t - p[k - 1].0)
        } else {
            let (a, b) = (p[k - 1], p[k]);
            a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0)
        }
    }
}

/// Наклон сдвига у края: точки от края, пока соседние сходятся (нет скачка вырезанного куска) и кусок не
/// длиннее EDGE_SECS; медиана попарных наклонов (Тейл — Сен), не круче MAX_DRIFT. Кусок короче
/// MIN_SLOPE_SECS — 0.
fn edge_slope(from_edge: impl Iterator<Item = (f64, f64)>) -> f64 {
    let mut piece: Vec<(f64, f64)> = Vec::new();
    for p in from_edge {
        if let (Some(first), Some(last)) = (piece.first(), piece.last()) {
            let jump = (p.1 - last.1).abs() > OUTLIER_SECS + MAX_DRIFT * (p.0 - last.0).abs();
            if jump || (p.0 - first.0).abs() > EDGE_SECS {
                break;
            }
        }
        piece.push(p);
    }
    let span = match (piece.first(), piece.last()) {
        (Some(first), Some(last)) => (last.0 - first.0).abs(),
        _ => 0.0,
    };
    if span < MIN_SLOPE_SECS {
        return 0.0;
    }
    let mut slopes: Vec<f64> = Vec::new();
    for (k, a) in piece.iter().enumerate() {
        for b in &piece[k + 1..] {
            let dt = b.0 - a.0;
            if dt.abs() >= 1.0 {
                slopes.push((b.1 - a.1) / dt);
            }
        }
    }
    slopes.sort_by(f64::total_cmp);
    slopes.get(slopes.len() / 2).map_or(0.0, |s| s.clamp(-MAX_DRIFT, MAX_DRIFT))
}

/// Выровнять реплики (start, end, text) по услышанным словам. Возвращает None, если слов нет.
pub fn align(cues: &[(f64, f64, &str)], words: &[Heard]) -> Option<Alignment> {
    let heard: Vec<(String, f64, f64)> = words
        .iter()
        .map(|(w, s, e)| (normalise(w), *s, e.max(*s)))
        .filter(|(w, _, _)| !w.is_empty())
        .collect();
    if heard.is_empty() || cues.is_empty() {
        return None;
    }
    let count = heard.len();
    let tokens: Vec<(f64, f64, Vec<String>)> = cues
        .iter()
        .map(|(s, e, t)| (*s, *e, t.split_whitespace().map(normalise).filter(|w| !w.is_empty()).collect()))
        .collect();
    let votes = votes(&tokens, &heard);
    let mut lines: Vec<Line> = tokens
        .iter()
        .map(|(start, _, toks)| Line { start: *start, span: toks.len(), written: toks.concat().chars().collect(), scores: HashMap::new(), scanned: Vec::new() })
        .collect();

    // Проход 1: окно вокруг местного сдвига по голосам редких слов.
    let starts_in_file: Vec<f64> = lines.iter().map(|line| line.start).collect();
    for (line, shift) in lines.iter_mut().zip(local_offsets(&starts_in_file, &votes)) {
        line.scan(&heard, shift);
    }
    let mut starts = solve(&lines, count);
    // Проход 2: окно вокруг кривой сдвига по уверенно совпавшим в первом проходе. Где голоса разошлись
    // с кривой (граница вырезки, участок без редких слов), добавляются новые кандидаты; прочее оценено.
    if let Some(curve) = Curve::fit(anchors(&starts, &lines, &heard)) {
        for line in &mut lines {
            let shift = curve.at(line.start);
            line.scan(&heard, shift);
        }
        starts = solve(&lines, count);
    }

    // Тайминги: совпавшие — по словам; конец не дальше начала следующей совпавшей реплики.
    let mut placed: Vec<Option<Placed>> = starts
        .iter()
        .zip(&lines)
        .map(|(start, line)| {
            start.map(|j| {
                let len = line.scores.get(&j).map(|v| v.1).unwrap_or(1).max(1);
                let last = (j + len - 1).min(count - 1);
                Placed { start: heard[j].1, end: heard[last].2, how: How::Aligned }
            })
        })
        .collect();
    let matched = placed.iter().filter(|p| p.is_some()).count();
    // Сдвиг файла — медиана (новое начало − начало в файле) по совпавшим репликам.
    let mut deltas: Vec<f64> = placed.iter().zip(cues).filter_map(|(p, (s, _, _))| p.as_ref().map(|p| p.start - s)).collect();
    deltas.sort_by(f64::total_cmp);
    let offset = match deltas.get(deltas.len() / 2) {
        Some(&median) => median,
        None => peak(votes.iter().map(|v| v.1).collect()).unwrap_or(0.0),
    };

    // Несопоставленные: сдвиг по кривой совпавших; длительность реплики из файла сохраняется.
    let curve = Curve::fit(anchors(&starts, &lines, &heard));
    for (slot, &(cs, ce, _)) in placed.iter_mut().zip(cues) {
        if slot.is_none() {
            let delta = curve.as_ref().map_or(offset, |c| c.at(cs));
            *slot = Some(Placed { start: cs + delta, end: ce + delta, how: How::Shifted });
        }
    }

    // Порядок и неперекрытие: реплика кончается не позже начала следующей, не короче MIN_DUR.
    let mut out: Vec<Placed> = placed.into_iter().map(|p| p.expect("every line is placed above")).collect();
    for index in 0..out.len() {
        out[index].start = out[index].start.max(0.0);
        if index + 1 < out.len() {
            let next_start = out[index + 1].start;
            if out[index].end > next_start {
                out[index].end = next_start.max(out[index].start + MIN_DUR);
            }
        }
        if out[index].end < out[index].start + MIN_DUR {
            out[index].end = out[index].start + MIN_DUR;
        }
    }
    Some(Alignment { cues: out, offset, share: matched as f64 / cues.len() as f64 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speak(from: f64, text: &str) -> Vec<Heard> {
        text.split_whitespace()
            .enumerate()
            .map(|(i, w)| (w.to_string(), from + i as f64 * 0.4, from + i as f64 * 0.4 + 0.3))
            .collect()
    }

    /// The characters two strings share in order, over both their lengths: 1 for
    /// the same text, lower for what either one misses or adds.
    fn dice(expected: &str, heard: &str) -> f64 {
        let (left, right) = (expected.chars().count(), heard.chars().count());
        if left + right == 0 {
            return 0.0;
        }
        2.0 * similarity(expected, heard) * left as f64 / (left + right) as f64
    }

    /// How much of the written line the recogniser heard, compared character by
    /// character rather than word by word.
    fn similarity(expected: &str, heard: &str) -> f64 {
        if expected.is_empty() || heard.is_empty() {
            return 0.0;
        }
        let left: Vec<char> = expected.chars().collect();
        let right: Vec<char> = heard.chars().collect();
        let mut previous = vec![0usize; right.len() + 1];
        let mut current = vec![0usize; right.len() + 1];
        for &lc in &left {
            for r in 0..right.len() {
                current[r + 1] = if lc == right[r] { previous[r] + 1 } else { current[r].max(previous[r + 1]) };
            }
            std::mem::swap(&mut previous, &mut current);
            current.iter_mut().for_each(|value| *value = 0);
        }
        previous[right.len()] as f64 / left.len() as f64
    }

    #[test]
    fn best_window_equals_the_studio_dice_over_each_window() {
        let heard: Vec<(String, f64, f64)> = ["neon", "arms", "the", "grass", "tonight", "again"]
            .iter()
            .enumerate()
            .map(|(i, w)| (w.to_string(), i as f64, i as f64 + 0.5))
            .collect();
        for line in ["Neon on the glass", "the grass tonight", "again"] {
            let toks: Vec<String> = line.split_whitespace().map(normalise).collect();
            let written: String = toks.concat();
            let chars: Vec<char> = written.chars().collect();
            for start in 0..heard.len() {
                let span = toks.len();
                let (got, len) = best_window(&chars, &heard, start, span);
                let want = (span.saturating_sub(1).max(1)..=span + 2)
                    .filter(|l| start + l <= heard.len())
                    .map(|l| dice(&written, &heard[start..start + l].iter().map(|h| h.0.as_str()).collect::<String>()))
                    .fold(0.0, f64::max);
                assert!((got - want).abs() < 1e-12, "{line} @{start}: {got} vs {want} (len {len})");
            }
        }
        assert_eq!(similarity("", "x"), 0.0);
    }

    #[test]
    fn a_shifted_release_snaps_to_the_speech() {
        // Субтитры другого релиза: всё на 3.2 с раньше речи.
        let mut words = speak(10.0, "where are you going tonight");
        words.extend(speak(14.0, "to the old harbour with my brother"));
        words.extend(speak(20.0, "come back before the storm"));
        let cues = [(6.8, 9.0, "Where are you going tonight?"), (10.8, 13.5, "To the old harbour, with my brother."), (16.8, 19.0, "Come back before the storm!")];
        let a = align(&cues, &words).unwrap();
        assert!((a.offset - 3.2).abs() < 0.3, "сдвиг файла {}", a.offset);
        assert_eq!(a.share, 1.0);
        assert_eq!(a.cues[0], Placed { start: 10.0, end: 11.9, how: How::Aligned });
        assert_eq!(a.cues[1].start, 14.0);
        assert!((a.cues[1].end - 16.7).abs() < 1e-9, "{:?}", a.cues[1]);
        assert_eq!(a.cues[2].start, 20.0);
    }

    #[test]
    fn a_release_off_by_more_than_the_window_is_found_by_rare_words() {
        let lines = [
            "the lighthouse keeper lost his lantern",
            "nobody believed the fisherman",
            "storms come from the western ridge",
            "bring the ropes before midnight",
        ];
        let mut words = Vec::new();
        let mut cues_owned = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let t = 10.0 + i as f64 * 6.0;
            words.extend(speak(t + 45.0, l));
            cues_owned.push((t, t + 3.0, *l));
        }
        let a = align(&cues_owned, &words).unwrap();
        assert_eq!(a.share, 1.0);
        assert!((a.offset - 45.0).abs() < 1e-9, "{}", a.offset);
    }

    #[test]
    fn misheard_words_still_place_the_line() {
        let words = speak(5.0, "neon arms the grass");
        let a = align(&[(4.0, 6.0, "Neon on the glass")], &words).unwrap();
        assert_eq!(a.cues[0].start, 5.0);
        assert_eq!(a.cues[0].how, How::Aligned);
    }

    #[test]
    fn a_repeated_line_is_consumed_in_order() {
        let mut words = speak(1.0, "yes");
        words.extend(speak(3.0, "i said no"));
        words.extend(speak(6.0, "yes"));
        let cues = [(0.5, 1.5, "Yes!"), (2.5, 4.5, "I said no."), (5.5, 6.5, "Yes!")];
        let a = align(&cues, &words).unwrap();
        assert_eq!(a.cues[0].start, 1.0);
        assert_eq!(a.cues[2].start, 6.0);
    }

    #[test]
    fn an_unheard_line_moves_with_its_neighbours_and_keeps_its_length() {
        let mut words = speak(12.0, "first line here");
        words.extend(speak(22.0, "third line here"));
        // Средняя реплика — пение, её слов нет в распознанном.
        let cues = [(10.0, 11.5, "First line here"), (15.0, 17.0, "la la la la"), (20.0, 21.5, "Third line here")];
        let a = align(&cues, &words).unwrap();
        assert_eq!(a.cues[1].how, How::Shifted);
        assert!((a.cues[1].start - 17.0).abs() < 1e-9, "{:?}", a.cues[1]);
        assert!((a.cues[1].end - a.cues[1].start - 2.0).abs() < 1e-9);
    }

    #[test]
    fn subtitles_in_another_language_barely_match() {
        let words = speak(1.0, "hello my friend how are you today");
        let cues = [(1.0, 2.0, "Привет, мой друг"), (2.0, 3.5, "Как ты сегодня?")];
        let a = align(&cues, &words).unwrap();
        assert!(a.share < MIN_ALIGNED_SHARE, "{}", a.share);
    }

    #[test]
    fn a_long_episode_aligns_fast_enough() {
        // 400 реплик × ~10 слов = 4000 слов; дрейф 1.5 с на весь эпизод.
        let vocab = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima", "mike"];
        let mut words = Vec::new();
        let mut cues_owned: Vec<(f64, f64, String)> = Vec::new();
        for i in 0..400usize {
            let text: Vec<&str> = (0..10).map(|k| vocab[(i * 7 + k * 3 + i / 13) % vocab.len()]).collect();
            let t = i as f64 * 5.0;
            words.extend(speak(t + 2.0 + 1.5 * i as f64 / 400.0, &text.join(" ")));
            cues_owned.push((t, t + 4.0, text.join(" ")));
        }
        let cues: Vec<(f64, f64, &str)> = cues_owned.iter().map(|(s, e, t)| (*s, *e, t.as_str())).collect();
        let started = std::time::Instant::now();
        let a = align(&cues, &words).unwrap();
        assert!(a.share > 0.9, "{}", a.share);
        assert!((a.cues[399].start - (399.0 * 5.0 + 2.0 + 1.5 * 399.0 / 400.0)).abs() < 0.05, "{:?}", a.cues[399]);
        assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
    }

    fn next(state: &mut u64) -> usize {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (*state >> 33) as usize
    }

    fn random_word(state: &mut u64) -> String {
        let len = 5 + next(state) % 4;
        (0..len).map(|_| (b'a' + (next(state) % 26) as u8) as char).collect()
    }

    /// Эпизод: реплика каждые 5 с (4 с на экране) из 8 слов — 3 частых и 5 прочих (5–8 случайных букв):
    /// у первых `own` реплик прочие слова свои, у остальных из общего набора в 40 слов, так что редких
    /// слов для голосов там нет. `speech(t)` — где в речи звучит реплика с началом t в файле, None — её
    /// речи в видео нет.
    fn episode(lines: usize, own: usize, speech: impl Fn(f64) -> Option<f64>) -> (Vec<(f64, f64, String)>, Vec<Heard>) {
        let common = ["that", "with", "have", "you", "the", "what", "this", "and"];
        let mut state = 7u64;
        let pool: Vec<String> = (0..40).map(|_| random_word(&mut state)).collect();
        let mut cues = Vec::new();
        let mut words = Vec::new();
        for i in 0..lines {
            let text: Vec<String> = (0..8)
                .map(|k| {
                    if k % 3 == 1 {
                        common[next(&mut state) % common.len()].to_string()
                    } else if i < own {
                        random_word(&mut state)
                    } else {
                        pool[next(&mut state) % pool.len()].clone()
                    }
                })
                .collect();
            let text = text.join(" ");
            let t = i as f64 * 5.0;
            if let Some(at) = speech(t) {
                words.extend(speak(at, &text));
            }
            cues.push((t, t + 4.0, text));
        }
        (cues, words)
    }

    #[test]
    fn framerate_drift_over_a_45_minute_episode_is_followed() {
        // 25 против 23.976 (4.3 %) в обе стороны: к концу 45 минут сдвиг уходит почти на две минуты.
        // Заставка и титры без речи — их реплики продолжают дрейф с наклоном.
        for ratio in [25.0 / 23.976, 23.976 / 25.0] {
            let truth = |t: f64| 2.0 + t * ratio;
            let speech = |t: f64| (15.0..2670.0).contains(&t).then(|| truth(t));
            let (owned, words) = episode(540, 540, speech);
            let cues: Vec<(f64, f64, &str)> = owned.iter().map(|(s, e, t)| (*s, *e, t.as_str())).collect();
            let a = align(&cues, &words).unwrap();
            assert!((a.share - 531.0 / 540.0).abs() < 1e-9, "{ratio}: {}", a.share);
            for (placed, (t, _, _)) in a.cues.iter().zip(&cues) {
                let how = if speech(*t).is_some() { How::Aligned } else { How::Shifted };
                assert_eq!(placed.how, how, "{ratio} @{t}");
                assert!((placed.start - truth(*t)).abs() < 0.05, "{ratio} @{t}: {placed:?} vs {}", truth(*t));
            }
        }
    }

    #[test]
    fn a_drifting_stretch_without_rare_words_is_found_through_the_curve() {
        // Первые 10 минут с редкими словами, дальше 35 минут без них: местный сдвиг там один — от края
        // голосов, а дрейф 4.3 % уводит речь за окно; ловит второй проход по наклону кривой.
        let truth = |t: f64| 2.0 + t * 25.0 / 23.976;
        let (owned, words) = episode(540, 120, |t| Some(truth(t)));
        let cues: Vec<(f64, f64, &str)> = owned.iter().map(|(s, e, t)| (*s, *e, t.as_str())).collect();
        let a = align(&cues, &words).unwrap();
        let exact = a.cues.iter().zip(&cues).filter(|(placed, (t, _, _))| placed.how == How::Aligned && (placed.start - truth(*t)).abs() < 1e-9).count();
        assert!(exact as f64 / cues.len() as f64 > 0.95, "{exact} of {}", cues.len());
        assert!((a.cues[539].start - truth(539.0 * 5.0)).abs() < 0.5, "{:?}", a.cues[539]);
    }

    #[test]
    fn a_cut_in_the_middle_moves_the_rest_of_the_episode() {
        // 22 минуты. -40: в релизе субтитров 40-секундный рекап, из видео вырезанный; +40: в видео 40 с,
        // которых нет в субтитрах.
        for jump in [-40.0, 40.0] {
            let speech = |t: f64| {
                if jump < 0.0 && (660.0..700.0).contains(&t) {
                    None
                } else if t >= 660.0 {
                    Some(t + jump)
                } else {
                    Some(t)
                }
            };
            let (owned, words) = episode(264, 264, speech);
            let cues: Vec<(f64, f64, &str)> = owned.iter().map(|(s, e, t)| (*s, *e, t.as_str())).collect();
            let a = align(&cues, &words).unwrap();
            for (placed, (t, _, _)) in a.cues.iter().zip(&cues) {
                match speech(*t) {
                    Some(at) => {
                        assert_eq!(placed.how, How::Aligned, "{jump} @{t}");
                        assert!((placed.start - at).abs() < 1e-9, "{jump} @{t}: {placed:?} vs {at}");
                    }
                    None => assert_eq!(placed.how, How::Shifted, "{jump} @{t}"),
                }
            }
        }
    }

    #[test]
    fn the_curve_drops_a_false_match_keeps_a_cut_and_extends_the_drift() {
        // Дрейф 0.04 с/с, на 300..340 с вырезано 40 с, на 102 с — ложное совпадение.
        let line = |t: f64| 1.0 + 0.04 * t - if t >= 340.0 { 40.0 } else { 0.0 };
        let mut anchors: Vec<(f64, f64)> = (0..60).chain(68..128).map(|i| i as f64 * 5.0).map(|t| (t, line(t))).collect();
        anchors.push((102.0, 30.0));
        let curve = Curve::fit(anchors).unwrap();
        for t in [-50.0, 0.0, 102.0, 295.0, 340.0, 345.0, 635.0, 1000.0] {
            assert!((curve.at(t) - line(t)).abs() < 1e-9, "@{t}: {} vs {}", curve.at(t), line(t));
        }
        assert_eq!(Curve::fit(vec![(10.0, 2.0)]).unwrap().at(500.0), 2.0);
    }
}
