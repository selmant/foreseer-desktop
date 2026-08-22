//! Jellium `HostExtension` adapter for Foreseer protocol v1.

use std::process::Command;
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use jfn_rust::{
    ExtensionSource, HostExtension, HostExtensionDescriptor, Presentation as JfnPresentation,
    RuntimeEvent, RuntimeHandle,
};
use serde_json::json;

use crate::auth::{AuthErrorCode, redeem_ticket, redemption_url};
use crate::config::{AppConfig, AppMode, validate_foreseer_url};
use crate::controller::{AppState, Controller, ControllerEvent, Presentation, RuntimeOps};
use crate::protocol::{NativeCommandV1, NativeEventV1, parse_command, serialize_event};
use crate::session::SessionBootstrap;
use crate::supervisor::{RuntimeHealthTracker, StandaloneSupervisor};

struct HandleRuntime {
    handle: RuntimeHandle,
}

impl RuntimeOps for HandleRuntime {
    fn post_frontend_event(&mut self, event: NativeEventV1) {
        if let Ok(bytes) = serialize_event(&event) {
            let _ = self.handle.post_message(ExtensionSource::Frontend, &bytes);
        }
    }

    fn set_presentation(&mut self, presentation: Presentation) {
        let mapped = match presentation {
            Presentation::Frontend => JfnPresentation::Frontend,
            Presentation::PrimaryWebPreparing => JfnPresentation::PrimaryWebPreparing,
            Presentation::PrimaryWeb => JfnPresentation::PrimaryWeb,
        };
        let _ = self.handle.set_presentation(mapped);
    }

    fn navigate_primary_web(&mut self, url: &str) -> bool {
        self.handle.navigate_primary_web(url)
    }

    fn complete_setup_navigation(&mut self, url: &str) -> bool {
        self.handle.complete_setup_navigation(url)
    }

    fn minimize(&mut self) {
        self.handle.minimize();
    }

    fn toggle_maximize(&mut self) {
        self.handle.toggle_maximize();
    }

    fn toggle_fullscreen(&mut self) {
        self.handle.toggle_fullscreen();
    }

    fn request_shutdown(&mut self) {
        self.handle.request_shutdown();
    }
}

#[derive(Clone)]
struct PendingBootstrap {
    request_id: String,
    bootstrap: SessionBootstrap,
}

struct Inner {
    controller: Controller<HandleRuntime>,
    frontend_url: String,
    allow_insecure_http: bool,
    agent: ureq::Agent,
    setup_agent: ureq::Agent,
    runtime: RuntimeHandle,
    pending_bootstrap: Option<PendingBootstrap>,
}

pub struct ForeseerExtension {
    descriptor: HostExtensionDescriptor,
    frontend_url: Mutex<String>,
    allow_insecure_http: Mutex<bool>,
    in_setup: bool,
    state: Mutex<Option<Inner>>,
    self_weak: Mutex<Option<Weak<ForeseerExtension>>>,
    standalone_supervisor: Option<Arc<Mutex<StandaloneSupervisor>>>,
    runtime_shutting_down: Arc<AtomicBool>,
    runtime_failed: Arc<AtomicBool>,
    automatic_restart_attempted: Arc<AtomicBool>,
}

impl ForeseerExtension {
    pub fn new(
        descriptor: HostExtensionDescriptor,
        frontend_url: String,
        allow_insecure_http: bool,
        in_setup: bool,
    ) -> Arc<Self> {
        Self::new_with_supervisor(
            descriptor,
            frontend_url,
            allow_insecure_http,
            in_setup,
            None,
        )
    }

    pub fn new_with_supervisor(
        descriptor: HostExtensionDescriptor,
        frontend_url: String,
        allow_insecure_http: bool,
        in_setup: bool,
        standalone_supervisor: Option<Arc<Mutex<StandaloneSupervisor>>>,
    ) -> Arc<Self> {
        let extension = Arc::new(Self {
            descriptor,
            frontend_url: Mutex::new(frontend_url),
            allow_insecure_http: Mutex::new(allow_insecure_http),
            in_setup,
            state: Mutex::new(None),
            self_weak: Mutex::new(None),
            standalone_supervisor,
            runtime_shutting_down: Arc::new(AtomicBool::new(false)),
            runtime_failed: Arc::new(AtomicBool::new(false)),
            automatic_restart_attempted: Arc::new(AtomicBool::new(false)),
        });
        if let Ok(mut slot) = extension.self_weak.lock() {
            *slot = Some(Arc::downgrade(&extension));
        }
        extension
    }

