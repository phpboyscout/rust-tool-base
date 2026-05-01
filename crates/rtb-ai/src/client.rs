//! [`AiClient`] — typed façade over `genai` + the Anthropic-direct
//! path.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use schemars::JsonSchema;
use secrecy::ExposeSecret;
use serde::de::DeserializeOwned;

use crate::anthropic::{AnthropicTransport, ReqwestAnthropic};
use crate::config::{validate_base_url, Config, Provider};
use crate::error::{redact, AiError};
use crate::message::{ContentBlock, Message, Usage};
use crate::thinking::ThinkingMode;

/// One-shot or streaming chat request.
#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    /// Optional system prompt. Goes to Anthropic's top-level
    /// `system` field; for `genai`-backed providers it lands in the
    /// first message with role `system`.
    pub system: Option<String>,
    /// Conversation history + the current user message. Last item
    /// is conventionally the user's turn.
    pub messages: Vec<Message>,
    /// Sampling temperature.
    pub temperature: Option<f32>,
    /// Hard cap on the assistant's reply.
    pub max_tokens: Option<u32>,
    /// Anthropic-only: enables prompt caching at the system prompt
    /// + first user message. Silently ignored on other providers.
    pub cache_control: bool,
    /// Anthropic-only: enables extended-thinking with the supplied
    /// budget. Silently ignored on other providers.
    pub thinking: Option<ThinkingMode>,
}

/// Non-streaming chat response.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// Assistant's reply.
    pub message: Message,
    /// Token counts the provider reported.
    pub usage: Usage,
    /// Citations, populated only on the Anthropic-direct path when
    /// the model emits them. Empty otherwise.
    pub citations: Vec<crate::message::Citation>,
}

/// One event from the streaming chat path.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ChatStreamEvent {
    /// Regular assistant token.
    Token(String),
    /// Anthropic-only: a token from the extended-thinking stream.
    /// Other providers never emit this.
    ThinkingToken(String),
    /// Final event, carrying the cumulative usage.
    Done(Usage),
    /// Stream-level error; ends the stream.
    Error(AiError),
}

/// Async stream of [`ChatStreamEvent`]s. The stream is `!Sync` to
/// avoid pinning trade-offs in callers; it is `Send` so it can move
/// across `tokio::spawn` boundaries.
pub struct ChatStream {
    inner: Pin<Box<dyn Stream<Item = ChatStreamEvent> + Send>>,
}

impl ChatStream {
    pub(crate) fn new(stream: Pin<Box<dyn Stream<Item = ChatStreamEvent> + Send>>) -> Self {
        Self { inner: stream }
    }
}

impl Stream for ChatStream {
    type Item = ChatStreamEvent;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

impl std::fmt::Debug for ChatStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatStream").finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------
// AiClient
// ---------------------------------------------------------------------

enum Backend {
    Anthropic(Arc<dyn AnthropicTransport>),
    Genai(genai::Client),
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Anthropic(_) => f.debug_struct("Backend::Anthropic").finish_non_exhaustive(),
            Self::Genai(_) => f.debug_struct("Backend::Genai").finish_non_exhaustive(),
        }
    }
}

/// Typed AI client. Construct via [`AiClient::new`].
#[derive(Debug)]
pub struct AiClient {
    config: Config,
    backend: Backend,
}

impl AiClient {
    /// Build a client. Validates `config.base_url`, builds the
    /// underlying HTTP client, and stamps the appropriate backend
    /// (Anthropic-direct or `genai`).
    ///
    /// # Errors
    ///
    /// [`AiError::InvalidConfig`] on a bad base URL, an empty API
    /// key, or a `reqwest::Client` build failure.
    pub fn new(config: Config) -> Result<Self, AiError> {
        Self::validate(&config)?;
        let backend = if config.provider.is_anthropic() {
            let client = build_reqwest_client(&config)?;
            tracing::info!(
                provider = ?config.provider,
                host = %backend_host(&config),
                "rtb-ai: AiClient ready (anthropic-direct)",
            );
            Backend::Anthropic(Arc::new(ReqwestAnthropic::new(Arc::new(client))))
        } else {
            // For genai-backed providers we let genai create the
            // underlying HTTP client. The API key is supplied via
            // env var of the relevant provider; genai resolves it
            // internally. We set the variable for the duration of
            // the constructor — see `genai_set_key` below.
            genai_set_key(&config);
            tracing::info!(
                provider = ?config.provider,
                host = %backend_host(&config),
                "rtb-ai: AiClient ready (genai)",
            );
            Backend::Genai(genai::Client::default())
        };
        Ok(Self { config, backend })
    }

