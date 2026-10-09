//! Куда уходят запросы приложения в интернет: напрямую, через прокси, заданный в Windows, или через свой —
//! HTTP, HTTPS, SOCKS5 или SOCKS4, с логином, если он есть. Порт net.rs студий на блокирующий reqwest.
//!
//! Каждый клиент строится здесь и спрашивает маршрут на каждый запрос, поэтому смена прокси действует сразу,
//! без перезапуска. Свой трафик приложения — llama-server, свой OpenAI-совместимый сервер на этой машине или
//! в локальной сети — через прокси не идёт никогда: прокси в интернете не знает дороги к 127.0.0.1.

use std::net::IpAddr;
use std::sync::RwLock;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use hyper_util::client::proxy::matcher::Matcher;
use reqwest::Url;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProxyMode {
    /// Напрямую, что бы ни говорили Windows и переменные окружения.
    Off,
    /// Как настроено в Windows (Параметры → Сеть → Прокси) или в HTTP_PROXY/HTTPS_PROXY/ALL_PROXY.
    #[default]
    System,
    /// Адрес, который дал пользователь.
    Custom,
}

impl ProxyMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "off" => Some(ProxyMode::Off),
            "system" => Some(ProxyMode::System),
            "custom" => Some(ProxyMode::Custom),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ProxyMode::Off => "off",
            ProxyMode::System => "system",
            ProxyMode::Custom => "custom",
        }
    }
}

/// Протокол адреса, записанного без схемы, как их раздают продавцы прокси: `host:port:user:password`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProxyKind {
    #[default]
    Http,
    Https,
    /// Имена разрешает сам прокси (socks5h): запрос имени не уходит через собственное соединение пользователя.
    Socks5,
    Socks4,
}

impl ProxyKind {
    fn scheme(self) -> &'static str {
        match self {
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
            ProxyKind::Socks5 => "socks5h",
            ProxyKind::Socks4 => "socks4a",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "http" => Some(ProxyKind::Http),
            "https" => Some(ProxyKind::Https),
            "socks5" => Some(ProxyKind::Socks5),
            "socks4" => Some(ProxyKind::Socks4),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
            ProxyKind::Socks5 => "socks5",
            ProxyKind::Socks4 => "socks4",
        }
    }
}

/// Маршрут. `address` — полный адрес вместе с паролем: живёт только в памяти процесса, наружу не отдаётся.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    pub address: Option<String>,
    pub kind: ProxyKind,
}

impl std::fmt::Debug for ProxySettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxySettings")
            .field("mode", &self.mode)
            .field("address", &self.address.as_deref().map(masked))
            .field("kind", &self.kind)
            .finish()
    }
}

const SCHEMES: [&str; 6] = ["http", "https", "socks5", "socks5h", "socks4", "socks4a"];

fn port(text: &str) -> bool {
    !text.is_empty() && text.len() <= 5 && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Всё, кроме незарезервированных символов RFC 3986: логин и пароль с любыми `@ : / ? # %` и не-ASCII остаются
/// одной частью адреса.
const USERINFO: &percent_encoding::AsciiSet =
    &percent_encoding::NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// Логин или пароль для вставки в адрес прокси.
pub fn encode_userinfo(text: &str) -> String {
    percent_encoding::utf8_percent_encode(text, USERINFO).to_string()
}

/// Логин или пароль из адреса прокси в том виде, в каком его ждёт прокси. Как reqwest: %XX, дающие не UTF-8,
/// становятся U+FFFD — тот же пароль уходит и из облачных запросов.
pub fn decode_userinfo(text: &str) -> String {
    percent_encoding::percent_decode_str(text).decode_utf8_lossy().into_owned()
}

/// Адрес прокси в любой из ходовых форм — одним URL: `scheme://user:password@host:port`,
/// `user:password@host:port`, `host:port:user:password`, `user:password:host:port` и `host:port`.
/// Формы без схемы берут схему `kind`.
pub fn normalize(address: &str, kind: ProxyKind) -> Result<Url> {
    let text = address.trim();
    if text.is_empty() {
        bail!("a proxy address is needed, such as 127.0.0.1:1080 or http://host:3128");
    }
    let written = if let Some((scheme, rest)) = text.split_once("://") {
        let scheme = match scheme.to_ascii_lowercase().as_str() {
            "socks" => "socks5h".to_string(),
            other => other.to_string(),
        };
        format!("{scheme}://{rest}")
    } else if text.starts_with('[') {
        format!("{}://{text}", kind.scheme())
    } else {
        // Сначала формы с двоеточиями: в пароле может быть @.
        let parts: Vec<&str> = text.split(':').collect();
        match parts.as_slice() {
            [host, number, user, password] if port(number) => {
                format!("{}://{}:{}@{host}:{number}", kind.scheme(), encode_userinfo(user), encode_userinfo(password))
            }
            [user, password, host, number] if port(number) && !host.contains('@') => {
                format!("{}://{}:{}@{host}:{number}", kind.scheme(), encode_userinfo(user), encode_userinfo(password))
            }
            _ if text.contains('@') => format!("{}://{text}", kind.scheme()),
            [host, number] if port(number) => format!("{}://{host}:{number}", kind.scheme()),
            _ => bail!(
                "{} is not a proxy address: write host:port, host:port:user:password or scheme://user:password@host:port",
                masked(text)
            ),
        }
    };
    let url = Url::parse(&written).with_context(|| format!("{} is not a proxy address", masked(text)))?;
    if !SCHEMES.contains(&url.scheme()) {
        bail!("a proxy is http, https, socks5 or socks4, not {}", url.scheme());
    }
    if url.host_str().is_none_or(str::is_empty) || url.port_or_known_default().is_none() {
        bail!("the proxy address needs a host and a port");
    }
    // Путь у прокси бывает только от пароля с / ? #, записанного в адрес как есть: хост и порт тогда чужие.
    if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
        bail!(
            "{} is not a proxy address: nothing follows host:port; a password with / ? # goes into the password field",
            masked(text)
        );
    }
    Ok(url)
}