    fn upgrade(&self) -> Option<Arc<Self>> {
        self.self_weak.lock().ok()?.as_ref()?.upgrade()
    }

    fn with_inner<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> Option<R> {
        self.state.lock().ok()?.as_mut().map(f)
    }

    fn enqueue(&self, event: ControllerEvent) {
        self.with_inner(|inner| {
            match &event {
                ControllerEvent::AuthRedeemed {
                    request_id,
                    bootstrap,
                    ..
                } => {
                    inner.pending_bootstrap = Some(PendingBootstrap {
                        request_id: request_id.clone(),
                        bootstrap: bootstrap.clone(),
                    });
                    tracing::info!(
                        target: "ForeseerExtension",
                        "auth redeemed; waiting for primary web load before bootstrap"
                    );
                }
                ControllerEvent::BootstrapReady { .. }
                | ControllerEvent::BootstrapFailed { .. }
                | ControllerEvent::AuthFailed { .. } => {
                    inner.pending_bootstrap = None;
                }
                _ => {}
            }
            inner.controller.handle_event(event);
        });
    }

    fn deliver_pending_bootstrap(&self, loaded_url: &str) {
        let Some((pending, runtime)) = self
            .with_inner(|inner| {
                let pending = inner.pending_bootstrap.as_ref()?;
                let loaded = url::Url::parse(loaded_url).ok()?;
                let expected = url::Url::parse(&pending.bootstrap.server_url).ok()?;
                if loaded.origin() != expected.origin() {
                    let request_id = pending.request_id.clone();
                    inner.pending_bootstrap = None;
                    inner
                        .controller
                        .handle_event(ControllerEvent::BootstrapFailed {
                            request_id,
                            code: AuthErrorCode::InvalidBootstrapResponse,
                        });
                    tracing::warn!(
                        target: "ForeseerExtension",
                        "primary web origin mismatch for pending bootstrap"
                    );
                    return None;
                }
                Some((pending.clone(), inner.runtime.clone()))
            })
            .flatten()
        else {
            return;
        };
        tracing::info!(
            target: "ForeseerExtension",
            "delivering session bootstrap to primary web"
        );
        Self::post_bootstrap(&runtime, &pending.bootstrap);
    }

    fn post_bootstrap(runtime: &RuntimeHandle, bootstrap: &SessionBootstrap) {
        let payload = json!({
            "type": "session.bootstrap",
            "serverUrl": bootstrap.server_url,
            "serverId": bootstrap.server_id,
            "userId": bootstrap.user_id,
            "deviceId": bootstrap.device_id,
            "accessToken": bootstrap.access_token,
            "bootstrapGeneration": bootstrap.bootstrap_generation,
        });
        if let Ok(text) = serde_json::to_string(&payload) {
            let _ = runtime.post_message(ExtensionSource::PrimaryWeb, text.as_bytes());
        }
    }
}

impl HostExtension for ForeseerExtension {
    fn descriptor(&self) -> HostExtensionDescriptor {
        self.descriptor.clone()
    }

    fn on_runtime_ready(&self, runtime: RuntimeHandle) {
        let frontend_url = self
            .frontend_url
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let allow_insecure_http = self.allow_insecure_http.lock().map(|g| *g).unwrap_or(false);
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .into();
        let setup_agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        let controller = Controller::new(
            HandleRuntime {
                handle: runtime.clone(),
            },
            self.in_setup,
        );
        if let Ok(mut guard) = self.state.lock() {
            *guard = Some(Inner {
                controller,
                frontend_url,
                allow_insecure_http,
                agent,
                setup_agent,
                runtime: runtime.clone(),
                pending_bootstrap: None,
            });
        }
        // This is the first point at which the managed child knows its CEF
        // frontend is live. A false playback state starts its delayed,
        // coalesced desktop catch-up pass without running work during startup.
        if let Some(supervisor) = &self.standalone_supervisor
            && let Ok(mut supervisor) = supervisor.lock()
        {
            supervisor.set_playback_active(false);
        }
        self.monitor_standalone_runtime(runtime);
    }

    fn admit_message(&self, source: ExtensionSource, _origin: &str, payload: &[u8]) -> bool {
        match source {
            ExtensionSource::Frontend => self.admit_frontend(payload),
            ExtensionSource::PrimaryWeb => self.admit_primary_web(payload),
        }
    }

