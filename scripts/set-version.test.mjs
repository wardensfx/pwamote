import { test } from "node:test";
import assert from "node:assert/strict";
import { parseVersion, setCargoWorkspaceVersion, setJsonVersion } from "./set-version.mjs";

test("parseVersion accepte les tags SemVer, avec ou sans v", () => {
  assert.equal(parseVersion("v1.2.3"), "1.2.3");
  assert.equal(parseVersion("1.2.3"), "1.2.3");
  assert.equal(parseVersion("refs/tags/v0.10.0"), "0.10.0");
  assert.equal(parseVersion("v2.0.0-beta.1"), "2.0.0-beta.1");
});

test("parseVersion refuse ce qui n'est pas SemVer", () => {
  for (const bad of ["", "v1.2", "v01.2.3", "release-1", "v1.2.3.4", "latest"]) {
    assert.throws(() => parseVersion(bad), /Version invalide/, bad);
  }
});

test("setCargoWorkspaceVersion ne touche que [workspace.package]", () => {
  const toml = `[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2021"\n\n[workspace.dependencies]\nserde = { version = "1" }\n`;
  const out = setCargoWorkspaceVersion(toml, "1.2.3");
  assert.match(out, /\[workspace\.package\]\nversion = "1\.2\.3"/);
  assert.match(out, /serde = \{ version = "1" \}/);
});

test("setCargoWorkspaceVersion échoue sans section", () => {
  assert.throws(() => setCargoWorkspaceVersion(`[package]\nversion = "0.1.0"\n`, "1.0.0"));
});

test("setJsonVersion garde la mise en forme et les autres clés", () => {
  const json = `{\n  "name": "x",\n  "version": "0.1.0",\n  "dependencies": {\n    "dep": { "version": "9.9.9" }\n  }\n}\n`;
  const out = setJsonVersion(json, "1.2.3");
  assert.equal(out, json.replace('"version": "0.1.0"', '"version": "1.2.3"'));
});
