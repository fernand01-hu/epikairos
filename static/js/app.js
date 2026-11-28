/**
 * EpiKairos - Mon planning: week grid, subject filters, .ics export and the
 * subscription links panel. Courses come from the last login (localStorage).
 */

let allCourses = [];
let visibleCourses = [];
let selectedCourseIds = new Set();
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

    // Filter Elements
    const hideCancelledSwitch = document.getElementById("hideCancelledSwitch");
    const typeButtonGroup = document.getElementById("typeButtonGroup");
    const selectAllTypesBtn = document.getElementById("selectAllTypesBtn");
    const deselectAllTypesBtn = document.getElementById("deselectAllTypesBtn");

    // Action Bar Elements
    const selectAllBtn = document.getElementById("selectAllBtn");
    const deselectAllBtn = document.getElementById("deselectAllBtn");
    const selectedCountBadge = document.getElementById("selectedCountBadge");
    const downloadIcsBtn = document.getElementById("downloadIcsBtn");
    const openFeedModalBtn = document.getElementById("openFeedModalBtn");

    // Calendar & View Switcher Elements
    const weekDaysGrid = document.getElementById("weekDaysGrid");
    const noWeekCoursesMessage = document.getElementById("noWeekCoursesMessage");
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
        return d.toLocaleTimeString(LOCALE, { hour: "2-digit", minute: "2-digit" });
    }

    function formatWeekLabel(monday) {
        const sunday = new Date(monday);
        sunday.setDate(monday.getDate() + 6);

        const months = Array.from({ length: 12 }, (_, i) =>
            new Date(2000, i, 1).toLocaleDateString(LOCALE, { month: "short" }));
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

        return t("Semaine {n} • {range}", { n: weekNum, range: dateRangeStr });
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
            typeButtonGroup.innerHTML = `<span class="text-muted small">${t("Aucune matière identifiée")}</span>`;
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
    // Phones: subject filters folded by default, the choice is remembered
    const filtersCard = document.getElementById("filtersCard");
    const toggleFiltersBtn = document.getElementById("toggleFiltersBtn");
    function setFiltersOpen(open) {
        filtersCard.classList.toggle("is-open", open);
        toggleFiltersBtn.setAttribute("aria-expanded", open ? "true" : "false");
        try { localStorage.setItem("zeus_filters_open", open ? "1" : "0"); } catch (e) { /* ignore */ }
    }
    // Called from applyFilters, which already runs while the page loads
    function updateFiltersSummary() {
        const filtersSummary = document.getElementById("filtersSummary");
        if (!filtersSummary) return;
        const all = document.querySelectorAll(".type-filter-chip").length;
        const active = document.querySelectorAll(".type-filter-chip:not(.unchecked)").length;
        filtersSummary.textContent = all === 0 ? "" : active === all ? t("Toutes les matières") : t("{n} / {total} matières", { n: active, total: all });
    }
    if (toggleFiltersBtn) {
        let open = false;
        try { open = localStorage.getItem("zeus_filters_open") === "1"; } catch (e) { /* ignore */ }
        setFiltersOpen(open);
        toggleFiltersBtn.addEventListener("click", () => setFiltersOpen(!filtersCard.classList.contains("is-open")));
    }

    function applyFilters() {
        updateFiltersSummary();
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

        const dayNames = ["Lun", "Mar", "Mer", "Jeu", "Ven", "Sam", "Dim"].map((d) => t(d));
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
                <div class="empty-title">${t("Aucun cours cette semaine")}</div>
                <button type="button" class="btn btn-sm btn-outline-secondary mt-2" id="jumpToClosestCourseBtn">${t("Aller à la prochaine semaine avec des cours")}</button>
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
            <input type="checkbox" class="form-check-input event-checkbox" data-id="${escapeHtml(c.id)}" ${isSelected ? "checked" : ""} title="${t("Sélectionner pour l'export")}" aria-label="${t("Sélectionner pour l'export")}">
            <div class="event-time">${formatTime(c.start)} – ${formatTime(c.end)}</div>
            <div class="event-title">${c.is_cancelled ? t("[Annulé]") + " " : ""}${escapeHtml(c.course_name)}</div>
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
        if (e.altKey || e.ctrlKey || e.metaKey || document.querySelector(".modal.show")) return;

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
            selectedCountBadge.textContent = t("{n} / {total} cours sélectionnés", { n: visibleSelectedCount, total: visibleCourses.length });
        }
    }

    // =========================================================================
    // 6. Batch Selection Controls
    // =========================================================================
    if (selectAllBtn) {
        selectAllBtn.addEventListener("click", () => {
            visibleCourses.forEach(c => selectedCourseIds.add(c.id));
            renderWeekCalendar();
            updateSelectedCounter();
        });
    }

    if (deselectAllBtn) {
        deselectAllBtn.addEventListener("click", () => {
            visibleCourses.forEach(c => selectedCourseIds.delete(c.id));
            renderWeekCalendar();
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
                alert(t("Veuillez sélectionner au moins un cours à exporter."));
                return;
            }

            try {
                downloadIcsBtn.disabled = true;
                downloadIcsBtn.innerHTML = `<span class="spinner-border spinner-border-sm me-1"></span> ${t("Génération...")}`;

                const res = await fetch("/api/export-ics", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        courses: coursesToExport,
                        calendar_name: "Auriga - EPITA Emploi du temps"
                    })
                });

                if (!res.ok) {
                    throw new Error(t("Erreur lors du téléchargement ({status})", { status: res.status }));
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
                downloadIcsBtn.innerHTML = `<i class="bi bi-download me-1"></i> ${t("Télécharger .ics")}`;
            }
        });
    }

    // =========================================================================
    // 8. Subscription links panel (3 links per account)
    // =========================================================================
    const feedModalEl = document.getElementById("feedModal");
    const feedModal = feedModalEl ? new bootstrap.Modal(feedModalEl) : null;
    const feedList = document.getElementById("feedList");
    const feedCount = document.getElementById("feedCount");
    const feedAlert = document.getElementById("feedAlert");
    const feedFiltersBadges = document.getElementById("feedFiltersBadges");
    const createFeedBtn = document.getElementById("createFeedBtn");
    const createFeedHint = document.getElementById("createFeedHint");
    const copyFeedback = document.getElementById("copyFeedback");
    let feedMax = 3;

    function showFeedError(message) {
        if (!feedAlert) return;
        feedAlert.textContent = message;
        feedAlert.classList.toggle("d-none", !message);
    }

    async function feedApi(url, options = {}) {
        const res = await fetch(url, { headers: { "Content-Type": "application/json" }, ...options });
        const data = await res.json().catch(() => ({}));
        if (!res.ok || data.success === false) throw new Error((data.error && t(data.error)) || t("Erreur serveur ({status})", { status: res.status }));
        return data;
    }

    /** Courses and filters currently shown on the page, as sent to the server. */
    function currentFeedSettings() {
        const allChips = Array.from(document.querySelectorAll(".type-filter-chip"));
        const activeChips = Array.from(document.querySelectorAll(".type-filter-chip:not(.unchecked)"));
        const isCourseFiltered = allChips.length > 0 && activeChips.length < allChips.length;
        const selected = visibleCourses.filter(c => selectedCourseIds.has(c.id));
        return {
            allowed_courses: isCourseFiltered ? activeChips.map(c => c.dataset.course).filter(Boolean) : null,
            hide_cancelled: hideCancelledSwitch ? hideCancelledSwitch.checked : false,
            courses: selected.length ? selected : (visibleCourses.length ? visibleCourses : allCourses),
            subjectsLabel: isCourseFiltered ? `${activeChips.length} / ${allChips.length}` : t("Toutes ({n})", { n: allChips.length }),
        };
    }

    function renderFilterSummary() {
        if (!feedFiltersBadges) return;
        const s = currentFeedSettings();
        const row = (label, value) => `
            <div class="col-sm-4 d-flex justify-content-between gap-2 border-bottom py-1">
                <span class="text-muted">${label}</span>
                <span class="text-end">${value}</span>
            </div>`;
        feedFiltersBadges.innerHTML = [
            row(t("Matières"), escapeHtml(s.subjectsLabel)),
            row(t("Cours annulés"), s.hide_cancelled ? t("Masqués") : t("Inclus")),
            row(t("Séances"), String(s.courses.length)),
        ].join("");
    }

    function formatFeedDate(seconds) {
        if (!seconds) return "";
        return new Date(seconds * 1000).toLocaleDateString(LOCALE, { day: "numeric", month: "short", year: "numeric" });
    }

    function renderFeeds(feeds) {
        if (feedCount) feedCount.textContent = `${feeds.length} / ${feedMax}`;
        if (createFeedBtn) createFeedBtn.disabled = feeds.length >= feedMax;
        if (createFeedHint) {
            createFeedHint.textContent = feeds.length > feedMax
                ? t("Vous avez {n} liens pour un maximum de {max} : supprimez ceux que vous n'utilisez plus.", { n: feeds.length, max: feedMax })
                : feeds.length === feedMax
                    ? t("Maximum atteint : supprimez un lien ou mettez à jour un lien existant.")
                    : "";
        }
        if (!feedList) return;
        if (feeds.length === 0) {
            feedList.innerHTML = `<div class="small text-muted">${t("Aucun lien pour le moment.")}</div>`;
            return;
        }
        feedList.innerHTML = feeds.map(f => {
            const details = [
                t("Créé le {date}", { date: escapeHtml(formatFeedDate(f.created_at)) }),
                f.subjects === null ? t("toutes les matières") : f.subjects > 1 ? t("{n} matières", { n: f.subjects }) : t("1 matière"),
                f.hide_cancelled ? t("annulés masqués") : t("annulés inclus"),
            ].join(" · ");
            return `
                <div class="feed-item border p-3" data-id="${escapeHtml(f.id)}">
                    <div class="d-flex justify-content-between flex-wrap gap-2 small mb-2">
                        <span>${details}</span>
                    </div>
                    <div class="input-group input-group-sm mb-2">
                        <input type="text" class="form-control font-monospace" readonly value="${escapeHtml(f.subscription_url)}" aria-label="${t("Lien d'abonnement")}">
                        <button type="button" class="btn btn-primary" data-copy="${escapeHtml(f.subscription_url)}">${t("Copier")}</button>
                        <a class="btn btn-outline-secondary" href="${escapeHtml(f.subscription_url)}" target="_blank" rel="noopener">${t("Ouvrir")}</a>
                    </div>
                    <div class="d-flex gap-2 flex-wrap">
                        <button type="button" class="btn btn-sm btn-outline-secondary" data-copy="${escapeHtml(f.webcal_url)}">${t("Copier le lien webcal")}</button>
                        <button type="button" class="btn btn-sm btn-outline-secondary" data-update>${t("Mettre à jour avec mes filtres actuels")}</button>
                        <button type="button" class="btn btn-sm btn-outline-danger ms-auto" data-delete>${t("Supprimer")}</button>
                    </div>
                </div>`;
        }).join("");
    }

    async function loadFeeds() {
        showFeedError("");
        renderFilterSummary();
        try {
            const data = await feedApi("/api/feeds");
            feedMax = data.max || 3;
            renderFeeds(data.feeds || []);
        } catch (err) {
            if (feedList) feedList.innerHTML = "";
            showFeedError(err.message);
        }
    }

    async function copyText(text) {
        try {
            await navigator.clipboard.writeText(text);
        } catch (e) {
            const tmp = document.createElement("textarea");
            tmp.value = text;
            document.body.appendChild(tmp);
            tmp.select();
            document.execCommand("copy");
            tmp.remove();
        }
        if (copyFeedback) {
            copyFeedback.classList.remove("d-none");
            setTimeout(() => copyFeedback.classList.add("d-none"), 3000);
        }
    }

    if (feedList) {
        feedList.addEventListener("click", async (e) => {
            const btn = e.target.closest("button");
            if (!btn) return;
            const item = btn.closest(".feed-item");
            if (btn.dataset.copy) {
                copyText(btn.dataset.copy);
                return;
            }
            if (!item) return;
            const id = item.dataset.id;
            showFeedError("");
            try {
                if (btn.hasAttribute("data-update")) {
                    btn.disabled = true;
                    const { subjectsLabel, ...settings } = currentFeedSettings();
                    await feedApi(`/api/feeds/${encodeURIComponent(id)}`, { method: "PUT", body: JSON.stringify(settings) });
                    btn.textContent = t("Mis à jour");
                    setTimeout(loadFeeds, 1200);
                } else if (btn.hasAttribute("data-delete")) {
                    if (!confirm(t("Supprimer ce lien ? Les agendas abonnés ne seront plus mis à jour."))) return;
                    await feedApi(`/api/feeds/${encodeURIComponent(id)}`, { method: "DELETE" });
                    loadFeeds();
                }
            } catch (err) {
                btn.disabled = false;
                showFeedError(err.message);
            }
        });
    }

    if (createFeedBtn) {
        createFeedBtn.addEventListener("click", async () => {
            showFeedError("");
            createFeedBtn.disabled = true;
            try {
                const { subjectsLabel, ...settings } = currentFeedSettings();
                await feedApi("/api/create-feed", { method: "POST", body: JSON.stringify(settings) });
                await loadFeeds();
            } catch (err) {
                showFeedError(err.message);
                createFeedBtn.disabled = false;
            }
        });
    }

    if (openFeedModalBtn && feedModal) {
        openFeedModalBtn.addEventListener("click", () => {
            feedModal.show();
            loadFeeds();
        });
    }

});
