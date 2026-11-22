// Shared by every page: service worker, theme button, show-password buttons.
// Progressive Web App Service Worker Registration
if ('serviceWorker' in navigator) {
    window.addEventListener('load', () => {
        navigator.serviceWorker.register('/sw.js', { scope: '/' })
            .catch(err => console.debug('[PWA] SW register error:', err));
    });
}

// Theme: Auto (follows the device) -> Light -> Dark -> Auto
(function() {
    const toggleBtn = document.getElementById('themeToggleBtn');
    const icon = document.getElementById('themeIcon');
    const media = window.matchMedia ? window.matchMedia('(prefers-color-scheme: dark)') : null;
    const MODES = {
        auto: { icon: 'bi bi-circle-half', label: t('Thème : automatique') },
        light: { icon: 'bi bi-sun', label: t('Thème : clair') },
        dark: { icon: 'bi bi-moon', label: t('Thème : sombre') },
    };
    const NEXT = { auto: 'light', light: 'dark', dark: 'auto' };

    function readMode() {
        try {
            const saved = localStorage.getItem('theme');
            return saved === 'light' || saved === 'dark' ? saved : 'auto';
        } catch (e) {
            return 'auto';
        }
    }

    function apply(mode) {
        const theme = mode === 'auto' ? (media && media.matches ? 'dark' : 'light') : mode;
        document.documentElement.setAttribute('data-bs-theme', theme);
        if (icon) icon.className = MODES[mode].icon;
        if (toggleBtn) {
            toggleBtn.title = MODES[mode].label;
            toggleBtn.setAttribute('aria-label', MODES[mode].label);
        }
        window.dispatchEvent(new CustomEvent('themeChanged', { detail: { theme: theme, mode: mode } }));
    }

    let mode = readMode();
    apply(mode);

    if (toggleBtn) {
        toggleBtn.addEventListener('click', () => {
            mode = NEXT[mode];
            try {
                if (mode === 'auto') localStorage.removeItem('theme');
                else localStorage.setItem('theme', mode);
            } catch (e) {}
            apply(mode);
        });
    }

    // In auto mode, follow the device live (e.g. iPhone switching to dark at sunset)
    if (media) {
        const onChange = () => { if (mode === 'auto') apply('auto'); };
        if (media.addEventListener) media.addEventListener('change', onChange);
        else if (media.addListener) media.addListener(onChange);
    }
})();

// Show / hide password fields
document.addEventListener('click', (e) => {
    const btn = e.target.closest('.password-toggle');
    if (!btn) return;
    const input = document.getElementById(btn.dataset.target);
    if (!input) return;
    const show = input.type === 'password';
    input.type = show ? 'text' : 'password';
    const label = show ? t('Masquer le mot de passe') : t('Afficher le mot de passe');
    btn.setAttribute('aria-pressed', String(show));
    btn.setAttribute('aria-label', label);
    btn.title = label;
    btn.querySelector('i').className = show ? 'bi bi-eye-slash' : 'bi bi-eye';
});

// Offline page: "Réessayer" button
document.getElementById('offlineRetryBtn')?.addEventListener('click', () => window.location.reload());

// Language button: remembers the choice for a year, then reloads in that language
document.getElementById('langToggleBtn')?.addEventListener('click', (e) => {
    const lang = e.currentTarget.dataset.lang === 'en' ? 'en' : 'fr';
    const secure = location.protocol === 'https:' ? '; Secure' : '';
    document.cookie = `lang=${lang}; Path=/; Max-Age=31536000; SameSite=Lax${secure}`;
    location.reload();
});
