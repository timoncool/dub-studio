//! Атомарная запись артефактов: временный файл рядом с целевым, затем rename. Временное имя никогда
//! не совпадает с целевым, поэтому кэш «по существованию файла» недописанный файл не видит.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Маркер временного файла в имени: `<stem>.dubtmp-<pid>-<n>.<ext>`.
pub const TMP_MARK: &str = ".dubtmp-";

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Временный путь рядом с `path`. Расширение сохраняется: ffmpeg выбирает формат по нему.
pub fn tmp_path(path: &Path) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let pid = std::process::id();
    let name = match path.extension() {
        Some(ext) => format!("{stem}{TMP_MARK}{pid}-{n}.{}", ext.to_string_lossy()),
        None => format!("{stem}{TMP_MARK}{pid}-{n}"),
    };
    path.with_file_name(name)
}

/// Временный ли это файл атомарной записи.
pub fn is_tmp(path: &Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().contains(TMP_MARK))
        .unwrap_or(false)
}

/// Pid процесса, создавшего временный файл (из имени).
fn tmp_owner_pid(name: &str) -> Option<u32> {
    let rest = &name[name.find(TMP_MARK)? + TMP_MARK.len()..];
    rest.split('-').next()?.parse().ok()
}

/// Произвести файл через `produce(tmp)` и переименовать в `path`. При ошибке временный файл удаляется.
pub fn write_with(
    path: &Path,
    produce: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let tmp = tmp_path(path);
    if let Err(e) = produce(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename {} -> {}: {e}", tmp.display(), path.display())
    })
}

/// Записать байты атомарно.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_with(path, |tmp| {
        std::fs::write(tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))
    })
}

/// Скопировать файл атомарно.
pub fn copy(src: &Path, dst: &Path) -> Result<(), String> {
    write_with(dst, |tmp| {
        std::fs::copy(src, tmp)
            .map(|_| ())
            .map_err(|e| format!("copy {} -> {}: {e}", src.display(), tmp.display()))
    })
}

/// Удалить в `dir` (без рекурсии) временные файлы, оставленные ДРУГИМ процессом (упавшим прошлым
/// запуском). Свои не трогаем: их может прямо сейчас дописывать соседний запрос. Возвращает число
/// удалённых.
pub fn cleanup_stale(dir: &Path) -> usize {
    let me = std::process::id();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.contains(TMP_MARK) {
            continue;
        }
        if tmp_owner_pid(&name) == Some(me) {
            continue;
        }
        let p = e.path();
        let removed = if p.is_dir() {
            std::fs::remove_dir_all(&p).is_ok()
        } else {
            std::fs::remove_file(&p).is_ok()
        };
        if removed {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dubcore_atomic_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn tmp_keeps_extension_and_never_equals_target() {
        let p = Path::new("/w/seg_s1.wav");
        let t = tmp_path(p);
        assert_eq!(t.extension().unwrap(), "wav");
        assert_ne!(t, p);
        assert!(is_tmp(&t));
        assert!(!is_tmp(p));
    }

    #[test]
    fn failed_producer_leaves_no_target_and_no_tmp() {
        let d = tmp_dir("fail");
        let target = d.join("out.wav");
        let r = write_with(&target, |tmp| {
            std::fs::write(tmp, b"half").unwrap();
            Err("обрыв".into())
        });
        assert!(r.is_err());
        assert!(!target.exists());
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn write_replaces_existing() {
        let d = tmp_dir("replace");
        let target = d.join("a.json");
        write(&target, b"1").unwrap();
        write(&target, b"22").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"22");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cleanup_removes_only_foreign_tmp() {
        let d = tmp_dir("cleanup");
        let foreign = d.join(format!("seg_s1{TMP_MARK}0-0.wav"));
        let own = tmp_path(&d.join("seg_s2.wav"));
        let keep = d.join("seg_s3.wav");
        for p in [&foreign, &own, &keep] {
            std::fs::write(p, b"x").unwrap();
        }
        assert_eq!(cleanup_stale(&d), 1);
        assert!(!foreign.exists());
        assert!(own.exists());
        assert!(keep.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
