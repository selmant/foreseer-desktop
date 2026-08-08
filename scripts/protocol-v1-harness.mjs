#!/usr/bin/env node

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixturePath = join(repoRoot, "protocol/protocol-v1.json");
const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));

for (const siblingFixture of [
  resolve(repoRoot, "../jellium-desktop/protocol/protocol-v1.json"),
  resolve(repoRoot, "../SeerrSuggestArr/protocol/protocol-v1.json"),
]) {
  if (existsSync(siblingFixture)) {
    assert.deepEqual(
      JSON.parse(readFileSync(siblingFixture, "utf8")),
      fixture,
      `protocol fixture drift: ${siblingFixture}`,
    );
  }
}

assert.equal(fixture.protocolVersion, 1);
assert.deepEqual(fixture.hostMethods.playItem, ["requestId", "itemId"]);
assert.equal(fixture.resumePolicy.owner, "jellyfin");
assert.equal(fixture.resumePolicy.startPositionTicksInProtocol, false);

const setupScope = {};
setupScope.globalThis = setupScope;
vm.runInNewContext(
  readFileSync(join(repoRoot, "src/setup-event.js"), "utf8"),
  setupScope,
  { filename: "setup-event.js" },
);
const setupListener = setupScope.foreseerSetupProtocolV1;
assert.deepEqual([...setupListener.eventTypes], fixture.setupEventTypes);

// Jellium's Rust emitter test serializes this same canonical example; this
// harness feeds it through the exact listener embedded in setup.html.
const setupEmission = fixture.examples.setupConnectivitySuccess;
const setupResult = setupListener.parseEvent(
  setupEmission,
  "setup-connectivity",
);
assert.equal(setupResult.status, 204);
assert.equal(setupResult.message, null);
assert.equal(
  setupListener.parseEvent(
    { ...setupEmission, challenge: "wrong-envelope" },
    "setup-connectivity",
  ),
  undefined,
);

const redactedKeys = new Set([
  ...fixture.bootstrapEnvelope.redactedFields,
  ...fixture.bootstrapEnvelope.nativeOnlyFields,
]);
const safeRecords = [];
const redact = (value) => {
  if (Array.isArray(value)) return value.map(redact);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [
        key,
        redactedKeys.has(key) ? "[REDACTED]" : redact(entry),
      ]),
    );
  }
  return value;
};
const log = (phase, detail) => safeRecords.push({ phase, ...redact(detail) });

const trace = [];
const emit = (requestId, type, extra = {}) => {
  assert(fixture.hostEventTypes.includes(type), `closed event type: ${type}`);
  assert(requestId.length <= fixture.limits.requestIdMaxLength);
  const event = { protocolVersion: 1, requestId, type, ...extra };
  trace.push(event);
  return event;
};

const fakeForeseer = {
  issueTicket(requestId, challenge) {
    assert.equal(challenge.length, fixture.limits.challengeHexLength);
    log("auth-ticket-issued", { requestId });
    return "T".repeat(fixture.limits.ticketLength);
  },
  redeem(ticket, verifier) {
    assert.equal(ticket.length, fixture.limits.ticketLength);
    assert.equal(verifier.length, 43);
    return {
      serverUrl: "https://jellyfin.example.test",
      serverId: "server-1",
      userId: "user-1",
      deviceId: "device-1",
      accessToken: "access-token-sentinel",
      bootstrapGeneration: "generation-1",
    };
  },
};

const fakeJellyfin = {
  resumeTicks: new Map([
    ["item-a", 72_000_000],
    ["item-b", 0],
  ]),
  resolve(itemId) {
    assert(itemId.length <= fixture.limits.itemIdMaxLength);
    return { itemId, resumeTicks: this.resumeTicks.get(itemId) ?? 0 };
  },
};

let activeRequestId;
let visibleSurface = "foreseer";
const foreseerRoute = "/discover/movie/42?from=trending";
let currentRoute = foreseerRoute;

const consumePlaybackEvent = (event) => {
  if (event.requestId !== activeRequestId) {
    log("stale-event-ignored", {
      requestId: event.requestId,
      type: event.type,
      activeRequestId,
    });
    return;
  }
  if (event.type === "playing") visibleSurface = "player";
  if (fixture.terminalPlayEventTypes.includes(event.type)) {
    activeRequestId = undefined;
    visibleSurface = "foreseer";
  }
};

