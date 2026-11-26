import {
  readdirSync,
  mkdirSync,
  copyFileSync,
  readFileSync,
  writeFileSync,
  lstatSync,
} from "node:fs";
import { resolve, basename } from "node:path";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { checkVersions, projectRoot } from "./check-version.mjs";

const platforms = {
  "windows-x64": {
    root: "src-tauri/target/release/bundle",
    folders: ["nsis", "msi"],
    extensions: [".exe", ".msi"],
  },
  "linux-x64": {
    root: "src-tauri/target/release/bundle",
    folders: ["deb", "appimage"],
    extensions: [".deb", ".AppImage"],
  },
  "macos-universal": {
    root: "src-tauri/target/universal-apple-darwin/release/bundle",
    folders: ["dmg"],
    extensions: [".dmg"],
  },
};

export function collect(platform, root = projectRoot) {
  const config = platforms[platform];
  assert.ok(config, `Unknown release platform: ${platform}`);
  const version = checkVersions(root);
  const output = resolve(root, "release-artifacts");
  mkdirSync(output, { recursive: true });
  const copied = [];
  for (const folder of config.folders) {
    const directory = resolve(root, config.root, folder);
    for (const file of readdirSync(directory, { withFileTypes: true })) {
      if (
        !file.isFile() ||
        !config.extensions.some((ext) => file.name.endsWith(ext))
      )
        continue;
      // Cached target directories can contain installers from an older version.
      if (!file.name.includes(`_${version}_`)) continue;
      copyFileSync(resolve(directory, file.name), resolve(output, file.name));
      copied.push(file.name);
    }
  }
  for (const extension of config.extensions)
    assert.ok(
      copied.some((name) => name.endsWith(extension)),
      `Missing ${platform} ${extension} installer`,
    );
  return copied;
}

export function manifest(root = projectRoot) {
  const version = checkVersions(root);
  const output = resolve(root, "release-artifacts");
  const extensions = Object.values(platforms).flatMap((p) => p.extensions);
  const files = readdirSync(output)
    .filter((name) => extensions.some((ext) => name.endsWith(ext)))
    .sort();
  for (const extension of extensions)
    assert.ok(
      files.some((name) => name.endsWith(extension)),
      `Refusing an incomplete release: missing ${extension}`,
    );
  const lines = files.map((name) => {
    assert.ok(
      name.includes(`_${version}_`) && !/[\r\n\\]/.test(name),
      `Invalid installer filename: ${name}`,
    );
    const file = resolve(output, basename(name));
    assert.ok(
      lstatSync(file).isFile() && lstatSync(file).size > 0,
      `Invalid installer: ${name}`,
    );
    return `${createHash("sha256").update(readFileSync(file)).digest("hex")}  ${name}`;
  });
  writeFileSync(resolve(output, "SHA256SUMS"), `${lines.join("\n")}\n`);
  return [...files, "SHA256SUMS"];
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const action = process.argv[2];
  assert.ok(
    ["collect", "manifest"].includes(action),
    "Use collect <platform> or manifest",
  );
  console.log(
    (action === "collect" ? collect(process.argv[3]) : manifest()).join("\n"),
  );
}
