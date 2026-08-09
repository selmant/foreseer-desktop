use base64::Engine;
use directories::ProjectDirs;
use jfn_rust::{
    ExternalFrontend, HostAuthError, HostAuthService, HostConfigService, HostOptions,
    JellyfinSessionBootstrap,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use url::Url;

mod setup;

const DEFAULT_FRONTEND_URL: &str = "https://foreseer.selmantrabzon.com";
const MAX_FORESEER_URL_LEN: usize = 2048;
const MAX_PENDING_AUTH_PROOFS: usize = 10;
const AUTH_PROOF_TTL: Duration = Duration::from_secs(60);
const PROTOCOL_VERSION: u8 = 1;
const BOOTSTRAP_ID_MAX_LEN: usize = 256;
const ACCESS_TOKEN_MAX_LEN: usize = 8192;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForeseerUrlError {
    Invalid,
    TooLong,
    UnsupportedScheme,
    MissingHost,
    CredentialsNotAllowed,
    InsecureHttpNotAllowed,
    InsecureHttpNonLocalHost,
}

impl ForeseerUrlError {
    const fn message(self) -> &'static str {
        match self {
            Self::Invalid => "Invalid server URL",
            Self::TooLong => "Server URL is too long",
            Self::UnsupportedScheme => "Server URL must use HTTP or HTTPS",
            Self::MissingHost => "Server URL must include a host",
            Self::CredentialsNotAllowed => "Server URL must not include credentials",
            Self::InsecureHttpNotAllowed => {
                "Server URL must use HTTPS unless HTTP is explicitly allowed"
            }
            Self::InsecureHttpNonLocalHost => {
                "HTTP is allowed only for localhost or a private IP address"
            }
        }
    }
}

fn validate_foreseer_url(
    input: &str,
    allow_insecure_http: bool,
) -> Result<String, ForeseerUrlError> {
    if input.is_empty() {
        return Err(ForeseerUrlError::Invalid);
    }
    if input.len() > MAX_FORESEER_URL_LEN {
        return Err(ForeseerUrlError::TooLong);
    }
    let parsed = Url::parse(input).map_err(|_| ForeseerUrlError::Invalid)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ForeseerUrlError::UnsupportedScheme);
    }
    let host = parsed.host_str().ok_or(ForeseerUrlError::MissingHost)?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ForeseerUrlError::CredentialsNotAllowed);
    }
    if parsed.scheme() == "http" {
        if !allow_insecure_http {
            return Err(ForeseerUrlError::InsecureHttpNotAllowed);
        }
        if !is_local_http_host(host) {
            return Err(ForeseerUrlError::InsecureHttpNonLocalHost);
        }
    }
    Ok(parsed.origin().ascii_serialization())
}

fn is_local_http_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => {
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
        }
        Err(_) => false,
    }
}

fn validate_bootstrap_server_url(input: &str) -> Result<String, ForeseerUrlError> {
    if input.is_empty() || input.len() > MAX_FORESEER_URL_LEN {
        return Err(ForeseerUrlError::Invalid);
    }
    let parsed = Url::parse(input).map_err(|_| ForeseerUrlError::Invalid)?;
    if parsed.scheme() != "https" {
        return Err(ForeseerUrlError::InsecureHttpNotAllowed);
    }
    if parsed.host_str().is_none() {
        return Err(ForeseerUrlError::MissingHost);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ForeseerUrlError::CredentialsNotAllowed);
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(ForeseerUrlError::Invalid);
    }
    Ok(parsed.to_string())
}

fn valid_request_id(request_id: &str) -> bool {
    !request_id.is_empty()
        && request_id.len() <= 64
        && request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub server_url: String,
    pub allow_insecure_http: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_FRONTEND_URL.to_string(),
            allow_insecure_http: false,
        }
    }
}

