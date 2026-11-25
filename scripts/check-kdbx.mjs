// Independent implementation: read Rust-written KDBX, edit and save for Rust to reopen.
// Uses only temporary test credentials created by the Rust integration test.
import fs from "node:fs/promises";
import assert from "node:assert/strict";
import kdbxweb from "kdbxweb";
import { argon2d, argon2id } from "hash-wasm";

kdbxweb.CryptoEngine.setArgon2Impl(
  (password, salt, memory, iterations, hashLength, parallelism, type) => {
    const hash = type === 0 ? argon2d : type === 2 ? argon2id : null;
    if (!hash) throw new Error("Unsupported Argon2 variant");
    return hash({
      password: new Uint8Array(password),
      salt: new Uint8Array(salt),
      memorySize: memory,
      iterations,
      hashLength,
      parallelism,
      outputType: "binary",
    });
  },
);
const file = process.argv[2];
assert.ok(file, "Pass the temporary fixture path");
const keyFileBytes = process.argv[3]
  ? await fs.readFile(process.argv[3])
  : null;
const keyFile = keyFileBytes
  ? keyFileBytes.buffer.slice(
      keyFileBytes.byteOffset,
      keyFileBytes.byteOffset + keyFileBytes.byteLength,
    )
  : undefined;
const credentials = new kdbxweb.Credentials(
  kdbxweb.ProtectedValue.fromString("test-master-password"),
  keyFile?.slice(0),
);
const bytes = await fs.readFile(file);
const db = await kdbxweb.Kdbx.load(
  bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  credentials,
);
if (keyFile) {
  const data = bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  );
  await assert.rejects(
    kdbxweb.Kdbx.load(
      data,
      new kdbxweb.Credentials(
        kdbxweb.ProtectedValue.fromString("test-master-password"),
      ),
    ),
  );
  await assert.rejects(
    kdbxweb.Kdbx.load(
      data,
      new kdbxweb.Credentials(
        kdbxweb.ProtectedValue.fromString("wrong password"),
        keyFile.slice(0),
      ),
    ),
  );
}
const entry = db
  .getDefaultGroup()
  .entries.find((e) => e.fields.get("Title") === "Test server");
assert.ok(entry, "Connection entry is readable");
assert.equal(entry.fields.get("UserName").getText(), "harbor-test-user");
assert.equal(entry.fields.get("Password").getText(), "test-ssh-secret");
assert.equal(entry.fields.get("Harbor.Host"), "127.0.0.1");
assert.equal(entry.fields.get("Harbor.Folder"), "Lab");
assert.deepEqual([...entry.tags].sort(), ["linux", "production"]);
const snippet = db
  .getDefaultGroup()
  .entries.find((e) => e.fields.get("Title") === "Portable snippet");
if (snippet) {
  assert.equal(snippet.fields.get("Harbor.Type"), "snippet");
  assert.equal(
    snippet.fields.get("Harbor.SnippetBody").getText(),
    "printf interop-snippet",
  );
  snippet.pushHistory();
  snippet.fields.set(
    "Harbor.SnippetBody",
    kdbxweb.ProtectedValue.fromString("printf updated-by-kdbxweb"),
  );
  snippet.times.update();
}
entry.pushHistory();
entry.tags = ["external-tag"];
entry.fields.set("Harbor.Folder", "External group");
entry.fields.set(
  "UserName",
  kdbxweb.ProtectedValue.fromString("updated-by-kdbxweb"),
);
entry.fields.set("Independent.Custom", "preserve me");
entry.times.update();
await fs.writeFile(file, new Uint8Array(await db.save()));
console.log(
  "Independent KDBX read, credential verification, history update, and write passed.",
);
