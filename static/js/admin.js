/**
 * EpiKairos admin page: accounts that logged in, and what each login brought
 * to the shared database.
 */

function escapeHtml(str) {
    if (str === null || str === undefined) return "";
    return String(str)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

function formatDate(seconds) {
    if (!seconds) return '<span class="text-muted">—</span>';
    return new Date(seconds * 1000).toLocaleString(LOCALE, {
        day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit",
    });
}

document.addEventListener("DOMContentLoaded", () => {
    const alertBox = document.getElementById("adminAlert");
    const tableBody = document.getElementById("userTableBody");
    const searchInput = document.getElementById("userSearchInput");
    const userCount = document.getElementById("userCount");
    let searchTimer = null;

    async function loadUsers() {
        try {
            const q = searchInput.value.trim();
            const res = await fetch(`/api/admin/users?q=${encodeURIComponent(q)}`);
            const data = await res.json().catch(() => ({}));
            if (!res.ok || data.success === false) throw new Error((data.error && t(data.error)) || t("Erreur serveur ({status})", { status: res.status }));
            alertBox.classList.add("d-none");
            userCount.textContent = `(${data.users.length})`;
            if (data.users.length === 0) {
                tableBody.innerHTML = `<tr><td colspan="4" class="text-muted small">${q ? t("Aucun utilisateur ne correspond.") : t("Aucun utilisateur enregistré.")}</td></tr>`;
                return;
            }
            tableBody.innerHTML = data.users.map(u => `
                <tr>
                    <td class="fw-semibold">${escapeHtml(u.username)}${u.admin ? ' <span class="badge bg-primary ms-1">Admin</span>' : ""}</td>
                    <td class="small">${formatDate(u.first_seen)}</td>
                    <td class="small">${formatDate(u.last_seen)}</td>
                    <td class="text-end"><button type="button" class="btn btn-sm btn-outline-secondary" data-logins="${escapeHtml(u.username)}">${t("Connexions")}</button></td>
                </tr>
            `).join("");
        } catch (err) {
            alertBox.textContent = err.message;
            alertBox.classList.remove("d-none");
        }
    }

    // ---------------------------------------------------------------------
    // One account's logins
    // ---------------------------------------------------------------------
    const loginsCard = document.getElementById("loginsCard");
    const loginsBody = document.getElementById("loginsBody");
    const lastCourses = document.getElementById("lastCourses");
    const lastCount = document.getElementById("lastCount");
    const lastFilter = document.getElementById("lastFilter");
    let lastData = [];
    let filter = "all";

    const day = (iso) => iso ? new Date(iso + "T12:00:00").toLocaleDateString(LOCALE, { day: "numeric", month: "short" }) : "—";
    const time = (ts) => new Date(ts).toLocaleTimeString(LOCALE, { hour: "2-digit", minute: "2-digit" });
    const STATUS = { added: () => t("nouveau"), updated: () => t("modifié"), unchanged: () => t("inchangé") };

    function renderLast() {
        const shown = lastData.filter(c => filter === "all" || c.status === filter);
        lastCount.textContent = lastData.length ? `(${shown.length} / ${lastData.length})` : "";
        if (!lastData.length) {
            lastCourses.innerHTML = `<div class="small text-muted">${t("Aucune liste : pas encore de connexion avec contribution.")}</div>`;
            return;
        }
        if (!shown.length) {
            lastCourses.innerHTML = `<div class="small text-muted">${t("Aucun cours dans cette catégorie.")}</div>`;
            return;
        }
        let html = "", current = "";
        [...shown].sort((a, b) => a.start.localeCompare(b.start)).forEach(c => {
            if (c.date !== current) {
                current = c.date;
                html += `<div class="admin-courses-day">${escapeHtml(new Date(c.date + "T12:00:00").toLocaleDateString(LOCALE, { weekday: "long", day: "numeric", month: "long" }))}</div>`;
            }
            html += `<div class="admin-course">
                <span class="time">${escapeHtml(time(c.start))}–${escapeHtml(time(c.end))}</span>
                <span class="name">${escapeHtml(c.name)}<small>${escapeHtml((c.groups || []).join(", "))}</small></span>
                <span class="tag is-${escapeHtml(c.status)}">${escapeHtml((STATUS[c.status] || STATUS.unchanged)())}</span>
            </div>`;
        });
        lastCourses.innerHTML = html;
    }

    async function showLogins(user) {
        document.getElementById("loginsUser").textContent = user;
        loginsBody.innerHTML = `<tr><td colspan="7" class="text-muted small">${t("Chargement...")}</td></tr>`;
        lastCourses.innerHTML = "";
        loginsCard.classList.remove("d-none");
        loginsCard.scrollIntoView({ behavior: "smooth", block: "start" });
        try {
            const res = await fetch(`/api/admin/logins?user=${encodeURIComponent(user)}`);
            const data = await res.json().catch(() => ({}));
            if (!res.ok || data.success === false) throw new Error((data.error && t(data.error)) || t("Erreur serveur ({status})", { status: res.status }));
            const events = data.events || [];
            loginsBody.innerHTML = events.length === 0
                ? `<tr><td colspan="7" class="text-muted small">${t("Aucune connexion enregistrée depuis cette mise à jour.")}</td></tr>`
                : events.map(e => {
                    if (!e.consent) {
                        return `<tr><td class="small">${formatDate(e.at)}</td><td colspan="6" class="small text-muted">${t("Contribution désactivée : rien n'a été ajouté.")}</td></tr>`;
                    }
                    const skipped = e.removal_skipped === "partial" ? ` <span class="text-warning" title="${t("Trop de cours manquants d'un coup : réponse d'Auriga probablement incomplète, rien n'a été supprimé.")}">⚠</span>` : "";
                    return `<tr>
                        <td class="small text-nowrap">${formatDate(e.at)}</td>
                        <td class="small text-nowrap">${escapeHtml(day(e.range_start))} → ${escapeHtml(day(e.range_end))}</td>
                        <td class="text-end">${e.received}</td>
                        <td class="text-end ${e.added ? "fw-semibold" : "text-muted"}">${e.added}</td>
                        <td class="text-end ${e.updated ? "fw-semibold" : "text-muted"}">${e.updated}</td>
                        <td class="text-end ${e.removed ? "fw-semibold text-danger" : "text-muted"}">${e.removed}${skipped}</td>
                        <td class="small">${escapeHtml((e.groups || []).join(", ")) || "—"}</td>
                    </tr>`;
                }).join("");
            lastData = (data.last && data.last.courses) || [];
            renderLast();
        } catch (err) {
            loginsBody.innerHTML = `<tr><td colspan="7" class="text-danger small">${escapeHtml(err.message)}</td></tr>`;
        }
    }

    tableBody.addEventListener("click", (e) => {
        const btn = e.target.closest("[data-logins]");
        if (btn) showLogins(btn.dataset.logins);
    });
    document.getElementById("loginsClose").addEventListener("click", () => loginsCard.classList.add("d-none"));
    lastFilter.addEventListener("click", (e) => {
        const btn = e.target.closest("[data-filter]");
        if (!btn) return;
        filter = btn.dataset.filter;
        lastFilter.querySelectorAll("button").forEach(b => b.classList.toggle("active", b === btn));
        renderLast();
    });

    searchInput.addEventListener("input", () => {
        clearTimeout(searchTimer);
        searchTimer = setTimeout(loadUsers, 250);
    });

    loadUsers();
});
