//! Провайдер LLM для стадии: своя Gemma (llama-server + mmproj), локальный OpenAI-совместимый сервер
//! пользователя (Ollama, LM Studio, vLLM) или OpenRouter. Перевод ("llm") и vision ("vision") выбираются
//! независимо (models::llm_backend): облачный перевод не требует локальных весов и не зависит от vision.
//!
//! Инвариант: путь своей Gemma неизменен (тот же ServerOpts/ubatch/mmproj). Выбранный, но не настроенный
//! провайдер (нет ключа, модели) — ошибка с причиной, а не тихий переход на другой.

use std::path::Path;

use dub_llm::openrouter::Capability;
use dub_llm::{ChatClient, LlamaServer, ServerOpts};

use crate::localize::Localize;
use crate::models::LlmBackend;

/// Режим вызова: плоский текст (перевод/ремикс) или мультимодальный (vision-анализ кадров).
#[derive(Clone, Copy, PartialEq)]
pub enum LlmMode {
    Text,
    Vision,
}

impl LlmMode {
    fn stage(self) -> &'static str {
        match self {
            LlmMode::Text => "llm",
            LlmMode::Vision => "vision",
        }
    }
}

/// Готовый провайдер: клиент чата + (для своей Gemma) живой llama-server, который глушится по Drop.
pub enum LlmProvider {
    /// Своя Gemma: сервер держим живым, пока провайдер в скоупе (Drop останавливает процесс).
    Local {
        _server: LlamaServer,
        client: ChatClient,
    },
    /// Локальный сервер пользователя или OpenRouter: только HTTP-клиент.
    Remote {
        client: ChatClient,
        label: String,
    },
}

impl LlmProvider {
    pub fn client(&self) -> &ChatClient {
        match self {
            LlmProvider::Local { client, .. } => client,
            LlmProvider::Remote { client, .. } => client,
        }
    }

    /// Кто отвечает — для строк прогресса.
    pub fn describe(&self) -> String {
        match self {
            LlmProvider::Local { .. } => "Gemma (llama-server)".to_string(),
            LlmProvider::Remote { label, .. } => label.clone(),
        }
    }
}

/// Параметры открытия провайдера (пути локальных весов + models_root для чтения настроек).
pub struct LlmOpen<'a> {
    pub llama_bin: &'a Path,
    pub mt_model: &'a Path,
    pub mmproj: &'a Path,
    pub models_root: &'a Path,
}

/// Свой llama-server с Gemma; `with_mmproj` — с проектором для кадров (он обязан быть на диске).
fn start_gemma(o: &LlmOpen, with_mmproj: bool) -> Result<LlmProvider, String> {
    if !o.llama_bin.is_file() {
        return Err(t!("llm-llama-server-missing", path = o.llama_bin.display().to_string()));
    }
    if !o.mt_model.is_file() {
        return Err(t!("llm-gemma-missing", path = o.mt_model.display().to_string()));
    }
    if with_mmproj && !o.mmproj.is_file() {
        return Err(t!("llm-mmproj-missing", path = o.mmproj.display().to_string()));
    }
    let mut opts = ServerOpts::new(o.llama_bin, o.mt_model)
        .with_ubatch(crate::models::sel_num(o.models_root, "llama_ubatch").map(|f| f as u32))
        .with_log_file(llama_log_path(o.models_root));
    if with_mmproj {
        opts = opts.with_mmproj(o.mmproj);
    }
    let server = LlamaServer::start(&opts).map_err(|e| e.localize())?;
    let client = ChatClient::new(server.base_url()).map_err(|e| t!("llm-chat-client", error = e.localize()))?;
    Ok(LlmProvider::Local { _server: server, client })
}

fn open_server(o: &LlmOpen, mode: LlmMode) -> Result<LlmProvider, String> {
    let (url, model) = server_of(o.models_root, mode)?;
    let client = ChatClient::openai_compatible(&url, model.clone(), crate::credentials::local_server_key_for(&url))
        .map_err(|e| t!("llm-local-client", error = e.localize()))?;
    Ok(LlmProvider::Remote { client, label: t!("llm-local-label", url = url.clone(), model = model.clone()) })
}

/// The address and the stage's model of the local server; no model is the reason it cannot start.
fn server_of(models_root: &Path, mode: LlmMode) -> Result<(String, String), String> {
    let url = crate::models::server_url(models_root);
    let model = crate::models::server_model(models_root, mode.stage());
    if model.trim().is_empty() {
        return Err(match mode {
            LlmMode::Text => t!("llm-local-no-text-model", url = url.clone()),
            LlmMode::Vision => t!("llm-local-no-vision-model", url = url.clone()),
        });
    }
    Ok((url, model))
}