impl AppConfig {
    pub fn config_file_path() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("JELLIUM_DESKTOP_CONFIG_DIR")
            .or_else(|| std::env::var_os("FORESEER_CONFIG_DIR"))
        {
            return Some(PathBuf::from(dir).join("config.json"));
        }
        ProjectDirs::from("com", "selmantrabzon", "Foreseer")
            .map(|dirs| dirs.config_dir().join("config.json"))
    }

    pub fn exists() -> bool {
        Self::config_file_path().is_some_and(|p| p.exists())
    }

    pub fn is_configured(&self) -> bool {
        validate_foreseer_url(&self.server_url, self.allow_insecure_http).is_ok()
    }

    pub fn load() -> Self {
        let path = Self::config_file_path();
        let mut config = if let Some(ref p) = path
            && let Ok(content) = fs::read_to_string(p)
            && let Ok(cfg) = serde_json::from_str::<AppConfig>(&content)
        {
            cfg
        } else {
            Self::default()
        };

        // Environment variables override config file values
        if let Ok(env_url) = std::env::var("FORESEER_URL")
            && !env_url.trim().is_empty()
        {
            config.server_url = env_url;
        }
        if std::env::var("FORESEER_ALLOW_INSECURE_HTTP").as_deref() == Ok("1") {
            config.allow_insecure_http = true;
        }

        config
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        if let Some(path) = Self::config_file_path() {
            self.save_to(&path)?;
        }
        Ok(())
    }

    pub fn save_to(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)?;
        Ok(())
    }
}

fn main() {
    configure_product_profile();
    let cli_requested_setup = handle_cli_args();

    let config_exists = AppConfig::exists();
    let config = AppConfig::load();
    let needs_setup = cli_requested_setup || !config_exists || !config.is_configured();

    let (frontend, frontend_url, allow_insecure_http) = if needs_setup {
        let setup_html = setup::get_setup_html("");
        let base64_html = base64::engine::general_purpose::STANDARD.encode(setup_html);
        let url = format!("data:text/html;base64,{}", base64_html);
        let frontend = ExternalFrontend::setup_document(&url).expect("generated setup document");
        (frontend, url, true)
    } else {
        let url = validate_foreseer_url(&config.server_url, config.allow_insecure_http)
            .expect("validated configured Foreseer URL");
        let frontend = ExternalFrontend::new(&url).expect("validated configured Foreseer URL");
        (frontend, url, config.allow_insecure_http)
    };

    let auth_service = Arc::new(ForeseerAuthService::new(&frontend_url, allow_insecure_http));
    AUTH_SERVICE_INSTANCE.set(Arc::clone(&auth_service)).ok();
    let mut options = HostOptions::with_external_frontend(frontend).with_auth_service(auth_service);
    if needs_setup {
        options = options.with_config_service(Arc::new(ForeseerConfigService::new()));
    }
    std::process::exit(jfn_rust::app::jfn_app_main_with(options));
}

fn handle_cli_args() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return false;
    }

    match args[1].as_str() {
        "--setup" => return true,
        "--help" | "-h" => {
            println!("Foreseer Desktop Client");
            println!();
            println!("USAGE:");
            println!("  foreseer-desktop [OPTIONS]");
            println!();
            println!("OPTIONS:");
            println!("  --setup            Open the graphical server setup page");
            println!("  --set-url <URL>    Set target Foreseer server URL in config file and exit");
            println!("  --allow-http       Allow insecure HTTP when saving server URL");
            println!("  --show-config      Display current config file path and settings");
            println!("  --help, -h         Show this help message");
            println!();
            println!("ENVIRONMENT VARIABLES:");
            println!("  FORESEER_URL                 Override server URL at launch");
            println!("  FORESEER_ALLOW_INSECURE_HTTP Set to 1 to allow non-HTTPS server URLs");
            std::process::exit(0);
        }
        "--show-config" => {
            let path = AppConfig::config_file_path();
            let config = AppConfig::load();
            println!(
                "Config Path: {}",
                path.map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Unknown".into())
            );
            println!("Server URL:  {}", config.server_url);
            println!("Allow HTTP:  {}", config.allow_insecure_http);
            std::process::exit(0);
        }
        "--set-url" => {
            if args.len() < 3 {
                eprintln!(
                    "Error: --set-url requires a URL argument (e.g. --set-url https://my-server.com)"
                );
                std::process::exit(1);
            }
            let url = args[2].clone();
            let allow_http = args.iter().any(|arg| arg == "--allow-http");
            let url = match validate_foreseer_url(&url, allow_http) {
                Ok(url) => url,
                Err(error) => {
                    eprintln!("Error: {}", error.message());
                    std::process::exit(1);
                }
            };

            let mut config = AppConfig::load();
            config.server_url = url.clone();
            config.allow_insecure_http = allow_http;
            if let Err(e) = config.save() {
                eprintln!("Error saving config: {}", e);
                std::process::exit(1);
            }
            println!("Successfully saved server URL to config.");
            std::process::exit(0);
        }
        _ => {}
    }
    false
}

