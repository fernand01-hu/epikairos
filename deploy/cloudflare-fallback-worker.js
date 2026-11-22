/**
 * EpiKairos — Cloudflare Worker shown when the home server is down.
 *
 * Every request goes to the real site as usual. If the server does not answer
 * (tunnel down, power cut, timeout, 5xx), the Worker answers instead:
 *   - pages: a small "temporarily unavailable" page (or a redirect to
 *     FALLBACK_URL if that variable is set in the Worker settings);
 *   - /feed/*.ics and /api/*: a plain 503, so Google Calendar keeps the
 *     courses it already has instead of reading an HTML page.
 *
 * Setup: Cloudflare dashboard > Workers & Pages > Create > Worker, paste this
 * file, Deploy, then Settings > Domains & Routes > Add route "epikairos.org/*".
 */

const TIMEOUT_MS = 15000;
const DOWN_STATUSES = new Set([502, 503, 504, 520, 521, 522, 523, 524, 525, 526, 530]);

export default {
    async fetch(request, env) {
        let response;
        try {
            response = await fetch(request, { signal: AbortSignal.timeout(TIMEOUT_MS) });
        } catch (err) {
            return unavailable(request, env);
        }
        if (DOWN_STATUSES.has(response.status)) return unavailable(request, env);
        return response;
    },
};

function unavailable(request, env) {
    const url = new URL(request.url);
    const headers = { "Retry-After": "300", "Cache-Control": "no-store" };

    if (url.pathname.startsWith("/feed/") || url.pathname.startsWith("/api/")) {
        return new Response("Service temporarily unavailable\n", {
            status: 503,
            headers: { ...headers, "Content-Type": "text/plain; charset=utf-8" },
        });
    }
    if (env.FALLBACK_URL) {
        return Response.redirect(env.FALLBACK_URL, 302);
    }
    return new Response(PAGE, {
        status: 503,
        headers: { ...headers, "Content-Type": "text/html; charset=utf-8" },
    });
}

const PAGE = `<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="60">
<meta name="robots" content="noindex">
<title>EpiKairos — momentanément indisponible</title>
<style>
  :root { color-scheme: light dark; --bg: #fafafa; --fg: #18181b; --muted: #71717a; --line: #e4e4e7; }
  @media (prefers-color-scheme: dark) { :root { --bg: #0c0c0e; --fg: #f4f4f5; --muted: #a1a1aa; --line: #27272a; } }
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: var(--bg); color: var(--fg);
         font: 16px/1.5 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; }
  main { max-width: 30rem; padding: 2rem 1rem; }
  h1 { font-size: 1.5rem; margin: 0 0 .75rem; }
  p { margin: 0 0 .75rem; }
  .muted { color: var(--muted); font-size: .875rem; }
  hr { border: 0; border-top: 1px solid var(--line); margin: 1.5rem 0; }
</style>
</head>
<body>
<main>
  <h1>EpiKairos</h1>
  <p>Le site est momentanément indisponible. Il revient bientôt.</p>
  <p class="muted">Vos liens d'abonnement ne sont pas perdus : votre agenda garde les cours déjà reçus et se remettra à jour tout seul.</p>
  <hr>
  <p>The site is temporarily unavailable and will be back soon.</p>
  <p class="muted">Your subscription links are safe: your calendar keeps the courses it already has and will update again on its own.</p>
  <p class="muted">Cette page se recharge toute seule chaque minute.</p>
</main>
</body>
</html>`;
