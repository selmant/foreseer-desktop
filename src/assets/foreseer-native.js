// Foreseer Native protocol v1 bridge. Talks only through jmpNative.extensionPostMessage.
(function installForeseerNative() {
  "use strict";
  const isSetupDocument = window.location.protocol === "data:";
  const REQUEST_ID = /^[A-Za-z0-9_-]{1,64}$/;
  const ITEM_ID = /^[A-Za-z0-9_-]{1,128}$/;
  const TICKET = /^[A-Za-z0-9_-]{43}$/;

  function post(command) {
    try {
      if (!window.jmpNative || typeof window.jmpNative.extensionPostMessage !== "function") {
        return false;
      }
      window.jmpNative.extensionPostMessage(JSON.stringify(command));
      return true;
    } catch (_) {
      return false;
    }
  }

  function showRuntimeRecovery(message) {
    if (isSetupDocument) return;
    document.title = "Foreseer Recovery";
    document.body.replaceChildren();
    const root = document.createElement("main");
    root.style.cssText = "max-width:42rem;margin:12vh auto;padding:2rem;font-family:system-ui,sans-serif;line-height:1.5";
    const heading = document.createElement("h1");
    heading.textContent = "Foreseer needs to restart";
    const detail = document.createElement("p");
    detail.textContent = message || "The bundled Foreseerr server stopped responding.";
    const hint = document.createElement("p");
    hint.textContent = "Close and reopen Foreseer to start the local server again. Your standalone data was not removed.";
    const quit = document.createElement("button");
    quit.type = "button";
    quit.textContent = "Quit";
    quit.addEventListener("click", function () {
      api.send({ type: "app.quit", id: crypto.randomUUID() });
    });
    const retry = document.createElement("button");
    retry.type = "button";
    retry.textContent = "Retry";
    retry.addEventListener("click", function () {
      retry.disabled = true;
      api.send({ type: "runtime.retry", id: crypto.randomUUID() });
    });
    const logs = document.createElement("button");
    logs.type = "button";
    logs.textContent = "Open Logs";
    logs.addEventListener("click", function () {
      api.send({ type: "runtime.open-logs", id: crypto.randomUUID() });
    });
    const remote = document.createElement("button");
    remote.type = "button";
    remote.textContent = "Use Remote Mode";
    remote.addEventListener("click", function () {
      remote.disabled = true;
      api.send({ type: "runtime.open-setup", id: crypto.randomUUID() });
    });
    root.append(heading, detail, hint, retry, logs, remote, quit);
    document.body.append(root);
  }

  const api = {
    protocolVersion: 1,
    hostName: "foreseer-desktop",
    hostVersion: "__HOST_VERSION__",
    capabilities: Object.freeze(
      isSetupDocument
        ? ["setup", "window-controls", "quit"]
        : [
            "play-item",
            "auth-bootstrap",
            "player-events",
            "session-reset",
            "browser-cache-clear",
            "mode-setup",
            "window-controls",
            "quit",
          ]
    ),
    send(command) {
      if (!command || typeof command !== "object" || typeof command.type !== "string") {
        return false;
      }
      if (typeof command.id !== "string" || !REQUEST_ID.test(command.id)) {
        return false;
      }
      switch (command.type) {
        case "auth.challenge":
        case "session.clear":
        case "runtime.retry":
        case "runtime.open-logs":
        case "runtime.open-setup":
        case "window.minimize":
        case "window.toggle-maximize":
        case "window.toggle-fullscreen":
        case "app.quit":
          return post(command);
        case "auth.complete":
        case "browser-cache.clear":
          return typeof command.ticket === "string" && TICKET.test(command.ticket)
            ? post(command)
            : false;
        case "play.item":
          return typeof command.itemId === "string" && ITEM_ID.test(command.itemId)
            ? post(command)
            : false;
        case "setup.check":
        case "setup.save":
          if (!isSetupDocument) return false;
          return typeof command.url === "string" && typeof command.allowHttp === "boolean"
            ? post(command)
            : false;
        default:
          return false;
      }
    },
  };

  Object.defineProperty(window, "foreseerNative", {
    configurable: false,
    enumerable: true,
    writable: false,
    value: Object.freeze(api),
  });

  window.addEventListener("jellium:extension-message", function (ev) {
    let detail = ev.detail;
    if (typeof detail === "string") {
      try {
        detail = JSON.parse(detail);
      } catch (_) {
        return;
      }
    }
    if (!detail || typeof detail !== "object") return;
    if (detail.type === "runtime-failed") {
      showRuntimeRecovery(detail.message);
    }
    if (detail.type === "runtime-recovered") {
      window.location.reload();
    }
    if (detail.type === "auth-challenge" || detail.type === "error") {
      console.info("[ForeseerNative] host event", detail.type, detail.id);
    }
    window.dispatchEvent(
      new CustomEvent("foreseer:native-event", {
        detail: Object.freeze(detail),
      })
    );
  });
})();
