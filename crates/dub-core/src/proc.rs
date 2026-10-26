//! Учёт дочерних процессов (ffmpeg, сепаратор, llama-server, whisper). Сервер ставит хук, который
//! относит процесс к выполняемой джобе, чтобы её отмена могла этот процесс убить. Без хука учёт ничего
//! не делает.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

/// Хук учёта: `alive=true` — процесс запущен, `false` — завершён и дождан.
pub type ChildHook = fn(pid: u32, alive: bool);

static HOOK: OnceLock<ChildHook> = OnceLock::new();

/// Поставить хук (один раз на процесс; повторные вызовы игнорируются).
pub fn set_hook(hook: ChildHook) {
    let _ = HOOK.set(hook);
}

/// Перенос привязки к джобе в новый поток: вызывается в порождающем потоке и возвращает то, что в новом
/// потоке привязывает его к той же джобе; результат держится, пока поток работает.
pub type Carry = fn() -> Box<dyn FnOnce() -> Box<dyn std::any::Any> + Send>;

static CARRY: OnceLock<Carry> = OnceLock::new();

/// Поставить перенос привязки (один раз на процесс; повторные вызовы игнорируются).
pub fn set_carry(carry: Carry) {
    let _ = CARRY.set(carry);
}

/// `std::thread::spawn`, после которого процессы, запущенные в новом потоке, учитываются за той же
/// джобой, что и у порождающего потока.
pub fn spawn<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> std::thread::JoinHandle<T> {
    let attach = CARRY.get().map(|carry| carry());
    std::thread::spawn(move || {
        let _attached = attach.map(|attach| attach());
        work()
    })
}

/// Отметка «процесс жив»; снимается при drop. Держать, пока хэндл процесса не закрыт: тогда pid не
/// может быть переиспользован другим процессом.
pub struct ChildGuard {
    pid: u32,
}

pub fn track(pid: u32) -> ChildGuard {
    if let Some(h) = HOOK.get() {
        h(pid, true);
    }
    ChildGuard { pid }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(h) = HOOK.get() {
            h(self.pid, false);
        }
    }
}

/// Ребёнок не переживает студию. На Windows это делает job object сервера (process_group), на Linux —
/// `PR_SET_PDEATHSIG` в порождённом процессе до exec: без него снятая `kill -9` студия оставила бы
/// llama-server или сепаратор держать видеокарту и порт. Вызывать на `Command` прямо перед `spawn`.
pub fn dies_with_parent(cmd: &mut Command) -> &mut Command {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;

        let parent = std::process::id() as libc::pid_t;
        // Флаг живёт в потоке и снимается успешным execve, поэтому ставится в pre_exec; внутри только
        // async-signal-safe вызовы.
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::other("the studio exited while the child was starting"));
                }
                Ok(())
            });
        }
    }
    outside_appimage(cmd)
}

/// Библиотеки, лежащие рядом с программой, видны её загрузчику: на Linux каталог встаёт первым в
/// LD_LIBRARY_PATH запуска (сборки сайдкаров не несут RUNPATH на свой каталог), на macOS — в DYLD_LIBRARY_PATH
/// (их @rpath указывает на каталог CI); Windows и так ищет DLL в каталоге программы и cwd.
pub fn libraries_beside<'a>(cmd: &'a mut Command, dir: &std::path::Path) -> &'a mut Command {
    #[cfg(not(windows))]
    {
        let var = if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
        let mut paths = vec![dir.to_path_buf()];
        if let Some(cur) = std::env::var_os(var) {
            paths.extend(std::env::split_paths(&cur));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env(var, joined);
        }
        outside_appimage(cmd);
    }
    #[cfg(windows)]
    let _ = dir;
    cmd
}

/// What the library path of a program becomes when the studio runs from an AppImage.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
enum LibraryPath {
    /// The program lives in the image, or nothing of the image is on the path.
    Keep,
    /// Only the image's folders were on the path.
    Remove,
    Set(std::ffi::OsString),
}