    fn on_runtime_event(&self, event: RuntimeEvent) {
        if let RuntimeEvent::PrimaryWebLoaded { url } = &event {
            self.deliver_pending_bootstrap(url);
        }
        match event {
            RuntimeEvent::PlaybackStarted => {
                if let Some(supervisor) = &self.standalone_supervisor
                    && let Ok(mut supervisor) = supervisor.lock()
                {
                    supervisor.set_playback_active(true);
                }
            }
            RuntimeEvent::PlaybackFinished
            | RuntimeEvent::PlaybackCanceled
            | RuntimeEvent::PlaybackError => {
                if let Some(supervisor) = &self.standalone_supervisor
                    && let Ok(mut supervisor) = supervisor.lock()
                {
                    supervisor.set_playback_active(false);
                }
            }
            RuntimeEvent::ShutdownBeginning => {
                self.runtime_shutting_down.store(true, Ordering::Release);
                if let Some(supervisor) = self.standalone_supervisor.clone() {
                    std::thread::spawn(move || {
                        if let Ok(mut supervisor) = supervisor.lock() {
                            supervisor.shutdown();
                        }
                    });
                }
            }
            _ => {}
        }
        let mapped = match event {
            RuntimeEvent::PlaybackStarted => Some(ControllerEvent::PlaybackStarted),
            RuntimeEvent::PlaybackFinished => Some(ControllerEvent::PlaybackFinished),
            RuntimeEvent::PlaybackCanceled => Some(ControllerEvent::PlaybackCanceled),
            RuntimeEvent::PlaybackError => Some(ControllerEvent::PlaybackError),
            RuntimeEvent::ShutdownBeginning => Some(ControllerEvent::Shutdown),
            _ => None,
        };
        if let Some(event) = mapped {
            self.enqueue(event);
        }
    }
}

impl ForeseerExtension {
    /// Poll only after CEF is live. Three failed status probes (or an exited
    /// child) transition the active frontend to its local recovery view. This
    /// keeps the extension's exact origin unchanged; it does not navigate to
    /// a broad or attacker-controlled error URL.
    fn monitor_standalone_runtime(&self, runtime: RuntimeHandle) {
        let Some(supervisor) = self.standalone_supervisor.clone() else {
            return;
        };
        let shutting_down = Arc::clone(&self.runtime_shutting_down);
        let runtime_failed = Arc::clone(&self.runtime_failed);
        let automatic_restart_attempted = Arc::clone(&self.automatic_restart_attempted);
        std::thread::spawn(move || {
            let mut tracker = RuntimeHealthTracker::default();
            loop {
                std::thread::sleep(Duration::from_secs(10));
                if shutting_down.load(Ordering::Acquire) {
                    return;
                }
                let failed = supervisor
                    .lock()
                    .map(|mut child| tracker.observe(child.health()))
                    .unwrap_or(true);
                if !failed {
                    continue;
                }
                if shutting_down.load(Ordering::Acquire) {
                    return;
                }
                if !automatic_restart_attempted.swap(true, Ordering::AcqRel) {
                    let recovered = supervisor
                        .lock()
                        .map(|mut child| child.retry_on_original_port())
                        .is_ok_and(|result| result.is_ok());
                    if recovered {
                        tracker = RuntimeHealthTracker::default();
                        let event = NativeEventV1::new("runtime", "runtime-recovered");
                        if let Ok(bytes) = serialize_event(&event) {
                            let _ = runtime.post_message(ExtensionSource::Frontend, &bytes);
                        }
                        continue;
                    }
                }
                runtime_failed.store(true, Ordering::Release);
                let _ = runtime.set_presentation(JfnPresentation::Frontend);
                let event = NativeEventV1::new("runtime", "runtime-failed")
                    .with_error("standalone_runtime_failed")
                    .with_message("The bundled Foreseerr server stopped responding.");
                if let Ok(bytes) = serialize_event(&event) {
                    let _ = runtime.post_message(ExtensionSource::Frontend, &bytes);
                }
                break;
            }
        });
    }

