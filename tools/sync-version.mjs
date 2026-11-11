#!/usr/bin/env node
// package.json is the only place the app version is written. `npm version`
// runs this as its version lifecycle script, and CI runs it with --check so
// the Cargo workspace cannot drift from the bundles.
import { readFileSync, writeFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const check = process.argv.includes("--check");
const version = JSON.parse(readFileSync(new URL("package.json", root), "utf8"))
  .version;

function fail(message) {
  console.error(`sync-version: ${message}`);
  process.exit(1);
}

if (typeof version !== "string" || !/^\d+\.\d+\.\d+(?:[-+].+)?$/.test(version)) {
  fail("package.json has no semver version");
}

const tauri = JSON.parse(
  readFileSync(new URL("src-tauri/tauri.conf.json", root), "utf8"),
);
if (tauri.version !== "../package.json") {
  fail('src-tauri/tauri.conf.json must point version at "../package.json"');
}

const cargoUrl = new URL("Cargo.toml", root);
const cargo = readFileSync(cargoUrl, "utf8");
const header = "[workspace.package]";
const start = cargo.indexOf(header);
const end = start === -1 ? -1 : cargo.indexOf("\n[", start + header.length);
if (start === -1) fail("Cargo.toml has no [workspace.package] section");

const block = cargo.slice(start, end === -1 ? undefined : end);
const current = block.match(/^version = "([^"]+)"/m);
if (!current) fail("Cargo.toml [workspace.package] has no version");

if (current[1] === version) {
  console.log(`version ${version} is in sync`);
  process.exit(0);
}
if (check) fail(`Cargo.toml says ${current[1]}, package.json says ${version}`);

const updated = block.replace(current[0], `version = "${version}"`);
writeFileSync(
  cargoUrl,
  cargo.slice(0, start) + updated + cargo.slice(start + block.length),
);
console.log(`Cargo.toml version ${current[1]} -> ${version}`);
