use base64::Engine;
use directories::ProjectDirs;
use jfn_rust::{
    ExternalFrontend, HostAuthError, HostAuthService, HostOptions, JellyfinSessionBootstrap,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use url::Url;

const DEFAULT_FRONTEND_URL: &str = "https://foreseer.selmantrabzon.com";

fn main() {
    configure_product_profile();
    let frontend_url =
        std::env::var("FORESEER_URL").unwrap_or_else(|_| DEFAULT_FRONTEND_URL.to_owned());
    let frontend = ExternalFrontend::new(&frontend_url).expect("valid FORESEER_URL");
    let allow_insecure_http = std::env::var("FORESEER_ALLOW_INSECURE_HTTP").as_deref() == Ok("1");
    let parsed_frontend = Url::parse(&frontend_url).expect("validated FORESEER_URL");
    assert!(
        secure_url_allowed(&parsed_frontend, allow_insecure_http),
        "FORESEER_URL must use HTTPS unless FORESEER_ALLOW_INSECURE_HTTP=1"
    );
    let auth_service = Arc::new(ForeseerAuthService::new(&frontend_url, allow_insecure_http));
    let options = HostOptions::with_external_frontend(frontend).with_auth_service(auth_service);
    std::process::exit(jfn_rust::app::jfn_app_main_with(options));
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

struct ForeseerAuthService {
    redeem_url: String,
    proof: RwLock<AuthProof>,
    agent: ureq::Agent,
    in_flight: Arc<AtomicBool>,
    auth_epoch: Arc<AtomicU64>,
    allow_insecure_http: bool,
}

struct AuthProof {
    verifier: String,
    challenge: String,
}

fn new_auth_proof() -> AuthProof {
    let verifier = base64_url(&random_bytes());
    let challenge = hex_digest(verifier.as_bytes());
    AuthProof {
        verifier,
        challenge,
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
            redeem_url,
            proof: RwLock::new(new_auth_proof()),
            agent: config.into(),
            in_flight: Arc::new(AtomicBool::new(false)),
            auth_epoch: Arc::new(AtomicU64::new(0)),
            allow_insecure_http,
        }
    }
}

impl HostAuthService for ForeseerAuthService {
    fn request_challenge(&self, _request_id: &str) -> Option<String> {
        self.proof.read().ok().map(|proof| proof.challenge.clone())
    }

    fn clear_session(&self) {
        if let Ok(mut proof) = self.proof.write() {
            *proof = new_auth_proof();
        }
        self.auth_epoch.fetch_add(1, Ordering::AcqRel);
    }

    fn complete_auth(
        &self,
        _request_id: String,
        ticket: String,
        callback: Box<dyn FnOnce(Result<JellyfinSessionBootstrap, HostAuthError>) + Send>,
    ) {
        if self
            .in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            callback(Err(HostAuthError::InvalidRequest));
            return;
        }
        let redeem_url = self.redeem_url.clone();
        let epoch_at_start = self.auth_epoch.load(Ordering::Acquire);
        let Some(verifier) = self.proof.read().ok().map(|proof| proof.verifier.clone()) else {
            self.in_flight.store(false, Ordering::Release);
            callback(Err(HostAuthError::InvalidRequest));
            return;
        };
        if self.auth_epoch.load(Ordering::Acquire) != epoch_at_start {
            self.in_flight.store(false, Ordering::Release);
            callback(Err(HostAuthError::SessionExpired));
            return;
        }
        let agent = self.agent.clone();
        let in_flight = Arc::clone(&self.in_flight);
        let auth_epoch = Arc::clone(&self.auth_epoch);
        let allow_insecure_http = self.allow_insecure_http;
        std::thread::spawn(move || {
            let mut result =
                redeem_ticket(&agent, &redeem_url, &ticket, &verifier, allow_insecure_http);
            if auth_epoch.load(Ordering::Acquire) != epoch_at_start {
                result = Err(HostAuthError::SessionExpired);
            }
            in_flight.store(false, Ordering::Release);
            callback(result);
        });
    }
}

fn redeem_ticket(
    agent: &ureq::Agent,
    redeem_url: &str,
    ticket: &str,
    verifier: &str,
    allow_insecure_http: bool,
) -> Result<JellyfinSessionBootstrap, HostAuthError> {
    let mut response = agent
        .post(redeem_url)
        .send_json(serde_json::json!({
            "ticket": ticket,
            "verifier": verifier,
            "protocolVersion": 1,
        }))
        .map_err(|_| HostAuthError::ServerUnreachable)?;
    let status = response.status().as_u16();
    let body: Value = response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_json()
        .map_err(|_| HostAuthError::InvalidBootstrapResponse)?;
    if !(200..300).contains(&status) {
        return Err(map_server_error(body.get("code").and_then(Value::as_str)));
    }
    let get = |name: &str, max_len: usize| {
        body.get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= max_len)
            .map(str::to_owned)
            .ok_or(HostAuthError::InvalidBootstrapResponse)
    };
    let server_url = get("serverUrl", 2048)?;
    let parsed_server =
        Url::parse(&server_url).map_err(|_| HostAuthError::InvalidBootstrapResponse)?;
    if !secure_url_allowed(&parsed_server, allow_insecure_http)
        || parsed_server.host_str().is_none()
        || !parsed_server.username().is_empty()
        || parsed_server.password().is_some()
        || parsed_server.query().is_some()
        || parsed_server.fragment().is_some()
    {
        return Err(HostAuthError::InvalidBootstrapResponse);
    }
    Ok(JellyfinSessionBootstrap {
        server_url,
        server_id: get("serverId", 256)?,
        user_id: get("userId", 256)?,
        device_id: get("deviceId", 256)?,
        access_token: get("accessToken", 8192)?,
        bootstrap_generation: get("bootstrapGeneration", 256)?,
    })
}

fn secure_url_allowed(url: &Url, allow_insecure_http: bool) -> bool {
    url.scheme() == "https" || (allow_insecure_http && url.scheme() == "http")
}

fn redemption_url(frontend_url: &str) -> Result<String, url::ParseError> {
    let mut origin = Url::parse(frontend_url)?;
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
    fn insecure_urls_require_an_explicit_override() {
        let http = Url::parse("http://jellyfin.test").unwrap();
        let https = Url::parse("https://jellyfin.test").unwrap();
        assert!(!secure_url_allowed(&http, false));
        assert!(secure_url_allowed(&http, true));
        assert!(secure_url_allowed(&https, false));
    }
}
