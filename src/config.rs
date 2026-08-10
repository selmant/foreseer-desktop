//! Product configuration and Foreseer URL validation.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use url::Url;

pub const DEFAULT_FRONTEND_URL: &str = "https://foreseer.selmantrabzon.com";
pub const MAX_FORESEER_URL_LEN: usize = 2048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForeseerUrlError {
    Invalid,
    TooLong,
    UnsupportedScheme,
    MissingHost,
    CredentialsNotAllowed,
    InsecureHttpNotAllowed,
    InsecureHttpNonLocalHost,
}

impl ForeseerUrlError {
    pub const fn message(self) -> &'static str {
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

pub fn validate_foreseer_url(
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

pub fn validate_bootstrap_server_url(input: &str) -> Result<String, ForeseerUrlError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insecure_foreseer_urls_require_local_override() {
        assert_eq!(
            validate_foreseer_url("http://example.com", false).unwrap_err(),
            ForeseerUrlError::InsecureHttpNotAllowed
        );
        assert!(validate_foreseer_url("http://127.0.0.1", true).is_ok());
        assert_eq!(
            validate_foreseer_url("http://example.com", true).unwrap_err(),
            ForeseerUrlError::InsecureHttpNonLocalHost
        );
    }

    #[test]
    fn rejects_credential_bearing_urls() {
        assert_eq!(
            validate_foreseer_url("https://user:pass@foreseer.example", false).unwrap_err(),
            ForeseerUrlError::CredentialsNotAllowed
        );
    }

    #[test]
    fn bootstrap_urls_are_always_https() {
        assert_eq!(
            validate_bootstrap_server_url("http://jellyfin.example").unwrap_err(),
            ForeseerUrlError::InsecureHttpNotAllowed
        );
        assert!(validate_bootstrap_server_url("https://jellyfin.example/").is_ok());
    }

    #[test]
    fn app_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = AppConfig {
            server_url: "https://foreseer.example".into(),
            allow_insecure_http: false,
        };
        cfg.save_to(&path).unwrap();
        let loaded: AppConfig = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, cfg);
    }
}