/// The library path `current` (`:`-separated) for `program` when the studio runs from the AppImage mounted at `appdir`:
/// a program outside the image loses the image's folders, one inside keeps them.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn library_path_outside(appdir: &std::path::Path, program: &std::path::Path, current: &std::ffi::OsStr) -> LibraryPath {
    if program.starts_with(appdir) {
        return LibraryPath::Keep;
    }
    let bytes = current.as_encoded_bytes();
    let parts: Vec<&[u8]> = bytes.split(|b| *b == b':').collect();
    // SAFETY: each part is a piece of an OsStr's encoded bytes cut at an ASCII byte.
    let kept: Vec<&[u8]> = parts
        .iter()
        .copied()
        .filter(|part| !std::path::Path::new(unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(part) }).starts_with(appdir))
        .collect();
    if kept.len() == parts.len() {
        LibraryPath::Keep
    } else if kept.iter().all(|part| part.is_empty()) {
        LibraryPath::Remove
    } else {
        let joined = kept.join(&b':');
        // SAFETY: the pieces of an OsStr's encoded bytes joined with an ASCII byte.
        LibraryPath::Set(unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(&joined) }.to_os_string())
    }
}

/// The program a command runs: its path, or the first match of its bare name on the command's PATH.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn program_of(cmd: &Command) -> std::path::PathBuf {
    let program = std::path::Path::new(cmd.get_program());
    if program.components().count() > 1 {
        return program.to_path_buf();
    }
    let path = cmd
        .get_envs()
        .find(|(k, _)| *k == "PATH")
        .map(|(_, v)| v.map(|v| v.to_os_string()))
        .unwrap_or_else(|| std::env::var_os("PATH"));
    path.and_then(|paths| std::env::split_paths(&paths).map(|dir| dir.join(program)).find(|p| p.is_file()))
        .unwrap_or_else(|| program.to_path_buf())
}

/// Inside an AppImage the studio's LD_LIBRARY_PATH starts with the image's own libraries ($APPDIR/usr/lib). A program
/// of the computer (the system's ffmpeg, yt-dlp and what it starts, the studio's downloaded engines) loading them fails
/// on systems newer than the image's (the image's libva: «undefined symbol vaMapBuffer2»), so a program outside the
/// image gets the library path without the image's folders. Outside an AppImage, and off Linux, nothing changes.
pub fn outside_appimage(cmd: &mut Command) -> &mut Command {
    #[cfg(target_os = "linux")]
    if let Some(appdir) = std::env::var_os("APPDIR").filter(|d| !d.is_empty()) {
        let current = match cmd.get_envs().find(|(k, _)| *k == "LD_LIBRARY_PATH") {
            Some((_, Some(v))) => Some(v.to_os_string()),
            Some((_, None)) => None,
            None => std::env::var_os("LD_LIBRARY_PATH"),
        };
        if let Some(current) = current {
            match library_path_outside(std::path::Path::new(&appdir), &program_of(cmd), &current) {
                LibraryPath::Keep => {}
                LibraryPath::Remove => {
                    cmd.env_remove("LD_LIBRARY_PATH");
                }
                LibraryPath::Set(v) => {
                    cmd.env("LD_LIBRARY_PATH", v);
                }
            }
        }
    }
    cmd
}

/// A command for a program of the computer or a downloaded one, which does not load the AppImage's libraries
/// ([`outside_appimage`]).
pub fn host_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    outside_appimage(&mut cmd);
    cmd
}

/// ffmpeg первым из PATH процесса (в его начале каталог скачанного студией), иначе по имени: голое имя Windows ищет
/// в каталоге приложения и системных каталогах раньше PATH.
pub fn ffmpeg() -> std::path::PathBuf {
    tool("ffmpeg")
}

/// ffprobe первым из PATH процесса, как [`ffmpeg`].
pub fn ffprobe() -> std::path::PathBuf {
    tool("ffprobe")
}

fn tool(name: &str) -> std::path::PathBuf {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    std::env::var_os("PATH")
        .and_then(|paths| std::env::split_paths(&paths).map(|dir| dir.join(&file)).find(|path| path.is_file()))
        .unwrap_or_else(|| std::path::PathBuf::from(file))
}

