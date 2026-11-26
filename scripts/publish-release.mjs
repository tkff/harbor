import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import assert from "node:assert/strict";
import { checkVersions, projectRoot } from "./check-version.mjs";
import { manifest } from "./release-assets.mjs";

const tag = process.env.RELEASE_TAG;
assert.ok(
  tag && process.env.GH_REPO && process.env.GH_TOKEN,
  "Release environment missing",
);
const version = checkVersions(projectRoot, tag);
const files = manifest().map((name) =>
  resolve(projectRoot, "release-artifacts", name),
);
function gh(args) {
  const result = spawnSync("gh", args, { stdio: "inherit", cwd: projectRoot });
  assert.equal(result.status, 0, `GitHub release command failed: ${args[1]}`);
}
const existing = spawnSync(
  "gh",
  ["release", "view", tag, "--json", "isDraft"],
  { encoding: "utf8" },
);
if (existing.status === 0) {
  assert.equal(
    JSON.parse(existing.stdout).isDraft,
    true,
    "Refusing to overwrite an already published release",
  );
  gh(["release", "upload", tag, ...files, "--clobber"]);
} else {
  gh([
    "release",
    "create",
    tag,
    ...files,
    "--verify-tag",
    "--title",
    `Harbor ${version}`,
    "--notes-file",
    "RELEASE_NOTES.md",
    "--draft",
    "--prerelease",
  ]);
}
// The release remains a draft if any upload fails. Publication is the final step.
gh([
  "release",
  "edit",
  tag,
  "--draft=false",
  "--prerelease",
  "--title",
  `Harbor ${version}`,
  "--notes-file",
  "RELEASE_NOTES.md",
]);
