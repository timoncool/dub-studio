//! App-библиотека кастингов (#115): профили персонажей, переносимые МЕЖДУ роликами. Хранение — на уровне
//! ПРИЛОЖЕНИЯ, а не в workspace проекта: `<repo_root>/casting_library/<slug>/`.
//!
//! Каждый профиль:
//!   casting.json          — Character[] (face+voice эмбеддинги, имена, голоса, заметки);
//!   avatars/char_<i>.jpg  — аватарки персонажей (для карточек в UI);
//!   voices/char_<i>.wav   — образцы голоса (проигрывание);
//!   meta.json             — {name, created, char_count}.
//!
//! Применение к другому ролику: analyze получает casting_ref=<slug> -> casting-стадия грузит профиль как
//! prev и матчит новые кластеры по face+voice similarity (переносит имена/голоса/заметки, см. casting.rs).
//!
//! Date::now() НЕ используем (в проекте запрещён недетерминизм по времени): `created` принимаем из тела
//! запроса на сохранение (фронт шлёт ISO-строку) или оставляем пусто.

use std::path::{Path, PathBuf};

/// Корень библиотеки: <repo_root>/casting_library (sibling к workspace). Создаётся лениво при сохранении.
pub fn library_root(repo_root: &Path) -> PathBuf {
    repo_root.join("casting_library")
}

/// Каталог одного профиля по slug (без валидации существования).
pub fn profile_dir(repo_root: &Path, slug: &str) -> PathBuf {
    library_root(repo_root).join(slug)
}

/// Метаданные профиля (meta.json).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProfileMeta {
    #[serde(default)]
    pub name: String,
    /// ISO-строка из тела запроса (не Date::now — недетерминизм запрещён). Пусто = не задано.
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub char_count: usize,
}

/// Транслит + kebab-case slug из отображаемого имени. Кириллица -> латиница (простая таблица), прочее ->
/// '-'; схлопываем повторы дефисов, тримим по краям. Пустой результат -> "casting".
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for ch in name.trim().to_lowercase().chars() {
        if let Some(tr) = translit_char(ch) {
            out.push_str(tr);
        } else if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            // любой разделитель/символ -> дефис (схлопнем ниже).
            if !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    let s = out.trim_matches('-').to_string();
    if s.is_empty() { "casting".to_string() } else { s }
}

/// Уникализировать slug в пределах библиотеки: если каталог занят — добавляем -2, -3, …
pub fn unique_slug(repo_root: &Path, base: &str) -> String {
    let root = library_root(repo_root);
    if !profile_dir_exists(&root, base) {
        return base.to_string();
    }
    for n in 2..10_000 {
        let cand = format!("{base}-{n}");
        if !profile_dir_exists(&root, &cand) {
            return cand;
        }
    }
    // крайне маловероятно; детерминированный запас.
    format!("{base}-x")
}

fn profile_dir_exists(root: &Path, slug: &str) -> bool {
    root.join(slug).is_dir()
}

/// Простая транслитерация кириллицы для slug (не обратимая — только для URL-safe имени каталога).
fn translit_char(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a", 'б' => "b", 'в' => "v", 'г' => "g", 'д' => "d", 'е' => "e", 'ё' => "e",
        'ж' => "zh", 'з' => "z", 'и' => "i", 'й' => "y", 'к' => "k", 'л' => "l", 'м' => "m",
        'н' => "n", 'о' => "o", 'п' => "p", 'р' => "r", 'с' => "s", 'т' => "t", 'у' => "u",
        'ф' => "f", 'х' => "h", 'ц' => "ts", 'ч' => "ch", 'ш' => "sh", 'щ' => "sch", 'ъ' => "",
        'ы' => "y", 'ь' => "", 'э' => "e", 'ю' => "yu", 'я' => "ya",
        _ => return None,
    })
}

