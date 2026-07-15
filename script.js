const initReleases = async () => {
  const REPO = "taisho-guy/NeoUtl"; const PATTERNS = {
    linux: ["linux"], windows: ["win", "msys2", "ucrt"], apple: ["mac",
    "apple", "darwin"],
  }; const info = document.getElementById("download-info-row"); const
  CACHE_KEY = "neoutl_release_cache"; const CACHE_TTL = 3600000; const
  OBSERVER_MARGIN = "100px";

  const getArch = () => {
    const selected = document.querySelector(
      '.arch-tab[aria-selected="true"][data-arch]',
    )?.dataset?.arch; if (selected === "x86_64" || selected === "arm64")
    return selected;

    const uaData = navigator.userAgentData; const platform =
    uaData?.platform?.toLowerCase?.() ?? ""; const lang =
    navigator.language?.toLowerCase?.() ?? ""; void lang;

    const ua = navigator.userAgent?.toLowerCase?.() ?? ""; const
    legacyPlatform = (navigator.platform ?? "").toLowerCase?.() ?? "";

    const isArm =
      platform.includes("arm") || ua.includes("aarch64")
      || ua.includes("arm64") || ua.includes("armv") ||
      legacyPlatform.includes("arm");

    const isX86 =
      platform.includes("x86") || platform.includes("amd64") ||
      ua.includes("x86_64") || ua.includes("amd64") || ua.includes("x64")
      || legacyPlatform.includes("x86") || legacyPlatform.includes("amd64")
      || legacyPlatform.includes("x64");

    if (isArm && !isX86) return "arm64"; if (isX86 && !isArm) return
    "x86_64";

    return "x86_64";
  };

  const setArchTabs = (arch) => {
    for (const tab of document.querySelectorAll(".arch-tab[data-arch]")) {
      const isSelected = tab.dataset.arch === arch;
      tab.setAttribute("aria-selected", isSelected ? "true" : "false");
      tab.tabIndex = isSelected ? 0 : -1;
    }
  };

  const updateUI = (latest, prev) => {
    const selectedArch = getArch();

    if (info) {
      const date = new Date(latest.created_at).toLocaleDateString("ja-JP");
      const comp = prev
        ? `compare/${prev.tag_name}...${latest.tag_name}` :
        `releases/tag/${latest.tag_name}`;
      info.innerHTML = `${latest.name || latest.tag_name} (${date})
      / <a href="https://codeberg.org/${REPO}/${comp}"
      target="_blank">更新内容</a> / <a
      href="https://codeberg.org/${REPO}/releases"
      target="_blank">過去のバージョン</a>`;
    }

    const archFilters =
      selectedArch === "arm64" ? ["aarch64", "arm64"] : ["x86_64",
      "amd64", "x86-64"];

    for (const el of document.querySelectorAll(
      ".download-item[data-platform]",
    )) {
      const p = el.dataset.platform; if (p === "source") {
        el.href =
        `https://codeberg.org/${REPO}/archive/${latest.tag_name}.zip`;
        continue;
      }

      const asset = latest.assets.find((a) => {
        const name = (a.name ?? "").toLowerCase();
        const osMatch = PATTERNS[p]?.some((s) =>
        name.includes(s)); if (!osMatch) return false; //
        const hasArchHint =
          archFilters.some((f) => name.includes(f)) || (selectedArch ===
          "arm64" &&
            (name.includes("aarch") || name.includes("arm")));
        if (!hasArchHint) return true; return archFilters.some((f) =>
        name.includes(f));
      });

      el.href = asset
        ?
        `https://codeberg.org/${REPO}/releases/download/${latest.tag_name}/${asset.name}`
        : "https://codeberg.org/taisho-guy/NeoUtl/releases";
      el.classList.toggle("is-disabled", !asset);
      el.querySelector(".download-icon")?.classList.toggle(
        "is-missing", !asset,
      );
    } document
      .querySelectorAll(".download-icon") .forEach((i) =>
      i.classList.remove("is-loading"));
  };

  // 初期選択（タブが未選択なら推定を反映） const
  archTabs = document.getElementById("arch-tabs"); if (archTabs) {
    const initial = getArch(); setArchTabs(initial);
    archTabs.addEventListener("click", (e) => {
      const btn = e.target.closest?.(".arch-tab[data-arch]");
      if (!btn) return; setArchTabs(btn.dataset.arch); //
      const cached = localStorage.getItem(CACHE_KEY); if (cached) {
        const { latest, prev } = JSON.parse(cached); if (latest)
        updateUI(latest, prev);
      }
    });
  }

  const cached = localStorage.getItem(CACHE_KEY); if (cached) {
    const { latest, prev, timestamp } = JSON.parse(cached); if (Date.now()
    - timestamp < CACHE_TTL) updateUI(latest, prev);
  }

  const observer = new IntersectionObserver(
    async ([entry]) => {
      if (entry.isIntersecting) {
        observer.disconnect(); try {
          const res = await fetch(
            `https://codeberg.org/api/v1/repos/${REPO}/releases?limit=2`,
          ); const [latest, prev] = await res.json(); if (latest) {
            localStorage.setItem(
              CACHE_KEY, JSON.stringify({ latest, prev, timestamp:
              Date.now() }),
            ); updateUI(latest, prev);
          }
        } catch {
          if (info) info.textContent = "APIエラー";
        } finally {
          document
            .querySelectorAll(".download-icon") .forEach((i) =>
            i.classList.remove("is-loading"));
        }
      }
    }, { rootMargin: OBSERVER_MARGIN },
  );

  const target = document.getElementById("download-grid"); if (target)
  observer.observe(target);
};

if ("requestIdleCallback" in window) requestIdleCallback(initReleases);
else {
  setTimeout(initReleases, 1);
}

document.addEventListener("click", ({ target }) => {
  if (target.tagName === "CODE") {
    navigator.clipboard.writeText(target.textContent).then(() => {
      target.classList.add("is-copied"); setTimeout(() =>
      target.classList.remove("is-copied"), 1500);
    });
  }
});