/// Разобранный адрес строкой без хвостового `/`, который URL дописывает к http(s)-адресу.
pub fn normalized_text(url: &Url) -> String {
    let text = url.as_str();
    if url.path() == "/" && url.query().is_none() && url.fragment().is_none() {
        text.trim_end_matches('/').to_string()
    } else {
        text.to_string()
    }
}

impl ProxySettings {
    /// Настройки, у которых свой адрес читается, или причина, почему нет.
    pub fn validated(self) -> Result<Self> {
        if self.mode == ProxyMode::Custom {
            normalize(self.address.as_deref().unwrap_or_default(), self.kind)?;
        }
        Ok(self)
    }

    /// Свой адрес, если он читается; None — нет своего адреса или он не читается.
    pub fn custom_url(&self) -> Option<Url> {
        normalize(self.address.as_deref()?, self.kind).ok()
    }
}

/// Адрес с паролем, заменённым на ***, — для логов и сообщений. Адрес, который не читается как URL с паролем,
/// прячет всё между схемой и последним `@`; запись продавца без `@` — всё после логина.
pub fn masked(text: &str) -> String {
    if let Some(at) = text.rfind('@') {
        let start = text.find("://").map_or(0, |scheme| scheme + 3).min(at);
        if !text[start..at].contains(['/', '?', '#', '\\']) {
            if let Ok(mut url) = Url::parse(text) {
                if url.has_host() && url.password().is_some() {
                    let _ = url.set_password(Some("***"));
                    return url.to_string();
                }
            }
        }
        return format!("{}***{}", &text[..start], &text[at..]);
    }
    let (scheme, body) = text.split_at(text.find("://").map_or(0, |scheme| scheme + 3));
    let parts: Vec<&str> = body.split(':').collect();
    if parts.len() < 4 || body.contains('[') {
        text.to_string()
    } else if port(parts[1]) {
        format!("{scheme}{}:***", parts[..3].join(":"))
    } else {
        format!("{scheme}{}:***:{}", parts[0], parts[parts.len() - 2..].join(":"))
    }
}

struct Route {
    settings: ProxySettings,
    /// Прокси Windows и окружения, прочитанный при установке маршрута.
    system: Matcher,
}

fn route() -> &'static RwLock<Route> {
    static ROUTE: std::sync::OnceLock<RwLock<Route>> = std::sync::OnceLock::new();
    ROUTE.get_or_init(|| RwLock::new(Route { settings: ProxySettings::default(), system: Matcher::from_system() }))
}

/// Сделать `settings` маршрутом каждого следующего запроса.
pub fn set(settings: ProxySettings) {
    let mut route = route().write().expect("proxy route");
    route.system = Matcher::from_system();
    route.settings = settings;
}

pub fn current() -> ProxySettings {
    route().read().expect("proxy route").settings.clone()
}