/// Whether the stage's provider is set up enough to start a job: the local server with a model, OpenRouter with a key
/// and a model that takes the stage's input. The studio's own Gemma is fetched by the job, the agent is asked when
/// the stage runs.
pub fn configured(models_root: &Path, mode: LlmMode) -> Result<(), String> {
    match crate::models::llm_backend(models_root, mode.stage()) {
        LlmBackend::Server => server_of(models_root, mode).map(|_| ()),
        LlmBackend::OpenRouter => openrouter_of(models_root, mode).map(|_| ()),
        LlmBackend::Local | LlmBackend::Agent => Ok(()),
    }
}

fn open_openrouter(o: &LlmOpen, mode: LlmMode) -> Result<LlmProvider, String> {
    let (key, model, entry) = openrouter_of(o.models_root, mode)?;
    let client = ChatClient::openrouter(key, model.clone())
        .map_err(|e| e.localize())?
        .with_profile(entry.as_ref().map(|entry| entry.profile()));
    Ok(LlmProvider::Remote { client, label: format!("OpenRouter · {model}") })
}

/// The OpenRouter key, the stage's model and its catalogue entry; a missing key or model, or a model that does not take
/// the stage's input, is the reason it cannot start.
fn openrouter_of(models_root: &Path, mode: LlmMode) -> Result<(String, String, Option<dub_llm::openrouter::CatalogModel>), String> {
    let key = crate::models::openrouter_key().ok_or_else(|| t!("llm-openrouter-no-key"))?;
    let model = crate::models::openrouter_model(models_root, mode.stage());
    if model.trim().is_empty() {
        return Err(match mode {
            LlmMode::Text => t!("llm-openrouter-no-text-model"),
            LlmMode::Vision => t!("llm-openrouter-no-vision-model"),
        });
    }
    let capability = if mode == LlmMode::Vision { Capability::Vision } else { Capability::Text };
    let entry = crate::openrouter::describe(models_root, &model);
    if let Some(entry) = &entry {
        if !entry.supports(capability) {
            return Err(match mode {
                LlmMode::Text => t!("llm-openrouter-not-text", model = model.clone()),
                LlmMode::Vision => t!("llm-openrouter-not-vision", model = model.clone()),
            });
        }
    }
    Ok((key, model, entry))
}

/// Агент, подключённый по MCP: свой чат-клиент студии на `/mcp/agent-llm` этого же сервера, который отдаёт каждый
/// запрос агенту (`mcp::agent_llm`); кадры vision доходят до него файлами. Модель `agent-<стадия>` называет агенту
/// цель запроса. Одна попытка: повтор спросил бы агента дважды.
fn open_agent(mode: LlmMode) -> Result<LlmProvider, String> {
    let port = crate::service::bound_port().ok_or_else(|| t!("llm-agent-no-port"))?;
    let client = ChatClient::openai_compatible(&format!("http://127.0.0.1:{port}/mcp/agent-llm"), format!("agent-{}", mode.stage()), None)
        .map_err(|e| t!("llm-local-client", error = e.localize()))?
        .with_retries(0);
    Ok(LlmProvider::Remote { client, label: t!("llm-agent-label") })
}

/// Открыть провайдер одной стадии: Text — перевод (своя Gemma без mmproj), Vision — кадры (своя Gemma с
/// mmproj). Err — человекочитаемая причина.
pub fn open(o: &LlmOpen, mode: LlmMode) -> Result<LlmProvider, String> {
    match crate::models::llm_backend(o.models_root, mode.stage()) {
        LlmBackend::Local => start_gemma(o, mode == LlmMode::Vision),
        LlmBackend::Server => open_server(o, mode),
        LlmBackend::OpenRouter => open_openrouter(o, mode),
        LlmBackend::Agent => open_agent(mode),
    }
}

/// Перевод и vision одного прохода анализа. Обе стадии на своей Gemma — один llama-server с mmproj на обе.
pub struct LlmPair {
    text: LlmProvider,
    vision: VisionSlot,
}

enum VisionSlot {
    /// Кадры смотрит тот же сервер, что переводит (своя Gemma с mmproj).
    Shared,
    Own(Box<LlmProvider>),
    /// vision недоступен — с причиной.
    Missing(String),
}

impl LlmPair {
    pub fn text(&self) -> &ChatClient {
        self.text.client()
    }

    pub fn vision(&self) -> Option<&ChatClient> {
        match &self.vision {
            VisionSlot::Shared => Some(self.text.client()),
            VisionSlot::Own(provider) => Some(provider.client()),
            VisionSlot::Missing(_) => None,
        }
    }

