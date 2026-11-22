/**
 * EpiKairos — Cloudflare Worker shown when the home server is down.
 *
 * Every request goes to the real site as usual. If the server does not answer
 * (tunnel down, power cut, timeout, 5xx), the Worker answers instead:
 *   - pages: a "temporarily down" page with contact details and, when the
 *     UPTIMEROBOT_KEY secret is set, live status from UptimeRobot (down since,
 *     uptime over 7 and 30 days). The key stays on Cloudflare, never in the page.
 *     (Or a redirect to FALLBACK_URL if that variable is set.)
 *   - /feed/*.ics and /api/*: a plain 503, so Google Calendar keeps the
 *     courses it already has instead of reading an HTML page.
 *
 * Deploy from this folder: npx wrangler deploy
 * Key (use a monitor-specific, read-only key): npx wrangler secret put UPTIMEROBOT_KEY
 */

const TIMEOUT_MS = 15000;
const DOWN_STATUSES = new Set([502, 503, 504, 520, 521, 522, 523, 524, 525, 526, 530]);
const CONTACT_EMAIL = "fernand.huttois@gmail.com";
const STATUS_CACHE_SECONDS = 60;   // UptimeRobot free API: 10 requests a minute

export default {
    async fetch(request, env, ctx) {
        let response;
        try {
            response = await fetch(request, { signal: AbortSignal.timeout(TIMEOUT_MS) });
        } catch (err) {
            return unavailable(request, env, ctx);
        }
        if (DOWN_STATUSES.has(response.status)) return unavailable(request, env, ctx);
        return response;
    },
};

async function unavailable(request, env, ctx) {
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
    const status = env.UPTIMEROBOT_KEY ? await monitorStatus(env.UPTIMEROBOT_KEY, ctx) : null;
    return new Response(page(status), {
        status: 503,
        headers: { ...headers, "Content-Type": "text/html; charset=utf-8" },
    });
}

/**
 * { downSince: unix seconds | null, uptime7: "99.12" | null, uptime30: ... } or null.
 * Cached for a minute; any error just hides the block.
 */
async function monitorStatus(key, ctx) {
    const cacheKey = new Request("https://epikairos-fallback.internal/uptimerobot-status");
    const cache = typeof caches !== "undefined" ? caches.default : null;
    try {
        const hit = cache && (await cache.match(cacheKey));
        if (hit) return await hit.json();

        const body = new URLSearchParams({
            api_key: key,
            format: "json",
            logs: "1",
            logs_limit: "10",
            custom_uptime_ratios: "7-30",
        });
        const res = await fetch("https://api.uptimerobot.com/v2/getMonitors", {
            method: "POST",
            headers: { "Content-Type": "application/x-www-form-urlencoded" },
            body,
            signal: AbortSignal.timeout(4000),
        });
        const data = await res.json();
        const m = data && data.stat === "ok" && data.monitors && data.monitors[0];
        if (!m) return null;

        // Status 8/9 = seems down / down; the latest "down" log gives the start time
        const isDown = m.status === 8 || m.status === 9;
        const lastDown = (m.logs || []).find(l => l.type === 1);
        const [u7, u30] = String(m.custom_uptime_ratio || "").split("-");
        const status = {
            downSince: isDown && lastDown ? lastDown.datetime : null,
            uptime7: u7 || null,
            uptime30: u30 || null,
        };
        if (cache) {
            const stored = new Response(JSON.stringify(status), {
                headers: { "Content-Type": "application/json", "Cache-Control": `max-age=${STATUS_CACHE_SECONDS}` },
            });
            const put = cache.put(cacheKey, stored);
            if (ctx && ctx.waitUntil) ctx.waitUntil(put); else await put;
        }
        return status;
    } catch (err) {
        return null;
    }
}

function parisTime(unixSeconds, locale) {
    return new Intl.DateTimeFormat(locale, {
        timeZone: "Europe/Paris",
        weekday: "long", day: "numeric", month: "long", hour: "2-digit", minute: "2-digit",
    }).format(new Date(unixSeconds * 1000));
}