    fn validate(config: &Config) -> Result<(), AiError> {
        if config.api_key.expose_secret().is_empty() {
            return Err(AiError::InvalidConfig("api_key must not be empty".into()));
        }
        if config.model.is_empty() {
            return Err(AiError::InvalidConfig("model must not be empty".into()));
        }
        if let Some(url) = &config.base_url {
            validate_base_url(url, config.allow_insecure_base_url)?;
        }
        Ok(())
    }

    /// One-shot chat completion.
    ///
    /// # Errors
    ///
    /// Any [`AiError`] variant — provider errors (4xx / 5xx), HTTP
    /// transport failures, rate-limit responses.
    pub async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AiError> {
        match &self.backend {
            Backend::Anthropic(t) => t.chat(&self.config, req).await,
            Backend::Genai(c) => genai_chat(c, &self.config, req).await,
        }
    }

    /// Streaming chat completion.
    ///
    /// # Errors
    ///
    /// Connection-time errors surface synchronously; per-event
    /// errors surface as [`ChatStreamEvent::Error`] inside the
    /// returned stream.
    pub async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, AiError> {
        match &self.backend {
            Backend::Anthropic(t) => t.chat_stream(&self.config, req).await,
            Backend::Genai(c) => genai_chat_stream(c, &self.config, req).await,
        }
    }

    /// Structured output: validates the response against `T`'s
    /// `JsonSchema` before deserialising.
    ///
    /// The request is augmented to instruct the model to emit JSON
    /// matching the schema — see [`ChatRequest::system`]; the
    /// caller's system prompt (if any) is prepended.
    ///
    /// # Errors
    ///
    /// [`AiError::SchemaValidation`] when the response doesn't match
    /// the schema; [`AiError::Deserialize`] when it matches the
    /// schema but `serde::Deserialize` for `T` rejects it; any
    /// underlying [`AiError`] from the chat call.
    pub async fn chat_structured<T>(&self, req: ChatRequest) -> Result<T, AiError>
    where
        T: DeserializeOwned + JsonSchema,
    {
        let schema = serde_json::to_value(schemars::schema_for!(T))
            .map_err(|e| AiError::InvalidConfig(redact(&e.to_string())))?;
        let augmented = augment_request_for_schema(req, &schema);
        let resp = self.chat(augmented).await?;
        let body =
            resp.message.content.iter().filter_map(ContentBlock::as_text).collect::<String>();
        let parsed: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| AiError::Deserialize(redact(&e.to_string())))?;
        let validator = jsonschema::validator_for(&schema)
            .map_err(|e| AiError::SchemaValidation(redact(&e.to_string())))?;
        if let Err(err) = validator.validate(&parsed) {
            return Err(AiError::SchemaValidation(redact(&err.to_string())));
        }
        serde_json::from_value::<T>(parsed)
            .map_err(|e| AiError::Deserialize(redact(&e.to_string())))
    }
}

fn build_reqwest_client(config: &Config) -> Result<reqwest::Client, AiError> {
    let mut builder = reqwest::Client::builder()
        .https_only(!config.allow_insecure_base_url)
        .timeout(config.timeout)
        .user_agent(concat!("rtb-ai/", env!("CARGO_PKG_VERSION")));
    if config.allow_insecure_base_url {
        // `https_only(false)` already accepts http; nothing more
        // needed but the explicit reset keeps the call self-
        // documenting.
        builder = builder.https_only(false);
    }
    builder.build().map_err(|e| AiError::InvalidConfig(redact(&e.to_string())))
}

fn backend_host(config: &Config) -> String {
    config.base_url.as_ref().and_then(|u| u.host_str().map(String::from)).unwrap_or_else(|| {
        match config.provider {
            Provider::Anthropic | Provider::AnthropicLocal => "api.anthropic.com".into(),
            Provider::OpenAi => "api.openai.com".into(),
            Provider::Gemini => "generativelanguage.googleapis.com".into(),
            Provider::Ollama => "localhost".into(),
            Provider::OpenAiCompatible => "openai-compatible".into(),
        }
    })
}

fn augment_request_for_schema(mut req: ChatRequest, schema: &serde_json::Value) -> ChatRequest {
    let instructions = format!(
        "You MUST respond with a single JSON value matching this schema. \
         No prose, no code fences:\n{schema}",
    );
    req.system = match req.system.take() {
        Some(prefix) => Some(format!("{prefix}\n\n{instructions}")),
        None => Some(instructions),
    };
    req
}