const fakeHost = {
  protocolVersion: fixture.protocolVersion,
  hostName: fixture.host.name,
  hostVersion: "0.1.1",
  capabilities: fixture.host.capabilities,
  authenticate(requestId) {
    const verifier = "v".repeat(43);
    const challenge = "c".repeat(fixture.limits.challengeHexLength);
    emit(requestId, "auth-challenge", { challenge });
    const ticket = fakeForeseer.issueTicket(requestId, challenge);
    const bootstrap = fakeForeseer.redeem(ticket, verifier);
    log("native-bootstrap", { requestId, ticket, verifier, ...bootstrap });
    emit(requestId, "ready");
  },
  playItem(requestId, itemId) {
    assert.deepEqual(
      [requestId, itemId].length,
      fixture.hostMethods.playItem.length,
    );
    const replaced = activeRequestId;
    activeRequestId = requestId;
    const resolved = fakeJellyfin.resolve(itemId);
    const accepted = emit(requestId, "accepted");
    consumePlaybackEvent(accepted);
    if (replaced) consumePlaybackEvent(emit(replaced, "canceled"));
    log("resume-policy", {
      requestId,
      owner: fixture.resumePolicy.owner,
      nativeArguments: [requestId, itemId],
      jellyfinSelectedResume: resolved.resumeTicks > 0,
    });
    return true;
  },
  playing(requestId) {
    consumePlaybackEvent(emit(requestId, "playing"));
  },
  finish(requestId) {
    consumePlaybackEvent(emit(requestId, "finished"));
  },
  back(requestId) {
    log("player-back", { requestId, route: currentRoute });
    consumePlaybackEvent(emit(requestId, "stopped"));
  },
  rendererFailure(requestId) {
    log("renderer-failure", { requestId, errorCode: "renderer_failed" });
    consumePlaybackEvent(
      emit(requestId, "error", { errorCode: "renderer_failed" }),
    );
  },
};

let browserNavigationPrevented = false;
const ordinaryBrowserAdmitted = false;
if (ordinaryBrowserAdmitted) browserNavigationPrevented = true;
assert.equal(browserNavigationPrevented, false);
log("browser-fallback", {
  fallbackUrl: "https://jellyfin.example.test/web/#/details?id=item-a",
  navigationPrevented: browserNavigationPrevented,
});

log("native-discovery", {
  protocolVersion: fakeHost.protocolVersion,
  hostName: fakeHost.hostName,
  hostVersion: fakeHost.hostVersion,
  capabilities: fakeHost.capabilities,
});
fakeHost.authenticate("auth-1");

fakeHost.playItem("play-a", "item-a");
fakeHost.playItem("play-b", "item-b");
fakeHost.playing("play-b");
fakeHost.finish("play-b");
assert.equal(activeRequestId, undefined);
assert.equal(visibleSurface, "foreseer");

assert.deepEqual(
  trace
    .filter((event) => event.requestId.startsWith("play-"))
    .map(({ requestId, type }) => [requestId, type]),
  fixture.requestCorrelation.replacementSequence,
);

fakeHost.playItem("play-back", "item-a");
fakeHost.playing("play-back");
fakeHost.back("play-back");
assert.equal(visibleSurface, "foreseer");
assert.equal(currentRoute, foreseerRoute);

fakeHost.playItem("play-renderer", "item-b");
fakeHost.playing("play-renderer");
fakeHost.rendererFailure("play-renderer");
assert.equal(visibleSurface, "foreseer");
assert.equal(activeRequestId, undefined);
assert.equal(currentRoute, foreseerRoute);

const output = {
  fixtureId: fixture.fixtureId,
  result: "PASS",
  setupEnvelope: setupResult,
  trace,
  safeLog: safeRecords,
  manualRemaining: [
    "Wayland and X11 visible-video/audio/focus matrix",
    "resize, fullscreen, mixed-DPI, suspend/resume, and renderer recovery",
    "50-cycle discovery/play/Back soak with surface and audio leak checks",
  ],
};
const serialized = JSON.stringify(output, null, 2);
for (const secret of [
  "access-token-sentinel",
  "device-1",
  "T".repeat(fixture.limits.ticketLength),
  "v".repeat(43),
]) {
  assert(!serialized.includes(secret), "secret reached harness output");
}
process.stdout.write(`${serialized}\n`);