function percent(value) {
    const n = Number(value);
    return Number.isFinite(n) ? `${n.toFixed(2)} %` : null;
}

function statusBlock(status) {
    if (!status) return "";
    const rows = [];
    if (status.downSince) {
        rows.push(`<li><span>Hors ligne depuis <em>· Down since</em></span><strong>${parisTime(status.downSince, "fr-FR")}</strong></li>`);
    }
    const u7 = percent(status.uptime7), u30 = percent(status.uptime30);
    if (u7) rows.push(`<li><span>Disponibilité 7 jours <em>· 7-day uptime</em></span><strong>${u7}</strong></li>`);
    if (u30) rows.push(`<li><span>Disponibilité 30 jours <em>· 30-day uptime</em></span><strong>${u30}</strong></li>`);
    if (!rows.length) return "";
    return `<ul class="status">${rows.join("")}</ul><p class="muted small">Source : UptimeRobot, vérifié toutes les 5 minutes.</p>`;
}

function page(status) {
    return `<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="60">
<meta name="robots" content="noindex">
<title>EpiKairos — temporairement hors ligne</title>
<style>
  :root { color-scheme: light dark; --bg: #fafafa; --fg: #18181b; --muted: #71717a; --line: #e4e4e7; --surface: #fff; --warn: #b45309; --link: #18181b; }
  @media (prefers-color-scheme: dark) { :root { --bg: #0c0c0e; --fg: #f4f4f5; --muted: #a1a1aa; --line: #27272a; --surface: #131316; --warn: #fbbf24; --link: #f4f4f5; } }
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: var(--bg); color: var(--fg);
         font: 16px/1.55 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; }
  main { max-width: 32rem; padding: 2rem 1rem; }
  h1 { font-size: 1.5rem; margin: 0 0 .25rem; }
  .badge { display: inline-block; color: var(--warn); border: 1px solid currentColor; padding: .15rem .6rem; font-size: .85rem; font-weight: 600; margin-bottom: 1rem; }
  p { margin: 0 0 .75rem; }
  a { color: var(--link); text-underline-offset: 2px; }
  .muted { color: var(--muted); }
  .small { font-size: .8rem; }
  em { font-style: normal; color: var(--muted); }
  .status { list-style: none; padding: 0; margin: 1.25rem 0 .4rem; border: 1px solid var(--line); background: var(--surface); }
  .status li { display: flex; justify-content: space-between; gap: 1rem; padding: .55rem .8rem; font-size: .9rem; }
  .status li + li { border-top: 1px solid var(--line); }
  .status strong { font-variant-numeric: tabular-nums; text-align: right; }
  .contact { border-top: 1px solid var(--line); margin-top: 1.25rem; padding-top: 1rem; }
</style>
</head>
<body>
<main>
  <h1>EpiKairos</h1>
  <span class="badge">Temporairement hors ligne · Temporarily down</span>
  <p>Le site est temporairement hors ligne. Il revient dès que possible.</p>
  <p class="muted">Vos liens d'abonnement ne sont pas perdus : votre agenda garde les cours déjà reçus et se remettra à jour tout seul.</p>
  <p class="muted">The site is temporarily down and will be back as soon as possible. Your subscription links are safe: your calendar keeps the courses it already has.</p>
  ${statusBlock(status)}
  <div class="contact">
    <p>Un problème ou une question ? Contactez-moi sur Discord, ou par e-mail : <a href="mailto:${CONTACT_EMAIL}">${CONTACT_EMAIL}</a></p>
    <p class="muted">Questions? Reach me on Discord or by email at <a href="mailto:${CONTACT_EMAIL}">${CONTACT_EMAIL}</a>.</p>
  </div>
  <p class="muted small">Cette page se recharge toute seule chaque minute.</p>
</main>
</body>
</html>`;
}
