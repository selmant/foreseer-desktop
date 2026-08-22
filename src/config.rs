//! Product configuration, migration, and Foreseer URL validation.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use url::Url;

pub const DEFAULT_FRONTEND_URL: &str = "https://foreseer.example.com";
pub const MAX_FORESEER_URL_LEN: usize = 2048;
pub const CONFIG_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MIN_CACHE_LIMIT_BYTES: u64 = 128 * 1024 * 1024;

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
    if parsed.scheme() == "http" && (!allow_insecure_http || !is_local_http_host(host)) {
        return Err(if allow_insecure_http {
            ForeseerUrlError::InsecureHttpNonLocalHost
        } else {
            ForeseerUrlError::InsecureHttpNotAllowed
        });
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
    let host = parsed.host_str().ok_or(ForeseerUrlError::MissingHost)?;
    match parsed.scheme() {
        "https" => {}
        "http" if is_local_http_host(host) => {}
        "http" => return Err(ForeseerUrlError::InsecureHttpNonLocalHost),
        _ => return Err(ForeseerUrlError::UnsupportedScheme),
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ForeseerUrlError::CredentialsNotAllowed);
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(ForeseerUrlError::Invalid);
    }
    Ok(parsed.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    Standalone,
    Remote,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub server_url: String,
    pub allow_insecure_http: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandaloneConfig {
    pub cache_limit_bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub mode: AppMode,
    pub remote: RemoteConfig,
    pub standalone: StandaloneConfig,
}
#[derive(Deserialize)]
struct LegacyConfig {
    server_url: String,
    #[serde(default)]
    allow_insecure_http: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            mode: AppMode::Standalone,
            remote: RemoteConfig {
                server_url: DEFAULT_FRONTEND_URL.into(),
                allow_insecure_http: false,
            },
            standalone: StandaloneConfig {
                cache_limit_bytes: DEFAULT_CACHE_LIMIT_BYTES,
            },
        }
    }
}
impl AppConfig {
    pub fn config_directory() -> Option<PathBuf> {
        std::env::var_os("JELLIUM_DESKTOP_CONFIG_DIR")
            .or_else(|| std::env::var_os("FORESEER_CONFIG_DIR"))
            .map(PathBuf::from)
            .or_else(|| {
                ProjectDirs::from("com", "selmantrabzon", "Foreseer")
                    .map(|d| d.config_dir().to_path_buf())
            })
    }
    pub fn cache_directory() -> Option<PathBuf> {
        std::env::var_os("FORESEER_CACHE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                ProjectDirs::from("com", "selmantrabzon", "Foreseer")
                    .map(|d| d.cache_dir().to_path_buf())
            })
    }
    pub fn config_file_path() -> Option<PathBuf> {
        Self::config_directory().map(|d| d.join("config.json"))
    }
    pub fn standalone_data_directory() -> Option<PathBuf> {
        Self::config_directory().map(|d| d.join("standalone"))
    }
    pub fn standalone_cache_directory() -> Option<PathBuf> {
        Self::cache_directory().map(|d| d.join("standalone"))
    }
    pub fn standalone_log_directory() -> Option<PathBuf> {
        Self::standalone_data_directory().map(|d| d.join("logs"))
    }
    pub fn exists() -> bool {
        Self::config_file_path().is_some_and(|p| p.exists())
    }
    pub fn is_configured(&self) -> bool {
        self.mode == AppMode::Standalone
            || validate_foreseer_url(&self.remote.server_url, self.remote.allow_insecure_http)
                .is_ok()
    }
    pub fn remote_url(&self) -> Result<String, ForeseerUrlError> {
        validate_foreseer_url(&self.remote.server_url, self.remote.allow_insecure_http)
    }
    pub fn load() -> Self {
        let mut config = Self::config_file_path()
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|text| {
                serde_json::from_str::<AppConfig>(&text).ok().or_else(|| {
                    serde_json::from_str::<LegacyConfig>(&text)
                        .ok()
                        .map(|legacy| Self {
                            schema_version: CONFIG_SCHEMA_VERSION,
                            mode: AppMode::Remote,
                            remote: RemoteConfig {
                                server_url: legacy.server_url,
                                allow_insecure_http: legacy.allow_insecure_http,
                            },
                            standalone: StandaloneConfig {
                                cache_limit_bytes: DEFAULT_CACHE_LIMIT_BYTES,
                            },
                        })
                })
            })
            .unwrap_or_default();
        config.schema_version = CONFIG_SCHEMA_VERSION;
        config.standalone.cache_limit_bytes = config
            .standalone
            .cache_limit_bytes
            .max(MIN_CACHE_LIMIT_BYTES);
        if let Ok(url) = std::env::var("FORESEER_URL")
            && !url.trim().is_empty()
        {
            config.mode = AppMode::Remote;
            config.remote.server_url = url;
        }
        if std::env::var("FORESEER_ALLOW_INSECURE_HTTP").as_deref() == Ok("1") {
            config.remote.allow_insecure_http = true;
        }
        config
    }
    pub fn save(&self) -> Result<(), std::io::Error> {
        if let Some(path) = Self::config_file_path() {
            self.save_to(&path)?;
        }
        Ok(())
    }
    pub fn save_to(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        // Keep a power loss or interrupted mode switch from replacing the
        // durable configuration with a partial JSON document.
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(temporary, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_config_is_migrated_to_remote_without_losing_values() {
        let legacy = r#"{"server_url":"https://foreseer.example","allow_insecure_http":false}"#;
        let value: LegacyConfig = serde_json::from_str(legacy).unwrap();
        let migrated = AppConfig {
            schema_version: CONFIG_SCHEMA_VERSION,
            mode: AppMode::Remote,
            remote: RemoteConfig {
                server_url: value.server_url,
                allow_insecure_http: value.allow_insecure_http,
            },
            standalone: StandaloneConfig {
                cache_limit_bytes: DEFAULT_CACHE_LIMIT_BYTES,
            },
        };
        assert_eq!(migrated.mode, AppMode::Remote);
        assert_eq!(migrated.remote.server_url, "https://foreseer.example");
    }
    #[test]
    fn defaults_are_standalone_and_use_two_gib_cache_budget() {
        let config = AppConfig::default();
        assert_eq!(config.mode, AppMode::Standalone);
        assert_eq!(config.standalone.cache_limit_bytes, 2_147_483_648);
    }
    #[test]
    fn standalone_cache_budget_has_a_safe_minimum() {
        assert_eq!(MIN_CACHE_LIMIT_BYTES, 128 * 1024 * 1024);
    }
    #[test]
    fn insecure_foreseer_urls_require_local_override() {
        assert_eq!(
            validate_foreseer_url("http://example.com", false).unwrap_err(),
            ForeseerUrlError::InsecureHttpNotAllowed
        );
        assert!(validate_foreseer_url("http://127.0.0.1", true).is_ok());
    }
    #[test]
    fn bootstrap_http_is_only_for_private_hosts() {
        assert_eq!(
            validate_bootstrap_server_url("http://jellyfin.example").unwrap_err(),
            ForeseerUrlError::InsecureHttpNonLocalHost
        );
        assert!(validate_bootstrap_server_url("http://192.168.40.3:8096").is_ok());
        assert!(validate_bootstrap_server_url("https://jellyfin.example").is_ok());
    }
}