    fn admit_frontend(&self, payload: &[u8]) -> bool {
        let Ok(command) = parse_command(payload) else {
            tracing::warn!(target: "ForeseerExtension", "rejected malformed frontend command");
            return false;
        };

        match command {
            NativeCommandV1::AuthComplete { id, ticket } => {
                tracing::info!(target: "ForeseerExtension", "auth.complete admitted");
                let Some((verifier, epoch, redeem_url, agent)) = self
                    .with_inner(|inner| {
                        let (verifier, epoch) = inner.controller.begin_auth_complete(&id)?;
                        let redeem_url = redemption_url(&inner.frontend_url).ok()?;
                        Some((verifier, epoch, redeem_url, inner.agent.clone()))
                    })
                    .flatten()
                else {
                    return true;
                };
                let Some(this) = self.upgrade() else {
                    return true;
                };
                std::thread::spawn(move || {
                    match redeem_ticket(&agent, &redeem_url, &ticket, &verifier) {
                        Ok(bootstrap) => {
                            tracing::info!(target: "ForeseerExtension", "auth ticket redeemed");
                            this.enqueue(ControllerEvent::AuthRedeemed {
                                request_id: id,
                                bootstrap,
                                auth_epoch: epoch,
                            })
                        }
                        Err(code) => {
                            tracing::warn!(
                                target: "ForeseerExtension",
                                "auth ticket redeem failed: {:?}",
                                code
                            );
                            this.enqueue(ControllerEvent::AuthFailed {
                                request_id: id,
                                code,
                                auth_epoch: epoch,
                            });
                        }
                    }
                });
                true
            }
            NativeCommandV1::SetupCheck {
                id,
                url,
                allow_http,
            } => self.start_setup_check(id, url, allow_http),
            NativeCommandV1::SetupSave {
                id,
                url,
                allow_http,
            } => self.save_setup(id, url, allow_http),
            NativeCommandV1::SetupStandalone { id } => self.save_standalone_setup(id),
            NativeCommandV1::BrowserCacheClear { id, ticket } => {
                self.clear_browser_cache(id, ticket)
            }
            NativeCommandV1::RuntimeRetry { id } => self.retry_standalone_runtime(id),
            NativeCommandV1::RuntimeOpenLogs { id } => self.open_standalone_logs(id),
            NativeCommandV1::RuntimeOpenSetup { id } => self.open_remote_setup(id),
            NativeCommandV1::PlayItem { id, item_id } => {
                tracing::info!(
                    target: "ForeseerExtension",
                    "play.item admitted item_id={item_id}"
                );
                self.with_inner(|inner| {
                    let ok = inner.controller.handle_command(NativeCommandV1::PlayItem {
                        id: id.clone(),
                        item_id: item_id.clone(),
                    });
                    if ok && matches!(inner.controller.state(), AppState::Resolving) {
                        let payload = json!({ "type": "play.item", "itemId": item_id, "id": id });
                        if let Ok(text) = serde_json::to_string(&payload) {
                            let _ = inner
                                .runtime
                                .post_message(ExtensionSource::PrimaryWeb, text.as_bytes());
                        }
                    }
                    ok
                })
                .unwrap_or(false)
            }
            NativeCommandV1::SessionClear { id } => self
                .with_inner(|inner| {
                    inner.pending_bootstrap = None;
                    let ok = inner
                        .controller
                        .handle_command(NativeCommandV1::SessionClear { id });
                    if ok {
                        let payload = json!({ "type": "session.clear" });
                        if let Ok(text) = serde_json::to_string(&payload) {
                            let _ = inner
                                .runtime
                                .post_message(ExtensionSource::PrimaryWeb, text.as_bytes());
                        }
                    }
                    ok
                })
                .unwrap_or(false),
            other => {
                if matches!(&other, NativeCommandV1::AuthChallenge { .. }) {
                    tracing::info!(target: "ForeseerExtension", "auth.challenge admitted");
                }
                self.with_inner(|inner| inner.controller.handle_command(other))
                    .unwrap_or(false)
            }
        }
    }

    fn save_standalone_setup(&self, id: String) -> bool {
        let mut config = AppConfig::load();
        config.mode = AppMode::Standalone;
        if config.save().is_err() {
            self.with_inner(|inner| {
                inner.controller.runtime.post_frontend_event(
                    NativeEventV1::new(id, "error")
                        .with_error("config_save_failed")
                        .with_message("Could not save standalone mode"),
                );
            });
            return true;
        }
        if let Err(message) = relaunch_application() {
            self.with_inner(|inner| {
                inner.controller.runtime.post_frontend_event(
                    NativeEventV1::new(id, "error")
                        .with_error("restart_failed")
                        .with_message(message),
                );
            });
            return true;
        }
        self.with_inner(|inner| {
            inner
                .controller
                .runtime
                .post_frontend_event(NativeEventV1::new(id, "save-config-success"));
            inner.controller.runtime.request_shutdown();
        });
        true
    }

