const initReleases = async () => {
    const REPO = 'taisho-guy/NeoUtl';
    const PATTERNS = { linux: ['linux'], windows: ['win', 'msys2', 'ucrt'], apple: ['mac', 'apple', 'darwin'] };
    const info = document.getElementById('download-info-row');
    const CACHE_KEY = 'neoutl_release_cache';
    const CACHE_TTL = 3600000; // 1時間
    const OBSERVER_MARGIN = '100px';

    const updateUI = (latest, prev) => {
        if (info) {
            const date = new Date(latest.created_at).toLocaleDateString('ja-JP');
            const comp = prev ? `compare/${prev.tag_name}...${latest.tag_name}` : `releases/tag/${latest.tag_name}`;
            info.innerHTML = `${latest.name || latest.tag_name} (${date}) / <a href="https://codeberg.org/${REPO}/${comp}" target="_blank">更新内容</a> / <a href="https://codeberg.org/${REPO}/releases" target="_blank">過去のバージョン</a>`;
        }

        for (const el of document.querySelectorAll('.download-item[data-platform]')) {
            const p = el.dataset.platform;
            if (p === 'source') { el.href = `https://codeberg.org/${REPO}/archive/${latest.tag_name}.zip`; continue; }

            const asset = latest.assets.find(a => PATTERNS[p]?.some(s => a.name.toLowerCase().includes(s)));
            el.href = asset ? `https://codeberg.org/${REPO}/releases/download/${latest.tag_name}/${asset.name}` : '#';
            el.classList.toggle('is-disabled', !asset);
            el.querySelector('.download-icon')?.classList.toggle('is-missing', !asset);
        }
        document.querySelectorAll('.download-icon').forEach(i => i.classList.remove('is-loading'));
    };

    const cached = localStorage.getItem(CACHE_KEY);
    if (cached) {
        const { latest, prev, timestamp } = JSON.parse(cached);
        if (Date.now() - timestamp < CACHE_TTL) updateUI(latest, prev);
    }

    const observer = new IntersectionObserver(async ([entry]) => {
        if (entry.isIntersecting) {
            observer.disconnect();
            try {
                const res = await fetch(`https://codeberg.org/api/v1/repos/${REPO}/releases?limit=2`);
                const [latest, prev] = await res.json();
                if (latest) {
                    localStorage.setItem(CACHE_KEY, JSON.stringify({ latest, prev, timestamp: Date.now() }));
                    updateUI(latest, prev);
                }
            } catch {
                if (info) info.textContent = 'APIエラー';
            } finally {
                document.querySelectorAll('.download-icon').forEach(i => i.classList.remove('is-loading'));
            }
        }
    }, { rootMargin: OBSERVER_MARGIN });

    const target = document.getElementById('download-grid');
    if (target) observer.observe(target);
};

if ('requestIdleCallback' in window) requestIdleCallback(initReleases);
else { setTimeout(initReleases, 1); }

document.addEventListener('click', ({target}) => {
    if (target.tagName === 'CODE') {
        navigator.clipboard.writeText(target.textContent).then(() => {
            const old = target.style.cssText;
            target.style.cssText = 'color:var(--bg);background:var(--accent)';
            setTimeout(() => target.style.cssText = old, 200);
        });
    }
});