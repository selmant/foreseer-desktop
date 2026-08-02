use base64::Engine;
use jfn_rust::{ExternalFrontend, HostAuthService, HostOptions, JellyfinSessionBootstrap};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;

const DEFAULT_FRONTEND_URL: &str = "https://foreseer.selmantrabzon.com";

fn main() {
    configure_product_profile();
    let frontend_url =
        std::env::var("FORESEER_URL").unwrap_or_else(|_| DEFAULT_FRONTEND_URL.to_owned());
    let frontend = ExternalFrontend::new(&frontend_url).expect("valid FORESEER_URL");
    let auth_service = Arc::new(ForeseerAuthService::new(frontend_url));
    let options = HostOptions::with_external_frontend(frontend).with_auth_service(auth_service);
    std::process::exit(jfn_rust::app::jfn_app_main_with(options));
}

fn configure_product_profile() {
    mirror_env("FORESEER_LOG_LEVEL", "JELLIUM_DESKTOP_LOG_LEVEL");
    mirror_env("FORESEER_LOG_FILE", "JELLIUM_DESKTOP_LOG_FILE");
    mirror_env("FORESEER_CONFIG_DIR", "JELLIUM_DESKTOP_CONFIG_DIR");
    mirror_env("FORESEER_CACHE_DIR", "JELLIUM_DESKTOP_CACHE_DIR");
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
    base_url: String,
    verifier: String,
    challenge: String,
}

impl ForeseerAuthService {
    fn new(base_url: String) -> Self {
        let verifier = base64_url(&random_bytes());
        let challenge = hex_digest(verifier.as_bytes());
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            verifier,
            challenge,
        }
    }
}

impl HostAuthService for ForeseerAuthService {
    fn request_challenge(&self, _request_id: &str) -> Option<String> {
        Some(self.challenge.clone())
    }

    fn complete_auth(
        &self,
        _request_id: String,
        ticket: String,
        callback: Box<dyn FnOnce(Result<JellyfinSessionBootstrap, String>) + Send>,
    ) {
        let base_url = self.base_url.clone();
        let verifier = self.verifier.clone();
        std::thread::spawn(move || {
            let result = redeem_ticket(&base_url, &ticket, &verifier);
            callback(result);
        });
    }
}

fn redeem_ticket(
    base_url: &str,
    ticket: &str,
    verifier: &str,
) -> Result<JellyfinSessionBootstrap, String> {
    let response = ureq::post(format!("{base_url}/api/v1/desktop/auth-tickets/redeem"))
        .send_json(serde_json::json!({
            "ticket": ticket,
            "verifier": verifier,
            "protocolVersion": 1,
        }))
        .map_err(|_| "server_unreachable".to_owned())?;
    let body: Value = response
        .into_body()
        .read_json()
        .map_err(|_| "invalid_bootstrap_response".to_owned())?;
    let get = |name: &str| {
        body.get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "invalid_bootstrap_response".to_owned())
    };
    Ok(JellyfinSessionBootstrap {
        server_url: get("serverUrl")?,
        server_id: get("serverId")?,
        user_id: get("userId")?,
        device_id: get("deviceId")?,
        access_token: get("accessToken")?,
        bootstrap_generation: get("bootstrapGeneration")?,
    })
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
