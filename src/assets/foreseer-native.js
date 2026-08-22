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