/// Аналог `Command::output()` с учётом процесса: stdin закрыт, stdout/stderr читаются целиком.
pub fn output(cmd: &mut Command) -> std::io::Result<Output> {
    dies_with_parent(cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let guard = track(child.id());
    let out_pipe = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = out_pipe {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let mut stderr = Vec::new();
    if let Some(mut p) = child.stderr.take() {
        let _ = p.read_to_end(&mut stderr);
    }
    let status = child.wait();
    drop(guard);
    let stdout = reader.join().unwrap_or_default();
    Ok(Output { status: status?, stdout, stderr })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    const APPDIR: &str = "/tmp/.mount_DubStuX1";

    #[test]
    fn a_program_of_the_computer_loses_the_images_libraries() {
        let current = OsStr::new("/tmp/.mount_DubStuX1/usr/lib:/home/u/Dub/models/higgs-engine:/tmp/.mount_DubStuX1/usr/lib/x86_64-linux-gnu");
        assert_eq!(
            library_path_outside(Path::new(APPDIR), Path::new("/usr/bin/ffmpeg"), current),
            LibraryPath::Set("/home/u/Dub/models/higgs-engine".into())
        );
    }

    #[test]
    fn a_path_of_the_image_alone_is_removed() {
        assert_eq!(library_path_outside(Path::new(APPDIR), Path::new("/usr/bin/xdg-open"), OsStr::new("/tmp/.mount_DubStuX1/usr/lib")), LibraryPath::Remove);
    }

    #[test]
    fn a_program_inside_the_image_keeps_them() {
        let current = OsStr::new("/tmp/.mount_DubStuX1/usr/lib");
        assert_eq!(library_path_outside(Path::new(APPDIR), Path::new("/tmp/.mount_DubStuX1/usr/bin/helper"), current), LibraryPath::Keep);
    }

    #[test]
    fn a_path_without_the_image_is_left_as_it_is() {
        let current = OsStr::new("/home/u/Dub/models/higgs-engine:/usr/local/cuda/lib64");
        assert_eq!(library_path_outside(Path::new(APPDIR), Path::new("/usr/bin/ffmpeg"), current), LibraryPath::Keep);
        assert_eq!(
            library_path_outside(Path::new(APPDIR), Path::new("/usr/bin/ffmpeg"), OsStr::new("/tmp/.mount_DubStuX1-other/lib")),
            LibraryPath::Keep,
            "a folder that only begins with the image's name is not the image's"
        );
    }

    /// The studio's environment as an AppImage gives it: the command of a system program gets the library path
    /// without the image, and the image's own program keeps it.
    #[cfg(target_os = "linux")]
    #[test]
    fn under_an_appimage_a_system_program_starts_without_its_libraries() {
        std::env::set_var("APPDIR", APPDIR);
        std::env::set_var("LD_LIBRARY_PATH", "/tmp/.mount_DubStuX1/usr/lib:/opt/cuda/lib64");
        let ffmpeg = host_command("/usr/bin/ffmpeg");
        let set = ffmpeg.get_envs().find(|(k, _)| *k == "LD_LIBRARY_PATH").and_then(|(_, v)| v.map(|v| v.to_os_string()));
        assert_eq!(set.as_deref(), Some(OsStr::new("/opt/cuda/lib64")));
        let mut beside = Command::new("/home/u/Dub/tools/llama/llama-server");
        libraries_beside(&mut beside, Path::new("/home/u/Dub/tools/llama"));
        let set = beside.get_envs().find(|(k, _)| *k == "LD_LIBRARY_PATH").and_then(|(_, v)| v.map(|v| v.to_os_string()));
        assert_eq!(set.as_deref(), Some(OsStr::new("/home/u/Dub/tools/llama:/opt/cuda/lib64")));
        let own = host_command("/tmp/.mount_DubStuX1/usr/bin/helper");
        assert!(own.get_envs().all(|(k, _)| k != "LD_LIBRARY_PATH"), "a program of the image inherits the path as it is");
        let out = output(&mut host_command("/bin/sh").args(["-c", "printf %s \"$LD_LIBRARY_PATH\""])).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "/opt/cuda/lib64");
    }

    #[test]
    fn a_bare_name_is_found_on_the_commands_path() {
        let dir = std::env::temp_dir().join("dub-core-program-of");
        std::fs::create_dir_all(&dir).unwrap();
        let name = if cfg!(windows) { "tool-here.exe" } else { "tool-here" };
        std::fs::write(dir.join(name), b"").unwrap();
        let mut cmd = Command::new(name);
        cmd.env("PATH", &dir);
        assert_eq!(program_of(&cmd), dir.join(name));
        assert_eq!(program_of(&Command::new("/usr/bin/ffmpeg")), Path::new("/usr/bin/ffmpeg"));
    }
}
