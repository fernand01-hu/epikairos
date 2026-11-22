// Runs in <head> before the page paints: applies the theme and cleans old storage.
(function() {
    try {
        // 'light' or 'dark' is a fixed choice; anything else follows the device.
        const savedTheme = localStorage.getItem('theme');
        const systemDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
        const theme = (savedTheme === 'light' || savedTheme === 'dark') ? savedTheme : (systemDark ? 'dark' : 'light');
        document.documentElement.setAttribute('data-bs-theme', theme);
    } catch (e) {}
    // Older versions stored the Auriga password here: keep only the login name.
    try {
        const creds = JSON.parse(localStorage.getItem('zeus_credentials') || 'null');
        if (creds && (creds.password || creds.access_token)) {
            localStorage.setItem('zeus_credentials', JSON.stringify({ username: creds.username || '' }));
        }
        localStorage.removeItem('zeus_consent');
    } catch (e) {}
})();
