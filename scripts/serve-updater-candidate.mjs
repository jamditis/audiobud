import { createHash } from "node:crypto";
import {
  createReadStream,
  linkSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:https";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SEMVER = /^\d+\.\d+\.\d+$/;
const BASE64 = /^[A-Za-z0-9+/]+={0,2}$/;

export function buildCandidateManifest({
  version,
  signature,
  archiveName,
  port,
  pubDate,
}) {
  if (typeof version !== "string" || !SEMVER.test(version)) {
    throw new Error(`Candidate version is not stable semver: ${version}`);
  }
  const expectedArchive = `AudioBud_${version}_x64-setup.nsis.zip`;
  if (archiveName !== expectedArchive) {
    throw new Error(
      `Candidate archive is ${archiveName}; expected ${expectedArchive}`,
    );
  }
  if (
    typeof signature !== "string" ||
    !BASE64.test(signature) ||
    /[\r\n]/.test(signature)
  ) {
    throw new Error("Candidate signature must be one base64 line");
  }
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`Candidate server port is invalid: ${port}`);
  }
  const date = new Date(pubDate);
  if (Number.isNaN(date.valueOf())) {
    throw new Error(`Candidate publication date is invalid: ${pubDate}`);
  }

  return {
    version,
    notes: "Private prepublication updater verification",
    pub_date: date.toISOString(),
    platforms: {
      "windows-x86_64": {
        signature,
        url: `https://localhost:${port}/${archiveName}`,
      },
    },
  };
}

export function candidateRoute(pathname, archiveName) {
  if (pathname === "/latest-candidate.json") return "manifest";
  if (pathname === `/${archiveName}`) return "archive";
  return null;
}

export function publishReadyFile(readyPath, readiness) {
  const temporaryPath = `${readyPath}.${process.pid}.tmp`;
  try {
    writeFileSync(temporaryPath, `${JSON.stringify(readiness, null, 2)}\n`, {
      encoding: "utf8",
      flag: "wx",
    });
    linkSync(temporaryPath, readyPath);
  } finally {
    rmSync(temporaryPath, { force: true });
  }
}

function requiredOption(name) {
  const index = process.argv.indexOf(name);
  const value = process.argv[index + 1];
  if (index === -1 || !value || value.startsWith("--")) {
    throw new Error(`Missing required option: ${name}`);
  }
  return value;
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function main() {
  const archivePath = resolve(requiredOption("--archive"));
  const signaturePath = resolve(requiredOption("--signature"));
  const pfxPath = resolve(requiredOption("--pfx"));
  const readyPath = resolve(requiredOption("--ready"));
  const version = requiredOption("--version");
  const pubDate = requiredOption("--pub-date");
  const archiveName = basename(archivePath);
  const signature = readFileSync(signaturePath, "utf8").trim();
  const passphrase = process.env.AUDIOBUD_CANDIDATE_PFX_PASSWORD;
  if (!passphrase) {
    throw new Error("AUDIOBUD_CANDIDATE_PFX_PASSWORD is required");
  }
  if (!statSync(archivePath).isFile() || statSync(archivePath).size === 0) {
    throw new Error(`Candidate archive is empty: ${archivePath}`);
  }

  let manifestJson;
  const server = createServer(
    { pfx: readFileSync(pfxPath), passphrase },
    (request, response) => {
      if (request.method !== "GET") {
        response.writeHead(405, { Allow: "GET" });
        response.end();
        return;
      }

      let pathname;
      try {
        pathname = new URL(request.url ?? "", "https://localhost").pathname;
      } catch {
        response.writeHead(400);
        response.end();
        return;
      }
      const route = candidateRoute(pathname, archiveName);
      if (route === "manifest") {
        response.writeHead(200, {
          "Cache-Control": "no-store",
          "Content-Length": Buffer.byteLength(manifestJson),
          "Content-Type": "application/json; charset=utf-8",
        });
        response.end(manifestJson);
        return;
      }
      if (route === "archive") {
        response.writeHead(200, {
          "Cache-Control": "no-store",
          "Content-Length": statSync(archivePath).size,
          "Content-Type": "application/octet-stream",
        });
        createReadStream(archivePath).pipe(response);
        return;
      }
      response.writeHead(404);
      response.end();
    },
  );

  server.on("error", (error) => {
    process.stderr.write(`${error.stack ?? error}\n`);
    process.exitCode = 1;
  });
  server.listen(0, "localhost", () => {
    const address = server.address();
    if (!address || typeof address === "string") {
      throw new Error("Candidate server did not bind to a TCP loopback port");
    }
    if (address.address !== "127.0.0.1" && address.address !== "::1") {
      throw new Error(`Candidate server escaped loopback: ${address.address}`);
    }
    const manifest = buildCandidateManifest({
      version,
      signature,
      archiveName,
      port: address.port,
      pubDate,
    });
    manifestJson = `${JSON.stringify(manifest, null, 2)}\n`;
    publishReadyFile(readyPath, {
      archive_sha256: sha256(archivePath),
      manifest_url: `https://localhost:${address.port}/latest-candidate.json`,
      port: address.port,
    });
  });

  const stop = () => server.close(() => process.exit(0));
  process.on("SIGINT", stop);
  process.on("SIGTERM", stop);
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  main();
}