fn configure_product_profile() {
    set_default_env("JELLIUM_DESKTOP_TITLE", "Foreseer".as_ref());
    set_default_env(
        "JELLIUM_DESKTOP_APP_ID",
        "com.selmantrabzon.Foreseer".as_ref(),
    );
    mirror_env("FORESEER_LOG_LEVEL", "JELLIUM_DESKTOP_LOG_LEVEL");
    mirror_env("FORESEER_LOG_FILE", "JELLIUM_DESKTOP_LOG_FILE");
    mirror_env("FORESEER_CONFIG_DIR", "JELLIUM_DESKTOP_CONFIG_DIR");
    mirror_env("FORESEER_CACHE_DIR", "JELLIUM_DESKTOP_CACHE_DIR");
    mirror_env("FORESEER_PLATFORM_PAINT", "JELLIUM_DESKTOP_PLATFORM_PAINT");
    mirror_env("FORESEER_MPV_HOME", "MPV_HOME");
    set_default_env(
        "JELLIUM_DESKTOP_HOST_VERSION",
        env!("CARGO_PKG_VERSION").as_ref(),
    );
    set_default_env(
        "JELLIUM_DESKTOP_HOST_JELLIUM_REVISION",
        include_str!("../jellium.rev").trim().as_ref(),
    );
    if let Some(project_dirs) = ProjectDirs::from("com", "selmantrabzon", "Foreseer") {
        set_default_env(
            "JELLIUM_DESKTOP_CONFIG_DIR",
            project_dirs.config_dir().as_os_str(),
        );
        set_default_env(
            "JELLIUM_DESKTOP_CACHE_DIR",
            project_dirs.cache_dir().as_os_str(),
        );
    }
}

fn set_default_env(target: &str, value: &std::ffi::OsStr) {
    if std::env::var_os(target).is_none() {
        // Startup is single-threaded here; Jellium reads these aliases later.
        unsafe { std::env::set_var(target, value) };
    }
}

fn mirror_env(source: &str, target: &str) {
    if std::env::var_os(target).is_none()
        && let Some(value) = std::env::var_os(source)
    {
        // These are product-level aliases for Jellium's existing bounded
        // startup options; explicit Jellium variables still take precedence.
        unsafe { std::env::set_var(target, value) };
    }
}

static AUTH_SERVICE_INSTANCE: std::sync::OnceLock<Arc<ForeseerAuthService>> =
    std::sync::OnceLock::new();

pub fn update_auth_service_frontend_url(url: &str, allow_insecure_http: bool) {
    if let Some(service) = AUTH_SERVICE_INSTANCE.get() {
        service.update_frontend_url(url, allow_insecure_http);
    }
}

struct ForeseerConfigService {
    agent: ureq::Agent,
}

impl ForeseerConfigService {
    fn new() -> Self {
        let config = setup_connectivity_agent();
        Self {
            agent: config.into(),
        }
    }
}

/// Setup may probe only the user-entered endpoint. Following a redirect would
/// turn that one confirmation into an unvalidated request to a different host,
/// scheme, or private address, so redirects are deliberately disabled.
fn setup_connectivity_agent() -> ureq::config::Config {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
}

impl HostConfigService for ForeseerConfigService {
    fn save_server_url(
        &self,
        _request_id: &str,
        url_str: &str,
        allow_http: bool,
    ) -> Result<(), String> {
        let url = validate_foreseer_url(url_str, allow_http)
            .map_err(|error| error.message().to_string())?;

        let mut config = AppConfig::load();
        config.server_url = url.clone();
        config.allow_insecure_http = allow_http;
        if let Err(e) = config.save() {
            return Err(e.to_string());
        }

        update_auth_service_frontend_url(&url, allow_http);
        let url_clone = url;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            jfn_rust::jfn_external_complete_setup_navigation(&url_clone);
        });

        Ok(())
    }

    fn check_server_connectivity(
        &self,
        _request_id: String,
        url_str: String,
        allow_http: bool,
        callback: Box<dyn FnOnce(Result<u16, String>) + Send>,
    ) {
        let url = match validate_foreseer_url(&url_str, allow_http) {
            Ok(url) => url,
            Err(error) => {
                callback(Err(error.message().to_string()));
                return;
            }
        };
        let parsed = match Url::parse(&url) {
            Ok(parsed) => parsed,
            Err(_) => {
                callback(Err("Invalid server URL".to_string()));
                return;
            }
        };

        let test_url = parsed
            .join("api/v1/desktop/auth-tickets/redeem")
            .unwrap_or(parsed)
            .to_string();

        let agent = self.agent.clone();
        std::thread::spawn(move || {
            let resp = agent.post(&test_url).send_json(serde_json::json!({
                "ticket": "test",
                "verifier": "test",
                "protocolVersion": PROTOCOL_VERSION,
            }));
            match resp {
                Ok(r) => callback(Ok(r.status().as_u16())),
                Err(e) => callback(Err(e.to_string())),
            }
        });
    }
}

