//! Tauri-оболочка Dub Studio. Ничего тяжёлого сама не делает: поднимает нативный `dub-server`
//! (axum, тот же REST/SSE-контракт, что бэкенд-питон) на 127.0.0.1:8793 (`DUB_STUDIO_PORT`) и
//! открывает окно на этот URL. Сервер сам раздаёт SPA (frontend/dist) и API на одном origin — фронт
//! работает с относительными путями без правок, а постоянный порт держит origin, и с ним
//! localStorage окна, одинаковым между запусками. Если на порту уже отвечает Dub Studio, второй
//! сервис не поднимается, а окно открывается на неё; второй запуск релизной сборки на порту по
//! умолчанию отдаёт фокус уже открытому окну.
//!
//! Где лежат ресурсы, данные, профиль WebView2 и временные файлы — решает модуль `layout`.

mod layout;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use dub_server::service::{self, Claim};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Ждать, пока встроенный сервер ответит на /health, или его поток не сообщит, что остановился.
fn wait_for_own_service(port: u16, stopped: &Receiver<Result<(), String>>) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if service::wait_until_serving(port, Duration::from_millis(500)) {
            return Ok(());
        }
        match stopped.try_recv() {
            Ok(Err(e)) => return Err(format!("Встроенный сервер Dub Studio не запустился: {e}")),
            Ok(Ok(())) => return Err("Встроенный сервер Dub Studio остановился сразу после запуска.".into()),
            Err(TryRecvError::Disconnected) => {
                return Err("Поток встроенного сервера Dub Studio аварийно завершился при запуске.".into())
            }
            Err(TryRecvError::Empty) => {}
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Встроенный сервер Dub Studio не ответил на 127.0.0.1:{port} за 30 с."
            ));
        }
    }
}