/// Список профилей библиотеки: (slug, meta). Каталоги без валидного casting.json пропускаем. Порядок —
/// по slug (детерминированно).
pub fn list_profiles(repo_root: &Path) -> Vec<(String, ProfileMeta)> {
    let root = library_root(repo_root);
    let mut out: Vec<(String, ProfileMeta)> = Vec::new();
    let Ok(rd) = std::fs::read_dir(&root) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let Some(slug) = p.file_name().and_then(|s| s.to_str()) else { continue };
        if !p.join("casting.json").is_file() {
            continue; // не профиль
        }
        let meta = read_meta(repo_root, slug);
        out.push((slug.to_string(), meta));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Прочитать meta.json профиля; отсутствует/битый -> дефолт с именем = slug и char_count из casting.json.
pub fn read_meta(repo_root: &Path, slug: &str) -> ProfileMeta {
    let dir = profile_dir(repo_root, slug);
    if let Ok(text) = std::fs::read_to_string(dir.join("meta.json")) {
        if let Ok(m) = serde_json::from_str::<ProfileMeta>(&text) {
            return m;
        }
    }
    // фолбэк: имя = slug, счётчик из casting.json.
    let char_count = dub_faces::load_casting(&dir.join("casting.json"))
        .map(|c| c.characters.len())
        .unwrap_or(0);
    ProfileMeta { name: slug.to_string(), created: String::new(), char_count }
}

/// Сохранить текущий casting проекта в библиотеку под slug: копируем casting.json + аватарки + образцы
/// голоса из workspace/<pid>/casting в casting_library/<slug>/. `display_name`/`created` -> meta.json.
/// Возвращает число персонажей (для meta/ответа). Проект без casting.json -> Err.
pub fn save_profile(
    repo_root: &Path,
    proj_dir: &Path,
    slug: &str,
    display_name: &str,
    created: &str,
) -> Result<usize, String> {
    let src_casting = proj_dir.join("casting.json");
    let casting = dub_faces::load_casting(&src_casting)
        .ok_or_else(|| t!("casting-no-casting-json"))?;
    let dir = profile_dir(repo_root, slug);
    let avatars = dir.join("avatars");
    let voices = dir.join("voices");
    std::fs::create_dir_all(&avatars).map_err(|e| format!("mkdir avatars: {e}"))?;
    std::fs::create_dir_all(&voices).map_err(|e| format!("mkdir voices: {e}"))?;

    // casting.json целиком (эмбеддинги нужны для матчинга при применении к другому ролику).
    dub_faces::save_casting(&dir.join("casting.json"), &casting).map_err(|e| crate::localize::Localize::localize(&e))?;

    // Имя файла в профиле = id персонажа (эндпоинты библиотеки резолвят avatars/<id>.jpg по id, НЕ по позиции
    // в списке). Источник — stored-путь персонажа (sample_frame/voice_sample), тот же, что отдают per-project
    // /casting/avatar и /casting/voice. Так копия устойчива к дыркам в char_<i> (фантом-фильтр пропускает
    // бит-парты -> id непрерывны не всегда) и к смене id при кросс-матче. Байты как есть (нативный JPG/WAV).
    for ch in casting.characters.iter() {
        if ch.id.is_empty() || ch.id.contains("..") || ch.id.contains('/') || ch.id.contains('\\') {
            continue; // защита от traversal в имени файла
        }
        if !ch.sample_frame.is_empty() && !ch.sample_frame.contains("..") {
            let av_src = proj_dir.join(&ch.sample_frame);
            if av_src.is_file() {
                let _ = std::fs::copy(&av_src, avatars.join(format!("{}.jpg", ch.id)));
            }
        }
        if !ch.voice_sample.is_empty() && !ch.voice_sample.contains("..") {
            let vo_src = proj_dir.join(&ch.voice_sample);
            if vo_src.is_file() {
                let _ = std::fs::copy(&vo_src, voices.join(format!("{}_voice.wav", ch.id)));
            }
        }
    }

    let meta = ProfileMeta {
        name: display_name.to_string(),
        created: created.to_string(),
        char_count: casting.characters.len(),
    };
    write_meta(&dir, &meta)?;
    Ok(casting.characters.len())
}

fn write_meta(dir: &Path, meta: &ProfileMeta) -> Result<(), String> {
    let json = serde_json::to_string_pretty(meta).map_err(|e| format!("meta serialize: {e}"))?;
    std::fs::write(dir.join("meta.json"), json).map_err(|e| format!("meta write: {e}"))
}

/// Удалить профиль (весь каталог). Нет каталога -> Ok (идемпотентно).
pub fn delete_profile(repo_root: &Path, slug: &str) -> Result<(), String> {
    let dir = profile_dir(repo_root, slug);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| t!("casting-profile-delete-failed", slug = slug.to_string(), error = e.to_string()))
}

/// Валидный slug для ФС-доступа (защита от traversal): непусто, только [a-z0-9-].
pub fn is_safe_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.contains("..")
}

/// Загрузить casting.json профиля библиотеки (для применения как prev при analyze). None если нет/битый.
/// Тот же slug-guard (is_safe_slug), что и на удаление/аватар — единый барьер от traversal по внешнему
/// casting_ref (query-параметр без валидации на входе).
pub fn load_profile_casting(repo_root: &Path, slug: &str) -> Option<dub_faces::Casting> {
    if !is_safe_slug(slug) {
        return None;
    }
    dub_faces::load_casting(&profile_dir(repo_root, slug).join("casting.json"))
}

/// Файл глоссария профиля сериала: {"entries": [...]}.
const GLOSSARY_FILE: &str = "glossary.json";

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SeriesGlossary {
    #[serde(default)]
    entries: Vec<dub_core::GlossaryEntry>,
}

/// Каталог существующего профиля: безопасный slug и casting.json на месте.
fn existing_profile(repo_root: &Path, slug: &str) -> Option<PathBuf> {
    let dir = profile_dir(repo_root, slug);
    (is_safe_slug(slug) && dir.join("casting.json").is_file()).then_some(dir)
}

