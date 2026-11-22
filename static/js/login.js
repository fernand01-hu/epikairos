// Login page: credentials or temporary token, then fetch the planning.
document.addEventListener("DOMContentLoaded", () => {
    const loginForm = document.getElementById("loginForm");
    const loginBtn = document.getElementById("loginBtn");
    const loginSpinner = document.getElementById("loginSpinner");
    const loginIcon = document.getElementById("loginIcon");
    const loginBtnText = document.getElementById("loginBtnText");
    const alertBox = document.getElementById("alertBox");
    const alertMessage = document.getElementById("alertMessage");
    const successBox = document.getElementById("successBox");
    const successMessage = document.getElementById("successMessage");

    // If user logged out, clear local storage
    if (window.location.search.includes("logged_out=1")) {
        try {
            localStorage.removeItem("zeus_courses");
            localStorage.removeItem("zeus_credentials");
            localStorage.removeItem("zeus_stats");
            localStorage.removeItem("zeus_consent");
            // Pages cached for offline use show the previous account's name
            if (window.caches) {
                caches.keys().then(keys => keys.forEach(k => caches.delete(k))).catch(() => {});
            }
        } catch (e) {}
    }

    // "jeton temporaire" link in the trust notice opens the token tab
    document.getElementById("useTokenLink")?.addEventListener("click", () => {
        document.getElementById("tab-token-btn")?.click();
        document.getElementById("tokenInput")?.focus();
    });

    function showAlert(msg) {
        alertMessage.textContent = msg;
        alertBox.classList.remove("d-none");
        successBox.classList.add("d-none");
    }

    function showSuccess(msg) {
        successMessage.textContent = msg;
        successBox.classList.remove("d-none");
        alertBox.classList.add("d-none");
    }

    loginForm.addEventListener("submit", async (e) => {
        e.preventDefault();
        alertBox.classList.add("d-none");
        successBox.classList.add("d-none");

        let tokenInput = document.getElementById("tokenInput").value.trim();
        const username = document.getElementById("usernameInput").value.trim();
        const password = document.getElementById("passwordInput").value.trim();
        const startDate = document.getElementById("startDateInput").value.trim();
        const endDate = document.getElementById("endDateInput").value.trim();
        const consent = document.getElementById("consentCheckbox").checked;

        const isTokenActive = document.getElementById("tab-token-btn")?.classList.contains("active");

        if (tokenInput) {
            if (tokenInput.toLowerCase().startsWith("bearer ")) {
                tokenInput = tokenInput.substring(7).trim();
            }
            tokenInput = tokenInput.replace(/^["']|["']$/g, '');
        }

        if (isTokenActive || (tokenInput && !username)) {
            if (!tokenInput) {
                showAlert(t("Veuillez coller votre jeton d'accès (access_token)."));
                return;
            }
            if (tokenInput.includes("…") || tokenInput.includes("...")) {
                showAlert(t("Le jeton collé est tronqué (il contient « … »). Dans l'onglet Réseau (F12), faites un clic droit sur le header Authorization puis « Copier la valeur »."));
                return;
            }
            try {
                const parts = tokenInput.split(".");
                if (parts.length === 3) {
                    const payload = JSON.parse(atob(parts[1].replace(/-/g, "+").replace(/_/g, "/")));
                    if (payload.exp && Date.now() / 1000 > payload.exp) {
                        showAlert(t("Ce jeton a expiré : il n'est valable qu'une minute. Rechargez Auriga (F5), copiez le nouveau jeton et collez-le tout de suite."));
                        return;
                    }
                }
            } catch (e) {}
        } else {
            if (!username || !password) {
                showAlert(t("Veuillez renseigner votre identifiant et mot de passe."));
                return;
            }
        }

        loginBtn.disabled = true;
        loginSpinner.classList.remove("d-none");
        loginIcon.classList.add("d-none");
        loginBtnText.textContent = t("Authentification et récupération en cours...");

        const payload = {
            start_date: startDate,
            end_date: endDate,
            consent: consent
        };

        if (isTokenActive || (tokenInput && !password)) {
            payload.access_token = tokenInput;
        } else {
            payload.username = username;
            payload.password = password;
        }

        try {
            const res = await fetch("/api/fetch-planning", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload)
            });

            const data = await res.json();
            if (!res.ok || !data.success) {
                throw new Error((data.error && t(data.error)) || t("Erreur serveur ({status})", { status: res.status }));
            }

            // Save retrieved courses and credentials in localStorage for the personal page
            localStorage.setItem("zeus_courses", JSON.stringify(data.courses || []));
            localStorage.setItem("zeus_stats", JSON.stringify(data.stats || {}));
            // Only the login name is kept: the password never stays in the browser.
            localStorage.setItem("zeus_credentials", JSON.stringify({ username: data.username || username }));

            showSuccess(t("Connexion réussie ! {n} cours récupérés. Redirection vers votre planning...", { n: data.courses.length }));

            setTimeout(() => {
                window.location.href = "/personal";
            }, 1000);

        } catch (err) {
            showAlert(t("Échec de la récupération : {error}", { error: err.message }));
            loginBtn.disabled = false;
            loginSpinner.classList.add("d-none");
            loginIcon.classList.remove("d-none");
            loginBtnText.textContent = t("Récupérer mon emploi du temps");
        }
    });
});