/// Эта машина и локальная сеть: свой сервер на соседнем компьютере прокси в интернете неизвестен так же,
/// как 127.0.0.1. Имена без точки — локальные, как в обходе `<local>` самой Windows.
pub fn is_local(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => address.is_loopback() || address.is_private() || address.is_link_local(),
        Ok(IpAddr::V6(address)) => address.is_loopback() || address.is_unique_local() || address.is_unicast_link_local(),
        Err(_) => !host.contains('.'),
    }
}

/// Прокси для одного запроса при `settings`, None — напрямую.
fn proxy_for(settings: &ProxySettings, system: &Matcher, url: &Url) -> Option<Url> {
    if is_local(url) {
        return None;
    }
    match settings.mode {
        ProxyMode::Off => None,
        ProxyMode::Custom => settings.custom_url(),
        ProxyMode::System => {
            let destination: http::Uri = url.as_str().parse().ok()?;
            let intercept = system.intercept(&destination)?;
            let mut proxy = Url::parse(&intercept.uri().to_string()).ok()?;
            if let Some((user, password)) = intercept.raw_auth() {
                let _ = proxy.set_username(user);
                let _ = proxy.set_password(Some(password));
            }
            Some(proxy)
        }
    }
}

/// Прокси текущего маршрута для адреса `target` (для клиентов, которым прокси задают, а не спрашивают на
/// каждый запрос, — закачки через ureq). None — напрямую; нечитаемый `target` — тоже напрямую. Логин и пароль
/// в адресе закодированы %XX (`decode_userinfo`).
pub fn proxy_url_for(target: &str) -> Option<Url> {
    let url = Url::parse(target).ok()?;
    let route = route().read().expect("proxy route");
    proxy_for(&route.settings, &route.system, &url)
}

/// Маршрут для клиента, которому прокси задают один раз при сборке и который сам умеет прокси Windows
/// (апдейтер Tauri в десктопе).
#[derive(Clone, PartialEq, Eq)]
pub enum Fixed {
    /// Прокси Windows и переменных окружения — клиент читает их сам.
    System,
    Direct,
    Through(Url),
}

impl std::fmt::Debug for Fixed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fixed::System => f.write_str("System"),
            Fixed::Direct => f.write_str("Direct"),
            Fixed::Through(url) => write!(f, "Through({})", masked(url.as_str())),
        }
    }
}

/// Текущий маршрут для такого клиента. Свой адрес, который не читается, — напрямую, как и у остальных запросов
/// (причину пишет `apply_proxy_route` сервера и показывает окно настроек).
pub fn fixed() -> Fixed {
    let settings = current();
    match settings.mode {
        ProxyMode::System => Fixed::System,
        ProxyMode::Off => Fixed::Direct,
        ProxyMode::Custom => settings.custom_url().map_or(Fixed::Direct, Fixed::Through),
    }
}

const USER_AGENT: &str = concat!("DubStudio/", env!("CARGO_PKG_VERSION"));

/// Строитель клиента, который ходит по маршруту настроек, с именем приложения: Hugging Face строже к клиентам
/// без него.
pub fn builder() -> reqwest::blocking::ClientBuilder {
    with_proxy(reqwest::blocking::Client::builder().user_agent(USER_AGENT))
}

/// Любой строитель клиента с маршрутом настроек.
pub fn with_proxy(builder: reqwest::blocking::ClientBuilder) -> reqwest::blocking::ClientBuilder {
    builder.proxy(reqwest::Proxy::custom(|url| {
        let route = route().read().expect("proxy route");
        proxy_for(&route.settings, &route.system, url)
    }))
}

/// Асинхронный строитель с тем же маршрутом настроек (Studio Hub в tokio).
pub fn async_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().user_agent(USER_AGENT).proxy(reqwest::Proxy::custom(|url| {
        let route = route().read().expect("proxy route");
        proxy_for(&route.settings, &route.system, url)
    }))
}

/// Клиент к процессу на этой машине (llama-server): мимо любого прокси, включая переменные окружения.
pub fn local_builder() -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder().no_proxy()
}

