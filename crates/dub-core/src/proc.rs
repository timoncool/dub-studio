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
    cmd
}

/// Библиотеки, лежащие рядом с программой, видны её загрузчику: на Linux каталог встаёт первым в
/// LD_LIBRARY_PATH запуска (сборки сайдкаров не несут RUNPATH на свой каталог); Windows и так ищет DLL в
/// каталоге программы и cwd.
pub fn libraries_beside<'a>(cmd: &'a mut Command, dir: &std::path::Path) -> &'a mut Command {
    #[cfg(not(windows))]
    {
        let mut paths = vec![dir.to_path_buf()];
        if let Some(cur) = std::env::var_os("LD_LIBRARY_PATH") {
            paths.extend(std::env::split_paths(&cur));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("LD_LIBRARY_PATH", joined);
        }
    }
    #[cfg(windows)]
    let _ = dir;
    cmd
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
