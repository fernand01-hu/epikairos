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
const CONTACT_DISCORD = "fernandhu_92257";
const STATUS_CACHE_SECONDS = 60;
const DAYS = 30;                   // bars in the uptime strip   // UptimeRobot free API: 10 requests a minute

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

    // Language: ?lang= (the FR/EN button), else the site's own "lang" cookie, else the browser
    const asked = url.searchParams.get("lang");
    const cookieLang = (request.headers.get("Cookie") || "").match(/(?:^|;\s*)lang=(fr|en)\b/);
    const browserEn = /^en\b/i.test(request.headers.get("Accept-Language") || "");
    const lang = asked === "en" || asked === "fr" ? asked : cookieLang ? cookieLang[1] : browserEn ? "en" : "fr";
    const out = new Headers({ ...headers, "Content-Type": "text/html; charset=utf-8" });
    if (asked === "en" || asked === "fr") {
        // Same cookie as the site's button, so the choice carries over when the site is back
        out.append("Set-Cookie", `lang=${asked}; Path=/; Max-Age=31536000; SameSite=Lax; Secure`);
    }

    const status = env.UPTIMEROBOT_KEY ? await monitorStatus(env.UPTIMEROBOT_KEY, ctx) : null;
    return new Response(page(status, lang, url.pathname), { status: 503, headers: out });
}

/**
 * { downSince: unix seconds | null, uptime30: "99.12" | null,
 *   days: [{ start: unix seconds, ratio: number | null }] (oldest first, last 30 days) } or null.
 * Cached for a minute; any error just hides the block.
 */
