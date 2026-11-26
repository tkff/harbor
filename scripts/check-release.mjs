import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
  unlinkSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { checkVersions } from "./check-version.mjs";
import { collect, manifest } from "./release-assets.mjs";

const root = mkdtempSync(resolve(tmpdir(), "harbor-release-test-"));
try {
  mkdirSync(resolve(root, "src-tauri"));
  const json = (name, value) =>
    writeFileSync(resolve(root, name), JSON.stringify(value));
  json("package.json", { version: "1.2.3" });
  json("package-lock.json", {
    version: "1.2.3",
    packages: { "": { version: "1.2.3" } },
  });
  json("src-tauri/tauri.conf.json", { version: "1.2.3" });
  writeFileSync(
    resolve(root, "src-tauri/Cargo.toml"),
    '[package]\nname = "harbor"\nversion = "1.2.3"\n[dependencies]\nversion = "9.9.9"\n',
  );
  writeFileSync(
    resolve(root, "src-tauri/Cargo.lock"),
    '[[package]]\nname = "dependency"\nversion = "9.9.9"\n[[package]]\nname = "harbor"\nversion = "1.2.3"\n',
  );
  assert.equal(checkVersions(root, "v1.2.3"), "1.2.3");
  assert.throws(() => checkVersions(root, "v1.2.4"));
  json("src-tauri/tauri.conf.json", { version: "1.2.4" });
  assert.throws(() => checkVersions(root));
  json("src-tauri/tauri.conf.json", { version: "1.2.3" });
  for (const [folder, name] of [
    ["release/bundle/nsis", "Harbor_1.2.3_x64-setup.exe"],
    ["release/bundle/msi", "Harbor_1.2.3_x64.msi"],
    ["release/bundle/deb", "Harbor_1.2.3_amd64.deb"],
    ["release/bundle/appimage", "Harbor_1.2.3_amd64.AppImage"],
    ["universal-apple-darwin/release/bundle/dmg", "Harbor_1.2.3_universal.dmg"],
  ]) {
    const path = resolve(root, "src-tauri/target", folder);
    mkdirSync(path, { recursive: true });
    writeFileSync(resolve(path, name), `Installer fixture: ${name}`);
  }
  collect("windows-x64", root);
  assert.throws(() => manifest(root), /incomplete release/);
  collect("linux-x64", root);
  collect("macos-universal", root);
  assert.equal(manifest(root).length, 6);
  const checksums = readFileSync(
    resolve(root, "release-artifacts/SHA256SUMS"),
    "utf8",
  )
    .trim()
    .split("\n");
  assert.equal(checksums.length, 5);
  for (const line of checksums) {
    const [hash, name] = line.split("  ");
    assert.equal(
      hash,
      createHash("sha256")
        .update(readFileSync(resolve(root, "release-artifacts", name)))
        .digest("hex"),
    );
  }
  writeFileSync(
    resolve(root, "src-tauri/target/release/bundle/nsis/Harbor_9.9.9_x64.exe"),
    "wrong version",
  );
  assert.equal(
    collect("windows-x64", root).length,
    2,
    "Old cached installers must be ignored",
  );
  unlinkSync(
    resolve(
      root,
      "src-tauri/target/release/bundle/nsis/Harbor_1.2.3_x64-setup.exe",
    ),
  );
  assert.throws(() => collect("windows-x64", root), /Missing/);
  assert.throws(() => collect("unknown", root));
  console.log(
    "Release version, installer completeness, and checksum checks passed.",
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
