/**
 * Auriga Calendar Web Application Client
 * Complete Calendar & Regex Filter UI
 */

let allCourses = [];
let visibleCourses = [];
let selectedCourseIds = new Set();
let userCredentials = { username: "", password: "", access_token: "" };
let currentCalendarDate = new Date();
let currentViewMode = "week"; // "week" | "table"

function escapeHtml(str) {
    if (str === null || str === undefined) return "";
    return String(str)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

document.addEventListener("DOMContentLoaded", () => {
    // DOM Elements - Login & Main UI
    const loginForm = document.getElementById("loginForm");
    const loginBtn = document.getElementById("loginBtn");
    const loginSpinner = document.getElementById("loginSpinner");
    const loginIcon = document.getElementById("loginIcon");
    const alertBox = document.getElementById("alertBox");
    const alertMessage = document.getElementById("alertMessage");
    const planningSection = document.getElementById("planningSection");

    // Filter Elements
    const includeRegexInput = document.getElementById("includeRegexInput");
    const excludeRegexInput = document.getElementById("excludeRegexInput");
    const hideCancelledSwitch = document.getElementById("hideCancelledSwitch");
    const typeButtonGroup = document.getElementById("typeButtonGroup");
    const selectAllTypesBtn = document.getElementById("selectAllTypesBtn");
    const deselectAllTypesBtn = document.getElementById("deselectAllTypesBtn");

    // Action Bar Elements
    const masterCheckbox = document.getElementById("masterCheckbox");
    const selectAllBtn = document.getElementById("selectAllBtn");
    const deselectAllBtn = document.getElementById("deselectAllBtn");
    const selectedCountBadge = document.getElementById("selectedCountBadge");
    const downloadIcsBtn = document.getElementById("downloadIcsBtn");
    const openFeedModalBtn = document.getElementById("openFeedModalBtn");

    // Calendar & View Switcher Elements
    const viewModeWeekBtn = document.getElementById("viewModeWeekBtn");
    const viewModeListBtn = document.getElementById("viewModeListBtn");
    const calendarWeekView = document.getElementById("calendarWeekView");
    const calendarTableView = document.getElementById("calendarTableView");
    const weekDaysGrid = document.getElementById("weekDaysGrid");
    const noWeekCoursesMessage = document.getElementById("noWeekCoursesMessage");
    const weekNavControls = document.getElementById("weekNavControls");
    const prevWeekBtn = document.getElementById("prevWeekBtn");
    const nextWeekBtn = document.getElementById("nextWeekBtn");
    const todayWeekBtn = document.getElementById("todayWeekBtn");
    const currentWeekLabel = document.getElementById("currentWeekLabel");

    // =========================================================================
    // Helpers: Course Type Information & Styling
    // =========================================================================
    function getTypeInfo(typeStr) {
        const t = (typeStr || "").trim().toUpperCase();
        if (t === "CME" || t === "CM") return { key: "cme", label: t };
        if (t === "TDE" || t === "TD") return { key: "tde", label: t };
        if (t === "TPE" || t === "TP") return { key: "tpe", label: t };
        if (t.startsWith("EX") || t.includes("EXAM") || t.includes("EVAL")) return { key: "ex", label: t };
        if (t.includes("CONF") || t.includes("SEMIN")) return { key: "conf", label: t };
        if (t.includes("EVN") || t.includes("EVEN") || t.includes("REUN")) return { key: "evn", label: t };
        if (t.includes("VC") || t.includes("VISIO")) return { key: "vc", label: t };
        if (t.includes("PRJ") || t.includes("PROJET") || t.includes("SOUT")) return { key: "prj", label: t };
        return { key: "autre", label: t || "AUTRE" };
    }

    // =========================================================================
    // Date & Time Formatting Utilities
    // =========================================================================
    function getMonday(d) {
        const date = new Date(d);
        const day = date.getDay(); // 0 is Sun, 1 is Mon...
        const diff = date.getDate() - day + (day === 0 ? -6 : 1);
        const monday = new Date(date.setDate(diff));
        monday.setHours(0, 0, 0, 0);
        return monday;
    }

    function isSameDay(d1, d2) {
        return (
            d1.getFullYear() === d2.getFullYear() &&
            d1.getMonth() === d2.getMonth() &&
            d1.getDate() === d2.getDate()
        );
    }

    function formatTime(isoStr) {
        if (!isoStr) return "";
        const d = new Date(isoStr);
        return d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
    }

    function formatWeekLabel(monday) {
        const sunday = new Date(monday);
        sunday.setDate(monday.getDate() + 6);

        const months = [
            "janv.", "févr.", "mars", "avr.", "mai", "juin",
            "juil.", "août", "sept.", "oct.", "nov.", "déc."
        ];
        const mDay = monday.getDate();
        const sDay = sunday.getDate();
        const mMonth = months[monday.getMonth()];
        const sMonth = months[sunday.getMonth()];
        const year = sunday.getFullYear();

        // Calculate ISO Week Number
        const target = new Date(monday.valueOf());
        const dayNr = (monday.getDay() + 6) % 7;
        target.setDate(target.getDate() - dayNr + 3);
        const firstThursday = target.valueOf();
        target.setMonth(0, 1);
        if (target.getDay() !== 4) {
            target.setMonth(0, 1 + ((4 - target.getDay()) + 7) % 7);
        }
        const weekNum = 1 + Math.ceil((firstThursday - target.valueOf()) / 604800000);

        let dateRangeStr = "";
        if (mMonth === sMonth) {
            dateRangeStr = `${mDay} - ${sDay} ${mMonth} ${year}`;
        } else {
            dateRangeStr = `${mDay} ${mMonth} - ${sDay} ${sMonth} ${year}`;
        }

        return `Semaine ${weekNum} • ${dateRangeStr}`;
    }

    // =========================================================================
    // Notifications Helper
    // =========================================================================
    function showAlert(msg) {
        alertMessage.textContent = msg;
        alertBox.classList.remove("d-none");
        alertBox.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }

    function hideAlert() {
        alertBox.classList.add("d-none");
    }

    // =========================================================================
    // Auto-load Saved Courses from localStorage (for /personal page)
    // =========================================================================
    const savedCoursesJson = localStorage.getItem("zeus_courses");
    const planningWorkspace = document.getElementById("planningWorkspace");
    const noDataAlert = document.getElementById("noDataAlert");
    const contributeBtn = document.getElementById("contributeBtn");

    if (savedCoursesJson) {
        try {
            allCourses = JSON.parse(savedCoursesJson) || [];
            selectedCourseIds = new Set(allCourses.map(c => c.id));
            const savedCreds = localStorage.getItem("zeus_credentials");
            if (savedCreds) userCredentials = JSON.parse(savedCreds);

            if (planningWorkspace) {
                planningWorkspace.classList.remove("d-none");
            }
            if (noDataAlert) {
                noDataAlert.classList.add("d-none");
            }

            const now = new Date();
            if (allCourses.length > 0) {
                const firstCourseDate = new Date(allCourses[0].start);
                const lastCourseDate = new Date(allCourses[allCourses.length - 1].start);
                if (now >= firstCourseDate && now <= lastCourseDate) {
                    currentCalendarDate = new Date();
                } else {
                    currentCalendarDate = new Date(firstCourseDate);
                }
            }

            const stats = JSON.parse(localStorage.getItem("zeus_stats") || "{}");
            const typesStr = Object.entries(stats.types || {})
                .map(([type, count]) => `${type}: ${count}`)
                .join(", ");
            const statsEl = document.getElementById("statsSummary");
            if (statsEl) {
                statsEl.innerHTML = `<strong>${allCourses.length}</strong> cours enregistrés (${typesStr || "Tous"})`;
            }

            // Check if contribution button should be visible
            const consentVal = localStorage.getItem("zeus_consent");
            if (consentVal === "0" && contributeBtn) {
                contributeBtn.classList.remove("d-none");
                contributeBtn.addEventListener("click", async () => {
                    try {
                        contributeBtn.disabled = true;
                        contributeBtn.innerHTML = '<span class="spinner-border spinner-border-sm me-1"></span> Envoi...';
                        const res = await fetch("/api/ingest-consent", {
                            method: "POST",
                            headers: { "Content-Type": "application/json" },
                            body: JSON.stringify({ courses: allCourses })
                        });
                        if (res.ok) {
                            localStorage.setItem("zeus_consent", "1");
                            contributeBtn.classList.add("d-none");
                            alert("Merci ! Vos cours ont été ajoutés anonymement à la base collaborative.");
                        }
                    } catch (e) {
                        alert(`Erreur : ${e.message}`);
                    } finally {
                        contributeBtn.disabled = false;
                    }
                });
            }

            renderTypeFilterButtons(allCourses);
            applyFilters();
        } catch (e) {
            console.error("Error loading saved courses", e);
        }
    } else {
        if (noDataAlert) {
            noDataAlert.classList.remove("d-none");
        }
    }

    // =========================================================================
    // 1. Auriga Login & Fetch Planning (if login form exists on page)
    // =========================================================================
    if (loginForm) {
        loginForm.addEventListener("submit", async (e) => {
            e.preventDefault();
            hideAlert();

            const tokenInput = document.getElementById("tokenInput").value.trim();
            const username = document.getElementById("usernameInput").value.trim();
            const password = document.getElementById("passwordInput").value.trim();
            const startDate = document.getElementById("startDateInput").value.trim();
            const endDate = document.getElementById("endDateInput").value.trim();

            const isTokenActive = document.getElementById("tab-token-btn")?.classList.contains("active");

            if (isTokenActive || (tokenInput && !username)) {
                if (!tokenInput) {
                    showAlert("Veuillez coller votre jeton d'accès (access_token).");
                    return;
                }
            } else {
                if (!username || !password) {
                    showAlert("Veuillez renseigner votre identifiant et mot de passe.");
                    return;
                }
            }

            userCredentials = { username, password, access_token: tokenInput };

            if (loginBtn) loginBtn.disabled = true;
            if (loginSpinner) loginSpinner.classList.remove("d-none");
            if (loginIcon) loginIcon.classList.add("d-none");

            const payload = {
                start_date: startDate,
                end_date: endDate,
                consent: true
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
                    throw new Error(data.error || `Erreur serveur (${res.status})`);
                }

                allCourses = data.courses || [];
                selectedCourseIds = new Set(allCourses.map(c => c.id));

                localStorage.setItem("zeus_courses", JSON.stringify(allCourses));
                localStorage.setItem("zeus_stats", JSON.stringify(data.stats || {}));
                localStorage.setItem("zeus_credentials", JSON.stringify(userCredentials));
                localStorage.setItem("zeus_consent", "1");

                const now = new Date();
                if (allCourses.length > 0) {
                    const firstCourseDate = new Date(allCourses[0].start);
                    const lastCourseDate = new Date(allCourses[allCourses.length - 1].start);
                    if (now >= firstCourseDate && now <= lastCourseDate) {
                        currentCalendarDate = new Date();
                    } else {
                        currentCalendarDate = new Date(firstCourseDate);
                    }
                } else {
                    currentCalendarDate = new Date();
                }

                const stats = data.stats || {};
                const typesStr = Object.entries(stats.types || {})
                    .map(([type, count]) => `${type}: ${count}`)
                    .join(", ");
                const statsEl = document.getElementById("statsSummary");
                if (statsEl) {
                    statsEl.innerHTML = `<strong>${stats.total}</strong> cours trouvés (${typesStr}) | <strong>${stats.cancelled}</strong> annulés`;
                }

                renderTypeFilterButtons(allCourses);

                if (planningSection) {
                    planningSection.classList.remove("d-none");
                    planningSection.scrollIntoView({ behavior: "smooth", block: "start" });
                }
                if (planningWorkspace) {
                    planningWorkspace.classList.remove("d-none");
                }
                if (noDataAlert) {
                    noDataAlert.classList.add("d-none");
                }

                applyFilters();

            } catch (err) {
                showAlert(`Impossible de récupérer votre emploi du temps : ${err.message}`);
            } finally {
                loginBtn.disabled = false;
                loginSpinner.classList.add("d-none");
                loginIcon.classList.remove("d-none");
            }
        });
    }

    // =========================================================================
    // 2. Dynamic Course Name Filter Chips (Replacing generic types)
    // =========================================================================
    function renderTypeFilterButtons(courses) {
        if (!typeButtonGroup) return;

        // Group courses by distinct course name (detected repeating subjects)
        const courseCounts = new Map();
        courses.forEach(c => {
            const name = (c.course_name || "Cours").trim();
            if (name) {
                courseCounts.set(name, (courseCounts.get(name) || 0) + 1);
            }
        });

        if (courseCounts.size === 0) {
            typeButtonGroup.innerHTML = '<span class="text-muted small">Aucune matière identifiée</span>';
            return;
        }

        // Sort courses by session count descending, then alphabetically
        const sortedCourses = Array.from(courseCounts.entries())
            .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0], "fr"))
            .map(e => e[0]);

        typeButtonGroup.innerHTML = "";

        sortedCourses.forEach(courseName => {
            const count = courseCounts.get(courseName);

            const btn = document.createElement("button");
            btn.type = "button";
            btn.className = "type-filter-chip course-filter-chip";
            btn.dataset.course = courseName;
            btn.title = `Matière : ${courseName} (${count} séance${count > 1 ? "s" : ""})`;
            btn.innerHTML = `
                <span class="chip-label text-truncate">${escapeHtml(courseName)}</span>
                <span class="chip-count">${count}</span>
            `;

            btn.addEventListener("click", () => {
                btn.classList.toggle("unchecked");
                applyFilters();
            });

            typeButtonGroup.appendChild(btn);
        });
    }

    if (selectAllTypesBtn) {
        selectAllTypesBtn.addEventListener("click", () => {
            document.querySelectorAll(".type-filter-chip").forEach(btn => btn.classList.remove("unchecked"));
            applyFilters();
        });
    }

    if (deselectAllTypesBtn) {
        deselectAllTypesBtn.addEventListener("click", () => {
            document.querySelectorAll(".type-filter-chip").forEach(btn => btn.classList.add("unchecked"));
            applyFilters();
        });
    }

    // =========================================================================
    // 3. Realtime Regex & Filter Application
    // =========================================================================
    function applyFilters() {
        const includeVal = includeRegexInput.value.trim();
        const excludeVal = excludeRegexInput.value.trim();
        const hideCancelled = hideCancelledSwitch.checked;

        // Collect checked course names (chips without .unchecked)
        const activeChipElements = Array.from(document.querySelectorAll(".type-filter-chip:not(.unchecked)"));
        const activeCourses = activeChipElements.map(cb => cb.dataset.course).filter(Boolean);
        const hasCourseFilters = document.querySelectorAll(".type-filter-chip").length > 0;

        let includePattern = null;
        let excludePattern = null;

        includeRegexInput.classList.remove("is-invalid");
        excludeRegexInput.classList.remove("is-invalid");

        if (includeVal) {
            try {
                includePattern = new RegExp(includeVal, "i");
            } catch (e) {
                includeRegexInput.classList.add("is-invalid");
            }
        }

        if (excludeVal) {
            try {
                excludePattern = new RegExp(excludeVal, "i");
            } catch (e) {
                excludeRegexInput.classList.add("is-invalid");
            }
        }

        visibleCourses = allCourses.filter(c => {
            if (hideCancelled && c.is_cancelled) {
                return false;
            }

            if (hasCourseFilters) {
                const cName = (c.course_name || "Cours").trim();
                if (!activeCourses.includes(cName)) {
                    return false;
                }
            }

            const text = c.searchable_text || "";

            if (includePattern && !includePattern.test(text)) {
                return false;
            }

            if (excludePattern && excludePattern.test(text)) {
                return false;
            }

            return true;
        });

        // Re-render both views
        renderWeekCalendar();
        renderCourseTable();
        updateSelectedCounter();
    }

    // Presets buttons
    document.querySelectorAll(".preset-btn").forEach(btn => {
        btn.addEventListener("click", () => {
            const target = btn.dataset.target;
            const val = btn.dataset.val;
            if (target === "include") {
                includeRegexInput.value = val;
            } else if (target === "exclude") {
                excludeRegexInput.value = val;
            }
            applyFilters();
        });
    });

    includeRegexInput.addEventListener("input", applyFilters);
    excludeRegexInput.addEventListener("input", applyFilters);
    hideCancelledSwitch.addEventListener("change", applyFilters);

    // =========================================================================
    // 4. Real Calendar Weekly View & Navigation (Left & Right Arrows)
    // =========================================================================
    function renderWeekCalendar() {
        if (!weekDaysGrid) return;
        weekDaysGrid.innerHTML = "";

        const monday = getMonday(currentCalendarDate);
        if (currentWeekLabel) {
            currentWeekLabel.textContent = formatWeekLabel(monday);
        }

        // Check if there are Sunday courses in this week to decide if we show 6 or 7 days
        const weekSunday = new Date(monday);
        weekSunday.setDate(monday.getDate() + 6);

        const hasSundayCourses = visibleCourses.some(c => {
            const d = new Date(c.start);
            return isSameDay(d, weekSunday);
        });

        const totalDaysToShow = hasSundayCourses ? 7 : 6;
        weekDaysGrid.classList.toggle("has-sunday", hasSundayCourses);
        const frenchDayNames = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche"];
        const shortMonths = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
        const todayDate = new Date();

        let totalCoursesInThisWeek = 0;

        for (let i = 0; i < totalDaysToShow; i++) {
            const currentDayDate = new Date(monday);
            currentDayDate.setDate(monday.getDate() + i);

            // Filter and sort courses on this day
            const dayCourses = visibleCourses.filter(c => isSameDay(new Date(c.start), currentDayDate));
            dayCourses.sort((a, b) => new Date(a.start) - new Date(b.start));
            totalCoursesInThisWeek += dayCourses.length;

            const isToday = isSameDay(currentDayDate, todayDate);

            // Day Column container
            const col = document.createElement("div");
            col.className = `day-column ${isToday ? "is-today" : ""}`;

            // Column Header
            const dayName = frenchDayNames[i];
            const dateNum = currentDayDate.getDate();
            const monthStr = shortMonths[currentDayDate.getMonth()];

            col.innerHTML = `
                <div class="day-column-header">
                    <div class="fw-semibold small">${dayName} ${dateNum} ${monthStr}</div>
                    <div class="small text-muted">${dayCourses.length} cours</div>
                </div>
                <div class="day-events-list"></div>
            `;

            const eventsList = col.querySelector(".day-events-list");

            if (dayCourses.length === 0) {
                eventsList.innerHTML = '<div class="text-center text-muted small py-4">Aucun cours</div>';
            } else {
                dayCourses.forEach(c => {
                    const info = getTypeInfo(c.activity_type);
                    const isSelected = selectedCourseIds.has(c.id);

                    const card = document.createElement("div");
                    card.className = `calendar-card border-${info.key} ${!isSelected ? "deselected" : ""} ${c.is_cancelled ? "card-cancelled" : ""}`;
                    card.dataset.id = c.id;

                    const roomsStr = c.rooms && c.rooms.length > 0 ? c.rooms.map(escapeHtml).join(", ") : "";
                    const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.map(escapeHtml).join(", ") : "";
                    const groupsStr = c.groups && c.groups.length > 0 ? c.groups.map(escapeHtml).join(", ") : "";

                    card.innerHTML = `
                        <div class="d-flex align-items-center justify-content-between mb-1">
                            <span class="event-time">
                                ${formatTime(c.start)} - ${formatTime(c.end)}
                            </span>
                            <div class="d-flex align-items-center gap-1">
                                <span class="badge badge-${info.key}">
                                    ${escapeHtml(c.activity_type)}
                                </span>
                                <input type="checkbox" class="form-check-input event-checkbox ms-1 m-0" data-id="${c.id}" ${isSelected ? "checked" : ""} title="Sélectionner pour export">
                            </div>
                        </div>
                        <div class="event-title" title="${escapeHtml(c.course_name)}">
                            ${c.is_cancelled ? '<span class="badge bg-danger-subtle text-danger border border-danger-subtle me-1">ANNULÉ</span>' : ''}
                            ${escapeHtml(c.course_name)}
                        </div>
                        <div class="event-meta">
                            ${roomsStr ? `<div class="text-truncate" title="Salle: ${roomsStr}">${roomsStr}</div>` : ""}
                            ${teachersStr ? `<div class="text-truncate" title="Enseignant: ${teachersStr}">${teachersStr}</div>` : ""}
                            ${groupsStr ? `<div class="text-truncate" title="Groupe: ${groupsStr}">${groupsStr}</div>` : ""}
                        </div>
                    `;

                    // Checkbox toggle logic
                    const cb = card.querySelector(".event-checkbox");
                    cb.addEventListener("click", (e) => {
                        e.stopPropagation();
                        toggleCourseSelection(c.id, cb.checked);
                    });

                    // Card click toggles checkbox
                    card.addEventListener("click", (e) => {
                        if (e.target !== cb) {
                            const newChecked = !selectedCourseIds.has(c.id);
                            cb.checked = newChecked;
                            toggleCourseSelection(c.id, newChecked);
                        }
                    });

                    eventsList.appendChild(card);
                });
            }

            weekDaysGrid.appendChild(col);
        }

        // If entire week has no courses, show empty week alert
        if (totalCoursesInThisWeek === 0 && visibleCourses.length > 0) {
            noWeekCoursesMessage.classList.remove("d-none");
            noWeekCoursesMessage.innerHTML = `
                <i class="bi bi-calendar-x fs-1 d-block mb-2 text-secondary"></i>
                <div class="fw-semibold fs-5">Aucun cours cette semaine</div>
                <div class="small text-muted mt-1">Utilisez les flèches <strong>&larr;</strong> et <strong>&rarr;</strong> ci-dessus pour naviguer dans votre calendrier.</div>
                <button type="button" class="btn btn-sm btn-outline-primary mt-3 rounded-pill fw-semibold" id="jumpToClosestCourseBtn">
                    <i class="bi bi-arrow-right-circle me-1"></i> Aller à la semaine avec des cours
                </button>
            `;
            const jumpBtn = document.getElementById("jumpToClosestCourseBtn");
            if (jumpBtn) {
                jumpBtn.addEventListener("click", () => {
                    if (visibleCourses.length > 0) {
                        currentCalendarDate = new Date(visibleCourses[0].start);
                        renderWeekCalendar();
                    }
                });
            }
        } else {
            noWeekCoursesMessage.classList.add("d-none");
        }
    }

    function toggleCourseSelection(id, isSelected) {
        if (isSelected) {
            selectedCourseIds.add(id);
        } else {
            selectedCourseIds.delete(id);
        }

        // Update card styles in calendar view
        document.querySelectorAll(`.calendar-card[data-id="${id}"]`).forEach(card => {
            if (isSelected) {
                card.classList.remove("deselected");
            } else {
                card.classList.add("deselected");
            }
            const cb = card.querySelector(".event-checkbox");
            if (cb) cb.checked = isSelected;
        });

        // Update row checkbox in table view
        document.querySelectorAll(`.row-select-cb[data-id="${id}"]`).forEach(cb => {
            cb.checked = isSelected;
        });

        updateSelectedCounter();
    }

    // Week Navigation Event Listeners
    if (prevWeekBtn) {
        prevWeekBtn.addEventListener("click", () => {
            currentCalendarDate.setDate(currentCalendarDate.getDate() - 7);
            renderWeekCalendar();
        });
    }

    if (nextWeekBtn) {
        nextWeekBtn.addEventListener("click", () => {
            currentCalendarDate.setDate(currentCalendarDate.getDate() + 7);
            renderWeekCalendar();
        });
    }

    if (todayWeekBtn) {
        todayWeekBtn.addEventListener("click", () => {
            currentCalendarDate = new Date();
            renderWeekCalendar();
        });
    }

    // Keyboard navigation (Left / Right arrow keys)
    document.addEventListener("keydown", (e) => {
        if (["INPUT", "TEXTAREA"].includes(document.activeElement?.tagName)) return;
        if (planningSection.classList.contains("d-none")) return;
        if (currentViewMode !== "week") return;

        if (e.key === "ArrowLeft") {
            currentCalendarDate.setDate(currentCalendarDate.getDate() - 7);
            renderWeekCalendar();
        } else if (e.key === "ArrowRight") {
            currentCalendarDate.setDate(currentCalendarDate.getDate() + 7);
            renderWeekCalendar();
        }
    });

    // View Mode Toggle (Semaine / Tableau)
    if (viewModeWeekBtn && viewModeListBtn) {
        viewModeWeekBtn.addEventListener("click", () => {
            currentViewMode = "week";
            viewModeWeekBtn.classList.add("btn-primary");
            viewModeWeekBtn.classList.remove("btn-outline-secondary");
            viewModeListBtn.classList.remove("btn-primary");
            viewModeListBtn.classList.add("btn-outline-secondary");
            calendarWeekView.classList.remove("d-none");
            calendarTableView.classList.add("d-none");
            weekNavControls.classList.remove("d-none");
        });

        viewModeListBtn.addEventListener("click", () => {
            currentViewMode = "table";
            viewModeListBtn.classList.add("btn-primary");
            viewModeListBtn.classList.remove("btn-outline-secondary");
            viewModeWeekBtn.classList.remove("btn-primary");
            viewModeWeekBtn.classList.add("btn-outline-secondary");
            calendarTableView.classList.remove("d-none");
            calendarWeekView.classList.add("d-none");
            weekNavControls.classList.add("d-none");
        });
    }

    // =========================================================================
    // 5. Render Course Table (Alternative View)
    // =========================================================================
    function formatFrenchDateTime(isoStart, isoEnd) {
        if (!isoStart) return "";
        try {
            const dStart = new Date(isoStart);
            const dateStr = dStart.toLocaleDateString("fr-FR", {
                weekday: "short",
                day: "numeric",
                month: "short"
            });
            const startH = dStart.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });

            let endH = "";
            if (isoEnd) {
                const dEnd = new Date(isoEnd);
                endH = " - " + dEnd.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
            }
            return `<span class="fw-semibold text-capitalize">${dateStr}</span><br><span class="text-muted small">${startH}${endH}</span>`;
        } catch {
            return isoStart;
        }
    }

    function renderCourseTable() {
        const tbody = document.getElementById("coursesTableBody");
        const noMsg = document.getElementById("noCoursesMessage");
        if (!tbody) return;

        tbody.innerHTML = "";

        if (visibleCourses.length === 0) {
            if (noMsg) noMsg.classList.remove("d-none");
        } else {
            if (noMsg) noMsg.classList.add("d-none");
        }

        const fragment = document.createDocumentFragment();

        visibleCourses.forEach(c => {
            const tr = document.createElement("tr");
            const isChecked = selectedCourseIds.has(c.id);
            if (c.is_cancelled) {
                tr.classList.add("course-row-cancelled");
            }

            const info = getTypeInfo(c.activity_type);

            const roomsFormatted = c.rooms && c.rooms.length > 0 ? c.rooms.map(escapeHtml).join("<br>") : '<span class="text-muted small">-</span>';
            const teachersFormatted = c.teachers && c.teachers.length > 0 ? c.teachers.map(escapeHtml).join("<br>") : '<span class="text-muted small">-</span>';
            const groupsFormatted = c.groups && c.groups.length > 0 ? c.groups.map(escapeHtml).join("<br>") : '<span class="text-muted small">-</span>';

            tr.innerHTML = `
                <td class="text-center">
                    <input class="form-check-input row-select-cb" type="checkbox" data-id="${c.id}" ${isChecked ? "checked" : ""}>
                </td>
                <td>${formatFrenchDateTime(c.start, c.end)}</td>
                <td>
                    <span class="badge badge-${info.key} px-2 py-1">
                        ${c.is_cancelled ? "[ANNULÉ] " : ""}${escapeHtml(c.activity_type)}
                    </span>
                </td>
                <td>
                    <div class="fw-semibold">${escapeHtml(c.course_name)}</div>
                    <div class="small text-muted font-monospace">${escapeHtml(c.course_code || "")}</div>
                </td>
                <td>${roomsFormatted}</td>
                <td>${teachersFormatted}</td>
                <td>${groupsFormatted}</td>
            `;

            fragment.appendChild(tr);
        });

        tbody.appendChild(fragment);

        tbody.querySelectorAll(".row-select-cb").forEach(cb => {
            cb.addEventListener("change", (e) => {
                const id = e.target.dataset.id;
                toggleCourseSelection(id, e.target.checked);
            });
        });
    }

    function updateSelectedCounter() {
        const visibleSelectedCount = visibleCourses.filter(c => selectedCourseIds.has(c.id)).length;
        if (selectedCountBadge) {
            selectedCountBadge.textContent = `${visibleSelectedCount} / ${visibleCourses.length} cours sélectionnés`;
        }
        if (masterCheckbox) {
            masterCheckbox.checked = visibleCourses.length > 0 && visibleSelectedCount === visibleCourses.length;
        }
    }

    // =========================================================================
    // 6. Batch Selection Controls
    // =========================================================================
    if (masterCheckbox) {
        masterCheckbox.addEventListener("change", (e) => {
            const shouldCheck = e.target.checked;
            visibleCourses.forEach(c => {
                if (shouldCheck) {
                    selectedCourseIds.add(c.id);
                } else {
                    selectedCourseIds.delete(c.id);
                }
            });
            renderWeekCalendar();
            document.querySelectorAll(".row-select-cb").forEach(cb => cb.checked = shouldCheck);
            updateSelectedCounter();
        });
    }

    if (selectAllBtn) {
        selectAllBtn.addEventListener("click", () => {
            visibleCourses.forEach(c => selectedCourseIds.add(c.id));
            renderWeekCalendar();
            document.querySelectorAll(".row-select-cb").forEach(cb => cb.checked = true);
            updateSelectedCounter();
        });
    }

    if (deselectAllBtn) {
        deselectAllBtn.addEventListener("click", () => {
            visibleCourses.forEach(c => selectedCourseIds.delete(c.id));
            renderWeekCalendar();
            document.querySelectorAll(".row-select-cb").forEach(cb => cb.checked = false);
            updateSelectedCounter();
        });
    }

    // =========================================================================
    // 7. Direct .ics File Download
    // =========================================================================
    if (downloadIcsBtn) {
        downloadIcsBtn.addEventListener("click", async () => {
            const coursesToExport = visibleCourses.filter(c => selectedCourseIds.has(c.id));
            if (coursesToExport.length === 0) {
                alert("Veuillez sélectionner au moins un cours à exporter.");
                return;
            }

            try {
                downloadIcsBtn.disabled = true;
                downloadIcsBtn.innerHTML = '<span class="spinner-border spinner-border-sm me-1"></span> Génération...';

                const res = await fetch("/api/export-ics", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        courses: coursesToExport,
                        calendar_name: "Auriga - EPITA Emploi du temps"
                    })
                });

                if (!res.ok) {
                    throw new Error(`Erreur lors du téléchargement (${res.status})`);
                }

                const blob = await res.blob();
                const url = window.URL.createObjectURL(blob);
                const a = document.createElement("a");
                a.style.display = "none";
                a.href = url;
                a.download = "auriga_planning.ics";
                document.body.appendChild(a);
                a.click();
                window.URL.revokeObjectURL(url);
                document.body.removeChild(a);

            } catch (err) {
                alert(`Erreur d'exportation : ${err.message}`);
            } finally {
                downloadIcsBtn.disabled = false;
                downloadIcsBtn.innerHTML = '<i class="bi bi-download me-1"></i> Télécharger .ics';
            }
        });
    }

    // =========================================================================
    // 8. Live Google Calendar Feed URL Modal
    // =========================================================================
    const feedModalEl = document.getElementById("feedModal");
    const feedModal = feedModalEl ? new bootstrap.Modal(feedModalEl) : null;
    const feedModalSpinner = document.getElementById("feedModalSpinner");
    const feedModalContent = document.getElementById("feedModalContent");
    const feedUrlInput = document.getElementById("feedUrlInput");
    const feedHttpsUrlInput = document.getElementById("feedHttpsUrlInput");
    const copyFeedUrlBtn = document.getElementById("copyFeedUrlBtn");
    const copyHttpsFeedUrlBtn = document.getElementById("copyHttpsFeedUrlBtn");
    const testFeedUrlBtn = document.getElementById("testFeedUrlBtn");
    const copyFeedback = document.getElementById("copyFeedback");
    const feedFiltersBadges = document.getElementById("feedFiltersBadges");
    const feedCredentialsStatus = document.getElementById("feedCredentialsStatus");
    const feedAccountBadge = document.getElementById("feedAccountBadge");
    const changeFeedCredsBtn = document.getElementById("changeFeedCredsBtn");
    const feedCredentialsPrompt = document.getElementById("feedCredentialsPrompt");
    const modalUsernameInput = document.getElementById("modalUsernameInput");
    const modalPasswordInput = document.getElementById("modalPasswordInput");
    const regenerateFeedWithCredsBtn = document.getElementById("regenerateFeedWithCredsBtn");

    function escapeHtml(str) {
        if (!str) return "";
        return String(str).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
    }

    async function generateCalendarFeed(credsOverride = null) {
        if (!feedModalSpinner || !feedModalContent) return;

        feedModalSpinner.classList.remove("d-none");
        feedModalContent.classList.add("d-none");
        if (copyFeedback) copyFeedback.classList.add("d-none");
        feedModalSpinner.innerHTML = `
            <div class="spinner-border text-primary" role="status">
                <span class="visually-hidden">Génération du lien...</span>
            </div>
            <div class="small text-muted mt-2">Génération du lien sécurisé avec vos filtres...</div>
        `;

        // Refresh credentials from localStorage if available
        const savedCreds = localStorage.getItem("zeus_credentials");
        if (savedCreds) {
            try { userCredentials = JSON.parse(savedCreds); } catch (e) {}
        }

        const username = credsOverride?.username !== undefined
            ? credsOverride.username
            : (userCredentials?.username || "");
        const password = credsOverride?.password !== undefined
            ? credsOverride.password
            : (userCredentials?.password || "");

        // Manage credentials UI status
        if (username && password) {
            if (feedCredentialsStatus) feedCredentialsStatus.classList.remove("d-none");
            if (feedAccountBadge) feedAccountBadge.textContent = username;
            if (feedCredentialsPrompt) feedCredentialsPrompt.classList.add("d-none");
        } else {
            if (feedCredentialsStatus) feedCredentialsStatus.classList.add("d-none");
            if (feedCredentialsPrompt) feedCredentialsPrompt.classList.remove("d-none");
            if (modalUsernameInput && !modalUsernameInput.value && username) {
                modalUsernameInput.value = username;
            }
        }

        // Collect filters from DOM
        const includeRegex = includeRegexInput ? includeRegexInput.value.trim() : "";
        const excludeRegex = excludeRegexInput ? excludeRegexInput.value.trim() : "";
        const hideCancelled = hideCancelledSwitch ? hideCancelledSwitch.checked : false;

        const allChips = Array.from(document.querySelectorAll(".type-filter-chip"));
        const activeChips = Array.from(document.querySelectorAll(".type-filter-chip:not(.unchecked)"));
        const checkedCourses = activeChips.map(cb => cb.dataset.course).filter(Boolean);
        const isCourseFiltered = allChips.length > 0 && activeChips.length < allChips.length;
        const allowedCoursesToSend = isCourseFiltered ? checkedCourses : null;

        const coursesToExport = visibleCourses.filter(c => selectedCourseIds.has(c.id));
        const payloadCourses = coursesToExport.length > 0 ? coursesToExport : (visibleCourses.length > 0 ? visibleCourses : allCourses);

        // Display active filter verification badges
        if (feedFiltersBadges) {
            const row = (label, value) => `
                <div class="col-sm-6 d-flex justify-content-between gap-2 border-bottom py-1">
                    <span class="text-muted">${label}</span>
                    <span class="text-end text-truncate">${value}</span>
                </div>`;
            feedFiltersBadges.innerHTML = [
                row("Inclure", includeRegex ? `<code>${escapeHtml(includeRegex)}</code>` : "Tous les cours"),
                row("Exclure", excludeRegex ? `<code>${escapeHtml(excludeRegex)}</code>` : "Aucun"),
                row("Matières", isCourseFiltered ? `${activeChips.length} / ${allChips.length}` : `Toutes (${allChips.length})`),
                row("Cours annulés", hideCancelled ? "Masqués" : "Inclus"),
                row("Séances", `${payloadCourses.length}`),
                row("Mise à jour", "Toutes les 12 h"),
            ].join("");
        }

        try {
            const res = await fetch("/api/create-feed", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    username: username,
                    password: password,
                    include_regex: includeRegex,
                    exclude_regex: excludeRegex,
                    allowed_courses: allowedCoursesToSend,
                    hide_cancelled: hideCancelled,
                    courses: payloadCourses
                })
            });

            const data = await res.json();
            if (!res.ok || !data.success) {
                throw new Error(data.error || "Impossible de générer le lien de flux.");
            }

            if (feedUrlInput) feedUrlInput.value = data.webcal_url;
            if (feedHttpsUrlInput) feedHttpsUrlInput.value = data.feed_url;
            if (testFeedUrlBtn) testFeedUrlBtn.href = data.feed_url;

            feedModalSpinner.classList.add("d-none");
            feedModalContent.classList.remove("d-none");

        } catch (err) {
            feedModalSpinner.innerHTML = `
                <div class="text-danger small mb-3">
                    <i class="bi bi-exclamation-circle me-1"></i> Erreur : ${escapeHtml(err.message)}
                </div>
                <button type="button" class="btn btn-sm btn-outline-primary rounded-pill px-3" id="retryFeedBtn">
                    <i class="bi bi-arrow-clockwise me-1"></i> Réessayer
                </button>
            `;
            document.getElementById("retryFeedBtn")?.addEventListener("click", () => generateCalendarFeed());
        }
    }

    if (openFeedModalBtn && feedModal) {
        openFeedModalBtn.addEventListener("click", () => {
            feedModal.show();
            generateCalendarFeed();
        });
    }

    if (changeFeedCredsBtn) {
        changeFeedCredsBtn.addEventListener("click", () => {
            if (feedCredentialsPrompt) feedCredentialsPrompt.classList.remove("d-none");
            if (feedCredentialsStatus) feedCredentialsStatus.classList.add("d-none");
        });
    }

    if (regenerateFeedWithCredsBtn) {
        regenerateFeedWithCredsBtn.addEventListener("click", () => {
            const userVal = modalUsernameInput ? modalUsernameInput.value.trim() : "";
            const passVal = modalPasswordInput ? modalPasswordInput.value.trim() : "";
            if (!userVal || !passVal) {
                alert("Veuillez renseigner votre identifiant et votre mot de passe Auriga.");
                return;
            }
            userCredentials.username = userVal;
            userCredentials.password = passVal;
            localStorage.setItem("zeus_credentials", JSON.stringify(userCredentials));
            generateCalendarFeed({ username: userVal, password: passVal });
        });
    }

    async function copyToClipboard(inputElement) {
        if (!inputElement || !inputElement.value) return;
        try {
            await navigator.clipboard.writeText(inputElement.value);
            if (copyFeedback) {
                copyFeedback.classList.remove("d-none");
                setTimeout(() => copyFeedback.classList.add("d-none"), 4000);
            }
        } catch {
            inputElement.select();
            document.execCommand("copy");
            if (copyFeedback) {
                copyFeedback.classList.remove("d-none");
                setTimeout(() => copyFeedback.classList.add("d-none"), 4000);
            }
        }
    }

    if (copyFeedUrlBtn) {
        copyFeedUrlBtn.addEventListener("click", () => copyToClipboard(feedUrlInput));
    }

    if (copyHttpsFeedUrlBtn) {
        copyHttpsFeedUrlBtn.addEventListener("click", () => copyToClipboard(feedHttpsUrlInput));
    }

});