// ---------------------------------------------------------------------
// genai-backed path
// ---------------------------------------------------------------------

fn genai_set_key(config: &Config) {
    // genai reads provider keys from environment variables. Setting
    // the var here makes the key reachable to genai's lazy client
    // builder. SAFETY: env var mutation is a known footgun under
    // racy multi-threaded constructor calls; rtb-ai callers
    // construct one client per process (the typical pattern).
    let var = match config.provider {
        Provider::OpenAi | Provider::OpenAiCompatible => "OPENAI_API_KEY",
        Provider::Gemini => "GEMINI_API_KEY",
        // Local inference + the Anthropic-direct path don't go
        // through genai's env-var key resolution.
        Provider::Ollama | Provider::Anthropic | Provider::AnthropicLocal => return,
    };
    // Safety: same-process env mutation. Documented above; matches
    // the rtb-config tests' rationale.
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var(var, config.api_key.expose_secret());
    }
}

async fn genai_chat(
    client: &genai::Client,
    config: &Config,
    req: ChatRequest,
) -> Result<ChatResponse, AiError> {
    let chat_req = build_genai_request(&req);
    let resp = client
        .exec_chat(&config.model, chat_req, None)
        .await
        .map_err(|e| AiError::Provider(redact(&e.to_string())))?;
    let text = resp.first_text().unwrap_or_default().to_string();
    let usage = genai_usage(&resp);
    Ok(ChatResponse { message: Message::assistant(text), usage, citations: Vec::new() })
}

async fn genai_chat_stream(
    client: &genai::Client,
    config: &Config,
    req: ChatRequest,
) -> Result<ChatStream, AiError> {
    let chat_req = build_genai_request(&req);
    let resp = client
        .exec_chat_stream(&config.model, chat_req, None)
        .await
        .map_err(|e| AiError::Provider(redact(&e.to_string())))?;
    let stream = futures_util::StreamExt::map(resp.stream, |event| {
        use genai::chat::ChatStreamEvent as G;
        match event {
            Ok(G::Chunk(chunk)) => ChatStreamEvent::Token(chunk.content),
            Ok(G::ReasoningChunk(chunk)) => ChatStreamEvent::ThinkingToken(chunk.content),
            Ok(G::End(end)) => ChatStreamEvent::Done(genai_usage_from_end(&end)),
            // Filtered out below — emitted as empty `Token`s and
            // dropped by the filter step. Keeps the match exhaustive
            // for future genai event variants.
            Ok(G::Start | G::ToolCallChunk(_) | G::ThoughtSignatureChunk(_)) => {
                ChatStreamEvent::Token(String::new())
            }
            Err(e) => ChatStreamEvent::Error(AiError::Provider(redact(&e.to_string()))),
        }
    });
    // Filter out empty `Start` / tool-chunk emits.
    let stream = futures_util::StreamExt::filter(stream, |e| {
        let keep = !matches!(e, ChatStreamEvent::Token(t) if t.is_empty());
        std::future::ready(keep)
    });
    Ok(ChatStream::new(Box::pin(stream)))
}

fn build_genai_request(req: &ChatRequest) -> genai::chat::ChatRequest {
    let mut chat = genai::chat::ChatRequest::default();
    if let Some(system) = &req.system {
        chat = chat.with_system(system.clone());
    }
    for msg in &req.messages {
        let text =
            msg.content.iter().filter_map(ContentBlock::as_text).collect::<Vec<_>>().join("\n");
        match msg.role {
            crate::message::Role::User => {
                chat = chat.append_message(genai::chat::ChatMessage::user(text));
            }
            crate::message::Role::Assistant => {
                chat = chat.append_message(genai::chat::ChatMessage::assistant(text));
            }
            crate::message::Role::System => {
                chat = chat.with_system(text);
            }
        }
    }
    chat
}

fn genai_usage(resp: &genai::chat::ChatResponse) -> Usage {
    let u = &resp.usage;
    Usage {
        input_tokens: u.prompt_tokens.unwrap_or(0) as u32,
        output_tokens: u.completion_tokens.unwrap_or(0) as u32,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
    }
}

fn genai_usage_from_end(end: &genai::chat::StreamEnd) -> Usage {
    end.captured_usage.as_ref().map_or_else(Usage::default, |u| Usage {
        input_tokens: u.prompt_tokens.unwrap_or(0) as u32,
        output_tokens: u.completion_tokens.unwrap_or(0) as u32,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
    })
}