async function monitorStatus(key, ctx) {
    const cacheKey = new Request("https://epikairos-fallback.internal/uptimerobot-status");
    const cache = typeof caches !== "undefined" ? caches.default : null;
    try {
        const hit = cache && (await cache.match(cacheKey));
        if (hit) return await hit.json();

        // One range per day (rolling 24 h windows ending now), oldest first
        const now = Math.floor(Date.now() / 1000);
        const starts = Array.from({ length: DAYS }, (_, i) => now - (DAYS - i) * 86400);
        const body = new URLSearchParams({
            api_key: key,
            format: "json",
            logs: "1",
            logs_limit: "10",
            custom_uptime_ratios: "30",
            custom_uptime_ranges: starts.map(s => `${s}_${s + 86400}`).join("-"),
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
        const ratios = String(m.custom_uptime_ranges || "").split("-").map(Number);
        const created = Number(m.create_datetime) || 0;
        const status = {
            downSince: isDown && lastDown ? lastDown.datetime : null,
            uptime30: String(m.custom_uptime_ratio || "") || null,
            // Days that ended before the monitor existed have no data
            days: starts.map((start, i) => ({
                start,
                ratio: start + 86400 <= created || !Number.isFinite(ratios[i]) ? null : ratios[i],
            })),
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

const TEXT = {
    fr: {
        locale: "fr-FR",
        title: "EpiKairos — temporairement hors ligne",
        badge: "Temporairement hors ligne",
        intro: "Le site revient dès que possible. Vos liens d'abonnement continuent de fonctionner : votre agenda garde les cours déjà reçus.",
        downSince: "Hors ligne depuis",
        uptime30: "sur 30 jours",
        uptimeTitle: "Disponibilité",
        noData: "pas de données",
        contact: "Une question ?",
        email: "E-mail",
        footer: "Page mise à jour chaque minute · données UptimeRobot",
        switchTo: "en", switchLabel: "EN", switchTitle: "Passer en anglais",
    },
    en: {
        locale: "en-GB",
        title: "EpiKairos — temporarily down",
        badge: "Temporarily down",
        intro: "The site will be back as soon as possible. Your subscription links keep working: your calendar keeps the courses it already has.",
        downSince: "Down since",
        uptime30: "over 30 days",
        uptimeTitle: "Uptime",
        noData: "no data",
        contact: "Questions?",
        email: "Email",
        footer: "Page refreshed every minute · data from UptimeRobot",
        switchTo: "fr", switchLabel: "FR", switchTitle: "Switch to French",
    },
};

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

function dayLabel(unixSeconds, locale) {
    return new Intl.DateTimeFormat(locale, { timeZone: "Europe/Paris", day: "numeric", month: "short" })
        .format(new Date(unixSeconds * 1000));
}

function barClass(ratio) {
    if (ratio === null) return "none";
    if (ratio >= 99.9) return "up";
    if (ratio >= 99.5) return "warn";
    return "down";
}

function statusBlock(status, t) {
    if (!status) return "";
    let html = "";
    if (status.downSince) {
        html += `<ul class="status"><li><span>${t.downSince}</span><strong>${parisTime(status.downSince, t.locale)}</strong></li></ul>`;
    }
    const days = status.days || [];
    if (days.some(d => d.ratio !== null)) {
        const bars = days.map(d => {
            const label = `${dayLabel(d.start + 86400, t.locale)} : ${d.ratio === null ? t.noData : d.ratio.toFixed(2) + " %"}`;
            return `<i class="bar is-${barClass(d.ratio)}" title="${label}"></i>`;
        }).join("");
        const total = percent(status.uptime30);
        html += `<div class="uptime">
    <div class="uptime-head"><span>${t.uptimeTitle}</span>${total ? `<strong>${total} <span class="muted">${t.uptime30}</span></strong>` : ""}</div>
    <div class="bars" role="img" aria-label="${t.uptimeTitle} ${total || ""} ${t.uptime30}">${bars}</div>
  </div>`;
    }
    return html;
}

function page(status, lang, path) {
    const t = TEXT[lang] || TEXT.fr;
    const here = encodeURI(path || "/");
    return `<!doctype html>
<html lang="${lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="60;url=${here}">
<meta name="robots" content="noindex">
<title>${t.title}</title>
<style>
  :root { color-scheme: light dark; --bg: #fafafa; --fg: #18181b; --muted: #71717a; --line: #e4e4e7; --surface: #fff; --warn: #b45309; }
  @media (prefers-color-scheme: dark) { :root { --bg: #0c0c0e; --fg: #f4f4f5; --muted: #a1a1aa; --line: #27272a; --surface: #131316; --warn: #fbbf24; } }
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: var(--bg); color: var(--fg);
         font: 16px/1.55 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; }
  main { width: 100%; max-width: 32rem; padding: 2rem 1rem; box-sizing: border-box; }
  header { display: flex; justify-content: space-between; align-items: center; margin-bottom: .5rem; }
  h1 { font-size: 1.5rem; margin: 0; }
  .lang { color: var(--fg); border: 1px solid var(--line); padding: .2rem .55rem; font-size: .75rem; font-weight: 700;
          letter-spacing: .04em; text-decoration: none; }
  .lang:hover { border-color: var(--muted); }
  .badge { display: inline-block; color: var(--warn); border: 1px solid currentColor; padding: .15rem .6rem; font-size: .85rem; font-weight: 600; margin-bottom: 1rem; }
  p { margin: 0 0 .75rem; }
  a { color: inherit; text-underline-offset: 2px; }
  .muted { color: var(--muted); }
  .small { font-size: .8rem; }
  .status { list-style: none; padding: 0; margin: 1.25rem 0; border: 1px solid var(--line); background: var(--surface); }
  .status li { display: flex; justify-content: space-between; gap: 1rem; padding: .55rem .8rem; font-size: .9rem; }
  .status li + li { border-top: 1px solid var(--line); }
  .status strong { font-variant-numeric: tabular-nums; text-align: right; }
  .uptime { margin: 1.25rem 0; padding: .75rem .8rem; border: 1px solid var(--line); background: var(--surface); }
  .uptime-head { display: flex; justify-content: space-between; align-items: baseline; gap: 1rem; font-size: .9rem; margin-bottom: .55rem; }
  .uptime-head strong { font-variant-numeric: tabular-nums; }
  .uptime-head .muted { font-weight: 400; font-size: .8rem; }
  .bars { display: flex; gap: 3px; height: 26px; }
  .bar { flex: 1; border-radius: 2px; background: #3f3f46; }
  .bar.is-up { background: #22c55e; }
  .bar.is-warn { background: #f59e0b; }
  .bar.is-down { background: #ef4444; }
  .bar.is-none { background: var(--line); }
  .contact { border-top: 1px solid var(--line); margin-top: 1.25rem; padding-top: 1rem; }
  .contact dl { display: grid; grid-template-columns: auto 1fr; gap: .25rem 1rem; margin: .5rem 0 0; }
  .contact dt { color: var(--muted); }
  .contact dd { margin: 0; font-weight: 600; word-break: break-all; }
</style>
</head>
<body>
<main>
  <header>
    <h1>EpiKairos</h1>
    <a class="lang" href="?lang=${t.switchTo}" title="${t.switchTitle}" aria-label="${t.switchTitle}">${t.switchLabel}</a>
  </header>
  <span class="badge">${t.badge}</span>
  <p>${t.intro}</p>
  ${statusBlock(status, t)}
  <div class="contact">
    <p>${t.contact}</p>
    <dl>
      <dt>Discord</dt><dd>${CONTACT_DISCORD}</dd>
      <dt>${t.email}</dt><dd><a href="mailto:${CONTACT_EMAIL}">${CONTACT_EMAIL}</a></dd>
    </dl>
  </div>
  <p class="muted small" style="margin-top:1.25rem">${t.footer}</p>
</main>
</body>
</html>`;
}
