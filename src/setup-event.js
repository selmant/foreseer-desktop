(function installForeseerSetupProtocol(scope) {
  "use strict";

  const eventTypes = Object.freeze([
    "connectivity-success",
    "save-config-success",
    "error",
  ]);

  function parseEvent(detail, expectedRequestId) {
    if (
      !detail ||
      typeof detail !== "object" ||
      detail.protocolVersion !== 1 ||
      detail.requestId !== expectedRequestId ||
      !eventTypes.includes(detail.type) ||
      "challenge" in detail ||
      "errorCode" in detail
    ) {
      return undefined;
    }
    if (
      detail.status !== null &&
      detail.status !== undefined &&
      (!Number.isInteger(detail.status) ||
        detail.status < 100 ||
        detail.status > 599)
    ) {
      return undefined;
    }
    if (
      detail.message !== null &&
      detail.message !== undefined &&
      (typeof detail.message !== "string" ||
        Array.from(detail.message).length > 256)
    ) {
      return undefined;
    }
    return Object.freeze({
      protocolVersion: 1,
      requestId: detail.requestId,
      type: detail.type,
      status: detail.status ?? null,
      message: detail.message ?? null,
    });
  }

  Object.defineProperty(scope, "foreseerSetupProtocolV1", {
    configurable: false,
    enumerable: false,
    writable: false,
    value: Object.freeze({ eventTypes, parseEvent }),
  });
})(globalThis);
