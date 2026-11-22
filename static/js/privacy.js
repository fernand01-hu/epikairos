// Privacy page: "Supprimer mes données" button.
document.getElementById("deleteAccountBtn")?.addEventListener("click", async (e) => {
    const btn = e.currentTarget;
    const error = document.getElementById("deleteError");
    if (!confirm(t("Supprimer votre identifiant et tous vos liens d'abonnement ? C'est définitif."))) return;
    btn.disabled = true;
    error.classList.add("d-none");
    try {
        const res = await fetch("/api/account/delete", { method: "POST", headers: { "Content-Type": "application/json" }, body: "{}" });
        const data = await res.json().catch(() => ({}));
        if (!res.ok || !data.success) throw new Error((data.error && t(data.error)) || t("Erreur serveur ({status})", { status: res.status }));
        try { localStorage.removeItem("zeus_courses"); } catch (err) { /* ignore */ }
        location.href = "/confidentialite?deleted=1";
    } catch (err) {
        error.textContent = err.message;
        error.classList.remove("d-none");
        btn.disabled = false;
    }
});
