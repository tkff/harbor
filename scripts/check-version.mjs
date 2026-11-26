import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

export const projectRoot = fileURLToPath(new URL("../", import.meta.url));
export function checkVersions(root = projectRoot, tag) {
  const json = (name) => JSON.parse(readFileSync(resolve(root, name), "utf8"));
  const version = json("package.json").version;
  assert.match(
    version,
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/,
    "Invalid release version",
  );
  const lock = json("package-lock.json");
  const cargo = readFileSync(
    resolve(root, "src-tauri/Cargo.toml"),
    "utf8",
  ).split(/\r?\n/);
  const start = cargo.indexOf("[package]");
  assert.ok(start >= 0, "Cargo package section missing");
  const packageLines = cargo.slice(start + 1);
  const end = packageLines.findIndex((line) => line.startsWith("["));
  const packageSection = (
    end < 0 ? packageLines : packageLines.slice(0, end)
  ).join("\n");
  const cargoLock = readFileSync(resolve(root, "src-tauri/Cargo.lock"), "utf8")
    .split("[[package]]")
    .find((part) => /^name = "harbor"$/m.test(part));
  const versions = {
    "package-lock.json": lock.version,
    "package-lock.json root": lock.packages?.[""]?.version,
    "Cargo.toml": packageSection.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
    "Cargo.lock": cargoLock?.match(/^version = "([^"]+)"/m)?.[1],
    "tauri.conf.json": json("src-tauri/tauri.conf.json").version,
  };
  for (const [name, actual] of Object.entries(versions))
    assert.equal(actual, version, `${name} does not match package.json`);
  if (tag !== undefined)
    assert.equal(
      tag,
      `v${version}`,
      "Release tag must match the application version",
    );
  return version;
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  console.log(
    `Validated Harbor ${checkVersions(projectRoot, process.env.RELEASE_TAG)}`,
  );
}