/// Окна ещё нет, консоль скрыта: единственный способ сказать пользователю, почему студия не
/// открылась, — нативный диалог.
fn fatal(message: &str) {
    eprintln!("[ERROR] {message}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Dub Studio")
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// tauri-plugin-single-instance называет мьютекс и окно только по identifier, общему у установленной,
/// портативной и дев-сборок: с ним дев-копия рядом с открытой установленной молча выходила бы, подняв
/// старое окно. Замок нужен только релизной сборке на порту по умолчанию; дев-сборка и копия с явным
/// `DUB_STUDIO_PORT` живут на своём порту, а на том же — открывают окно на уже работающий сервис.
fn single_instance_wanted() -> bool {
    !cfg!(debug_assertions) && !service::port_is_explicit()
}

/// Поднять встроенный сервис на порту или найти уже работающий. Err — текст для пользователя.
fn start_or_reuse_service(repo_root: &std::path::Path) -> Result<u16, String> {
    let port = service::listen_port()?;
    match service::claim_port(port, Duration::from_secs(20)).map_err(|busy| busy.to_string())? {
        Claim::Bound(listener) => {
            let root = repo_root.to_path_buf();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("dub-server".into())
                .spawn(move || {
                    let result = dub_server::serve_blocking(&root, listener).map_err(|e| format!("{e:#}"));
                    if let Err(e) = &result {
                        eprintln!("[ERROR] встроенный dub-server остановился: {e}");
                    }
                    let _ = tx.send(result);
                })
                .map_err(|e| format!("Не удалось запустить поток сервера Dub Studio: {e}"))?;
            wait_for_own_service(port, &rx)?;
        }
        Claim::AlreadyRunning(running) => {
            eprintln!(
                "на 127.0.0.1:{port} уже работает Dub Studio {} ({}): окно откроется на неё",
                running.version, running.service_executable
            );
        }
    }
    Ok(port)
}

/// Окружение сервера до старта: путь к onnxruntime 1.28 (PATH под движок/инструменты добавит
/// augment_path_for_tools внутри serve_blocking).
fn setup_server_env(repo_root: &PathBuf) {
    dub_server::set_ort_dylib_env(repo_root, &layout::executable_directory());
}

/// Проверка обновления на GitHub-релизе и (по согласию юзера) установка. Драйвится из Rust: фронт
/// грузится с внешнего http-URL встроенного сервера, где Tauri JS-IPC ненадёжен, а Rust-апдейтер
/// работает независимо от webview. Тихо выходит при отсутствии апдейта/сети. На лету ставится только
/// копия из NSIS-установщика; портатив (нельзя перезаписать запущенный ~489-МБ каталог), MSI
/// (msiexec не принимает /D=, а Program Files без повышения прав недоступен) и сборка без типа
/// бандла получают предложение открыть страницу релиза.
const RELEASES_URL: &str = "https://github.com/timoncool/dub-studio/releases/latest";
fn spawn_update_check(app: tauri::AppHandle, portable: bool) {
    use tauri::utils::config::BundleType;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    use tauri_plugin_updater::UpdaterExt;
    let install_in_place =
        !portable && tauri::utils::platform::bundle_type() == Some(BundleType::Nsis);
    tauri::async_runtime::spawn(async move {
        // Прокси из настроек: GitHub, откуда обновления, — среди сайтов, ради которых прокси и ставят.
        let builder = match dub_server::net::fixed() {
            dub_server::net::Fixed::System => app.updater_builder(),
            dub_server::net::Fixed::Direct => app.updater_builder().no_proxy(),
            dub_server::net::Fixed::Through(proxy) => app.updater_builder().proxy(proxy),
        };
        // Установщик — ребёнок этого процесса, а процесс сидит в своём job с kill-on-close: без
        // освобождения установщик умер бы вместе со студией, ничего не поставив. Свой хук заменяет
        // штатный, поэтому cleanup_before_exit вызывается здесь же.
        let cleanup = app.clone();
        let mut builder = builder.on_before_exit(move || {
            cleanup.cleanup_before_exit();
            dub_server::process_group::terminate_group_members();
            if !dub_server::process_group::release_children() {
                eprintln!("[ERROR] установщик обновления не выведен из job object студии");
            }
        });
        if install_in_place {
            // Без /D= установщик, запущенный из студии, ставит копию в папку по умолчанию, а не в
            // текущую; NSIS требует его последним аргументом и без кавычек.
            builder = builder.installer_arg(format!("/D={}", layout::executable_directory().display()));
        }
        let updater = match builder.build() {
            Ok(u) => u,
            Err(_) => return,
        };
        let update = match updater.check().await {
            Ok(Some(u)) => u,
            _ => return, // нет апдейта или ошибка сети -> тихо
        };
        let ver = update.version.clone();
        if !install_in_place {
            let open = app
                .dialog()
                .message(format!(
                    "Доступна новая версия {ver}. Открыть страницу загрузки?"
                ))
                .title("Обновление Dub Studio")
                .kind(MessageDialogKind::Info)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Открыть".into(),
                    "Позже".into(),
                ))
                .blocking_show();
            if open {
                // Браузер, запущенный изнутри job студии, закрылся бы вместе с ней.
                let opened = dub_server::process_group::detach_from_group(
                    &mut std::process::Command::new(if cfg!(windows) { "explorer" } else { "xdg-open" }),
                )
                .arg(RELEASES_URL)
                .spawn();
                if let Err(e) = opened {
                    app.dialog()
                        .message(format!("Не удалось открыть {RELEASES_URL}: {e}"))
                        .title("Обновление Dub Studio")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                }
            }
            return;
        }
        let yes = app
            .dialog()
            .message(format!(
                "Доступна новая версия {ver}. Обновить сейчас? Приложение перезапустится."
            ))
            .title("Обновление Dub Studio")
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Обновить".into(),
                "Позже".into(),
            ))
            .blocking_show();
        if !yes {
            return;
        }
        match update.download_and_install(|_, _| {}, || {}).await {
            Ok(_) => {
                app.restart(); // Windows: инсталлятор сам закроет приложение; на прочих ОС перезапустим
            }
            Err(e) => {
                app.dialog()
                    .message(format!("Не удалось обновить: {e}"))
                    .title("Обновление Dub Studio")
                    .kind(MessageDialogKind::Error)
                    .blocking_show();
            }
        }
    });
}

