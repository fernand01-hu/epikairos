/**
 * EpiKairos admin panel: moderator roles and club calendars overview.
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

function formatLastSeen(seconds) {
    if (!seconds) return '<span class="text-muted">Jamais</span>';
    return new Date(seconds * 1000).toLocaleString("fr-FR", {
        day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit",
    });
}

const ROLE_LABELS = { admin: "Admin", moderator: "Modérateur", user: "Utilisateur" };

document.addEventListener("DOMContentLoaded", () => {
    const alertBox = document.getElementById("adminAlert");
    const moderatorList = document.getElementById("moderatorList");
    const moderatorCount = document.getElementById("moderatorCount");
    const userTableBody = document.getElementById("userTableBody");
    const userSearchInput = document.getElementById("userSearchInput");
    const addModeratorForm = document.getElementById("addModeratorForm");
    const addModeratorInput = document.getElementById("addModeratorInput");
    const calendarTableBody = document.getElementById("calendarTableBody");

    let searchTimer = null;

    function showError(message) {
        alertBox.textContent = message;
        alertBox.classList.remove("d-none");
    }

    async function api(url, options = {}) {
        const res = await fetch(url, {
            headers: { "Content-Type": "application/json" },
            ...options,
        });
        const data = await res.json().catch(() => ({}));
        if (!res.ok || data.success === false) {
            throw new Error(data.error || `Erreur serveur (${res.status})`);
        }
        return data;
    }

    // -------------------------------------------------------------------------
    // Users & moderators
    // -------------------------------------------------------------------------
    async function loadModerators() {
        const data = await api("/api/admin/users");
        const moderators = data.users.filter(u => u.role === "moderator");
        moderatorCount.textContent = moderators.length ? `${moderators.length}` : "";

        if (moderators.length === 0) {
            moderatorList.innerHTML = '<div class="small text-muted">Aucun modérateur pour le moment.</div>';
            return;
        }
        moderatorList.innerHTML = moderators.map(u => `
            <div class="admin-list-row">
                <div class="min-w-0">
                    <div class="fw-semibold text-truncate">${escapeHtml(u.username)}</div>
                    <div class="small text-muted">${u.calendars} calendrier${u.calendars > 1 ? "s" : ""} · ${u.last_seen ? "vu " + formatLastSeen(u.last_seen) : "jamais connecté"}</div>
                </div>
                <button class="btn btn-sm btn-outline-danger" data-username="${escapeHtml(u.username)}" data-role="user">Retirer</button>
            </div>
        `).join("");
    }

    async function loadUsers() {
        const q = userSearchInput.value.trim();
        const data = await api(`/api/admin/users?q=${encodeURIComponent(q)}`);
        if (data.users.length === 0) {
            userTableBody.innerHTML = `<tr><td colspan="3" class="text-muted small">${q ? "Aucun utilisateur ne correspond." : "Aucun utilisateur enregistré."}</td></tr>`;
            return;
        }
        userTableBody.innerHTML = data.users.map(u => {
            let action;
            if (u.role === "admin") {
                action = '<span class="badge bg-primary">Admin</span>';
            } else if (u.role === "moderator") {
                action = `<button class="btn btn-sm btn-outline-secondary" data-username="${escapeHtml(u.username)}" data-role="user">Modérateur · Retirer</button>`;
            } else {
                action = `<button class="btn btn-sm btn-outline-secondary" data-username="${escapeHtml(u.username)}" data-role="moderator">Nommer modérateur</button>`;
            }
            return `
                <tr>
                    <td class="fw-semibold">${escapeHtml(u.username)}</td>
                    <td class="small">${formatLastSeen(u.last_seen)}</td>
                    <td class="text-end">${action}</td>
                </tr>
            `;
        }).join("");
    }

    async function setRole(username, role) {
        alertBox.classList.add("d-none");
        if (role === "user" && !confirm(`Retirer le rôle de modérateur à ${username} ? Ses calendriers resteront visibles mais il ne pourra plus les modifier.`)) {
            return;
        }
        try {
            await api("/api/admin/users/role", {
                method: "POST",
                body: JSON.stringify({ username, role }),
            });
            await Promise.all([loadModerators(), loadUsers()]);
        } catch (err) {
            showError(err.message);
        }
    }

    // One delegated handler for every role button on the page
    document.addEventListener("click", (e) => {
        const btn = e.target.closest("button[data-username][data-role]");
        if (btn) setRole(btn.dataset.username, btn.dataset.role);
    });

    addModeratorForm.addEventListener("submit", async (e) => {
        e.preventDefault();
        const username = addModeratorInput.value.trim();
        if (!username) return;
        await setRole(username, "moderator");
        if (alertBox.classList.contains("d-none")) addModeratorInput.value = "";
    });

    userSearchInput.addEventListener("input", () => {
        clearTimeout(searchTimer);
        searchTimer = setTimeout(() => loadUsers().catch(err => showError(err.message)), 250);
    });

    // -------------------------------------------------------------------------
    // Club calendars
    // -------------------------------------------------------------------------
    async function loadCalendars() {
        const data = await api("/api/community/calendars");
        if (data.calendars.length === 0) {
            calendarTableBody.innerHTML = '<tr><td colspan="4" class="text-muted small">Aucun calendrier créé pour le moment.</td></tr>';
            return;
        }
        calendarTableBody.innerHTML = data.calendars.map(c => `
            <tr>
                <td>
                    <span class="calendar-dot" style="--dot: ${escapeHtml(c.color)}"></span>
                    <span class="fw-semibold">${escapeHtml(c.name)}</span>
                    ${c.description ? `<div class="small text-muted">${escapeHtml(c.description)}</div>` : ""}
                </td>
                <td>${escapeHtml(c.owner)}</td>
                <td>${c.event_count}</td>
                <td class="text-end">
                    <button class="btn btn-sm btn-outline-danger" data-delete-calendar="${c.id}" data-name="${escapeHtml(c.name)}">Supprimer</button>
                </td>
            </tr>
        `).join("");
    }

    calendarTableBody.addEventListener("click", async (e) => {
        const btn = e.target.closest("button[data-delete-calendar]");
        if (!btn) return;
        if (!confirm(`Supprimer le calendrier « ${btn.dataset.name} » et tous ses événements ?`)) return;
        try {
            await api(`/api/community/calendars/${btn.dataset.deleteCalendar}`, { method: "DELETE" });
            await Promise.all([loadCalendars(), loadModerators()]);
        } catch (err) {
            showError(err.message);
        }
    });

    Promise.all([loadModerators(), loadUsers(), loadCalendars()]).catch(err => showError(err.message));
});