struct ForeseerAuthService {
    redeem_url: RwLock<String>,
    proofs: RwLock<PendingAuthProofs>,
    agent: ureq::Agent,
    auth_epoch: Arc<AtomicU64>,
    allow_insecure_http: AtomicBool,
}

struct AuthProof {
    verifier: String,
    challenge: String,
    created_at: Instant,
}

#[derive(Default)]
struct PendingAuthProofs {
    by_request_id: HashMap<String, AuthProof>,
}

impl PendingAuthProofs {
    fn purge_expired(&mut self, now: Instant) {
        self.by_request_id
            .retain(|_, proof| now.duration_since(proof.created_at) <= AUTH_PROOF_TTL);
    }

    fn insert(&mut self, request_id: &str, proof: AuthProof, now: Instant) -> Option<()> {
        self.purge_expired(now);
        if self.by_request_id.contains_key(request_id)
            || self.by_request_id.len() >= MAX_PENDING_AUTH_PROOFS
        {
            return None;
        }
        self.by_request_id.insert(request_id.to_string(), proof);
        Some(())
    }

    fn take(&mut self, request_id: &str, now: Instant) -> Option<AuthProof> {
        self.purge_expired(now);
        self.by_request_id.remove(request_id)
    }

    fn clear(&mut self) {
        self.by_request_id.clear();
    }
}

fn new_auth_proof() -> AuthProof {
    let verifier = base64_url(&random_bytes());
    let challenge = hex_digest(verifier.as_bytes());
    AuthProof {
        verifier,
        challenge,
        created_at: Instant::now(),
    }
}

impl ForeseerAuthService {
    fn new(frontend_url: &str, allow_insecure_http: bool) -> Self {
        let redeem_url = redemption_url(frontend_url).expect("validated FORESEER_URL");
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build();
        Self {
            redeem_url: RwLock::new(redeem_url),
            proofs: RwLock::new(PendingAuthProofs::default()),
            agent: config.into(),
            auth_epoch: Arc::new(AtomicU64::new(0)),
            allow_insecure_http: AtomicBool::new(allow_insecure_http),
        }
    }

    fn update_frontend_url(&self, frontend_url: &str, allow_insecure_http: bool) {
        if let Ok(new_redeem) = redemption_url(frontend_url)
            && let Ok(mut redeem) = self.redeem_url.write()
        {
            *redeem = new_redeem;
            self.allow_insecure_http
                .store(allow_insecure_http, Ordering::Relaxed);
        }
    }
}

impl HostAuthService for ForeseerAuthService {
    fn request_challenge(&self, request_id: &str) -> Option<String> {
        if !valid_request_id(request_id) {
            return None;
        }
        let proof = new_auth_proof();
        let challenge = proof.challenge.clone();
        let mut proofs = self.proofs.write().ok()?;
        proofs.insert(request_id, proof, Instant::now())?;
        Some(challenge)
    }

    fn clear_session(&self) {
        if let Ok(mut proofs) = self.proofs.write() {
            proofs.clear();
        }
        self.auth_epoch.fetch_add(1, Ordering::AcqRel);
    }

