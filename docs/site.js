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

/* Refresh the published checksums from the latest release so they never
   describe an older build than the download button points at. The markup keeps
   the most recent known values, so a failed or blocked request changes
   nothing. */
const checksumList = document.querySelector("[data-checksum-list]");

if (checksumList) {
  const releaseLabel = document.querySelector("[data-checksum-release]");
  const statusLine = document.querySelector("[data-checksum-status]");
  const commandLine = document.querySelector("[data-checksum-command]");

  const renderChecksums = (assets) => {
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
