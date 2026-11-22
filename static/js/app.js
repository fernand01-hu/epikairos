/**
 * Auriga Calendar Web Application Client
 * Calendar & filter UI
 */

let allCourses = [];
let visibleCourses = [];
let selectedCourseIds = new Set();
let userCredentials = { username: "", password: "", access_token: "" };
let currentCalendarDate = new Date();

function escapeHtml(str) {
    if (str === null || str === undefined) return "";
    return String(str)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

// Height of one hour in the week time grid, in pixels.
const HOUR_PX = 56;

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
                localStorage.setItem("zeus_credentials", JSON.stringify({ username: userCredentials.username || "" }));

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
    // 3. Filter application
    // =========================================================================
    function applyFilters() {
        const hideCancelled = hideCancelledSwitch.checked;

        // Collect checked course names (chips without .unchecked)
        const activeChipElements = Array.from(document.querySelectorAll(".type-filter-chip:not(.unchecked)"));
        const activeCourses = activeChipElements.map(cb => cb.dataset.course).filter(Boolean);
        const hasCourseFilters = document.querySelectorAll(".type-filter-chip").length > 0;

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

            return true;
        });

        // Re-render both views
        renderWeekCalendar();
        updateSelectedCounter();
    }

    hideCancelledSwitch.addEventListener("change", applyFilters);

    // =========================================================================
    // 4. Real Calendar Weekly View & Navigation (Left & Right Arrows)
    // =========================================================================
    function minutesOfDay(d) {
        return d.getHours() * 60 + d.getMinutes();
    }

    /** Puts overlapping courses side by side: sets c._lane and c._lanes. */
    function assignLanes(items) {
        items.sort((x, y) => x._s - y._s || y._e - x._e);
        let cluster = [];
        let clusterEnd = -1;
        const flush = () => {
            const laneEnds = [];
            cluster.forEach(it => {
                let lane = laneEnds.findIndex(end => end <= it._s);
                if (lane === -1) { lane = laneEnds.length; laneEnds.push(0); }
                laneEnds[lane] = it._e;
                it._lane = lane;
            });
            cluster.forEach(it => { it._lanes = laneEnds.length; });
            cluster = [];
        };
        items.forEach(it => {
            if (cluster.length && it._s >= clusterEnd) flush();
            cluster.push(it);
            clusterEnd = Math.max(clusterEnd, it._e);
        });
        if (cluster.length) flush();
    }

    function renderWeekCalendar() {
        if (!weekDaysGrid) return;

        const monday = getMonday(currentCalendarDate);
        if (currentWeekLabel) {
            currentWeekLabel.textContent = formatWeekLabel(monday);
        }

        const days = [];
        for (let i = 0; i < 7; i++) {
            const d = new Date(monday);
            d.setDate(monday.getDate() + i);
            days.push(d);
        }

        const weekCourses = visibleCourses
            .map(c => {
                const start = new Date(c.start);
                const end = new Date(c.end);
                const dayIndex = days.findIndex(d => isSameDay(d, start));
                return { c, start, end, dayIndex, _s: minutesOfDay(start), _e: Math.max(minutesOfDay(end), minutesOfDay(start) + 15) };
            })
            .filter(x => x.dayIndex !== -1);

        // Sunday only when it has courses; hours span 8h–19h, widened to fit the week.
        const showSunday = weekCourses.some(x => x.dayIndex === 6);
        const dayCount = showSunday ? 7 : 6;
        let firstHour = 8;
        let lastHour = 19;
        weekCourses.forEach(x => {
            firstHour = Math.min(firstHour, Math.floor(x._s / 60));
            lastHour = Math.max(lastHour, Math.ceil(x._e / 60));
        });
        firstHour = Math.max(0, firstHour);
        lastHour = Math.min(24, lastHour);
        const bodyHeight = (lastHour - firstHour) * HOUR_PX;
        const toPx = m => ((m - firstHour * 60) / 60) * HOUR_PX;

        const dayNames = ["Lun", "Mar", "Mer", "Jeu", "Ven", "Sam", "Dim"];
        const today = new Date();

        let html = `<div class="tg" style="--days: ${dayCount}; --hour: ${HOUR_PX}px;">`;
        html += '<div class="tg-head"><div class="tg-corner"></div>';
        for (let i = 0; i < dayCount; i++) {
            const d = days[i];
            html += `<div class="tg-dayhead${isSameDay(d, today) ? " is-today" : ""}"><span>${dayNames[i]}</span> <strong>${d.getDate()}</strong></div>`;
        }
        html += `</div><div class="tg-body" style="height: ${bodyHeight}px;"><div class="tg-axis">`;
        for (let h = firstHour + 1; h < lastHour; h++) {
            html += `<span style="top: ${(h - firstHour) * HOUR_PX}px;">${String(h).padStart(2, "0")}:00</span>`;
        }
        html += "</div>";
        for (let i = 0; i < dayCount; i++) {
            html += `<div class="tg-col${isSameDay(days[i], today) ? " is-today" : ""}" data-day="${i}"></div>`;
        }
        html += "</div></div>";
        weekDaysGrid.innerHTML = html;

        const cols = weekDaysGrid.querySelectorAll(".tg-col");
        for (let i = 0; i < dayCount; i++) {
            const items = weekCourses.filter(x => x.dayIndex === i);
            assignLanes(items);
            items.forEach(x => cols[i].appendChild(buildEventCard(x, toPx)));
        }

        // Current time line
        const todayIndex = days.findIndex(d => isSameDay(d, today));
        const nowM = minutesOfDay(today);
        if (todayIndex !== -1 && todayIndex < dayCount && nowM >= firstHour * 60 && nowM <= lastHour * 60) {
            const line = document.createElement("div");
            line.className = "tg-now";
            line.style.top = `${toPx(nowM)}px`;
            cols[todayIndex].appendChild(line);
        }

        if (weekCourses.length === 0 && visibleCourses.length > 0) {
            noWeekCoursesMessage.classList.remove("d-none");
            noWeekCoursesMessage.innerHTML = `
                <div class="empty-title">Aucun cours cette semaine</div>
                <button type="button" class="btn btn-sm btn-outline-secondary mt-2" id="jumpToClosestCourseBtn">Aller à la prochaine semaine avec des cours</button>
            `;
            const jumpBtn = document.getElementById("jumpToClosestCourseBtn");
            if (jumpBtn) {
                jumpBtn.addEventListener("click", () => {
                    const ref = getMonday(currentCalendarDate);
                    const upcoming = visibleCourses
                        .map(c => new Date(c.start))
                        .sort((x, y) => x - y);
                    const next = upcoming.find(d => d >= ref) || upcoming[upcoming.length - 1];
                    if (next) {
                        currentCalendarDate = new Date(next);
                        renderWeekCalendar();
                    }
                });
            }
        } else {
            noWeekCoursesMessage.classList.add("d-none");
        }
    }

    function buildEventCard(x, toPx) {
        const c = x.c;
        const info = getTypeInfo(c.activity_type);
        const isSelected = selectedCourseIds.has(c.id);
        const top = toPx(x._s);
        const height = Math.max(22, toPx(x._e) - top - 2);

        const card = document.createElement("div");
        card.className = `calendar-card tg-event border-${info.key}${!isSelected ? " deselected" : ""}${c.is_cancelled ? " card-cancelled" : ""}${height < 60 ? " is-compact" : ""}`;
        card.dataset.id = c.id;
        card.style.top = `${top}px`;
        card.style.height = `${height}px`;
        card.style.left = `calc(${(x._lane / x._lanes) * 100}% + 2px)`;
        card.style.width = `calc(${100 / x._lanes}% - 4px)`;

        const rooms = c.rooms && c.rooms.length ? c.rooms.join(", ") : "";
        const teachers = c.teachers && c.teachers.length ? c.teachers.join(", ") : "";
        card.title = [`${formatTime(c.start)} - ${formatTime(c.end)}`, c.course_name, rooms, teachers].filter(Boolean).join("\n");

        card.innerHTML = `
            <input type="checkbox" class="form-check-input event-checkbox" data-id="${escapeHtml(c.id)}" ${isSelected ? "checked" : ""} title="Sélectionner pour l'export" aria-label="Sélectionner pour l'export">
            <div class="event-time">${formatTime(c.start)} – ${formatTime(c.end)}</div>
            <div class="event-title">${c.is_cancelled ? "[Annulé] " : ""}${escapeHtml(c.course_name)}</div>
            ${rooms ? `<div class="event-meta">${escapeHtml(rooms)}</div>` : ""}
        `;

        const cb = card.querySelector(".event-checkbox");
        cb.addEventListener("click", (e) => {
            e.stopPropagation();
            toggleCourseSelection(c.id, cb.checked);
        });
        card.addEventListener("click", (e) => {
            if (e.target !== cb) {
                const newChecked = !selectedCourseIds.has(c.id);
                cb.checked = newChecked;
                toggleCourseSelection(c.id, newChecked);
            }
        });
        return card;
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

        if (e.key === "ArrowLeft") {
            currentCalendarDate.setDate(currentCalendarDate.getDate() - 7);
            renderWeekCalendar();
        } else if (e.key === "ArrowRight") {
            currentCalendarDate.setDate(currentCalendarDate.getDate() + 7);
            renderWeekCalendar();
        }
    });

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
                <button type="button" class="btn btn-sm btn-outline-primary px-3" id="retryFeedBtn">
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
            localStorage.setItem("zeus_credentials", JSON.stringify({ username: userVal }));
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