/// Глоссарий профиля сериала. None — такого профиля нет; Err — glossary.json есть, но не читается.
/// Профиль без glossary.json — пустой глоссарий.
pub fn read_glossary(repo_root: &Path, slug: &str) -> Option<Result<Vec<dub_core::GlossaryEntry>, String>> {
    let dir = existing_profile(repo_root, slug)?;
    let path = dir.join(GLOSSARY_FILE);
    if !path.is_file() {
        return Some(Ok(Vec::new()));
    }
    Some(
        std::fs::read_to_string(&path)
            .map_err(|e| format!("{}: {e}", path.display()))
            .and_then(|text| serde_json::from_str::<SeriesGlossary>(&text).map_err(|e| format!("{}: {e}", path.display())))
            .map(|g| g.entries),
    )
}

/// Записать глоссарий профиля (атомарно). Err — профиля нет или запись не удалась.
pub fn write_glossary(repo_root: &Path, slug: &str, entries: &[dub_core::GlossaryEntry]) -> Result<(), String> {
    let dir = existing_profile(repo_root, slug).ok_or_else(|| t!("casting-profile-not-found", slug = slug.to_string()))?;
    let json = serde_json::to_string_pretty(&SeriesGlossary { entries: entries.to_vec() }).map_err(|e| e.to_string())?;
    dub_core::atomic::write(&dir.join(GLOSSARY_FILE), json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_series_glossary_lives_beside_its_profile() {
        let _language = crate::i18n::test_language("ru");
        let repo = std::env::temp_dir().join(format!("dublib_gloss_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        assert!(read_glossary(&repo, "show").is_none(), "no profile");
        assert!(write_glossary(&repo, "show", &[]).unwrap_err().contains("не найден"));
        let dir = profile_dir(&repo, "show");
        std::fs::create_dir_all(&dir).unwrap();
        dub_faces::save_casting(&dir.join("casting.json"), &dub_faces::Casting::default()).unwrap();
        assert_eq!(read_glossary(&repo, "show").unwrap().unwrap(), vec![]);
        let e = dub_core::GlossaryEntry { term: "Hogwarts".into(), translation: "Хогвартс".into(), ..Default::default() };
        write_glossary(&repo, "show", std::slice::from_ref(&e)).unwrap();
        assert_eq!(read_glossary(&repo, "show").unwrap().unwrap(), vec![e]);
        std::fs::write(dir.join(GLOSSARY_FILE), "{broken").unwrap();
        assert!(read_glossary(&repo, "show").unwrap().is_err());
        assert!(read_glossary(&repo, "../show").is_none());
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn slugify_translit_and_kebab() {
        assert_eq!(slugify("Ходячие мертвецы"), "hodyachie-mertvetsy");
        assert_eq!(slugify("Season 1 / Ep. 2"), "season-1-ep-2");
        assert_eq!(slugify("  Босс!!!  "), "boss");
        assert_eq!(slugify(""), "casting");
        assert_eq!(slugify("---"), "casting");
    }

    #[test]
    fn safe_slug_rejects_traversal() {
        assert!(is_safe_slug("hodyachie-mertvetsy"));
        assert!(is_safe_slug("season-1"));
        assert!(!is_safe_slug(""));
        assert!(!is_safe_slug("../etc"));
        assert!(!is_safe_slug("Foo/Bar"));
        assert!(!is_safe_slug("UPPER"));
        assert!(!is_safe_slug("a b"));
    }

    #[test]
    fn unique_slug_increments() {
        let dir = std::env::temp_dir().join(format!("dublib_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = library_root(&dir);
        std::fs::create_dir_all(root.join("hero")).unwrap();
        assert_eq!(unique_slug(&dir, "hero"), "hero-2");
        std::fs::create_dir_all(root.join("hero-2")).unwrap();
        assert_eq!(unique_slug(&dir, "hero"), "hero-3");
        assert_eq!(unique_slug(&dir, "fresh"), "fresh");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_list_delete_roundtrip() {
        let repo = std::env::temp_dir().join(format!("dublib_rt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        let proj = repo.join("workspace").join("pid0");
        std::fs::create_dir_all(proj.join("casting")).unwrap();
        // минимальный casting.json проекта.
        let mut casting = dub_faces::Casting::default();
        casting.characters.push(dub_faces::Character { id: "char_0".into(), name: "Босс".into(), ..Default::default() });
        dub_faces::save_casting(&proj.join("casting.json"), &casting).unwrap();

        let slug = unique_slug(&repo, &slugify("Мой сериал"));
        let n = save_profile(&repo, &proj, &slug, "Мой сериал", "2026-07-18").unwrap();
        assert_eq!(n, 1);
        let profiles = list_profiles(&repo);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].0, "moy-serial");
        assert_eq!(profiles[0].1.name, "Мой сериал");
        assert_eq!(profiles[0].1.char_count, 1);
        // применение: casting.json профиля читается.
        assert!(load_profile_casting(&repo, &slug).is_some());
        // удаление идемпотентно.
        delete_profile(&repo, &slug).unwrap();
        assert!(list_profiles(&repo).is_empty());
        delete_profile(&repo, &slug).unwrap(); // повтор — не ошибка
        let _ = std::fs::remove_dir_all(&repo);
    }
}