/// Отвечают ли Hugging Face (откуда модели) и OpenRouter через `settings` — до сохранения. Ответ с любым
/// статусом считается, неудавшееся соединение — нет.
pub fn test(settings: ProxySettings) -> Result<serde_json::Value> {
    let settings = settings.validated()?;
    let system = Matcher::from_system();
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .proxy(reqwest::Proxy::custom(move |url| proxy_for(&settings, &system, url)))
        .timeout(Duration::from_secs(15))
        .build()?;
    let reach = |url: &str| client.get(url).send().map(|_| ()).map_err(|error| why(&error));
    let (hub, openrouter) = std::thread::scope(|scope| {
        let hub = scope.spawn(|| reach("https://huggingface.co/api/models?limit=1"));
        let openrouter = scope.spawn(|| reach("https://openrouter.ai/api/v1/models?limit=1"));
        (
            hub.join().unwrap_or_else(|_| Err("the Hugging Face probe panicked".into())),
            openrouter.join().unwrap_or_else(|_| Err("the OpenRouter probe panicked".into())),
        )
    });
    Ok(serde_json::json!({
        "huggingface": hub.is_ok(),
        "huggingface_error": hub.err(),
        "openrouter": openrouter.is_ok(),
        "openrouter_error": openrouter.err(),
    }))
}

