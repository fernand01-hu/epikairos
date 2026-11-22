/**
 * EpiKairos - Planning: timetable of the selected groups and teachers.
 * Sidebar with the group tree and the teacher list, weekly or daily grid.
 */

let groupTreeData = null;
let selectedGroupPatterns = new Set(["S3S4 L3", "S3 Lyon L3", "2627 - S3 Lyon L3"]);
const TEACHER_STORAGE_KEY = "zeus_selected_teachers";
let selectedTeachers = new Set();
try {
    const stored = JSON.parse(localStorage.getItem(TEACHER_STORAGE_KEY) || "[]");
    if (Array.isArray(stored)) selectedTeachers = new Set(stored.filter(t => typeof t === "string"));
} catch (e) { /* private mode or bad JSON: start empty */ }
let currentCalendarDate = new Date();
let currentViewMode = "week"; // "week" | "day"
let activeCourses = [];
const HOUR_HEIGHT = 52; // Must match CSS --zeus-hour-height

function escapeHtml(str) {
    if (str === null || str === undefined) return "";
    return String(str)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

/** YYYY-MM-DD in local time (toISOString() would shift dates to UTC). */
function toLocalDateStr(d) {
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

async function apiJson(url, options = {}) {
    const res = await fetch(url, { headers: { "Content-Type": "application/json" }, ...options });
    const data = await res.json().catch(() => ({}));
    if (!res.ok || data.success === false) {
        throw new Error((data.error && t(data.error)) || t("Erreur serveur ({status})", { status: res.status }));
    }
    return data;
}

document.addEventListener("DOMContentLoaded", () => {
    // Elements - Sidebar & Search
    const groupTreeContainer = document.getElementById("groupTreeContainer");
    const treeSearchInput = document.getElementById("treeSearchInput");
    const btnSearchTree = document.getElementById("btnSearchTree");
    const btnClearSearch = document.getElementById("btnClearSearch");
    const btnClearSelection = document.getElementById("btnClearSelection");
    const btnToggleGroupSection = document.getElementById("btnToggleGroupSection");
    const groupChevron = document.getElementById("groupChevron");
    const teacherListContainer = document.getElementById("teacherListContainer");
    const btnToggleTeacherSection = document.getElementById("btnToggleTeacherSection");
    const teacherChevron = document.getElementById("teacherChevron");
    const teacherSelectedCount = document.getElementById("teacherSelectedCount");

    // Elements - Sidebar Toggle
    const appBody = document.querySelector(".zeus-app-body");
    const btnToggleSidebar = document.getElementById("btnToggleSidebar");
    const btnCloseSidebar = document.getElementById("btnCloseSidebar");
    const toggleSidebarIcon = document.getElementById("toggleSidebarIcon");
    const toggleSidebarLabel = document.getElementById("toggleSidebarLabel");

    // Elements - Timetable Topbar & Grid
    const btnPrevWeek = document.getElementById("btnPrevWeek");
    const btnNextWeek = document.getElementById("btnNextWeek");
    const btnToday = document.getElementById("btnToday");
    const weekTitleLabel = document.getElementById("weekTitleLabel");
    const btnViewWeek = document.getElementById("btnViewWeek");
    const btnViewDay = document.getElementById("btnViewDay");
    const topbarDatePicker = document.getElementById("topbarDatePicker");

    const gridDaysHeaderContainer = document.getElementById("gridDaysHeaderContainer");
    const gridTimeColumn = document.getElementById("gridTimeColumn");
    const gridColumnsContainer = document.getElementById("gridColumnsContainer");
    const gridBody = document.getElementById("gridBody");

    // Modals
    const courseDetailModalEl = document.getElementById("courseDetailModal");
    const courseDetailModal = courseDetailModalEl ? new bootstrap.Modal(courseDetailModalEl) : null;

    // =========================================================================
    // 1. Date & Week Utilities
    // =========================================================================
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

    function getIsoWeekNumber(d) {
        const target = new Date(d.valueOf());
        const dayNr = (d.getDay() + 6) % 7;
        target.setDate(target.getDate() - dayNr + 3);
        const firstThursday = target.valueOf();
        target.setMonth(0, 1);
        if (target.getDay() !== 4) {
            target.setMonth(0, 1 + ((4 - target.getDay()) + 7) % 7);
        }
        return 1 + Math.ceil((firstThursday - target.valueOf()) / 604800000);
    }

    function formatTime(isoStr) {
        if (!isoStr) return "";
        const d = new Date(isoStr);
        return d.toLocaleTimeString(LOCALE, { hour: "2-digit", minute: "2-digit" });
    }

    // =========================================================================
    // 2. Build 24-Hour Time Column
    // =========================================================================
    function initTimeColumn() {
        gridTimeColumn.innerHTML = "";
        for (let h = 0; h < 24; h++) {
            const label = document.createElement("div");
            label.className = "grid-hour-label";
            label.textContent = `${String(h).padStart(2, "0")}:00`;
            gridTimeColumn.appendChild(label);
        }
    }

    initTimeColumn();

    // =========================================================================
    // 3. Hierarchical Group Treeview
    // =========================================================================
    async function loadGroupTree() {
        try {
            const res = await fetch("/api/groups/tree");
            if (!res.ok) throw new Error(t("Erreur de chargement de l'arbre"));
            groupTreeData = await res.json();
            renderTree(groupTreeData);
            fetchSchedule();
        } catch (err) {
            groupTreeContainer.innerHTML = `<div class="text-danger small p-2">Erreur: ${escapeHtml(err.message)}</div>`;
        }
    }

    function renderTree(treeData) {
        groupTreeContainer.innerHTML = "";
        if (treeData && treeData.id === "root" && treeData.children) {
            treeData.children.forEach(child => {
                const childElement = createTreeNodeElement(child);
                groupTreeContainer.appendChild(childElement);
            });
        } else if (Array.isArray(treeData)) {
            treeData.forEach(node => {
                groupTreeContainer.appendChild(createTreeNodeElement(node));
            });
        } else {
            const rootElement = createTreeNodeElement(treeData);
            groupTreeContainer.appendChild(rootElement);
        }
    }

    function createTreeNodeElement(node) {
        const container = document.createElement("div");
        container.className = "tree-node";
        container.dataset.id = node.id;
        const searchTerms = [node.label || "", ...(node.patterns || [])].join(" ").toLowerCase();
        container.dataset.label = searchTerms;

        const hasChildren = node.children && node.children.length > 0;
        const isDefaultExpanded = node.expanded !== false;

        const content = document.createElement("div");
        content.className = "tree-node-content";

        // Toggle arrow icon
        const toggleSpan = document.createElement("span");
        toggleSpan.className = "tree-toggle";
        if (hasChildren) {
            toggleSpan.textContent = isDefaultExpanded ? "▼" : "►";
        } else {
            toggleSpan.innerHTML = "&nbsp;";
        }

        // Checkbox
        const cb = document.createElement("input");
        cb.type = "checkbox";
        cb.className = "tree-checkbox";
        cb.id = `tree_cb_${node.id}`;
        cb.dataset.id = node.id;

        // Check if matching initial patterns
        if (node.patterns) {
            const isMatched = node.patterns.some(p => selectedGroupPatterns.has(p));
            if (isMatched) cb.checked = true;
        }

        // Label
        const label = document.createElement("span");
        label.className = "tree-label";
        label.textContent = node.label;

        content.appendChild(toggleSpan);
        content.appendChild(cb);
        content.appendChild(label);
        container.appendChild(content);

        // Children container
        let childrenContainer = null;
        if (hasChildren) {
            childrenContainer = document.createElement("div");
            childrenContainer.className = `tree-children ${isDefaultExpanded ? "" : "collapsed"}`;

            node.children.forEach(child => {
                const childEl = createTreeNodeElement(child);
                childrenContainer.appendChild(childEl);
            });

            container.appendChild(childrenContainer);

            // Toggle click
            toggleSpan.addEventListener("click", (e) => {
                e.stopPropagation();
                const isCollapsed = childrenContainer.classList.toggle("collapsed");
                toggleSpan.textContent = isCollapsed ? "►" : "▼";
            });

            label.addEventListener("click", (e) => {
                e.stopPropagation();
                const isCollapsed = childrenContainer.classList.toggle("collapsed");
                toggleSpan.textContent = isCollapsed ? "►" : "▼";
            });
        }

        // Checkbox change handler (propagates to descendants)
        cb.addEventListener("change", (e) => {
            const isChecked = e.target.checked;
            if (childrenContainer) {
                childrenContainer.querySelectorAll(".tree-checkbox").forEach(childCb => {
                    childCb.checked = isChecked;
                });
            }
            updateSelectedPatternsFromTree();
            fetchSchedule();
        });

        return container;
    }

    function updateSelectedPatternsFromTree() {
        selectedGroupPatterns.clear();

        function collectPatterns(node) {
            const cb = document.getElementById(`tree_cb_${node.id}`);
            if (cb && cb.checked) {
                if (node.patterns && node.patterns.length > 0) {
                    node.patterns.forEach(p => selectedGroupPatterns.add(p));
                } else if (node.label) {
                    selectedGroupPatterns.add(node.label);
                }
            }
            if (node.children) {
                node.children.forEach(collectPatterns);
            }
        }

        if (groupTreeData) {
            collectPatterns(groupTreeData);
        }
    }

    // =========================================================================
    // Teachers: every teacher found in the shared database, checkable like groups
    // =========================================================================
    function saveSelectedTeachers() {
        try {
            localStorage.setItem(TEACHER_STORAGE_KEY, JSON.stringify(Array.from(selectedTeachers)));
        } catch (e) { /* storage unavailable: selection lasts for this visit */ }
        teacherSelectedCount.textContent = selectedTeachers.size ? `(${selectedTeachers.size})` : "";
    }

    async function loadTeachers() {
        try {
            const data = await apiJson("/api/teachers");
            renderTeachers(data.teachers || []);
        } catch (err) {
            teacherListContainer.innerHTML = `<div class="text-danger small p-2">Erreur: ${escapeHtml(err.message)}</div>`;
        }
    }

    function renderTeachers(teachers) {
        teacherListContainer.innerHTML = "";
        if (teachers.length === 0) {
            teacherListContainer.innerHTML = `<div class="small text-muted py-2">${t("Aucun professeur pour le moment. La liste se remplit quand des étudiants se connectent.")}</div>`;
            return;
        }
        // Forget selections for teachers that no longer appear in any course
        const known = new Set(teachers.map(t => t.name));
        selectedTeachers.forEach(name => { if (!known.has(name)) selectedTeachers.delete(name); });
        saveSelectedTeachers();

        teachers.forEach((t, i) => {
            const row = document.createElement("div");
            row.className = "tree-node teacher-row";
            row.dataset.label = t.name.toLowerCase();

            const content = document.createElement("label");
            content.className = "tree-node-content";
            content.htmlFor = `teacher_cb_${i}`;

            const spacer = document.createElement("span");
            spacer.className = "tree-toggle";

            const cb = document.createElement("input");
            cb.type = "checkbox";
            cb.className = "tree-checkbox teacher-checkbox";
            cb.id = `teacher_cb_${i}`;
            cb.checked = selectedTeachers.has(t.name);
            cb.addEventListener("change", () => {
                if (cb.checked) selectedTeachers.add(t.name);
                else selectedTeachers.delete(t.name);
                saveSelectedTeachers();
                fetchSchedule();
            });

            const name = document.createElement("span");
            name.className = "tree-label";
            name.textContent = t.name;

            const count = document.createElement("span");
            count.className = "teacher-course-count";
            count.textContent = t.courses;
            count.title = `${t.courses} cours`;

            content.append(spacer, cb, name, count);
            row.appendChild(content);
            teacherListContainer.appendChild(row);
        });
        performTreeSearch();
    }

    btnToggleTeacherSection.addEventListener("click", () => {
        const isHidden = teacherListContainer.classList.toggle("d-none");
        teacherChevron.className = isHidden ? "bi bi-chevron-down" : "bi bi-chevron-up";
    });

    // Tree search functionality
    function performTreeSearch() {
        const query = treeSearchInput.value.trim().toLowerCase();
        teacherListContainer.querySelectorAll(".teacher-row").forEach(row => {
            row.classList.toggle("d-none", !!query && !row.dataset.label.includes(query));
        });
        if (!query) {
            // Reset visibility
            groupTreeContainer.querySelectorAll(".tree-node").forEach(n => n.classList.remove("d-none"));
            return;
        }

        groupTreeContainer.querySelectorAll(".tree-node").forEach(node => {
            const label = node.dataset.label || "";
            const matches = label.includes(query);
            if (matches) {
                node.classList.remove("d-none");
                // Expand ancestors
                let parent = node.parentElement;
                while (parent && parent !== groupTreeContainer) {
                    if (parent.classList.contains("tree-children")) {
                        parent.classList.remove("collapsed");
                        const prevSibling = parent.previousElementSibling;
                        if (prevSibling) {
                            const toggle = prevSibling.querySelector(".tree-toggle");
                            if (toggle) toggle.textContent = "▼";
                        }
                    }
                    if (parent.classList.contains("tree-node")) {
                        parent.classList.remove("d-none");
                    }
                    parent = parent.parentElement;
                }
            } else {
                // If it has visible children, keep it visible; otherwise hide
                const hasVisibleChild = node.querySelector(`.tree-node:not(.d-none)`) !== null;
                if (!hasVisibleChild) {
                    node.classList.add("d-none");
                }
            }
        });
    }

    treeSearchInput.addEventListener("input", performTreeSearch);
    btnSearchTree.addEventListener("click", performTreeSearch);

    btnClearSearch.addEventListener("click", () => {
        treeSearchInput.value = "";
        groupTreeContainer.querySelectorAll(".tree-node").forEach(n => n.classList.remove("d-none"));
        teacherListContainer.querySelectorAll(".teacher-row").forEach(n => n.classList.remove("d-none"));
    });

    btnClearSelection.addEventListener("click", () => {
        groupTreeContainer.querySelectorAll(".tree-checkbox").forEach(cb => cb.checked = false);
        teacherListContainer.querySelectorAll(".teacher-checkbox").forEach(cb => cb.checked = false);
        selectedGroupPatterns.clear();
        selectedTeachers.clear();
        saveSelectedTeachers();
        activeCourses = [];
        renderTimetableGrid();
    });

    btnToggleGroupSection.addEventListener("click", () => {
        const isHidden = groupTreeContainer.classList.toggle("d-none");
        groupChevron.className = isHidden ? "bi bi-chevron-down" : "bi bi-chevron-up";
    });

    // =========================================================================
    // 4. Fetch Schedule for Selected Groups
    // =========================================================================
    function visibleWeekRange() {
        const monday = getMonday(currentCalendarDate);
        const sunday = new Date(monday);
        sunday.setDate(monday.getDate() + 6);
        return { start: toLocalDateStr(monday), end: toLocalDateStr(sunday) };
    }

    async function fetchCourses() {
        const patterns = Array.from(selectedGroupPatterns);
        const teachers = Array.from(selectedTeachers);
        if (patterns.length === 0 && teachers.length === 0) {
            activeCourses = [];
            return;
        }

        const range = visibleWeekRange();
        try {
            const res = await fetch("/api/groups/schedule", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    patterns: patterns,
                    teachers: teachers,
                    start_date: range.start,
                    end_date: range.end
                })
            });

            const data = await res.json();
            if (data.success) {
                activeCourses = data.courses || [];
            }
        } catch (e) {
            console.error("Error fetching group schedule", e);
        }
    }

    async function fetchSchedule() {
        await fetchCourses();
        renderTimetableGrid();
    }

    // =========================================================================
    // 5. Render Weekly Timetable Grid
    // =========================================================================
    function getTypeCardClass(type) {
        const t = (type || "").trim().toUpperCase();
        if (t === "CME" || t === "CM") return "card-type-cme";
        if (t === "TDE" || t === "TD") return "card-type-tde";
        if (t === "TPE" || t === "TP") return "card-type-tpe";
        if (t.startsWith("EX") || t.includes("EXAM") || t.includes("EVAL")) return "card-type-ex";
        if (t.includes("CONF")) return "card-type-conf";
        if (t.includes("EVN") || t.includes("EVEN")) return "card-type-evn";
        return "card-type-autre";
    }

    /** Splits overlapping items of one day into side-by-side lanes. */
    function assignLanes(items) {
        items.sort((a, b) => a.start - b.start || b.end - a.end);
        let cluster = [];
        let clusterEnd = 0;
        const closeCluster = () => {
            const lanes = Math.max(1, ...cluster.map(it => it.lane + 1));
            cluster.forEach(it => { it.lanes = lanes; });
            cluster = [];
        };
        items.forEach(item => {
            if (cluster.length && item.start >= clusterEnd) closeCluster();
            const laneEnds = [];
            cluster.forEach(it => { laneEnds[it.lane] = Math.max(laneEnds[it.lane] || 0, it.end); });
            let lane = laneEnds.findIndex(end => end <= item.start);
            if (lane === -1) lane = laneEnds.length;
            item.lane = lane;
            cluster.push(item);
            clusterEnd = Math.max(clusterEnd, item.end);
        });
        if (cluster.length) closeCluster();
    }

    function buildCourseCard(c) {
        const card = document.createElement("div");
        card.className = `zeus-event-card ${getTypeCardClass(c.activity_type)}`;

        const roomsStr = c.rooms && c.rooms.length > 0 ? c.rooms.map(escapeHtml).join(", ") : "";
        const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.map(escapeHtml).join(", ") : "";

        card.innerHTML = `
            <div class="zeus-event-title" title="${escapeHtml(c.course_name)}">${escapeHtml(c.course_name)}</div>
            <div class="zeus-event-meta">
                <span>${formatTime(c.start)} - ${formatTime(c.end)}</span><br>
                ${roomsStr ? `<span>${roomsStr}</span><br>` : ""}
                ${teachersStr ? `<span>${teachersStr}</span>` : ""}
            </div>
        `;
        card.addEventListener("click", () => showCourseDetailModal(c));
        return card;
    }

    /** Keeps the day headers over their columns and on one line. */
    function fitDayHeaders() {
        const header = gridDaysHeaderContainer && gridDaysHeaderContainer.parentElement;
        const body = document.getElementById("gridBody");
        if (header && body) header.style.paddingRight = `${body.offsetWidth - body.clientWidth}px`;
        const cells = gridDaysHeaderContainer ? [...gridDaysHeaderContainer.children] : [];
        cells.forEach(c => { if (c.dataset.full) c.textContent = c.dataset.full; });
        if (cells.some(c => c.scrollWidth > c.clientWidth + 1)) {
            cells.forEach(c => { if (c.dataset.short) c.textContent = c.dataset.short; });
        }
    }
    window.addEventListener("resize", fitDayHeaders);

    function renderTimetableGrid() {
        const monday = getMonday(currentCalendarDate);
        const weekNum = getIsoWeekNumber(monday);

        // Update Topbar Title
        weekTitleLabel.textContent = t("Semaine {n}", { n: weekNum });
        topbarDatePicker.value = toLocalDateStr(currentCalendarDate);

        // Days configuration
        // 2024-01-01 was a Monday
        const frenchDayNames = Array.from({ length: 7 }, (_, i) =>
            new Date(2024, 0, 1 + i).toLocaleDateString(LOCALE, { weekday: "long" }));
        const shortMonths = Array.from({ length: 12 }, (_, i) =>
            new Date(2000, i, 1).toLocaleDateString(LOCALE, { month: "short" }));
        const today = new Date();

        // Clear header and columns
        gridDaysHeaderContainer.innerHTML = "";
        gridColumnsContainer.innerHTML = "";

        const daysToShow = currentViewMode === "day" ? 1 : 7;
        const startDayIndex = currentViewMode === "day" ? (currentCalendarDate.getDay() + 6) % 7 : 0;

        let earliestCourseMinutes = 8 * 60; // Default 08:00

        for (let i = 0; i < daysToShow; i++) {
            const dayOffset = currentViewMode === "day" ? startDayIndex : i;
            const dayDate = new Date(monday);
            dayDate.setDate(monday.getDate() + dayOffset);

            const isToday = isSameDay(dayDate, today);
            const dayName = frenchDayNames[dayOffset];
            const dateNum = dayDate.getDate();
            const monthStr = shortMonths[dayDate.getMonth()];

            // 1. Day Header Cell
            const headerCell = document.createElement("div");
            headerCell.className = `grid-day-header ${isToday ? "is-today" : ""}`;
            headerCell.textContent = `${dayName} ${dateNum} ${monthStr}`;
            // Shorter label used when the column is too narrow (browser zoom, small screens)
            headerCell.dataset.full = headerCell.textContent;
            headerCell.dataset.short = `${dayDate.toLocaleDateString(LOCALE, { weekday: "short" }).replace(/\.$/, "")} ${dateNum}`;
            headerCell.title = headerCell.textContent;
            gridDaysHeaderContainer.appendChild(headerCell);

            // 2. Day Column in Grid
            const dayCol = document.createElement("div");
            dayCol.className = "grid-day-column";
            dayCol.dataset.date = toLocalDateStr(dayDate);

            // Create 24 hour slots
            for (let h = 0; h < 24; h++) {
                const slot = document.createElement("div");
                slot.className = "grid-hour-slot";
                dayCol.appendChild(slot);
            }

            // Current time red line (if today)
            if (isToday) {
                const nowMinutes = today.getHours() * 60 + today.getMinutes();
                const nowTop = (nowMinutes / 60) * HOUR_HEIGHT;
                const nowLine = document.createElement("div");
                nowLine.className = "now-indicator-line";
                nowLine.style.top = `${nowTop}px`;
                dayCol.appendChild(nowLine);
            }

            // Render Courses for this day
            const dayItems = [
                ...activeCourses
                    .filter(c => isSameDay(new Date(c.start), dayDate))
                    .map(c => ({ kind: "course", data: c, start: new Date(c.start), end: new Date(c.end) })),
            ];
            assignLanes(dayItems);

            dayItems.forEach(item => {
                const startMin = item.start.getHours() * 60 + item.start.getMinutes();
                const endMin = item.end.getHours() * 60 + item.end.getMinutes();
                const durationMin = Math.max(endMin - startMin, 30);

                if (startMin < earliestCourseMinutes) {
                    earliestCourseMinutes = startMin;
                }

                const card = buildCourseCard(item.data);
                card.style.top = `${(startMin / 60) * HOUR_HEIGHT}px`;
                card.style.height = `${Math.max((durationMin / 60) * HOUR_HEIGHT - 2, 24)}px`;
                // Side-by-side layout when items overlap
                card.style.left = `calc(${(item.lane / item.lanes) * 100}% + 2px)`;
                card.style.right = `calc(${((item.lanes - item.lane - 1) / item.lanes) * 100}% + 2px)`;

                dayCol.appendChild(card);
            });

            gridColumnsContainer.appendChild(dayCol);
        }

        // Auto-scroll to earliest course or 08:00
        const scrollTarget = Math.max((earliestCourseMinutes / 60) * HOUR_HEIGHT - 20, 0);
        gridBody.scrollTop = scrollTarget;
        fitDayHeaders();
    }

    function showCourseDetailModal(c) {
        document.getElementById("courseDetailTitle").textContent = c.course_name;
        const modalBody = document.getElementById("courseDetailBody");

        const roomsStr = c.rooms && c.rooms.length > 0 ? c.rooms.map(escapeHtml).join(", ") : t("Non spécifiée");
        const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.map(escapeHtml).join(", ") : t("Non spécifié");
        const groupsStr = c.groups && c.groups.length > 0 ? c.groups.map(escapeHtml).join(", ") : t("Non spécifié");

        modalBody.innerHTML = `
            ${c.is_cancelled ? '<div class="mb-3"><span class="badge bg-danger px-3 py-1">${t("ANNULÉ")}</span></div>' : ''}
            <dl class="row mb-0 small">
                <dt class="col-sm-4 text-muted">${t("Horaires :")}</dt>
                <dd class="col-sm-8 fw-semibold">${formatTime(c.start)} - ${formatTime(c.end)} (${c.duration} min)</dd>

                <dt class="col-sm-4 text-muted">${t("Date :")}</dt>
                <dd class="col-sm-8">${new Date(c.start).toLocaleDateString(LOCALE, { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</dd>

                <dt class="col-sm-4 text-muted">${t("Salle(s) :")}</dt>
                <dd class="col-sm-8">${roomsStr}</dd>

                <dt class="col-sm-4 text-muted">${t("Enseignant(s) :")}</dt>
                <dd class="col-sm-8">${teachersStr}</dd>

                <dt class="col-sm-4 text-muted">${t("Groupe(s) :")}</dt>
                <dd class="col-sm-8">👥 ${groupsStr}</dd>

                <dt class="col-sm-4 text-muted">${t("Statut :")}</dt>
                <dd class="col-sm-8"><span class="badge bg-light text-dark border">${escapeHtml(c.status || "PLANIFIE")}</span></dd>
            </dl>
        `;

        courseDetailModal.show();
    }

    // =========================================================================
    // 6. Navigation Controls
    // =========================================================================
    btnPrevWeek.addEventListener("click", () => {
        const offset = currentViewMode === "day" ? 1 : 7;
        currentCalendarDate.setDate(currentCalendarDate.getDate() - offset);
        fetchSchedule();
    });

    btnNextWeek.addEventListener("click", () => {
        const offset = currentViewMode === "day" ? 1 : 7;
        currentCalendarDate.setDate(currentCalendarDate.getDate() + offset);
        fetchSchedule();
    });

    btnToday.addEventListener("click", () => {
        currentCalendarDate = new Date();
        fetchSchedule();
    });

    topbarDatePicker.addEventListener("change", (e) => {
        if (e.target.value) {
            currentCalendarDate = new Date(e.target.value);
            fetchSchedule();
        }
    });

    btnViewWeek.addEventListener("click", () => {
        currentViewMode = "week";
        btnViewWeek.classList.add("active");
        btnViewDay.classList.remove("active");
        renderTimetableGrid();
    });

    btnViewDay.addEventListener("click", () => {
        currentViewMode = "day";
        btnViewDay.classList.add("active");
        btnViewWeek.classList.remove("active");
        renderTimetableGrid();
    });

    // =========================================================================
    // 7. Sidebar Toggle & Modals
    // =========================================================================
    function setSidebarCollapsed(collapsed) {
        if (!appBody) return;
        if (collapsed) {
            appBody.classList.add("sidebar-collapsed");
            if (toggleSidebarIcon) {
                toggleSidebarIcon.className = "bi bi-layout-sidebar";
            }
            if (toggleSidebarLabel) {
                toggleSidebarLabel.textContent = t("Afficher le menu");
            }
            if (btnToggleSidebar) {
                btnToggleSidebar.title = t("Afficher le menu de sélection");
                btnToggleSidebar.classList.add("btn-primary");
                btnToggleSidebar.classList.remove("btn-outline-secondary");
            }
        } else {
            appBody.classList.remove("sidebar-collapsed");
            if (toggleSidebarIcon) {
                toggleSidebarIcon.className = "bi bi-layout-sidebar-inset";
            }
            if (toggleSidebarLabel) {
                toggleSidebarLabel.textContent = t("Masquer le menu");
            }
            if (btnToggleSidebar) {
                btnToggleSidebar.title = t("Masquer le menu de sélection");
                btnToggleSidebar.classList.remove("btn-primary");
                btnToggleSidebar.classList.add("btn-outline-secondary");
            }
        }
        if (window.matchMedia("(min-width: 768px)").matches) {
            try {
                localStorage.setItem("zeus_sidebar_collapsed", collapsed ? "1" : "0");
            } catch (e) {}
        }
    }

    if (btnToggleSidebar) {
        btnToggleSidebar.addEventListener("click", () => {
            const isCurrentlyCollapsed = appBody && appBody.classList.contains("sidebar-collapsed");
            setSidebarCollapsed(!isCurrentlyCollapsed);
        });
    }

    if (btnCloseSidebar) {
        btnCloseSidebar.addEventListener("click", () => {
            setSidebarCollapsed(true);
        });
    }

    // Phones: "Voir le planning" closes the full-screen menu
    document.getElementById("btnShowTimetable")?.addEventListener("click", () => setSidebarCollapsed(true));

    // Phones: one day at a time, and the menu (a full-screen panel there) starts closed
    const isPhone = window.matchMedia("(max-width: 767.98px)").matches;
    if (isPhone) {
        currentViewMode = "day";
        // A weekend day is empty: open on the coming Monday instead
        const dow = currentCalendarDate.getDay();
        if (dow === 0 || dow === 6) {
            currentCalendarDate.setDate(currentCalendarDate.getDate() + (dow === 0 ? 1 : 2));
            if (topbarDatePicker) {
                const d = currentCalendarDate;
                topbarDatePicker.value = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
            }
        }
        btnViewDay.classList.add("active");
        btnViewWeek.classList.remove("active");
        setSidebarCollapsed(true);
    } else {
        try {
            if (localStorage.getItem("zeus_sidebar_collapsed") === "1") {
                setSidebarCollapsed(true);
            }
        } catch (e) {}
    }

    // Initial Load (the group tree loads the schedule once ready)
    loadGroupTree();
    loadTeachers();

});
