// Ensure the aria2c sidecar for the current platform exists before
// `tauri dev` / `tauri build` runs. Binaries are NOT committed — this
// fetches them on demand (skips instantly when already present).
//
// Usage: node scripts/ensure-sidecars.mjs [--force]
import { execSync, spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
} from "node:fs";
import { join } from "node:path";

const FORCE = process.argv.includes("--force");
const BIN = "src-tauri/binaries";
mkdirSync(BIN, { recursive: true });

function run(cmd, opts = {}) {
  const r = spawnSync(cmd, { shell: true, stdio: "inherit", ...opts });
  if (r.status !== 0) process.exit(r.status ?? 1);
}

function findFile(dir, name) {
  try {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, e.name);
      if (e.isDirectory()) {
        const hit = findFile(p, name);
        if (hit) return hit;
      } else if (e.name === name) {
        return p;
      }
    }
  } catch {
    /* ignore */
  }
  return null;
}

switch (process.platform) {
  case "darwin": {
    // Build from source with Apple TLS (one-time; cached afterwards).
    run(`bash scripts/fetch-aria2.sh${FORCE ? " --force" : ""}`);
    break;
  }
  case "win32": {
    const dest = join(BIN, "aria2c-x86_64-pc-windows-msvc.exe");
    const gnu = join(BIN, "aria2c-x86_64-pc-windows-gnu.exe");
    if (!FORCE && existsSync(dest)) {
      console.log(`[ensure-sidecars] present: ${dest}`);
      copyFileSync(dest, gnu);
      break;
    }
    console.log("[ensure-sidecars] downloading Windows static aria2c...");
    mkdirSync(".aria2-build", { recursive: true });
    const zip = ".aria2-build/aria2-win-x64.zip";
    run(
      `curl -fL --retry 3 -o ${zip} https://github.com/abcfy2/aria2-static-build/releases/download/continuous/aria2-x86_64-w64-mingw32_static.zip`
    );
    // bsdtar ships with Windows 10+ and extracts zip archives.
    const tar = spawnSync("tar", ["-xf", zip, "-C", ".aria2-build"], {
      shell: true,
    });
    if (tar.status !== 0) {
      run(
        `powershell -NoProfile -Command "Expand-Archive -Force ${zip} .aria2-build"`
      );
    }
    const hit =
      findFile(".aria2-build", "aria2c.exe") ??
      (() => {
        console.error("[ensure-sidecars] aria2c.exe not found in archive");
        process.exit(1);
      })();
    copyFileSync(hit, dest);
    copyFileSync(dest, gnu);
    console.log(`[ensure-sidecars] saved: ${dest}`);
    break;
  }
  default:
    // Linux builds are not officially supported yet; fall back to a
    // system-wide aria2c (resolve_binary() also checks PATH at runtime).
    console.log(
      "[ensure-sidecars] unsupported host — relying on system aria2c in PATH"
    );
}
