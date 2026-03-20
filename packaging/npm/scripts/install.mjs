import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const packageDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const packageJson = JSON.parse(readFileSync(path.join(packageDir, "package.json"), "utf8"));
const version = packageJson.version;

if (process.env.SXMC_NPM_SKIP_DOWNLOAD === "1") {
  process.exit(0);
}

const target = resolveTarget();
const releaseTag = `v${version}`;
const archiveName = `sxmc-${releaseTag}-${target.target}.${target.archiveExt}`;
const checksumName = `${archiveName}.sha256`;
const downloadBase =
  process.env.SXMC_NPM_DOWNLOAD_BASE ??
  `https://github.com/aihxp/sxmc/releases/download/${releaseTag}`;
const url = `${downloadBase}/${archiveName}`;
const checksumUrl = `${downloadBase}/${checksumName}`;

const vendorDir = path.join(packageDir, "vendor");
const tempDir = mkdtempSync(path.join(tmpdir(), "sxmc-npm-"));
const archivePath = path.join(tempDir, archiveName);
const extractDir = path.join(tempDir, "extract");

try {
  mkdirSync(vendorDir, { recursive: true });
  mkdirSync(extractDir, { recursive: true });

  const response = await fetch(url, {
    headers: {
      "User-Agent": "@aihxp/sxmc npm installer",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to download ${url} (${response.status} ${response.statusText})`);
  }

  const buffer = Buffer.from(await response.arrayBuffer());
  await writeFile(archivePath, buffer);
  await verifyArchive(buffer, checksumUrl, checksumName);

  if (target.archiveExt === "zip") {
    execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-Command",
        `Expand-Archive -Path '${archivePath}' -DestinationPath '${extractDir}' -Force`,
      ],
      { stdio: "inherit" },
    );
  } else {
    execFileSync("tar", ["-xzf", archivePath, "-C", extractDir], { stdio: "inherit" });
  }

  const packageRoot = path.join(extractDir, `sxmc-${releaseTag}-${target.target}`);
  const binaryName = process.platform === "win32" ? "sxmc.exe" : "sxmc";
  const sourceBinary = path.join(packageRoot, binaryName);
  const destinationBinary = path.join(vendorDir, binaryName);

  if (!existsSync(sourceBinary)) {
    throw new Error(`Downloaded archive did not contain ${binaryName}`);
  }

  copyFileSync(sourceBinary, destinationBinary);
  if (process.platform !== "win32") {
    chmodSync(destinationBinary, 0o755);
  }
} finally {
  rmSync(tempDir, { recursive: true, force: true });
}

function resolveTarget() {
  if (process.platform === "darwin" && process.arch === "arm64") {
    return { target: "aarch64-apple-darwin", archiveExt: "tar.gz" };
  }
  if (process.platform === "darwin" && process.arch === "x64") {
    return { target: "x86_64-apple-darwin", archiveExt: "tar.gz" };
  }
  if (process.platform === "linux" && process.arch === "x64") {
    return { target: "x86_64-unknown-linux-gnu", archiveExt: "tar.gz" };
  }
  if (process.platform === "win32" && process.arch === "x64") {
    return { target: "x86_64-pc-windows-msvc", archiveExt: "zip" };
  }

  throw new Error(
    `Unsupported platform for prebuilt sxmc binaries: ${process.platform}/${process.arch}. Use cargo install sxmc or build from source instead.`,
  );
}

async function verifyArchive(buffer, checksumUrl, checksumName) {
  const response = await fetch(checksumUrl, {
    headers: {
      "User-Agent": "@aihxp/sxmc npm installer",
    },
  });

  if (!response.ok) {
    throw new Error(
      `Failed to download ${checksumName} (${response.status} ${response.statusText})`,
    );
  }

  const checksumText = (await response.text()).trim();
  const expected = checksumText.split(/\s+/)[0]?.toLowerCase();
  const actual = createHash("sha256").update(buffer).digest("hex");

  if (!expected || expected !== actual) {
    throw new Error(
      `Checksum mismatch for ${checksumName}: expected ${expected ?? "missing"}, got ${actual}`,
    );
  }
}