    fn clear_browser_cache(&self, id: String, ticket: String) -> bool {
        let Some((agent, endpoint, runtime)) = self
            .with_inner(|inner| {
                let parsed = url::Url::parse(&inner.frontend_url).ok()?;
                let origin = parsed.origin().ascii_serialization();
                Some((
                    inner.agent.clone(),
                    format!("{origin}/api/v1/desktop/browser-cache/redeem"),
                    inner.runtime.clone(),
                ))
            })
            .flatten()
        else {
            return true;
        };
        std::thread::spawn(move || {
            let accepted = agent
                .post(&endpoint)
                .send_json(json!({ "ticket": ticket, "protocolVersion": 1 }))
                .ok()
                .is_some_and(|response| response.status().as_u16() == 204);
            let event = if accepted && runtime.clear_http_cache() {
                NativeEventV1::new(id, "browser-cache-cleared")
            } else {
                NativeEventV1::new(id, "error").with_error("browser_cache_clear_failed")
            };
            if let Ok(bytes) = serialize_event(&event) {
                let _ = runtime.post_message(ExtensionSource::Frontend, &bytes);
            }
        });
        true
    }

    fn retry_standalone_runtime(&self, id: String) -> bool {
        if !self.runtime_failed.swap(false, Ordering::AcqRel) {
            self.with_inner(|inner| {
                inner.controller.runtime.post_frontend_event(
                    NativeEventV1::new(id, "error").with_error("runtime_retry_unavailable"),
                );
            });
            return true;
        }
        let Some((supervisor, runtime)) = self
            .standalone_supervisor
            .clone()
            .zip(self.with_inner(|inner| inner.runtime.clone()))
        else {
            return true;
        };
        let Some(this) = self.upgrade() else {
            return true;
        };
        std::thread::spawn(move || {
            let recovered: Result<(), String> = match supervisor.lock() {
                Ok(mut child) => child
                    .retry_on_original_port()
                    .map_err(|error| error.to_string()),
                Err(_) => Err("Standalone supervisor is unavailable".into()),
            };
            match recovered {
                Ok(()) => {
                    let event = NativeEventV1::new(id, "runtime-recovered");
                    if let Ok(bytes) = serialize_event(&event) {
                        let _ = runtime.post_message(ExtensionSource::Frontend, &bytes);
                    }
                    this.monitor_standalone_runtime(runtime);
                }
                Err(message) => {
                    this.runtime_failed.store(true, Ordering::Release);
                    let event = NativeEventV1::new(id, "error")
                        .with_error("runtime_retry_failed")
                        .with_message(message);
                    if let Ok(bytes) = serialize_event(&event) {
                        let _ = runtime.post_message(ExtensionSource::Frontend, &bytes);
                    }
                }
            }
        });
        true
    }

    fn open_standalone_logs(&self, id: String) -> bool {
        let result = AppConfig::standalone_log_directory()
            .ok_or_else(|| "Standalone log directory is unavailable".to_string())
            .and_then(|directory| {
                std::fs::create_dir_all(&directory)
                    .map_err(|error| format!("Could not prepare log directory: {error}"))?;
                open_directory(&directory)
            });
        self.with_inner(|inner| {
            let event = match result {
                Ok(()) => NativeEventV1::new(id, "logs-opened"),
                Err(message) => NativeEventV1::new(id, "error")
                    .with_error("open_logs_failed")
                    .with_message(message),
            };
            inner.controller.runtime.post_frontend_event(event);
        });
        true
    }

    fn open_remote_setup(&self, id: String) -> bool {
        match relaunch_setup() {
            Ok(()) => {
                self.with_inner(|inner| {
                    inner
                        .controller
                        .runtime
                        .post_frontend_event(NativeEventV1::new(id, "setup-opened"));
                    inner.controller.runtime.request_shutdown();
                });
            }
            Err(message) => {
                self.with_inner(|inner| {
                    inner.controller.runtime.post_frontend_event(
                        NativeEventV1::new(id, "error")
                            .with_error("setup_open_failed")
                            .with_message(message),
                    );
                });
            }
        }
        true
    }