/// Ошибка вместе с причинами под ней: одно «error sending request» не говорит, что прокси отверг логин или
/// недоступен.
pub fn why(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(inner) = cause {
        let line = inner.to_string();
        if !text.contains(&line) {
            text.push_str(": ");
            text.push_str(&line);
        }
        cause = inner.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    fn custom(address: &str, kind: ProxyKind) -> ProxySettings {
        ProxySettings { mode: ProxyMode::Custom, address: Some(address.into()), kind }
    }

    #[test]
    fn every_usual_way_of_writing_a_proxy_is_read() {
        let read = |text: &str, kind| normalize(text, kind).unwrap().to_string();
        assert_eq!(read("127.0.0.1:1080", ProxyKind::Socks5), "socks5h://127.0.0.1:1080");
        assert_eq!(read("proxy.example:3128", ProxyKind::Http), "http://proxy.example:3128/");
        assert_eq!(read("1.2.3.4:8000:user:p@ss", ProxyKind::Http), "http://user:p%40ss@1.2.3.4:8000/");
        assert_eq!(read("user:pass:1.2.3.4:8000", ProxyKind::Socks5), "socks5h://user:pass@1.2.3.4:8000");
        assert_eq!(read("user:pass@1.2.3.4:8000", ProxyKind::Https), "https://user:pass@1.2.3.4:8000/");
        assert_eq!(read("SOCKS5://u:p@h:1080", ProxyKind::Http), "socks5://u:p@h:1080");
        assert_eq!(read("socks://h:1080", ProxyKind::Http), "socks5h://h:1080");
        assert_eq!(read("socks4://h:1080", ProxyKind::Http), "socks4://h:1080");
        assert_eq!(read("[::1]:1080", ProxyKind::Socks5), "socks5h://[::1]:1080");
        for bad in ["", "proxy", "ftp://h:21", "h:port", "a:b:c"] {
            assert!(normalize(bad, ProxyKind::Http).is_err(), "{bad}");
        }
        assert_eq!(normalized_text(&normalize("proxy.example:3128", ProxyKind::Http).unwrap()), "http://proxy.example:3128");
        assert_eq!(normalized_text(&normalize("h:1080", ProxyKind::Socks5).unwrap()), "socks5h://h:1080");
    }

    #[test]
    fn this_machine_and_the_local_network_are_never_proxied() {
        let settings = custom("socks5h://user:pass@10.0.0.2:1080", ProxyKind::Http);
        let system = Matcher::from_system();
        for local in [
            "http://127.0.0.1:18087/health",
            "http://localhost:11434/v1/models",
            "http://[::1]:8793/",
            "http://192.168.1.20:11434/v1",
            "http://10.0.0.5:1234/v1",
            "http://gpu-box:11434/v1",
        ] {
            assert_eq!(proxy_for(&settings, &system, &url(local)), None, "{local}");
        }
        let through = proxy_for(&settings, &system, &url("https://huggingface.co/x")).unwrap();
        assert_eq!((through.scheme(), through.username(), through.password()), ("socks5h", "user", Some("pass")));
        let off = ProxySettings { mode: ProxyMode::Off, ..ProxySettings::default() };
        assert_eq!(proxy_for(&off, &system, &url("https://huggingface.co/x")), None);
    }

    #[test]
    fn a_password_never_reaches_a_log() {
        assert_eq!(masked("socks5://me:secret@host:1080"), "socks5://me:***@host:1080");
        assert_eq!(masked("1.2.3.4:8000:user:secret"), "1.2.3.4:8000:user:***");
        assert_eq!(masked("user:secret:1.2.3.4:8000"), "user:***:1.2.3.4:8000");
        assert_eq!(masked("1.2.3.4:8000:user:sec:ret"), "1.2.3.4:8000:user:***");
        assert_eq!(masked("http://bob:sec/ret@1.2.3.4:8000"), "http://***@1.2.3.4:8000");
        assert_eq!(masked("bob:secret@1.2.3.4:8000"), "***@1.2.3.4:8000");
        for unreadable in ["http://bob:sec?ret@h:1", "http://bob:sec#ret@h:1", "http://bob:12/secret@h:1", "http://bob:se\\cret@h:1", "http://b:s@cret@h:1"] {
            let shown = masked(unreadable);
            assert!(!shown.contains("sec") && !shown.contains("cret"), "{unreadable} -> {shown}");
        }
        assert_eq!(masked("http://proxy.example:3128"), "http://proxy.example:3128");
        assert_eq!(masked("[::1]:1080"), "[::1]:1080");
        let debug = format!("{:?}", custom("http://me:secret@host:3128", ProxyKind::Http));
        assert!(!debug.contains("secret"), "{debug}");
        let fixed = format!("{:?}", Fixed::Through(url("http://me:secret@host:3128")));
        assert!(!fixed.contains("secret"), "{fixed}");
        for broken in ["http://bob:sec/ret@1.2.3.4:8000", "http://bob:sec?ret@1.2.3.4:8000", "http://bob:12/secret@h:1", "h:1:bob:sec:ret"] {
            let error = format!("{:#}", normalize(broken, ProxyKind::Http).expect_err(broken));
            assert!(!error.contains("sec") && !error.contains("ret@"), "{broken} -> {error}");
        }
    }

    #[test]
    fn any_password_survives_the_address() {
        let system = Matcher::from_system();
        for password in ["pa/ss", "pa?ss", "pa#ss", "p@ss", "pa:ss", "p@ss:1/x", "100%", "pa\\ss", "пароль с пробелом"] {
            let address = format!("http://bob:{}@1.2.3.4:8000", encode_userinfo(password));
            let settings = custom(&address, ProxyKind::Http);
            let through = settings.custom_url().unwrap_or_else(|| panic!("{password} makes the address unreadable"));
            assert_eq!(decode_userinfo(through.password().unwrap()), password);
            assert_eq!((through.host_str(), through.port()), (Some("1.2.3.4"), Some(8000)));
            assert_eq!(proxy_for(&settings, &system, &url("https://huggingface.co/x")), Some(through));
            assert!(!masked(&address).contains(&encode_userinfo(password)), "{address}");
        }
        let seller = normalize("1.2.3.4:8000:b@b:p@ss:w", ProxyKind::Socks5);
        assert!(seller.is_err(), "a colon inside a seller password is ambiguous");
        let seller = normalize("1.2.3.4:8000:b@b:p@ss/w", ProxyKind::Socks5).unwrap();
        assert_eq!((decode_userinfo(seller.username()), decode_userinfo(seller.password().unwrap())), ("b@b".into(), "p@ss/w".into()));
    }

    #[test]
    fn a_changed_route_applies_to_the_next_request() {
        set(custom("http://proxy.example:3128", ProxyKind::Http));
        assert_eq!(proxy_url_for("https://openrouter.ai/api/v1/models").map(String::from).as_deref(), Some("http://proxy.example:3128/"));
        assert_eq!(proxy_url_for("http://127.0.0.1:11434/v1/models"), None);
        assert_eq!(fixed(), Fixed::Through(url("http://proxy.example:3128")));
        set(custom("http://bob:pa/ss@proxy.example:3128", ProxyKind::Http));
        assert_eq!(fixed(), Fixed::Direct, "an unreadable address of its own goes direct, as every other request");
        set(ProxySettings { mode: ProxyMode::Off, ..ProxySettings::default() });
        assert_eq!(proxy_url_for("https://openrouter.ai/api/v1/models"), None);
        assert_eq!(fixed(), Fixed::Direct);
        set(ProxySettings::default());
        assert_eq!(fixed(), Fixed::System);
    }

    #[test]
    fn modes_and_kinds_round_trip_through_their_names() {
        for mode in [ProxyMode::Off, ProxyMode::System, ProxyMode::Custom] {
            assert_eq!(ProxyMode::parse(mode.as_str()), Some(mode));
        }
        for kind in [ProxyKind::Http, ProxyKind::Https, ProxyKind::Socks5, ProxyKind::Socks4] {
            assert_eq!(ProxyKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ProxyMode::parse("on"), None);
    }
}
