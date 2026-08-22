//! Managed loopback Foreseerr child lifecycle.
//!
//! This module deliberately has no knowledge of CEF.  It owns only the bundled
//! Node process and exposes a validated, exact loopback origin to the caller.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::config::AppConfig;

pub const READY_PREFIX: &str = "FORESEERR_DESKTOP_READY ";
pub const READY_PROTOCOL_VERSION: u32 = 1;
const READY_TIMEOUT: Duration = Duration::from_secs(90);
const BUNDLED_FORESEERR_VERSION_FILE: &str = include_str!("../foreseerr.rev");

fn bundled_foreseerr_version() -> &'static str {
    BUNDLED_FORESEERR_VERSION_FILE.trim()
}

#[derive(Debug, Deserialize, Serialize)]
struct RuntimeVersion {
    foreseerr_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyRecord {
    pub protocol_version: u32,
    pub pid: u32,
    pub origin: String,
    pub foreseerr_version: String,
    pub commit: String,
    pub schema_version: u32,
}

#[derive(Debug)]
pub enum SupervisorError {
    ResourcesNotFound(PathBuf),
    Spawn(std::io::Error),
    Startup(String),
    InvalidReady(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeHealth {
    Healthy,
    Unhealthy,
    Exited,
}
impl std::fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResourcesNotFound(path) => write!(
                f,
                "Bundled Foreseerr resources were not found at {}",
                path.display()
            ),
            Self::Spawn(err) => write!(f, "Could not start bundled Foreseerr: {err}"),
            Self::Startup(message) | Self::InvalidReady(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for SupervisorError {}

pub struct StandaloneSupervisor {
    child: Child,
    stdin: Option<ChildStdin>,
    pub origin: String,
    pub diagnostics: Vec<String>,
}

impl StandaloneSupervisor {
    pub fn start(config: &AppConfig) -> Result<Self, SupervisorError> {
        let resource_root = resource_root()?;
        let node = resource_root
            .join("node")
            .join(if cfg!(windows) { "node.exe" } else { "node" });
        let launcher = resource_root.join("foreseerr").join("launcher.js");
        if !node.is_file() || !launcher.is_file() {
            return Err(SupervisorError::ResourcesNotFound(resource_root));
        }
        let config_dir = AppConfig::standalone_data_directory().ok_or_else(|| {
            SupervisorError::Startup("No platform configuration directory is available".into())
        })?;
        let cache_dir = AppConfig::standalone_cache_directory().ok_or_else(|| {
            SupervisorError::Startup("No platform cache directory is available".into())
        })?;
        let log_dir = AppConfig::standalone_log_directory().ok_or_else(|| {
            SupervisorError::Startup("No platform log directory is available".into())
        })?;
        for directory in [
            &config_dir,
            &cache_dir,
            &log_dir,
            &config_dir.join("state"),
            &config_dir.join("backups"),
        ] {
            std::fs::create_dir_all(directory).map_err(SupervisorError::Spawn)?;
        }
        backup_before_upgrade(&config_dir)?;
        let mut command = Command::new(node);
        command
            .arg(launcher)
            .current_dir(resource_root.join("foreseerr"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        command.process_group(0);
        for key in [
            "PORT",
            "HOST",
            "CONFIG_DIRECTORY",
            "CACHE_DIRECTORY",
            "LOG_DIRECTORY",
            "NODE_OPTIONS",
            "NODE_PATH",
        ] {
            command.env_remove(key);
        }
        command
            .env("FORESEERR_RUNTIME", "desktop")
            .env("CONFIG_DIRECTORY", &config_dir)
            .env("CACHE_DIRECTORY", &cache_dir)
            .env("LOG_DIRECTORY", &log_dir)
            .env("HOST", "127.0.0.1")
            .env("PORT", "0")
            .env(
                "FORESEER_CACHE_LIMIT_BYTES",
                config.standalone.cache_limit_bytes.to_string(),
            );
        let mut child = command.spawn().map_err(SupervisorError::Spawn)?;
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let (tx, rx) = mpsc::channel();
        let tx_stdout = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx_stdout.send((false, line.unwrap_or_default())).is_err() {
                    break;
                }
            }
        });
        let tx_stderr = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                if tx_stderr.send((true, line.unwrap_or_default())).is_err() {
                    break;
                }
            }
        });
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut diagnostics = Vec::new();
        loop {
            if let Some(status) = child.try_wait().map_err(SupervisorError::Spawn)? {
                return Err(SupervisorError::Startup(format!(
                    "Bundled Foreseerr exited before readiness ({status}); {}",
                    diagnostics.join("\n")
                )));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let _ = child.kill();
                return Err(SupervisorError::Startup(format!(
                    "Timed out waiting for bundled Foreseerr readiness; {}",
                    diagnostics.join("\n")
                )));
            }
            if let Ok((is_stderr, line)) =
                rx.recv_timeout(remaining.min(Duration::from_millis(250)))
            {
                if diagnostics.len() == 200 {
                    diagnostics.remove(0);
                }
                diagnostics.push(redact(&line));
                if !is_stderr && line.starts_with(READY_PREFIX) {
                    let ready: ReadyRecord = serde_json::from_str(&line[READY_PREFIX.len()..])
                        .map_err(|_| {
                            SupervisorError::InvalidReady(
                                "Bundled Foreseerr emitted malformed readiness data".into(),
                            )
                        })?;
                    validate_ready(&ready)?;
                    if ready.foreseerr_version != bundled_foreseerr_version() {
                        let _ = child.kill();
                        return Err(SupervisorError::InvalidReady(format!(
                            "Bundled Foreseerr version mismatch (expected {}, got {})",
                            bundled_foreseerr_version(),
                            ready.foreseerr_version
                        )));
                    }
                    let supervisor = Self {
                        stdin: child.stdin.take(),
                        child,
                        origin: ready.origin,
                        diagnostics,
                    };
                    if !supervisor.status_is_healthy() {
                        return Err(SupervisorError::Startup(
                            "Bundled Foreseerr did not pass its status health check".into(),
                        ));
                    }
                    write_runtime_version(&config_dir, &ready.foreseerr_version)?;
                    return Ok(supervisor);
                }
            }
        }
    }
    pub fn set_playback_active(&mut self, active: bool) {
        self.send_control(&format!(
            r#"{{"type":"runtime-state","playbackActive":{active}}}"#
        ));
    }
    /// Poll only after readiness. Callers can require three consecutive
    /// unhealthy results before presenting recovery UI.
    pub fn health(&mut self) -> RuntimeHealth {
        match self.child.try_wait() {
            Ok(Some(_)) => RuntimeHealth::Exited,
            Ok(None) if self.status_is_healthy() => RuntimeHealth::Healthy,
            Ok(None) | Err(_) => RuntimeHealth::Unhealthy,
        }
    }
    pub fn shutdown(&mut self) {
        self.send_control(r#"{"type":"shutdown","deadlineMs":10000}"#);
        #[cfg(unix)]
        if let Ok(pid) = i32::try_from(self.child.id()) {
            // The child is the leader of a dedicated group, so this also
            // reaches Node helpers spawned for native modules.
            unsafe { libc::kill(-pid, libc::SIGTERM) };
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        #[cfg(unix)]
        if let Ok(pid) = i32::try_from(self.child.id()) {
            unsafe { libc::kill(-pid, libc::SIGKILL) };
        }
        #[cfg(not(unix))]
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    fn send_control(&mut self, message: &str) {
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = writeln!(stdin, "{message}");
            let _ = stdin.flush();
        }
    }

    fn status_is_healthy(&self) -> bool {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .http_status_as_error(false)
            .build()
            .into();
        agent
            .get(&format!("{}/api/v1/status", self.origin))
            .call()
            .ok()
            .is_some_and(|response| response.status().is_success())
    }
}
impl Drop for StandaloneSupervisor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn resource_root() -> Result<PathBuf, SupervisorError> {
    let executable = std::env::current_exe().map_err(SupervisorError::Spawn)?;
    let base = executable
        .parent()
        .unwrap_or(Path::new("."))
        .join("resources");
    if base.is_dir() {
        Ok(base)
    } else {
        Err(SupervisorError::ResourcesNotFound(base))
    }
}
fn validate_ready(ready: &ReadyRecord) -> Result<(), SupervisorError> {
    if ready.protocol_version != READY_PROTOCOL_VERSION {
        return Err(SupervisorError::InvalidReady(
            "Bundled Foreseerr uses an unsupported desktop protocol".into(),
        ));
    }
    let url = url::Url::parse(&ready.origin).map_err(|_| {
        SupervisorError::InvalidReady("Bundled Foreseerr supplied an invalid origin".into())
    })?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().unwrap_or(0) == 0
        || url.username() != ""
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(SupervisorError::InvalidReady(
            "Bundled Foreseerr supplied a non-loopback origin".into(),
        ));
    }
    Ok(())
}
fn redact(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if ["authorization", "cookie", "token", "ticket"]
        .iter()
        .any(|secret| lower.contains(secret))
    {
        "[redacted child diagnostic]".into()
    } else {
        line.into()
    }
}

fn backup_before_upgrade(config_dir: &Path) -> Result<(), SupervisorError> {
    let state_file = config_dir.join("state/runtime-version.json");
    let previous = fs::read_to_string(&state_file)
        .ok()
        .and_then(|text| serde_json::from_str::<RuntimeVersion>(&text).ok());
    let Some(previous) = previous else {
        return Ok(());
    };
    if previous.foreseerr_version == bundled_foreseerr_version() {
        return Ok(());
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup = config_dir
        .join("backups")
        .join(format!("{}-{timestamp}", previous.foreseerr_version));
    fs::create_dir_all(&backup).map_err(SupervisorError::Spawn)?;
    for relative in [
        "settings.json",
        "db/db.sqlite3",
        "db/db.sqlite3-wal",
        "db/db.sqlite3-shm",
    ] {
        let source = config_dir.join(relative);
        if source.is_file() {
            let target = backup.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(SupervisorError::Spawn)?;
            }
            fs::copy(source, target).map_err(SupervisorError::Spawn)?;
        }
    }
    let metadata = serde_json::json!({
        "previousVersion": previous.foreseerr_version,
        "newVersion": bundled_foreseerr_version(),
        "createdAt": timestamp,
    });
    fs::write(
        backup.join("metadata.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .map_err(SupervisorError::Spawn)?;
    let backups = config_dir.join("backups");
    let mut entries: Vec<_> = fs::read_dir(&backups)
        .map_err(SupervisorError::Spawn)?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    while entries.len() > 3 {
        let oldest = entries.remove(0);
        fs::remove_dir_all(oldest.path()).map_err(SupervisorError::Spawn)?;
    }
    Ok(())
}

fn write_runtime_version(config_dir: &Path, version: &str) -> Result<(), SupervisorError> {
    let state = config_dir.join("state/runtime-version.json");
    let payload = serde_json::to_vec_pretty(&RuntimeVersion {
        foreseerr_version: version.into(),
    })
    .map_err(|error| SupervisorError::Startup(error.to_string()))?;
    fs::write(state, payload).map_err(SupervisorError::Spawn)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_requires_exact_loopback_origin() {
        let ready = ReadyRecord {
            protocol_version: 1,
            pid: 1,
            origin: "http://127.0.0.1:43127".into(),
            foreseerr_version: "0.6.2".into(),
            commit: "test".into(),
            schema_version: 1,
        };
        assert!(validate_ready(&ready).is_ok());
        let mut bad = ready;
        bad.origin = "http://localhost:43127".into();
        assert!(validate_ready(&bad).is_err());
    }
}