    fn start_setup_check(&self, id: String, url: String, allow_http: bool) -> bool {
        let Some((generation, agent)) = self
            .with_inner(|inner| {
                if !inner.controller.in_setup() {
                    inner.controller.runtime.post_frontend_event(
                        NativeEventV1::new(id.clone(), "error").with_error("invalid_request"),
                    );
                    return None;
                }
                if let Err(err) = validate_foreseer_url(&url, allow_http) {
                    inner.controller.runtime.post_frontend_event(
                        NativeEventV1::new(id.clone(), "error")
                            .with_error("invalid_request")
                            .with_message(err.message()),
                    );
                    return None;
                }
                Some((
                    inner.controller.setup_generation(),
                    inner.setup_agent.clone(),
                ))
            })
            .flatten()
        else {
            return true;
        };

        let Some(this) = self.upgrade() else {
            return true;
        };
        std::thread::spawn(move || {
            let result = match validate_foreseer_url(&url, allow_http) {
                Ok(normalized) => match url::Url::parse(&normalized) {
                    Ok(parsed) => {
                        let test_url = parsed
                            .join("api/v1/desktop/auth-tickets/redeem")
                            .unwrap_or(parsed)
                            .to_string();
                        match agent.post(&test_url).send_json(serde_json::json!({
                            "ticket": "test",
                            "verifier": "test",
                            "protocolVersion": 1,
                        })) {
                            Ok(r) => Ok(r.status().as_u16()),
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    Err(_) => Err("Invalid server URL".to_string()),
                },
                Err(err) => Err(err.message().to_string()),
            };
            this.enqueue(ControllerEvent::SetupCheckResult {
                request_id: id,
                generation,
                result,
            });
        });
        true
    }

    fn save_setup(&self, id: String, url: String, allow_http: bool) -> bool {
        let normalized = match validate_foreseer_url(&url, allow_http) {
            Ok(url) => url,
            Err(err) => {
                let _ = self.with_inner(|inner| {
                    inner.controller.runtime.post_frontend_event(
                        NativeEventV1::new(id, "error")
                            .with_error("invalid_request")
                            .with_message(err.message()),
                    );
                });
                return true;
            }
        };
        let mut config = AppConfig::load();
        config.mode = AppMode::Remote;
        config.remote.server_url = normalized.clone();
        config.remote.allow_insecure_http = allow_http;
        let _ = config.save();
        if let Ok(mut url_guard) = self.frontend_url.lock() {
            *url_guard = normalized.clone();
        }
        if let Ok(mut allow_guard) = self.allow_insecure_http.lock() {
            *allow_guard = allow_http;
        }
        self.with_inner(|inner| {
            inner.frontend_url = normalized.clone();
            inner.allow_insecure_http = allow_http;
            inner.controller.handle_command(NativeCommandV1::SetupSave {
                id,
                url: normalized,
                allow_http,
            })
        })
        .unwrap_or(false)
    }

    fn admit_primary_web(&self, payload: &[u8]) -> bool {
        let Ok(text) = std::str::from_utf8(payload) else {
            return false;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
            return false;
        };
        let Some(kind) = value.get("type").and_then(|v| v.as_str()) else {
            return false;
        };
        match kind {
            "session.ready" => {
                self.enqueue(ControllerEvent::BootstrapReady {
                    server_id: value
                        .get("serverId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    user_id: value
                        .get("userId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    generation: value
                        .get("generation")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
                true
            }
            "session.failed" => {
                let request_id = self
                    .with_inner(|inner| {
                        inner
                            .controller
                            .active_request_id()
                            .unwrap_or("session")
                            .to_string()
                    })
                    .unwrap_or_else(|| "session".into());
                self.enqueue(ControllerEvent::BootstrapFailed {
                    request_id,
                    code: AuthErrorCode::InvalidBootstrapResponse,
                });
                true
            }
            "playback.stopped" => {
                self.enqueue(ControllerEvent::PlaybackCanceled);
                true
            }
            _ => false,
        }
    }
}

fn relaunch_application() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not locate Foreseer executable: {error}"))?;
    Command::new(executable)
        .env_remove("FORESEER_SETUP_RELAUNCHED")
        .env_remove("FORESEER_URL")
        .env_remove("FORESEER_ALLOW_INSECURE_HTTP")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not restart Foreseer: {error}"))
}

fn relaunch_setup() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not locate Foreseer executable: {error}"))?;
    Command::new(executable)
        .arg("--setup")
        .env_remove("FORESEER_SETUP_RELAUNCHED")
        .env_remove("FORESEER_URL")
        .env_remove("FORESEER_ALLOW_INSECURE_HTTP")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open Foreseer setup: {error}"))
}

fn open_directory(directory: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let command = ("xdg-open", directory);
    #[cfg(target_os = "windows")]
    let command = ("explorer.exe", directory);
    #[cfg(target_os = "macos")]
    let command = ("open", directory);
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    return Err("Opening logs is unsupported on this platform".into());

    Command::new(command.0)
        .arg(command.1)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open logs: {error}"))
}
