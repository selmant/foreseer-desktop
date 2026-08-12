//! Jellium `HostExtension` adapter for Foreseer protocol v1.

use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use jfn_rust::{
    ExtensionSource, HostExtension, HostExtensionDescriptor, Presentation as JfnPresentation,
    RuntimeEvent, RuntimeHandle,
};
use serde_json::json;

use crate::auth::{AuthErrorCode, redeem_ticket, redemption_url};
use crate::config::{AppConfig, validate_foreseer_url};
use crate::controller::{AppState, Controller, ControllerEvent, Presentation, RuntimeOps};
use crate::protocol::{NativeCommandV1, NativeEventV1, parse_command, serialize_event};
use crate::session::SessionBootstrap;

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
}

impl ForeseerExtension {
    pub fn new(
        descriptor: HostExtensionDescriptor,
        frontend_url: String,
        allow_insecure_http: bool,
        in_setup: bool,
    ) -> Arc<Self> {
        let extension = Arc::new(Self {
            descriptor,
            frontend_url: Mutex::new(frontend_url),
            allow_insecure_http: Mutex::new(allow_insecure_http),
            in_setup,
            state: Mutex::new(None),
            self_weak: Mutex::new(None),
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
                runtime,
                pending_bootstrap: None,
            });
        }
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
        config.server_url = normalized.clone();
        config.allow_insecure_http = allow_http;
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
