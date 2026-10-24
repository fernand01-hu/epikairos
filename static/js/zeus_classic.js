/**
 * Zeus Classic UI Client Script - EPITA / 3IE Replica
 * Controls Group Treeview, Hourly Timetable Grid, Room Search, and ICS Generation
 */

let groupTreeData = null;
let selectedGroupPatterns = new Set(["S3S4 L3", "S3 Lyon L3", "2627 - S3 Lyon L3"]);
let currentCalendarDate = new Date();
let currentViewMode = "week"; // "week" | "day"
let activeCourses = [];
let activeEvents = [];
let communityCalendars = [];
let canCreateCalendars = false;
let selectedCalendarIds = null; // Set of ids, null until loaded

const CALENDAR_STORAGE_KEY = "zeus_community_calendars";
const CALENDAR_COLORS = ["#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6", "#14b8a6", "#ec4899", "#64748b"];

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
        throw new Error(data.error || `Erreur serveur (${res.status})`);
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
    const btnGenerateIcs = document.getElementById("btnGenerateIcs");
    const btnFindRoom = document.getElementById("btnFindRoom");
    const btnToggleGroupSection = document.getElementById("btnToggleGroupSection");
    const groupChevron = document.getElementById("groupChevron");

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
    const findRoomModalEl = document.getElementById("findRoomModal");
    const findRoomModal = findRoomModalEl ? new bootstrap.Modal(findRoomModalEl) : null;
    const courseDetailModalEl = document.getElementById("courseDetailModal");
    const courseDetailModal = courseDetailModalEl ? new bootstrap.Modal(courseDetailModalEl) : null;
    const authSyncModalEl = document.getElementById("authSyncModal");
    const authSyncModal = authSyncModalEl ? new bootstrap.Modal(authSyncModalEl) : null;

    // User profile display
    const userDisplayName = document.getElementById("userDisplayName");
    const savedCreds = localStorage.getItem("zeus_credentials");
    if (savedCreds) {
        try {
            const parsed = JSON.parse(savedCreds);
            if (parsed.username && userDisplayName) {
                userDisplayName.textContent = parsed.username.split("@")[0];
            }
        } catch (e) {}
    }

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
        return d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
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
            if (!res.ok) throw new Error("Erreur de chargement de l'arbre");
            groupTreeData = await res.json();
            renderTree(groupTreeData);
            fetchSchedule();
        } catch (err) {
            groupTreeContainer.innerHTML = `<div class="text-danger small p-2">Erreur: ${err.message}</div>`;
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

    // Tree search functionality
    function performTreeSearch() {
        const query = treeSearchInput.value.trim().toLowerCase();
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
    });

    btnClearSelection.addEventListener("click", () => {
        groupTreeContainer.querySelectorAll(".tree-checkbox").forEach(cb => cb.checked = false);
        selectedGroupPatterns.clear();
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
        if (patterns.length === 0) {
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

    async function fetchEvents() {
        if (!selectedCalendarIds || selectedCalendarIds.size === 0) {
            activeEvents = [];
            return;
        }
        const range = visibleWeekRange();
        const ids = Array.from(selectedCalendarIds).join(",");
        try {
            const data = await apiJson(`/api/community/events?start=${range.start}&end=${range.end}&calendars=${ids}`);
            activeEvents = data.events || [];
        } catch (e) {
            console.error("Error fetching community events", e);
            activeEvents = [];
        }
    }

    async function fetchSchedule() {
        await Promise.all([fetchCourses(), fetchEvents()]);
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

    function buildEventCard(ev) {
        const card = document.createElement("div");
        card.className = "zeus-event-card community-event";
        card.style.setProperty("--t", ev.color);
        card.innerHTML = `
            <div class="zeus-event-title" title="${escapeHtml(ev.title)}">${escapeHtml(ev.title)}</div>
            <div class="zeus-event-meta">
                <span>${formatTime(ev.start)} - ${formatTime(ev.end)}</span><br>
                <span>${escapeHtml(ev.calendar_name)}${ev.location ? " · " + escapeHtml(ev.location) : ""}</span>
            </div>
        `;
        card.addEventListener("click", () => {
            if (ev.can_edit) {
                openEventModal(ev);
            } else {
                showEventDetailModal(ev);
            }
        });
        return card;
    }

    function renderTimetableGrid() {
        const monday = getMonday(currentCalendarDate);
        const weekNum = getIsoWeekNumber(monday);

        // Update Topbar Title
        weekTitleLabel.textContent = `Semaine ${weekNum}`;
        topbarDatePicker.value = toLocalDateStr(currentCalendarDate);

        // Days configuration
        const frenchDayNames = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
        const shortMonths = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
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
                ...activeEvents
                    .filter(ev => isSameDay(new Date(ev.start), dayDate))
                    .map(ev => ({ kind: "event", data: ev, start: new Date(ev.start), end: new Date(ev.end) })),
            ];
            assignLanes(dayItems);

            dayItems.forEach(item => {
                const startMin = item.start.getHours() * 60 + item.start.getMinutes();
                const endMin = item.end.getHours() * 60 + item.end.getMinutes();
                const durationMin = Math.max(endMin - startMin, 30);

                if (startMin < earliestCourseMinutes) {
                    earliestCourseMinutes = startMin;
                }

                const card = item.kind === "course" ? buildCourseCard(item.data) : buildEventCard(item.data);
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
    }

    function showCourseDetailModal(c) {
        document.getElementById("courseDetailTitle").textContent = c.course_name;
        const modalBody = document.getElementById("courseDetailBody");

        const roomsStr = c.rooms && c.rooms.length > 0 ? c.rooms.map(escapeHtml).join(", ") : "Non spécifiée";
        const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.map(escapeHtml).join(", ") : "Non spécifié";
        const groupsStr = c.groups && c.groups.length > 0 ? c.groups.map(escapeHtml).join(", ") : "Non spécifié";

        modalBody.innerHTML = `
            <div class="mb-3">
                <span class="badge bg-primary px-3 py-1 fs-6">${escapeHtml(c.activity_type)}</span>
                ${c.is_cancelled ? '<span class="badge bg-danger px-3 py-1 fs-6 ms-1">ANNULÉ</span>' : ''}
            </div>
            <dl class="row mb-0 small">
                <dt class="col-sm-4 text-muted">Horaires :</dt>
                <dd class="col-sm-8 fw-semibold">${formatTime(c.start)} - ${formatTime(c.end)} (${c.duration} min)</dd>

                <dt class="col-sm-4 text-muted">Date :</dt>
                <dd class="col-sm-8">${new Date(c.start).toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</dd>

                <dt class="col-sm-4 text-muted">Salle(s) :</dt>
                <dd class="col-sm-8">${roomsStr}</dd>

                <dt class="col-sm-4 text-muted">Enseignant(s) :</dt>
                <dd class="col-sm-8">${teachersStr}</dd>

                <dt class="col-sm-4 text-muted">Groupe(s) :</dt>
                <dd class="col-sm-8">👥 ${groupsStr}</dd>

                <dt class="col-sm-4 text-muted">Statut :</dt>
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
                toggleSidebarLabel.textContent = "Afficher le menu";
            }
            if (btnToggleSidebar) {
                btnToggleSidebar.title = "Afficher le menu de sélection";
                btnToggleSidebar.classList.add("btn-primary");
                btnToggleSidebar.classList.remove("btn-outline-secondary");
            }
        } else {
            appBody.classList.remove("sidebar-collapsed");
            if (toggleSidebarIcon) {
                toggleSidebarIcon.className = "bi bi-layout-sidebar-inset";
            }
            if (toggleSidebarLabel) {
                toggleSidebarLabel.textContent = "Masquer le menu";
            }
            if (btnToggleSidebar) {
                btnToggleSidebar.title = "Masquer le menu de sélection";
                btnToggleSidebar.classList.remove("btn-primary");
                btnToggleSidebar.classList.add("btn-outline-secondary");
            }
        }
        try {
            localStorage.setItem("zeus_sidebar_collapsed", collapsed ? "1" : "0");
        } catch (e) {}
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

    try {
        if (localStorage.getItem("zeus_sidebar_collapsed") === "1") {
            setSidebarCollapsed(true);
        }
    } catch (e) {}

    if (btnFindRoom && findRoomModal) {
        btnFindRoom.addEventListener("click", () => {
            findRoomModal.show();
        });
    }

    document.getElementById("quickRoomSearchForm").addEventListener("submit", async (e) => {
        e.preventDefault();
        const dateVal = document.getElementById("modalRoomDate").value;
        const startVal = document.getElementById("modalRoomStart").value;
        const endVal = document.getElementById("modalRoomEnd").value;
        const resultsEl = document.getElementById("modalRoomsResults");

        resultsEl.innerHTML = `<div class="text-center py-3"><div class="spinner-border spinner-border-sm text-primary"></div> Recherche...</div>`;

        try {
            const res = await fetch("/api/database/free-rooms", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ date: dateVal, start_time: startVal, end_time: endVal })
            });
            const data = await res.json();

            let html = `
                <div class="d-flex gap-2 mb-3">
                    <span class="badge bg-success px-3 py-2 fs-6">${data.free_count} salles libres</span>
                    <span class="badge bg-danger-subtle text-danger border border-danger px-3 py-2 fs-6">${data.occupied_count} occupées</span>
                </div>
                <div class="row g-2" style="max-height: 340px; overflow-y: auto;">
            `;

            (data.free_rooms || []).forEach(r => {
                html += `
                    <div class="col-sm-6">
                        <div class="p-2 border rounded bg-success-subtle border-success small">
                            <strong class="text-success"><i class="bi bi-door-open me-1"></i>${r.room_name}</strong>
                            <div class="text-muted" style="font-size:0.75rem;">${r.free_until}</div>
                        </div>
                    </div>
                `;
            });

            html += "</div>";
            resultsEl.innerHTML = html;
        } catch (err) {
            resultsEl.innerHTML = `<div class="text-danger small">Erreur: ${err.message}</div>`;
        }
    });

    // Generate ICS download
    if (btnGenerateIcs) {
        btnGenerateIcs.addEventListener("click", async () => {
            if (activeCourses.length === 0) {
                alert("Aucun cours sélectionné. Cochez au moins un groupe dans l'arbre pour générer l'ICS.");
                return;
            }

            try {
                btnGenerateIcs.disabled = true;
                btnGenerateIcs.innerHTML = '<span class="spinner-border spinner-border-sm me-1"></span> Génération...';

                const res = await fetch("/api/export-ics", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        courses: activeCourses,
                        calendar_name: "EpiKairos - Emploi du Temps EPITA"
                    })
                });

                if (!res.ok) throw new Error("Erreur de téléchargement");

                const blob = await res.blob();
                const url = window.URL.createObjectURL(blob);
                const a = document.createElement("a");
                a.style.display = "none";
                a.href = url;
                a.download = "epikairos_planning.ics";
                document.body.appendChild(a);
                a.click();
                window.URL.revokeObjectURL(url);
                document.body.removeChild(a);

            } catch (err) {
                alert(`Erreur : ${err.message}`);
            } finally {
                btnGenerateIcs.disabled = false;
                btnGenerateIcs.innerHTML = "Générer un ICS";
            }
        });
    }

    // Auriga Sync Modal triggers
    const userProfileBadge = document.getElementById("userProfileBadge");
    if (userProfileBadge) {
        userProfileBadge.addEventListener("click", () => {
            authSyncModal.show();
        });
    }

    const menuLoginTrigger = document.getElementById("menuLoginTrigger");
    if (menuLoginTrigger) {
        menuLoginTrigger.addEventListener("click", (e) => {
            e.preventDefault();
            authSyncModal.show();
        });
    }

    document.getElementById("modalLoginForm").addEventListener("submit", async (e) => {
        e.preventDefault();
        const username = document.getElementById("modalUsername").value.trim();
        const password = document.getElementById("modalPassword").value.trim();
        const start_date = document.getElementById("modalSyncStart").value;
        const end_date = document.getElementById("modalSyncEnd").value;
        const btnSubmit = document.getElementById("btnModalSyncSubmit");

        btnSubmit.disabled = true;
        btnSubmit.innerHTML = '<span class="spinner-border spinner-border-sm me-1"></span> Synchronisation...';

        try {
            const res = await fetch("/api/fetch-planning", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ username, password, start_date, end_date, consent: true })
            });
            const data = await res.json();
            if (!res.ok || !data.success) throw new Error(data.error || "Erreur de connexion");

            localStorage.setItem("zeus_credentials", JSON.stringify({ username, password }));
            if (userDisplayName) {
                userDisplayName.textContent = username.split("@")[0];
            }
            authSyncModal.hide();
            alert(`Synchronisation réussie ! ${data.courses.length} cours récupérés.`);
            fetchSchedule();
        } catch (err) {
            alert(`Erreur : ${err.message}`);
        } finally {
            btnSubmit.disabled = false;
            btnSubmit.innerHTML = "Synchroniser mon emploi du temps";
        }
    });

    // =========================================================================
    // 8. Sidebar tabs: Groupes / Événements
    // =========================================================================
    const tabGroupsBtn = document.getElementById("tabGroupsBtn");
    const tabEventsBtn = document.getElementById("tabEventsBtn");
    const groupsPanel = document.getElementById("groupsPanel");
    const eventsPanel = document.getElementById("eventsPanel");

    function showSidebarTab(tab) {
        const isEvents = tab === "events";
        groupsPanel.classList.toggle("d-none", isEvents);
        eventsPanel.classList.toggle("d-none", !isEvents);
        tabGroupsBtn.classList.toggle("active", !isEvents);
        tabEventsBtn.classList.toggle("active", isEvents);
        tabGroupsBtn.setAttribute("aria-selected", String(!isEvents));
        tabEventsBtn.setAttribute("aria-selected", String(isEvents));
        if (isEvents) setSidebarCollapsed(false);
    }

    tabGroupsBtn.addEventListener("click", () => showSidebarTab("groups"));
    tabEventsBtn.addEventListener("click", () => showSidebarTab("events"));
    if (window.location.hash === "#events") showSidebarTab("events");

    // =========================================================================
    // 9. Community calendars (clubs & associations)
    // =========================================================================
    const communityCalendarList = document.getElementById("communityCalendarList");
    const moderatorActions = document.getElementById("moderatorActions");

    const calendarModalEl = document.getElementById("calendarModal");
    const calendarModal = new bootstrap.Modal(calendarModalEl);
    const calendarForm = document.getElementById("calendarForm");
    const calendarFormError = document.getElementById("calendarFormError");
    const calendarNameInput = document.getElementById("calendarNameInput");
    const calendarDescriptionInput = document.getElementById("calendarDescriptionInput");
    const calendarColorSwatches = document.getElementById("calendarColorSwatches");
    const btnDeleteCalendar = document.getElementById("btnDeleteCalendar");
    let editingCalendar = null;

    const eventModalEl = document.getElementById("eventModal");
    const eventModal = new bootstrap.Modal(eventModalEl);
    const eventForm = document.getElementById("eventForm");
    const eventFormError = document.getElementById("eventFormError");
    const eventCalendarSelect = document.getElementById("eventCalendarSelect");
    const eventTitleInput = document.getElementById("eventTitleInput");
    const eventDateInput = document.getElementById("eventDateInput");
    const eventStartInput = document.getElementById("eventStartInput");
    const eventEndInput = document.getElementById("eventEndInput");
    const eventLocationInput = document.getElementById("eventLocationInput");
    const eventDescriptionInput = document.getElementById("eventDescriptionInput");
    const btnDeleteEvent = document.getElementById("btnDeleteEvent");
    let editingEvent = null;

    function saveSelectedCalendars() {
        try {
            localStorage.setItem(CALENDAR_STORAGE_KEY, JSON.stringify(Array.from(selectedCalendarIds)));
        } catch (e) {}
    }

    async function loadCommunityCalendars() {
        try {
            const data = await apiJson("/api/community/calendars");
            communityCalendars = data.calendars || [];
            canCreateCalendars = !!data.can_create;
        } catch (err) {
            communityCalendarList.innerHTML = `<div class="text-danger small p-2">${escapeHtml(err.message)}</div>`;
            return;
        }

        const knownIds = new Set(communityCalendars.map(c => c.id));
        let stored = null;
        try {
            stored = JSON.parse(localStorage.getItem(CALENDAR_STORAGE_KEY) || "null");
        } catch (e) {}
        // First visit: show every calendar so students discover club events
        selectedCalendarIds = Array.isArray(stored)
            ? new Set(stored.filter(id => knownIds.has(id)))
            : new Set(knownIds);

        moderatorActions.classList.toggle("d-none", !canCreateCalendars);
        renderCalendarList();
    }

    function renderCalendarList() {
        if (communityCalendars.length === 0) {
            communityCalendarList.innerHTML = `<div class="small text-muted py-2">${canCreateCalendars ? "Aucun calendrier. Créez le premier ci-dessous." : "Aucun calendrier de club pour le moment."}</div>`;
            return;
        }
        communityCalendarList.innerHTML = "";
        communityCalendars.forEach(cal => {
            const row = document.createElement("label");
            row.className = "calendar-row";
            row.innerHTML = `
                <input type="checkbox" class="tree-checkbox" ${selectedCalendarIds.has(cal.id) ? "checked" : ""}>
                <span class="calendar-dot"></span>
                <span class="calendar-row-text">
                    <span class="calendar-row-name">${escapeHtml(cal.name)}</span>
                    <span class="calendar-row-meta">${cal.event_count} événement${cal.event_count > 1 ? "s" : ""} · ${escapeHtml(cal.owner)}</span>
                </span>
                ${cal.can_edit ? `<button type="button" class="btn btn-sm btn-link icon-btn calendar-edit-btn" title="Modifier" aria-label="Modifier ${escapeHtml(cal.name)}"><i class="bi bi-pencil"></i></button>` : ""}
            `;
            row.querySelector(".calendar-dot").style.setProperty("--dot", cal.color);
            row.querySelector("input").addEventListener("change", (e) => {
                if (e.target.checked) selectedCalendarIds.add(cal.id);
                else selectedCalendarIds.delete(cal.id);
                saveSelectedCalendars();
                fetchSchedule();
            });
            const editBtn = row.querySelector(".calendar-edit-btn");
            if (editBtn) {
                editBtn.addEventListener("click", (e) => {
                    e.preventDefault();
                    openCalendarModal(cal);
                });
            }
            communityCalendarList.appendChild(row);
        });
    }

    // --- Calendar modal ------------------------------------------------------
    function renderColorSwatches(selected) {
        calendarColorSwatches.innerHTML = "";
        CALENDAR_COLORS.forEach(color => {
            const id = `swatch_${color.slice(1)}`;
            const wrapper = document.createElement("span");
            wrapper.innerHTML = `
                <input type="radio" class="btn-check" name="calendarColor" id="${id}" value="${color}" ${color === selected ? "checked" : ""}>
                <label class="color-swatch" for="${id}" title="${color}"></label>
            `;
            wrapper.querySelector("label").style.setProperty("--dot", color);
            calendarColorSwatches.appendChild(wrapper);
        });
    }

    function openCalendarModal(cal) {
        editingCalendar = cal || null;
        calendarFormError.classList.add("d-none");
        document.getElementById("calendarModalTitle").textContent = cal ? "Modifier le calendrier" : "Nouveau calendrier";
        calendarNameInput.value = cal ? cal.name : "";
        calendarDescriptionInput.value = cal ? cal.description : "";
        const usedColors = new Set(communityCalendars.map(c => c.color));
        renderColorSwatches(cal ? cal.color : (CALENDAR_COLORS.find(c => !usedColors.has(c)) || CALENDAR_COLORS[0]));
        btnDeleteCalendar.classList.toggle("d-none", !cal);
        calendarModal.show();
    }

    document.getElementById("btnNewCalendar").addEventListener("click", () => openCalendarModal(null));

    calendarForm.addEventListener("submit", async (e) => {
        e.preventDefault();
        const color = (calendarForm.querySelector("input[name=calendarColor]:checked") || {}).value || CALENDAR_COLORS[0];
        const body = JSON.stringify({
            name: calendarNameInput.value,
            description: calendarDescriptionInput.value,
            color,
        });
        try {
            if (editingCalendar) {
                await apiJson(`/api/community/calendars/${editingCalendar.id}`, { method: "PUT", body });
            } else {
                const data = await apiJson("/api/community/calendars", { method: "POST", body });
                selectedCalendarIds.add(data.id);
                saveSelectedCalendars();
            }
            calendarModal.hide();
            await loadCommunityCalendars();
            fetchSchedule();
        } catch (err) {
            calendarFormError.textContent = err.message;
            calendarFormError.classList.remove("d-none");
        }
    });

    btnDeleteCalendar.addEventListener("click", async () => {
        if (!editingCalendar) return;
        if (!confirm(`Supprimer « ${editingCalendar.name} » et ses ${editingCalendar.event_count} événement(s) ?`)) return;
        try {
            await apiJson(`/api/community/calendars/${editingCalendar.id}`, { method: "DELETE" });
            calendarModal.hide();
            await loadCommunityCalendars();
            fetchSchedule();
        } catch (err) {
            calendarFormError.textContent = err.message;
            calendarFormError.classList.remove("d-none");
        }
    });

    // --- Event modal ---------------------------------------------------------
    function openEventModal(ev) {
        const editable = communityCalendars.filter(c => c.can_edit);
        if (editable.length === 0) {
            alert("Créez d'abord un calendrier pour y ajouter des événements.");
            openCalendarModal(null);
            return;
        }
        editingEvent = ev || null;
        eventFormError.classList.add("d-none");
        document.getElementById("eventModalTitle").textContent = ev ? "Modifier l'événement" : "Nouvel événement";

        eventCalendarSelect.innerHTML = editable
            .map(c => `<option value="${c.id}">${escapeHtml(c.name)}</option>`)
            .join("");

        if (ev) {
            eventCalendarSelect.value = String(ev.calendar_id);
            eventTitleInput.value = ev.title;
            eventDateInput.value = ev.start_local.slice(0, 10);
            eventStartInput.value = ev.start_local.slice(11, 16);
            eventEndInput.value = ev.end_local.slice(11, 16);
            eventLocationInput.value = ev.location;
            eventDescriptionInput.value = ev.description;
        } else {
            const today = new Date();
            const inWeek = getMonday(currentCalendarDate) <= today && today < new Date(getMonday(currentCalendarDate).getTime() + 7 * 864e5);
            eventTitleInput.value = "";
            eventDateInput.value = toLocalDateStr(inWeek ? today : currentCalendarDate);
            eventStartInput.value = "18:00";
            eventEndInput.value = "20:00";
            eventLocationInput.value = "";
            eventDescriptionInput.value = "";
        }
        btnDeleteEvent.classList.toggle("d-none", !ev);
        eventModal.show();
    }

    document.getElementById("btnNewEvent").addEventListener("click", () => openEventModal(null));

    eventForm.addEventListener("submit", async (e) => {
        e.preventDefault();
        const calendarId = Number(eventCalendarSelect.value);
        const date = eventDateInput.value;
        const body = JSON.stringify({
            calendar_id: calendarId,
            title: eventTitleInput.value,
            start: `${date}T${eventStartInput.value}`,
            end: `${date}T${eventEndInput.value}`,
            location: eventLocationInput.value,
            description: eventDescriptionInput.value,
        });
        try {
            if (editingEvent) {
                await apiJson(`/api/community/events/${editingEvent.id}`, { method: "PUT", body });
            } else {
                await apiJson(`/api/community/calendars/${calendarId}/events`, { method: "POST", body });
            }
            eventModal.hide();
            // Make sure the moderator sees what they just published
            if (!selectedCalendarIds.has(calendarId)) {
                selectedCalendarIds.add(calendarId);
                saveSelectedCalendars();
            }
            currentCalendarDate = new Date(`${date}T12:00`);
            await loadCommunityCalendars();
            fetchSchedule();
        } catch (err) {
            eventFormError.textContent = err.message;
            eventFormError.classList.remove("d-none");
        }
    });

    btnDeleteEvent.addEventListener("click", async () => {
        if (!editingEvent) return;
        if (!confirm(`Supprimer « ${editingEvent.title} » ?`)) return;
        try {
            await apiJson(`/api/community/events/${editingEvent.id}`, { method: "DELETE" });
            eventModal.hide();
            await loadCommunityCalendars();
            fetchSchedule();
        } catch (err) {
            eventFormError.textContent = err.message;
            eventFormError.classList.remove("d-none");
        }
    });

    function showEventDetailModal(ev) {
        document.getElementById("courseDetailTitle").textContent = ev.title;
        document.getElementById("courseDetailBody").innerHTML = `
            <div class="mb-3 d-flex align-items-center gap-2">
                <span class="calendar-dot" style="--dot: ${escapeHtml(ev.color)}"></span>
                <span class="fw-semibold">${escapeHtml(ev.calendar_name)}</span>
            </div>
            <dl class="row mb-0 small">
                <dt class="col-sm-4 text-muted">Date</dt>
                <dd class="col-sm-8">${new Date(ev.start).toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long" })}</dd>
                <dt class="col-sm-4 text-muted">Horaires</dt>
                <dd class="col-sm-8">${formatTime(ev.start)} - ${formatTime(ev.end)}</dd>
                ${ev.location ? `<dt class="col-sm-4 text-muted">Lieu</dt><dd class="col-sm-8">${escapeHtml(ev.location)}</dd>` : ""}
            </dl>
            ${ev.description ? `<p class="small mt-3 mb-0" style="white-space: pre-line;">${escapeHtml(ev.description)}</p>` : ""}
        `;
        courseDetailModal.show();
    }

    // Initial Load
    loadCommunityCalendars().then(() => fetchSchedule());
    loadGroupTree();

});
