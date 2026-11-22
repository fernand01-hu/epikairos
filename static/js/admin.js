/**
 * EpiKairos admin page: accounts that logged in.
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
                tableBody.innerHTML = `<tr><td colspan="3" class="text-muted small">${q ? t("Aucun utilisateur ne correspond.") : t("Aucun utilisateur enregistré.")}</td></tr>`;
                return;
            }
            tableBody.innerHTML = data.users.map(u => `
                <tr>
                    <td class="fw-semibold">${escapeHtml(u.username)}${u.admin ? ' <span class="badge bg-primary ms-1">Admin</span>' : ""}</td>
                    <td class="small">${formatDate(u.first_seen)}</td>
                    <td class="small">${formatDate(u.last_seen)}</td>
                </tr>
            `).join("");
        } catch (err) {
            alertBox.textContent = err.message;
            alertBox.classList.remove("d-none");
        }
    }

    searchInput.addEventListener("input", () => {
        clearTimeout(searchTimer);
        searchTimer = setTimeout(loadUsers, 250);
    });

    loadUsers();
});