    fn complete_auth(
        &self,
        request_id: String,
        ticket: String,
        callback: Box<dyn FnOnce(Result<JellyfinSessionBootstrap, HostAuthError>) + Send>,
    ) {
        if !valid_request_id(&request_id) {
            callback(Err(HostAuthError::InvalidRequest));
            return;
        }
        let redeem_url = self
            .redeem_url
            .read()
            .ok()
            .map(|r| r.clone())
            .unwrap_or_default();
        let epoch_at_start = self.auth_epoch.load(Ordering::Acquire);
        let verifier = {
            let mut proofs = match self.proofs.write() {
                Ok(p) => p,
                Err(_) => {
                    callback(Err(HostAuthError::InvalidRequest));
                    return;
                }
            };
            let Some(proof) = proofs.take(&request_id, Instant::now()) else {
                callback(Err(HostAuthError::InvalidRequest));
                return;
            };
            proof.verifier
        };
        if self.auth_epoch.load(Ordering::Acquire) != epoch_at_start {
            callback(Err(HostAuthError::SessionExpired));
            return;
        }
        let agent = self.agent.clone();
        let auth_epoch = Arc::clone(&self.auth_epoch);
        let allow_insecure_http = self.allow_insecure_http.load(Ordering::Relaxed);
        std::thread::spawn(move || {
            let mut result =
                redeem_ticket(&agent, &redeem_url, &ticket, &verifier, allow_insecure_http);
            if auth_epoch.load(Ordering::Acquire) != epoch_at_start {
                result = Err(HostAuthError::SessionExpired);
            }
            callback(result);
        });
    }
}

fn redeem_ticket(
    agent: &ureq::Agent,
    redeem_url: &str,
    ticket: &str,
    verifier: &str,
    _allow_insecure_http: bool,
) -> Result<JellyfinSessionBootstrap, HostAuthError> {
    let mut response = match agent.post(redeem_url).send_json(serde_json::json!({
        "ticket": ticket,
        "verifier": verifier,
        "protocolVersion": PROTOCOL_VERSION,
    })) {
        Ok(r) => r,
        Err(_) => {
            tracing::warn!("Foreseer redeem POST failed");
            return Err(HostAuthError::ServerUnreachable);
        }
    };
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body: RedemptionErrorResponse = response
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_json()
            .unwrap_or_default();
        let error = map_server_error(body.code.as_deref());
        tracing::warn!(
            status,
            error_code = error.code(),
            "Foreseer redeem rejected"
        );
        return Err(error);
    }
    let body: RedemptionBootstrapResponse = match response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_json()
    {
        Ok(body) => body,
        Err(_) => {
            tracing::warn!(status, "Foreseer redeem response has an invalid schema");
            return Err(HostAuthError::InvalidBootstrapResponse);
        }
    };
    parse_redemption_bootstrap(body)
}

#[derive(Default, Deserialize)]
struct RedemptionErrorResponse {
    code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RedemptionBootstrapResponse {
    server_url: Option<String>,
    server_id: Option<String>,
    user_id: Option<String>,
    device_id: Option<String>,
    access_token: Option<String>,
    bootstrap_generation: Option<String>,
}

fn required_bootstrap_field(
    name: &'static str,
    value: Option<String>,
    max_len: usize,
) -> Result<String, HostAuthError> {
    value
        .filter(|value| !value.is_empty() && value.len() <= max_len)
        .ok_or_else(|| {
            tracing::warn!(
                field = name,
                "Foreseer redeem has an invalid bootstrap field"
            );
            HostAuthError::InvalidBootstrapResponse
        })
}

fn parse_redemption_bootstrap(
    body: RedemptionBootstrapResponse,
) -> Result<JellyfinSessionBootstrap, HostAuthError> {
    let server_url = required_bootstrap_field("serverUrl", body.server_url, MAX_FORESEER_URL_LEN)?;
    let server_url = match validate_bootstrap_server_url(&server_url) {
        Ok(server_url) => server_url,
        Err(_) => {
            tracing::warn!(
                field = "serverUrl",
                "Foreseer redeem has an invalid bootstrap field"
            );
            return Err(HostAuthError::InvalidBootstrapResponse);
        }
    };
    Ok(JellyfinSessionBootstrap {
        server_url,
        server_id: required_bootstrap_field("serverId", body.server_id, BOOTSTRAP_ID_MAX_LEN)?,
        user_id: required_bootstrap_field("userId", body.user_id, BOOTSTRAP_ID_MAX_LEN)?,
        device_id: required_bootstrap_field("deviceId", body.device_id, BOOTSTRAP_ID_MAX_LEN)?,
        access_token: required_bootstrap_field(
            "accessToken",
            body.access_token,
            ACCESS_TOKEN_MAX_LEN,
        )?,
        bootstrap_generation: required_bootstrap_field(
            "bootstrapGeneration",
            body.bootstrap_generation,
            BOOTSTRAP_ID_MAX_LEN,
        )?,
    })
}

fn redemption_url(frontend_url: &str) -> Result<String, url::ParseError> {
    let mut origin = Url::parse(frontend_url)?;
    if origin.scheme() == "data" {
        return Ok(String::new());
    }
    origin.set_path("/");
    origin.set_query(None);
    origin.set_fragment(None);
    origin
        .join("api/v1/desktop/auth-tickets/redeem")
        .map(|url| url.to_string())
}

fn map_server_error(code: Option<&str>) -> HostAuthError {
    match code {
        Some("session_expired" | "session_required") => HostAuthError::SessionExpired,
        Some("ticket_expired") => HostAuthError::TicketExpired,
        Some("ticket_used") => HostAuthError::TicketUsed,
        Some("not_linked") => HostAuthError::NotLinked,
        Some("token_invalid") => HostAuthError::TokenInvalid,
        Some("server_unreachable") => HostAuthError::ServerUnreachable,
        Some("unsupported_media_server") => HostAuthError::UnsupportedMediaServer,
        _ => HostAuthError::InvalidRequest,
    }
}

fn random_bytes() -> [u8; 32] {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).expect("OS random source available");
    bytes
}

