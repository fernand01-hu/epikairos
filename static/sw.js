/**
 * EpiKairos Service Worker
 * Version: 1.1.0
 */

const CACHE_NAME = 'epikairos-shell-v9';
const CORE_ASSETS = [
    '/manifest.json',
    '/static/css/style.css',
    '/static/icons/icon-192.png',
    '/static/icons/icon-512.png',
    '/static/icons/favicon.svg',
    '/offline'
];

// Install: Pre-cache app shell and core assets
self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(CACHE_NAME).then((cache) => {
            return cache.addAll(CORE_ASSETS).catch((err) => {
                console.warn('[SW] Pre-caching partial failure, continuing:', err);
            });
        }).then(() => self.skipWaiting())
    );
});

// Activate: Clean up old caches
self.addEventListener('activate', (event) => {
    event.waitUntil(
        caches.keys().then((keys) => {
            return Promise.all(
                keys.map((key) => {
                    if (key !== CACHE_NAME) {
                        return caches.delete(key);
                    }
                })
            );
        }).then(() => self.clients.claim())
    );
});

// Fetch: Strategy depending on request type
self.addEventListener('fetch', (event) => {
    const request = event.request;
    const url = new URL(request.url);

    // Only handle this site's own GET requests. Other origins (the Bootstrap CDN)
    // go straight to the browser: the worker's own CSP (connect-src 'self') would
    // block them, and the page's CSP already allows them.
    if (request.method !== 'GET' || url.origin !== self.location.origin) {
        return;
    }

    // Dynamic APIs and ICS feeds: Network-first (no aggressive caching of user credentials/live feed)
    if (url.pathname.startsWith('/api/') || url.pathname.startsWith('/feed/')) {
        event.respondWith(
            fetch(request).catch(() => {
                return new Response(JSON.stringify({ error: 'Réseau indisponible (Mode hors-ligne)' }), {
                    status: 503,
                    headers: { 'Content-Type': 'application/json; charset=utf-8' }
                });
            })
        );
        return;
    }

    // HTML Navigation requests (pages): Network-first with offline fallback
    if (request.mode === 'navigate' || (request.headers.get('accept') && request.headers.get('accept').includes('text/html'))) {
        event.respondWith(
            fetch(request)
                .then((networkResponse) => {
                    if (networkResponse && networkResponse.status === 200) {
                        const responseClone = networkResponse.clone();
                        caches.open(CACHE_NAME).then((cache) => cache.put(request, responseClone));
                    }
                    return networkResponse;
                })
                .catch(async () => {
                    const cachedResponse = await caches.match(request);
                    if (cachedResponse) {
                        return cachedResponse;
                    }
                    // Return offline fallback page
                    const offlinePage = await caches.match('/offline');
                    return offlinePage || new Response(
                        '<html><head><meta charset="utf-8"><title>Hors ligne — EpiKairos</title></head><body style="font-family:sans-serif;text-align:center;padding:50px;"><h1>Mode hors-ligne</h1><p>Vous n\'êtes actuellement pas connecté à Internet.</p></body></html>',
                        { headers: { 'Content-Type': 'text/html; charset=utf-8' } }
                    );
                })
        );
        return;
    }

    // Static Assets (CSS, JS, Fonts, Images): Cache-first with network fallback
    event.respondWith(
        caches.match(request).then((cachedResponse) => {
            if (cachedResponse) {
                // Fetch in background to update cache if possible (stale-while-revalidate style)
                fetch(request).then((networkResponse) => {
                    if (networkResponse && networkResponse.status === 200) {
                        caches.open(CACHE_NAME).then((cache) => cache.put(request, networkResponse));
                    }
                }).catch(() => {/* Ignore network errors */});
                return cachedResponse;
            }

            return fetch(request).then((networkResponse) => {
                if (networkResponse && networkResponse.status === 200) {
                    const responseClone = networkResponse.clone();
                    caches.open(CACHE_NAME).then((cache) => cache.put(request, responseClone));
                }
                return networkResponse;
            });
        })
    );
});
