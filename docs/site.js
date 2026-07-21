document.documentElement.classList.add("js");

const revealItems = document.querySelectorAll("[data-reveal]");
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

if (reduceMotion.matches || !("IntersectionObserver" in window)) {
  revealItems.forEach((item) => item.classList.add("is-visible"));
} else {
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        entry.target.classList.add("is-visible");
        observer.unobserve(entry.target);
      }
    },
    { rootMargin: "0px 0px -8%", threshold: 0.12 },
  );

  revealItems.forEach((item) => observer.observe(item));
}

const header = document.querySelector(".site-header");
const updateHeader = () => {
  header?.classList.toggle("is-scrolled", window.scrollY > 18);
};

updateHeader();
window.addEventListener("scroll", updateHeader, { passive: true });

/* Read the latest release once, then use it for both the published checksums
   and the download buttons, so neither can describe an older build than the
   other. The markup keeps the most recent known checksums and links to the
   releases page, so a failed or blocked request changes nothing. */
const checksumList = document.querySelector("[data-checksum-list]");
const downloadLinks = document.querySelectorAll("[data-download]");

if (checksumList || downloadLinks.length > 0) {
  const releaseLabel = document.querySelector("[data-checksum-release]");
  const statusLine = document.querySelector("[data-checksum-status]");
  const commandLine = document.querySelector("[data-checksum-command]");

  /* Only ever hand the button a real asset URL on this repository. A hostile
     or mistaken response cannot turn the button into a javascript: URL or
     point it at another host. */
  const isReleaseDownload = (value) => {
    try {
      const url = new URL(value);
      return (
        url.protocol === "https:" &&
        url.hostname === "github.com" &&
        url.pathname.startsWith("/jamditis/audiobud/releases/download/")
      );
    } catch {
      return false;
    }
  };

  /* Callers pass the assets that already carry a published digest, so the
     button never points at a file this page cannot also give a checksum for. */
  const linkDownloads = (assets) => {
    for (const link of downloadLinks) {
      const suffix = link.dataset.download;
      if (!suffix) continue;

      const asset = assets.find((item) => item.name.endsWith(suffix));
      if (!asset || !isReleaseDownload(asset.browser_download_url)) continue;
      link.href = asset.browser_download_url;
    }
  };

  const renderChecksums = (assets) => {
    if (!checksumList) return;
    checksumList.replaceChildren();

    for (const asset of assets) {
      const row = document.createElement("li");
      row.className = "checksum-row";

      const name = document.createElement("span");
      name.className = "checksum-name";
      name.textContent = asset.name;

      const value = document.createElement("code");
      value.className = "checksum-value";
      value.textContent = asset.digest.replace(/^sha256:/, "");

      row.append(name, value);
      checksumList.append(row);
    }
  };

  fetch("https://api.github.com/repos/jamditis/audiobud/releases/latest", {
    headers: { Accept: "application/vnd.github+json" },
  })
    .then((response) => {
      if (!response.ok) throw new Error(`GitHub responded ${response.status}`);
      return response.json();
    })
    .then((release) => {
      const assets = (release.assets ?? []).filter((asset) =>
        asset?.digest?.startsWith("sha256:"),
      );

      if (assets.length === 0) return;

      linkDownloads(assets);
      renderChecksums(assets);

      if (releaseLabel && release.tag_name) {
        releaseLabel.textContent = `Release ${release.tag_name}`;
      }

      const installer = assets.find((asset) => asset.name.endsWith(".exe"));
      if (commandLine && installer) {
        commandLine.textContent = `Get-FileHash -Algorithm SHA256 .\\${installer.name}`;
      }

      if (statusLine) {
        statusLine.textContent =
          "Read from the GitHub release just now. Compare against your own copy before installing.";
      }
    })
    .catch(() => {
      /* The markup already carries the last published checksums. */
    });
}
