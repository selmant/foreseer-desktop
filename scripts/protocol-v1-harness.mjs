#!/usr/bin/env node
/**
 * Deterministic protocol v1 fixture harness (no CEF/network).
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const fixture = JSON.parse(readFileSync(join(root, "protocol/protocol-v1.json"), "utf8"));
const cargoToml = readFileSync(join(root, "Cargo.toml"), "utf8");
const versionMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
const pkgVersion = versionMatch ? versionMatch[1] : "";
const failures = [];

function assert(cond, msg) {
  if (!cond) failures.push(msg);
}

assert(fixture.protocolVersion === 1, "protocolVersion must be 1");
assert(fixture.host.name === "foreseer-desktop", "host name must be foreseer-desktop");
assert(fixture.eventName === "foreseer:native-event", "event name mismatch");
assert(fixture.limits.maxPayloadBytes === 16384, "payload limit must be 16KiB");
assert(fixture.browserFallback.required === true, "browser fallback required when no native host");
assert(
  fixture.browserFallback.navigationPreventedOnlyAfterNativeAdmission === true,
  "navigation must wait for native admission"
);
assert(typeof pkgVersion === "string" && pkgVersion.length > 0, "package version present");

const types = new Set(fixture.commands.map((c) => c.type));
for (const required of [
  "auth.challenge",
  "auth.complete",
  "session.clear",
  "play.item",
  "setup.check",
  "setup.save",
  "window.minimize",
  "app.quit",
]) {
  assert(types.has(required), `missing command ${required}`);
}

if (failures.length) {
  console.error("protocol-v1 harness failed:");
  for (const f of failures) console.error(" -", f);
  process.exit(1);
}
console.log("protocol-v1 harness ok", { protocolVersion: 1, packageVersion: pkgVersion });
