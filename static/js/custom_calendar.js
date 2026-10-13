/**
 * Zeus Auriga - Custom Calendar Generator
 */

let customCourses = [];
let currentCalendarDate = new Date();
let currentViewMode = "week";

function escapeHtml(str) {
    if (!str) return "";
    return String(str)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

document.addEventListener("DOMContentLoaded", () => {
    const teachersSelect = document.getElementById("teachersSelect");
    const roomsSelect = document.getElementById("roomsSelect");
    const groupsSelect = document.getElementById("groupsSelect");
    const customKeywordInput = document.getElementById("customKeywordInput");
    const customStartDate = document.getElementById("customStartDate");
    const customEndDate = document.getElementById("customEndDate");
    const customFilterForm = document.getElementById("customFilterForm");
    const resetCustomFiltersBtn = document.getElementById("resetCustomFiltersBtn");

    const customCourseCountBadge = document.getElementById("customCourseCountBadge");
    const downloadCustomIcsBtn = document.getElementById("downloadCustomIcsBtn");

    const weekDaysGrid = document.getElementById("weekDaysGrid");
    const noWeekCoursesMessage = document.getElementById("noWeekCoursesMessage");
    const noCoursesMessage = document.getElementById("noCoursesMessage");
    const currentWeekLabel = document.getElementById("currentWeekLabel");
    const prevWeekBtn = document.getElementById("prevWeekBtn");
    const nextWeekBtn = document.getElementById("nextWeekBtn");
    const todayWeekBtn = document.getElementById("todayWeekBtn");

    const viewModeWeekBtn = document.getElementById("viewModeWeekBtn");
    const viewModeListBtn = document.getElementById("viewModeListBtn");
    const viewModeRecurringBtn = document.getElementById("viewModeRecurringBtn");
    const calendarWeekView = document.getElementById("calendarWeekView");
    const calendarTableView = document.getElementById("calendarTableView");
    const calendarRecurringView = document.getElementById("calendarRecurringView");
    const recurringDaysGrid = document.getElementById("recurringDaysGrid");
    const recurringCountBadge = document.getElementById("recurringCountBadge");
    const weekNavControls = document.getElementById("weekNavControls");

    // =========================================================================
    // Helpers: Course Type Styling
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

    function getMonday(d) {
        const date = new Date(d);
        const day = date.getDay();
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
        const months = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
        const mDay = monday.getDate();
        const sDay = sunday.getDate();
        const mMonth = months[monday.getMonth()];
        const sMonth = months[sunday.getMonth()];
        const year = sunday.getFullYear();

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
    // 1. Fetch Options (Teachers, Rooms, Groups structured by Promo)
    // =========================================================================
    async function loadFilterOptions() {
        try {
            const res = await fetch("/api/database/options");
            if (!res.ok) return;
            const data = await res.json();

            // Populate Teachers
            teachersSelect.innerHTML = "";
            (data.teachers || []).forEach(t => {
                const opt = document.createElement("option");
                opt.value = t;
                opt.textContent = t;
                teachersSelect.appendChild(opt);
            });

            // Populate Rooms (clean names without floor/level)
            roomsSelect.innerHTML = "";
            (data.rooms || []).forEach(r => {
                const opt = document.createElement("option");
                opt.value = r;
                opt.textContent = r;
                roomsSelect.appendChild(opt);
            });

            // Populate Groups structured by Promotion hierarchy
            groupsSelect.innerHTML = "";
            if (data.promotions && data.promotions.length > 0) {
                data.promotions.forEach(promo => {
                    const optgroup = document.createElement("optgroup");
                    optgroup.label = `${promo.name} (${promo.cycle})`;
                    (promo.classes || []).forEach(cls => {
                        const opt = document.createElement("option");
                        opt.value = cls.name;
                        opt.textContent = `${cls.display_name} — ${promo.code}`;
                        optgroup.appendChild(opt);
                    });
                    groupsSelect.appendChild(optgroup);
                });
            } else {
                (data.groups || []).forEach(g => {
                    const opt = document.createElement("option");
                    opt.value = g;
                    opt.textContent = g;
                    groupsSelect.appendChild(opt);
                });
            }

        } catch (e) {
            console.error("Failed to load options", e);
        }
    }

    loadFilterOptions();

    // =========================================================================
    // 2. Query Custom Courses
    // =========================================================================
    function getSelectedValues(selectEl) {
        return Array.from(selectEl.selectedOptions).map(o => o.value);
    }

    async function executeQuery() {
        const teachers = getSelectedValues(teachersSelect);
        const rooms = getSelectedValues(roomsSelect);
        const groups = getSelectedValues(groupsSelect);
        const keyword = customKeywordInput.value.trim();
        const startDate = customStartDate.value;
        const endDate = customEndDate.value;

        const payload = {
            teachers: teachers,
            rooms: rooms,
            groups: groups,
            keyword: keyword,
            start_date: startDate,
            end_date: endDate,
        };

        const submitBtn = document.getElementById("applyCustomFiltersBtn");
        submitBtn.disabled = true;
        submitBtn.innerHTML = '<span class="spinner-border spinner-border-sm me-1"></span> Recherche...';

        try {
            const res = await fetch("/api/database/custom-courses", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload)
            });

            const data = await res.json();
            if (!res.ok) throw new Error(data.error || "Erreur de recherche");

            customCourses = data.courses || [];
            customCourseCountBadge.textContent = `${customCourses.length} cours trouvés`;

            if (customCourses.length > 0) {
                downloadCustomIcsBtn.disabled = false;
                noCoursesMessage.classList.add("d-none");

                // Start week at first course date
                currentCalendarDate = new Date(customCourses[0].start);
            } else {
                downloadCustomIcsBtn.disabled = true;
                noCoursesMessage.classList.remove("d-none");
                noCoursesMessage.innerHTML = `
                    <i class="bi bi-search fs-1 d-block mb-2 text-secondary"></i>
                    Aucun cours ne correspond à ces critères.
                `;
            }

            renderWeekCalendar();
            renderTable();

        } catch (err) {
            alert(`Erreur : ${err.message}`);
        } finally {
            submitBtn.disabled = false;
            submitBtn.innerHTML = '<i class="bi bi-search me-1"></i> Générer l\'agenda';
        }
    }

    customFilterForm.addEventListener("submit", (e) => {
        e.preventDefault();
        executeQuery();
    });

    resetCustomFiltersBtn.addEventListener("click", () => {
        teachersSelect.selectedIndex = -1;
        roomsSelect.selectedIndex = -1;
        groupsSelect.selectedIndex = -1;
        customKeywordInput.value = "";
        customCourses = [];
        customCourseCountBadge.textContent = "0 cours trouvés";
        downloadCustomIcsBtn.disabled = true;
        noCoursesMessage.classList.remove("d-none");
        weekDaysGrid.innerHTML = "";
    });

    // =========================================================================
    // 3. Render Custom Weekly Calendar
    // =========================================================================
    function renderWeekCalendar() {
        if (!weekDaysGrid) return;
        weekDaysGrid.innerHTML = "";

        const monday = getMonday(currentCalendarDate);
        if (currentWeekLabel) {
            currentWeekLabel.textContent = formatWeekLabel(monday);
        }

        const weekSunday = new Date(monday);
        weekSunday.setDate(monday.getDate() + 6);

        const hasSundayCourses = customCourses.some(c => {
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

            const dayCourses = customCourses.filter(c => isSameDay(new Date(c.start), currentDayDate));
            dayCourses.sort((a, b) => new Date(a.start) - new Date(b.start));
            totalCoursesInThisWeek += dayCourses.length;

            const isToday = isSameDay(currentDayDate, todayDate);

            const col = document.createElement("div");
            col.className = `day-column ${isToday ? "is-today" : ""}`;

            const dayName = frenchDayNames[i];
            const dateNum = currentDayDate.getDate();
            const monthStr = shortMonths[currentDayDate.getMonth()];

            col.innerHTML = `
                <div class="day-column-header">
                    <div class="d-flex align-items-center justify-content-center gap-1">
                        <span class="fw-bold fs-6">${dayName}</span>
                        ${isToday ? '<span class="badge bg-primary text-white rounded-pill px-2 py-0 small ms-1" style="font-size:0.65rem;">Aujourd\'hui</span>' : ''}
                    </div>
                    <div class="text-muted small">${dateNum} ${monthStr}</div>
                    <div class="small fw-semibold text-secondary mt-1" style="font-size: 0.75rem;">
                        ${dayCourses.length} cours
                    </div>
                </div>
                <div class="day-events-list"></div>
            `;

            const eventsList = col.querySelector(".day-events-list");

            if (dayCourses.length === 0) {
                eventsList.innerHTML = `
                    <div class="text-center text-muted py-5 small opacity-75">
                        <i class="bi bi-cup-hot d-block mb-1 text-secondary fs-5"></i>
                        <span>Aucun cours</span>
                    </div>
                `;
            } else {
                dayCourses.forEach(c => {
                    const info = getTypeInfo(c.activity_type);
                    const card = document.createElement("div");
                    card.className = `calendar-card border-${info.key}`;

                    const roomsStr = c.rooms && c.rooms.length > 0 ? c.rooms.join(", ") : "";
                    const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.join(", ") : "";
                    const groupsStr = c.groups && c.groups.length > 0 ? c.groups.join(", ") : "";

                    card.innerHTML = `
                        <div class="d-flex align-items-center justify-content-between mb-1">
                            <span class="event-time">
                                <i class="bi bi-clock me-1 text-primary"></i>
                                ${formatTime(c.start)} - ${formatTime(c.end)}
                            </span>
                            <span class="badge badge-${info.key} px-2 py-1" style="font-size: 0.7rem;">
                                ${escapeHtml(c.activity_type)}
                            </span>
                        </div>
                        <div class="event-title" title="${escapeHtml(c.course_name)}">
                            ${escapeHtml(c.course_name)}
                        </div>
                        <div class="event-meta">
                            ${roomsStr ? `<div class="text-truncate" title="Salle: ${escapeHtml(roomsStr)}"><i class="bi bi-geo-alt-fill text-danger me-1"></i>${escapeHtml(roomsStr)}</div>` : ""}
                            ${teachersStr ? `<div class="text-truncate" title="Enseignant: ${escapeHtml(teachersStr)}"><i class="bi bi-person-fill text-primary me-1"></i>${escapeHtml(teachersStr)}</div>` : ""}
                            ${groupsStr ? `<div class="text-truncate" title="Groupe: ${escapeHtml(groupsStr)}"><i class="bi bi-people-fill text-secondary me-1"></i>${escapeHtml(groupsStr)}</div>` : ""}
                        </div>
                    `;

                    eventsList.appendChild(card);
                });
            }

            weekDaysGrid.appendChild(col);
        }

        if (totalCoursesInThisWeek === 0 && customCourses.length > 0) {
            noWeekCoursesMessage.classList.remove("d-none");
        } else {
            noWeekCoursesMessage.classList.add("d-none");
        }
    }

    function renderTable() {
        const tbody = document.getElementById("coursesTableBody");
        if (!tbody) return;
        tbody.innerHTML = "";

        customCourses.forEach(c => {
            const info = getTypeInfo(c.activity_type);
            const tr = document.createElement("tr");
            const roomsFormatted = c.rooms && c.rooms.length > 0 ? c.rooms.map(escapeHtml).join("<br>") : "-";
            const teachersFormatted = c.teachers && c.teachers.length > 0 ? c.teachers.map(escapeHtml).join("<br>") : "-";
            const groupsFormatted = c.groups && c.groups.length > 0 ? c.groups.map(escapeHtml).join("<br>") : "-";

            tr.innerHTML = `
                <td>
                    <span class="fw-semibold">${new Date(c.start).toLocaleDateString("fr-FR", { weekday: "short", day: "numeric", month: "short" })}</span><br>
                    <span class="text-muted small">${formatTime(c.start)} - ${formatTime(c.end)}</span>
                </td>
                <td><span class="badge badge-${info.key} px-2 py-1">${escapeHtml(c.activity_type)}</span></td>
                <td><div class="fw-semibold">${escapeHtml(c.course_name)}</div></td>
                <td>${roomsFormatted}</td>
                <td>${teachersFormatted}</td>
                <td>${groupsFormatted}</td>
            `;
            tbody.appendChild(tr);
        });
    }

    // Week Navigation
    prevWeekBtn.addEventListener("click", () => {
        currentCalendarDate.setDate(currentCalendarDate.getDate() - 7);
        renderWeekCalendar();
    });

    nextWeekBtn.addEventListener("click", () => {
        currentCalendarDate.setDate(currentCalendarDate.getDate() + 7);
        renderWeekCalendar();
    });

    todayWeekBtn.addEventListener("click", () => {
        currentCalendarDate = new Date();
        renderWeekCalendar();
    });

    // View Switcher
    function setActiveView(mode) {
        currentViewMode = mode;
        viewModeWeekBtn.classList.toggle("btn-primary", mode === "week");
        viewModeWeekBtn.classList.toggle("btn-outline-secondary", mode !== "week");
        viewModeListBtn.classList.toggle("btn-primary", mode === "list");
        viewModeListBtn.classList.toggle("btn-outline-secondary", mode !== "list");
        if (viewModeRecurringBtn) {
            viewModeRecurringBtn.classList.toggle("btn-primary", mode === "recurring");
            viewModeRecurringBtn.classList.toggle("btn-outline-secondary", mode !== "recurring");
        }

        calendarWeekView.classList.toggle("d-none", mode !== "week");
        calendarTableView.classList.toggle("d-none", mode !== "list");
        if (calendarRecurringView) {
            calendarRecurringView.classList.toggle("d-none", mode !== "recurring");
        }

        if (weekNavControls) {
            weekNavControls.classList.toggle("d-none", mode === "recurring");
        }
    }

    viewModeWeekBtn.addEventListener("click", () => {
        setActiveView("week");
    });

    viewModeListBtn.addEventListener("click", () => {
        setActiveView("list");
    });

    if (viewModeRecurringBtn) {
        viewModeRecurringBtn.addEventListener("click", () => {
            setActiveView("recurring");
            loadRecurringCalendar();
        });
    }

    async function loadRecurringCalendar() {
        if (!recurringDaysGrid) return;
        recurringDaysGrid.innerHTML = '<div class="p-4 text-center text-muted"><span class="spinner-border spinner-border-sm me-2"></span>Chargement du planning type récurrent...</div>';

        const keyword = customKeywordInput.value.trim();
        const selectedTeachers = getSelectedValues(teachersSelect);

        let url = `/api/database/recurring-courses?`;
        if (keyword) url += `&keyword=${encodeURIComponent(keyword)}`;
        if (selectedTeachers.length === 1) url += `&teacher=${encodeURIComponent(selectedTeachers[0])}`;

        try {
            const res = await fetch(url);
            const data = await res.json();
            if (!data.success) throw new Error(data.error || "Erreur");

            if (recurringCountBadge) {
                recurringCountBadge.textContent = `${data.count} créneaux identifiés`;
            }

            renderRecurringGrid(data.weekly_grid || {});
        } catch (e) {
            recurringDaysGrid.innerHTML = `<div class="p-4 text-center text-danger">Erreur : ${e.message}</div>`;
        }
    }

    function renderRecurringGrid(weeklyGrid) {
        recurringDaysGrid.innerHTML = "";
        const days = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi"];

        days.forEach(dayName => {
            const courses = weeklyGrid[dayName] || [];
            const dayCol = document.createElement("div");
            dayCol.className = "week-day-col";

            const header = document.createElement("div");
            header.className = "week-day-header";
            header.innerHTML = `
                <div class="day-name">${dayName}</div>
                <div class="small text-muted">${courses.length} cours type${courses.length > 1 ? "s" : ""}</div>
            `;
            dayCol.appendChild(header);

            const body = document.createElement("div");
            body.className = "week-day-body";

            if (courses.length === 0) {
                const empty = document.createElement("div");
                empty.className = "text-center text-muted small py-4 opacity-50";
                empty.textContent = "Aucun cours";
                body.appendChild(empty);
            } else {
                courses.forEach(c => {
                    const card = document.createElement("div");
                    const typeInfo = getTypeInfo(c.activity_type);
                    card.className = `course-card ${typeInfo.key} mb-2`;

                    const roomDisplay = c.primary_room || "Salle non assignée";
                    const teacherDisplay = c.primary_teacher || "Enseignant";
                    const groupDisplay = c.group_display || c.promo_name || "";
                    const freqText = c.occurrences_count > 1 ? `${c.frequency} (${c.occurrences_count}x)` : "Ponctuel";

                    card.innerHTML = `
                        <div class="d-flex align-items-center justify-content-between mb-1">
                            <span class="badge ${typeInfo.key} fw-bold" style="font-size:0.7rem;">${typeInfo.label}</span>
                            <span class="text-muted fw-semibold" style="font-size:0.75rem;">${c.start_time} - ${c.end_time}</span>
                        </div>
                        <div class="course-name fw-bold" title="${c.course_name}">${c.course_name}</div>
                        <div class="small text-truncate mt-1 text-muted" title="${roomDisplay}">
                            <i class="bi bi-geo-alt-fill text-danger me-1"></i>${roomDisplay}
                        </div>
                        <div class="small text-truncate text-muted" title="${teacherDisplay}">
                            <i class="bi bi-person-fill text-primary me-1"></i>${teacherDisplay}
                        </div>
                        <div class="d-flex align-items-center justify-content-between mt-2 pt-1 border-top" style="font-size:0.7rem;">
                            <span class="badge bg-light text-dark border text-truncate" style="max-width: 110px;" title="${groupDisplay}">
                                ${c.subgroup && c.subgroup !== 'Promo' ? 'Classe ' + c.subgroup : (c.promo_name || 'Promo')}
                            </span>
                            <span class="badge bg-primary-subtle text-primary border border-primary">${freqText}</span>
                        </div>
                    `;
                    body.appendChild(card);
                });
            }

            dayCol.appendChild(body);
            recurringDaysGrid.appendChild(dayCol);
        });
    }

    // Download Custom .ics
    downloadCustomIcsBtn.addEventListener("click", async () => {
        if (customCourses.length === 0) return;

        try {
            downloadCustomIcsBtn.disabled = true;
            downloadCustomIcsBtn.innerHTML = '<span class="spinner-border spinner-border-sm me-1"></span> Téléchargement...';

            const res = await fetch("/api/export-ics", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    courses: customCourses,
                    calendar_name: "EpiKairos - Agenda Personnalisé EPITA"
                })
            });

            if (!res.ok) throw new Error("Erreur de téléchargement");

            const blob = await res.blob();
            const url = window.URL.createObjectURL(blob);
            const a = document.createElement("a");
            a.style.display = "none";
            a.href = url;
            a.download = "epikairos_custom_planning.ics";
            document.body.appendChild(a);
            a.click();
            window.URL.revokeObjectURL(url);
            document.body.removeChild(a);

        } catch (err) {
            alert(`Erreur : ${err.message}`);
        } finally {
            downloadCustomIcsBtn.disabled = false;
            downloadCustomIcsBtn.innerHTML = '<i class="bi bi-download me-1"></i> Télécharger l\'agenda (.ics)';
        }
    });

});
