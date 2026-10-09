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

/// Локальный OpenAI-совместимый сервер: адрес, модель стадии, ключ из хранилища секретов — если он сохранён для
/// этого адреса.
fn open_server(o: &LlmOpen, mode: LlmMode) -> Result<LlmProvider, String> {
    let url = crate::models::server_url(o.models_root);
    let model = crate::models::server_model(o.models_root, mode.stage());
    if model.trim().is_empty() {
        return Err(match mode {
            LlmMode::Text => t!("llm-local-no-text-model", url = url.clone()),
            LlmMode::Vision => t!("llm-local-no-vision-model", url = url.clone()),
        });
    }
    let client = ChatClient::openai_compatible(&url, model.clone(), crate::credentials::local_server_key_for(&url))
        .map_err(|e| t!("llm-local-client", error = e.localize()))?;
    Ok(LlmProvider::Remote { client, label: t!("llm-local-label", url = url.clone(), model = model.clone()) })
}

/// OpenRouter: ключ, модель стадии из настроек, запись каталога (что модель принимает и как думает).
fn open_openrouter(o: &LlmOpen, mode: LlmMode) -> Result<LlmProvider, String> {
    let key = crate::models::openrouter_key().ok_or_else(|| t!("llm-openrouter-no-key"))?;
    let model = crate::models::openrouter_model(o.models_root, mode.stage());
    if model.trim().is_empty() {
        return Err(match mode {
            LlmMode::Text => t!("llm-openrouter-no-text-model"),
            LlmMode::Vision => t!("llm-openrouter-no-vision-model"),
        });
    }
    let capability = if mode == LlmMode::Vision { Capability::Vision } else { Capability::Text };
    let entry = crate::openrouter::describe(o.models_root, &model);
    if let Some(entry) = &entry {
        if !entry.supports(capability) {
            return Err(match mode {
                LlmMode::Text => t!("llm-openrouter-not-text", model = model.clone()),
                LlmMode::Vision => t!("llm-openrouter-not-vision", model = model.clone()),
            });
        }
    }
    let client = ChatClient::openrouter(key, model.clone())
        .map_err(|e| e.localize())?
        .with_profile(entry.as_ref().map(|entry| entry.profile()));
    Ok(LlmProvider::Remote { client, label: format!("OpenRouter · {model}") })
}

/// Открыть провайдер одной стадии: Text — перевод (своя Gemma без mmproj), Vision — кадры (своя Gemma с
/// mmproj). Err — человекочитаемая причина.
pub fn open(o: &LlmOpen, mode: LlmMode) -> Result<LlmProvider, String> {
    match crate::models::llm_backend(o.models_root, mode.stage()) {
        LlmBackend::Local => start_gemma(o, mode == LlmMode::Vision),
        LlmBackend::Server => open_server(o, mode),
        LlmBackend::OpenRouter => open_openrouter(o, mode),
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