/// Спрятать ОКНО консоли. Exe — console-subsystem (чтобы дети ffmpeg/llama/roformer наследовали ОДНУ
/// консоль и НЕ плодили своих окон на каждый спавн). Сама консоль остаётся выделенной (дети её наследуют),
/// прячем только её ОКНО -> исчезает чёрное окно и его пустая запись в ALT+TAB/панели задач; в переключателе
/// остаётся лишь GUI-окно (с иконкой, см. .icon() ниже). Дети по-прежнему не открывают окон.
#[cfg(windows)]
fn hide_console_window() {
    extern "system" {
        fn GetConsoleWindow() -> isize;
        fn ShowWindow(hwnd: isize, n_cmd_show: i32) -> i32;
        fn GetConsoleProcessList(lpdw_process_list: *mut u32, dw_process_count: u32) -> u32;
    }
    unsafe {
        // Прячем ТОЛЬКО собственную консоль. Если exe запущен ИЗ существующего терминала, наш процесс
        // делит его консоль (к ней привязано >1 процесса) — это ЧУЖОЕ окно терминала пользователя, трогать
        // нельзя. При двойном клике из Проводника загрузчик создаёт нам отдельную консоль (count==1).
        let mut pids = [0u32; 4];
        let n = GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32);
        if n != 1 {
            return;
        }
        let hwnd = GetConsoleWindow();
        if hwnd != 0 {
            ShowWindow(hwnd, 0); // SW_HIDE
        }
    }
}

pub fn run() {
    // Что бы ни завершило процесс — окно, диспетчер задач, taskkill, падение, — llama-server,
    // bs_roformer-cli, whisper и ffmpeg уходят вместе с ним.
    if !dub_server::process_group::bind_children_to_this_process() {
        eprintln!("[ERROR] процесс не встал в свой job object: сайдкары гасятся только своими деструкторами");
    }
    #[cfg(windows)]
    hide_console_window();
    let placed = match layout::resolve().and_then(|l| layout::apply_environment(&l).map(|_| l)) {
        Ok(l) => l,
        Err(e) => layout::fatal(&e),
    };
    let repo_root = placed.server_root;
    dub_server::ensure_library_path(&repo_root);
    setup_server_env(&repo_root);
    // Маршрут прокси до проверки обновлений: сервер ставит его в своём потоке, без гарантии, что раньше.
    dub_server::init_proxy_route(&repo_root);

    let context = tauri::generate_context!();
    service::set_app_version(context.package_info().version.to_string());

    let mut builder = tauri::Builder::default();
    if single_instance_wanted() {
        // Первым: второй запуск должен уйти до остальной настройки, отдав фокус открытому окну.
        // Сервис поднимается только в .setup(), который Tauri зовёт после setup плагинов: второй
        // запуск выходит здесь, не тронув порт, и сервер живёт в процессе, чьё окно останется.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let shown = win.unminimize().and_then(|_| win.show()).and_then(|_| win.set_focus());
                if let Err(e) = shown {
                    eprintln!("[ERROR] повторный запуск: окно Dub Studio не вышло на передний план: {e}");
                }
            }
        }));
    }
    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            // axum-бэкенд поднимается В ЭТОМ ЖЕ процессе на фоновом потоке — ОДИН exe, без dub-server.exe-сайдкара.
            // Окно открывается только после ответа /health, чтобы не встать на пустую страницу.
            let port = match start_or_reuse_service(&repo_root) {
                Ok(p) => p,
                Err(message) => {
                    fatal(&message);
                    app.handle().exit(1);
                    return Ok(());
                }
            };
            let url = format!("http://127.0.0.1:{port}/");
            // Иконка бандла для GUI-окна: без явной установки окно оставалось пустым в ALT+TAB/панели задач
            // (иконка висела на консольном окне). Ставим её на само GUI-окно.
            let icon = app.default_window_icon().cloned();
            let win = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(url.parse().expect("валидный URL")),
            )
            .title(format!("Dub Studio {}", app.package_info().version))
            .inner_size(1600.0, 1040.0)
            .min_inner_size(1280.0, 800.0)
            .resizable(true)
            // Tauri v2 по умолчанию перехватывает OS-drop файлов -> HTML5 onDrop в дропзоне НЕ срабатывает
            // (юзеры жаловались «перетаскивание не работает»). Отключаем перехват -> webview сам ловит drop.
            .disable_drag_drop_handler()
            .build()?;
            // Иконка окна (ALT+TAB/таскбар) — ПОСЛЕ создания: не паникуем, если не выйдет, окно рабочее.
            if let Some(ic) = icon {
                let _ = win.set_icon(ic);
            }
            // авто-обновление: проверка на GitHub-релизе в фоне, установка по согласию (см. spawn_update_check)
            spawn_update_check(app.handle().clone(), layout::is_portable());
            Ok(())
        })
        .run(context)
        .expect("ошибка запуска Tauri");
}