    /// Строка прогресса: кто переводит, кто смотрит кадры (или почему не смотрит никто).
    pub fn describe(&self) -> String {
        let vision = match &self.vision {
            VisionSlot::Shared => self.text.describe(),
            VisionSlot::Own(provider) => provider.describe(),
            VisionSlot::Missing(reason) => t!("llm-vision-missing", reason = reason.clone()),
        };
        t!("llm-pair", text = self.text.describe(), vision = vision)
    }
}

/// Открыть перевод и vision анализа. Перевод обязателен (Err — перевод невозможен); vision без модели не
/// роняет перевод — фазы кадров и аудио-контекста пропускаются с причиной.
pub fn open_pair(o: &LlmOpen) -> Result<LlmPair, String> {
    let text_backend = crate::models::llm_backend(o.models_root, "llm");
    let vision_backend = crate::models::llm_backend(o.models_root, "vision");
    if text_backend == LlmBackend::Local && vision_backend == LlmBackend::Local {
        let with_mmproj = o.mmproj.is_file();
        let text = start_gemma(o, with_mmproj)?;
        let vision = if with_mmproj {
            VisionSlot::Shared
        } else {
            VisionSlot::Missing(t!("llm-mmproj-missing", path = o.mmproj.display().to_string()))
        };
        return Ok(LlmPair { text, vision });
    }
    let text = open(o, LlmMode::Text)?;
    let vision = match open(o, LlmMode::Vision) {
        Ok(provider) => VisionSlot::Own(Box::new(provider)),
        Err(reason) => VisionSlot::Missing(reason),
    };
    Ok(LlmPair { text, vision })
}

/// Лог llama-server рядом с данными приложения: <корень>/logs/llama-server.log (models_root = <корень>/models).
pub fn llama_log_path(models_root: &Path) -> std::path::PathBuf {
    models_root.parent().unwrap_or(models_root).join("logs").join("llama-server.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dub-provider-{tag}-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn open_in(root: &Path, mode: LlmMode) -> Result<LlmProvider, String> {
        let missing = root.join("missing.gguf");
        open(&LlmOpen { llama_bin: &missing, mt_model: &missing, mmproj: &missing, models_root: root }, mode)
    }

    fn pair_in(root: &Path) -> Result<LlmPair, String> {
        let missing = root.join("missing.gguf");
        open_pair(&LlmOpen { llama_bin: &missing, mt_model: &missing, mmproj: &missing, models_root: root })
    }

    fn error<T>(result: Result<T, String>) -> String {
        match result {
            Ok(_) => panic!("an error was expected"),
            Err(e) => e,
        }
    }

    #[test]
    fn a_local_server_translates_without_any_local_weights() {
        let root = scratch("server");
        crate::models::set_selection(&root, "llm_provider", "server").unwrap();
        crate::models::set_selection(&root, "srv_llm", "gemma3:12b").unwrap();
        let provider = open_in(&root, LlmMode::Text).unwrap();
        assert_eq!(provider.client().endpoint(), dub_llm::Endpoint::OpenAiCompatible);
        assert_eq!(provider.client().model(), Some("gemma3:12b"));
        assert!(provider.describe().contains("127.0.0.1:11434"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_chosen_provider_without_its_model_says_so() {
        let _language = crate::i18n::test_language("ru");
        let root = scratch("unset");
        crate::models::set_selection(&root, "llm_provider", "server").unwrap();
        assert!(error(open_in(&root, LlmMode::Text)).contains("модель не выбрана"));
        crate::models::set_selection(&root, "vision_provider", "server").unwrap();
        assert!(error(open_in(&root, LlmMode::Vision)).contains("vision"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn translation_goes_on_when_vision_has_no_model() {
        let _language = crate::i18n::test_language("ru");
        let root = scratch("pair");
        crate::models::set_selection(&root, "llm_provider", "server").unwrap();
        crate::models::set_selection(&root, "srv_llm", "qwen3:8b").unwrap();
        crate::models::set_selection(&root, "vision_provider", "local").unwrap();
        let pair = pair_in(&root).unwrap();
        assert_eq!(pair.text().model(), Some("qwen3:8b"));
        assert!(pair.vision().is_none(), "no Gemma on disk: vision is skipped, translation is not");
        assert!(pair.describe().contains("llama-server не найден"), "{}", pair.describe());

        crate::models::set_selection(&root, "srv_vision", "qwen2.5vl:7b").unwrap();
        crate::models::set_selection(&root, "vision_provider", "server").unwrap();
        let pair = pair_in(&root).unwrap();
        assert_eq!(pair.vision().and_then(ChatClient::model), Some("qwen2.5vl:7b"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_own_gemma_is_required_only_when_chosen() {
        let _language = crate::i18n::test_language("ru");
        let root = scratch("local");
        assert!(error(pair_in(&root)).contains("llama-server не найден"));
        assert!(error(open_in(&root, LlmMode::Text)).contains("llama-server не найден"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
