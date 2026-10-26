// Builds Patina's file-type icons from the Material Icon Theme package
// (https://github.com/material-extensions/vscode-material-icon-theme, MIT).
//
// Writes:
//   src/lib/material-icons.json  name/extension → icon file lookup
//   static/material-icons/*.svg  the icons that lookup refers to
//
// Both are generated, so they're git-ignored. Runs before dev, build and check.
import { copyFileSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { generateManifest } = require("material-icon-theme");

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const iconsDir = join(dirname(require.resolve("material-icon-theme/package.json")), "icons");
const outDir = join(root, "static", "material-icons");
const outMap = join(root, "src", "lib", "material-icons.json");

const manifest = generateManifest();
const used = new Set();

/** Icon id → its SVG file name, remembering which files to copy. */
function file(id) {
  const def = manifest.iconDefinitions[id];
  if (!def) return null;
  const name = basename(def.iconPath);
  used.add(name);
  return name;
}

function table(ids) {
  const out = {};
  for (const [key, id] of Object.entries(ids)) {
    const name = file(id);
    if (name) out[key.toLowerCase()] = name;
  }
  return out;
}

const map = {
  file: file(manifest.file),
  folder: file(manifest.folder),
  fileNames: table(manifest.fileNames),
  fileExtensions: table(manifest.fileExtensions),
  folderNames: table(manifest.folderNames),
};

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });
const available = new Set(readdirSync(iconsDir));
let copied = 0;
for (const name of used) {
  if (!available.has(name)) continue;
  copyFileSync(join(iconsDir, name), join(outDir, name));
  copied++;
}
writeFileSync(outMap, JSON.stringify(map));
console.log(`material-icons: ${copied} icons, ${Object.keys(map.fileExtensions).length} extensions`);