fn base64_url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

    struct CaptureGuard(Arc<Mutex<Vec<u8>>>);

    impl Write for CaptureGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("capture lock").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureGuard;

        fn make_writer(&'a self) -> Self::Writer {
            CaptureGuard(Arc::clone(&self.0))
        }
    }

    fn capture_logs<T>(run: impl FnOnce() -> T) -> (T, String) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(CaptureWriter(Arc::clone(&captured)))
            .finish();
        let result = tracing::subscriber::with_default(subscriber, run);
        let logs =
            String::from_utf8(captured.lock().expect("capture lock").clone()).expect("UTF-8 logs");
        (result, logs)
    }

    fn valid_bootstrap_response(token: &str) -> RedemptionBootstrapResponse {
        RedemptionBootstrapResponse {
            server_url: Some("https://jellyfin.example".to_string()),
            server_id: Some("server".to_string()),
            user_id: Some("user".to_string()),
            device_id: Some("device".to_string()),
            access_token: Some(token.to_string()),
            bootstrap_generation: Some("generation".to_string()),
        }
    }

    fn redeem_against_response(
        status: u16,
        body: &str,
    ) -> Result<JellyfinSessionBootstrap, HostAuthError> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener");
        let address = listener.local_addr().expect("test listener address");
        let body = body.to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("one redemption request");
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).expect("read redemption request");
            let reason = if status == 200 { "OK" } else { "Bad Request" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write redemption response");
        });
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(2)))
            .http_status_as_error(false)
            .build()
            .into();
        let result = redeem_ticket(
            &agent,
            &format!("http://{address}/redeem"),
            "test-ticket",
            "test-verifier",
            false,
        );
        server.join().expect("redemption server exits");
        result
    }

    #[test]
    fn redemption_endpoint_uses_only_the_frontend_origin() {
        assert_eq!(
            redemption_url("https://foreseer.example/app?source=desktop#top").unwrap(),
            "https://foreseer.example/api/v1/desktop/auth-tickets/redeem"
        );
    }

    #[test]
    fn maps_only_closed_server_error_codes() {
        assert_eq!(
            map_server_error(Some("ticket_used")),
            HostAuthError::TicketUsed
        );
        assert_eq!(
            map_server_error(Some("server_unreachable")),
            HostAuthError::ServerUnreachable
        );
        assert_eq!(
            map_server_error(Some("secret-token-from-upstream")),
            HostAuthError::InvalidRequest
        );
    }

    #[test]
    fn protocol_v1_fixture_matches_host_envelope_limits_and_package_version() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../protocol/protocol-v1.json"))
                .expect("valid protocol fixture");
        assert_eq!(
            fixture["fixtureId"],
            "foreseer-native-protocol-v1-2026-08-08"
        );
        assert_eq!(fixture["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(fixture["host"]["name"], "jellium-desktop");
        assert_eq!(fixture["host"]["versionSource"], "package-metadata");
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.2.0");
        assert_eq!(fixture["limits"]["requestIdMaxLength"], 64);
        assert_eq!(
            fixture["limits"]["serverUrlMaxLength"],
            MAX_FORESEER_URL_LEN
        );
        assert_eq!(
            fixture["limits"]["bootstrapIdMaxLength"],
            BOOTSTRAP_ID_MAX_LEN
        );
        assert_eq!(
            fixture["limits"]["accessTokenMaxLength"],
            ACCESS_TOKEN_MAX_LEN
        );
        assert_eq!(
            fixture["bootstrapEnvelope"]["wireFields"],
            serde_json::json!([
                "serverUrl",
                "serverId",
                "userId",
                "deviceId",
                "accessToken",
                "bootstrapGeneration"
            ])
        );
        assert_eq!(
            fixture["envelopes"]["setupEventFields"],
            serde_json::json!(["status", "message"])
        );

        let typed: RedemptionBootstrapResponse = serde_json::from_value(serde_json::json!({
            "serverUrl": "https://jellyfin.example",
            "serverId": "server",
            "userId": "user",
            "deviceId": "device",
            "accessToken": "token",
            "bootstrapGeneration": "generation",
        }))
        .expect("fixture wire fields deserialize into the typed envelope");
        let bootstrap = parse_redemption_bootstrap(typed).expect("typed bootstrap is valid");
        assert_eq!(bootstrap.server_url, "https://jellyfin.example/");
        assert_eq!(bootstrap.server_id, "server");
        assert_eq!(bootstrap.user_id, "user");
        assert_eq!(bootstrap.device_id, "device");
        assert_eq!(bootstrap.access_token, "token");
        assert_eq!(bootstrap.bootstrap_generation, "generation");
    }

    #[test]
    fn insecure_foreseer_urls_require_a_local_explicit_override() {
        assert_eq!(
            validate_foreseer_url("http://foreseer.test", false),
            Err(ForeseerUrlError::InsecureHttpNotAllowed)
        );
        assert_eq!(
            validate_foreseer_url("http://foreseer.test", true),
            Err(ForeseerUrlError::InsecureHttpNonLocalHost)
        );
        assert_eq!(
            validate_foreseer_url("http://192.168.1.10", true),
            Ok("http://192.168.1.10".to_string())
        );
        assert_eq!(
            validate_foreseer_url("https://foreseer.test/path?ignored=yes", false),
            Ok("https://foreseer.test".to_string())
        );
    }

    #[test]
    fn app_config_serialization_roundtrip() {
        let config = AppConfig {
            server_url: "https://my-server.example.com".to_string(),
            allow_insecure_http: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn app_config_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let config = AppConfig {
            server_url: "https://custom.example.com".to_string(),
            allow_insecure_http: false,
        };
        config.save_to(&config_path).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let loaded: AppConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    fn app_config_is_configured_check() {
        let empty_config = AppConfig {
            server_url: "  ".to_string(),
            allow_insecure_http: false,
        };
        assert!(!empty_config.is_configured());

        let custom_config = AppConfig {
            server_url: "https://foreseer.my-domain.com".to_string(),
            allow_insecure_http: false,
        };
        assert!(custom_config.is_configured());
    }

    #[test]
    fn rejects_credential_bearing_urls_before_persisting_them() {
        assert_eq!(
            validate_foreseer_url("https://user:password@foreseer.example", false),
            Err(ForeseerUrlError::CredentialsNotAllowed)
        );
        assert_eq!(
            validate_foreseer_url("https://foreseer.example/one/two?x=y#fragment", false),
            Ok("https://foreseer.example".to_string())
        );
    }

    #[test]
    fn bootstrap_urls_are_always_https_even_when_frontend_http_is_allowed() {
        assert_eq!(
            validate_bootstrap_server_url("http://192.168.1.10:8096"),
            Err(ForeseerUrlError::InsecureHttpNotAllowed)
        );
        assert_eq!(
            validate_bootstrap_server_url("https://jellyfin.example/base"),
            Ok("https://jellyfin.example/base".to_string())
        );
    }

    #[test]
    fn setup_connectivity_does_not_follow_cross_origin_redirects() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener");
        let address = listener.local_addr().expect("test listener address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("one connectivity request");
            let mut request = [0_u8; 2048];
            let _ = stream
                .read(&mut request)
                .expect("read connectivity request");
            stream
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .expect("write redirect response");
        });
        let agent: ureq::Agent = setup_connectivity_agent().into();
        let response = agent
            .post(&format!("http://{address}/"))
            .send_json(serde_json::json!({"probe": true}))
            .expect("redirect response is returned without a second request");
        assert_eq!(response.status().as_u16(), 302);
        server.join().expect("redirect server exits");
    }

    #[test]
    fn auth_proofs_are_request_correlated_and_expire() {
        let now = Instant::now();
        let mut proofs = PendingAuthProofs::default();
        let first = AuthProof {
            verifier: "first-verifier".to_string(),
            challenge: "first-challenge".to_string(),
            created_at: now,
        };
        let second = AuthProof {
            verifier: "second-verifier".to_string(),
            challenge: "second-challenge".to_string(),
            created_at: now,
        };
        assert!(proofs.insert("first", first, now).is_some());
        assert!(proofs.insert("second", second, now).is_some());
        assert!(proofs.insert("first", new_auth_proof(), now).is_none());
        assert_eq!(
            proofs.take("second", now).map(|proof| proof.verifier),
            Some("second-verifier".to_string())
        );
        assert_eq!(
            proofs.take("first", now).map(|proof| proof.verifier),
            Some("first-verifier".to_string())
        );

        let expired = AuthProof {
            verifier: "expired".to_string(),
            challenge: "expired".to_string(),
            created_at: now - AUTH_PROOF_TTL - Duration::from_secs(1),
        };
        assert!(proofs.insert("expired", expired, now).is_some());
        assert!(
            proofs
                .take("expired", now + AUTH_PROOF_TTL + Duration::from_secs(1))
                .is_none()
        );
    }

    #[test]
    fn every_invalid_bootstrap_field_path_redacts_the_access_token() {
        let token = "access-token-sentinel-must-not-log";
        for field in [
            "serverUrl",
            "serverId",
            "userId",
            "deviceId",
            "accessToken",
            "bootstrapGeneration",
        ] {
            for oversized in [false, true] {
                let mut body = valid_bootstrap_response(token);
                let value = if oversized {
                    Some(match field {
                        "serverUrl" => format!("https://jellyfin.example/{}", "x".repeat(2048)),
                        "accessToken" => format!("{token}{}", "x".repeat(ACCESS_TOKEN_MAX_LEN)),
                        _ => "x".repeat(BOOTSTRAP_ID_MAX_LEN + 1),
                    })
                } else {
                    None
                };
                match field {
                    "serverUrl" => body.server_url = value,
                    "serverId" => body.server_id = value,
                    "userId" => body.user_id = value,
                    "deviceId" => body.device_id = value,
                    "accessToken" => body.access_token = value,
                    "bootstrapGeneration" => body.bootstrap_generation = value,
                    _ => unreachable!(),
                }
                let (result, logs) = capture_logs(|| parse_redemption_bootstrap(body));
                assert!(
                    matches!(result, Err(HostAuthError::InvalidBootstrapResponse)),
                    "{field} oversized={oversized}"
                );
                assert!(!logs.contains(token), "{field} oversized={oversized}");
                assert!(logs.contains(field), "{field} oversized={oversized}");
            }
        }
    }

    #[test]
    fn invalid_bootstrap_urls_never_log_the_access_token() {
        let token = "url-token-sentinel-must-not-log";
        for invalid_url in [
            "not a url",
            "http://jellyfin.example",
            "https://user:password@jellyfin.example",
            "https://jellyfin.example/?token=secret",
        ] {
            let mut body = valid_bootstrap_response(token);
            body.server_url = Some(invalid_url.to_string());
            let (result, logs) = capture_logs(|| parse_redemption_bootstrap(body));
            assert!(matches!(
                result,
                Err(HostAuthError::InvalidBootstrapResponse)
            ));
            assert!(!logs.contains(token), "invalid URL: {invalid_url}");
            assert!(!logs.contains(invalid_url), "invalid URL: {invalid_url}");
            assert!(logs.contains("serverUrl"));
        }
    }

    #[test]
    fn non_success_and_malformed_redemption_bodies_never_enter_logs() {
        let token = "wire-token-sentinel-must-not-log";
        let non_success = format!(r#"{{"code":"ticket_used","accessToken":"{token}"}}"#);
        let (result, logs) = capture_logs(|| redeem_against_response(400, &non_success));
        assert!(matches!(result, Err(HostAuthError::TicketUsed)));
        assert!(!logs.contains(token));
        assert!(!logs.contains(&non_success));

        let malformed = format!(r#"{{"accessToken":"{token}","serverUrl":}}"#);
        let (result, logs) = capture_logs(|| redeem_against_response(200, &malformed));
        assert!(matches!(
            result,
            Err(HostAuthError::InvalidBootstrapResponse)
        ));
        assert!(!logs.contains(token));
        assert!(!logs.contains(&malformed));
        assert!(logs.contains("invalid schema"));
    }
}
