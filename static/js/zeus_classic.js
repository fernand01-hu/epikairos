/**
 * Zeus Classic UI Client Script - EPITA / 3IE Replica
 * Controls Group Treeview, Hourly Timetable Grid, Room Search, and ICS Generation
 */

let groupTreeData = null;
let selectedGroupPatterns = new Set(["S3S4 L3", "S3 Lyon L3", "2627 - S3 Lyon L3"]);
let currentCalendarDate = new Date();
let currentViewMode = "week"; // "week" | "day"
let activeCourses = [];

const HOUR_HEIGHT = 52; // Must match CSS --zeus-hour-height

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
    const findRoomModal = new bootstrap.Modal(document.getElementById("findRoomModal"));
    const courseDetailModal = new bootstrap.Modal(document.getElementById("courseDetailModal"));
    const authSyncModal = new bootstrap.Modal(document.getElementById("authSyncModal"));

    // User profile display
    const userDisplayName = document.getElementById("userDisplayName");
    const savedCreds = localStorage.getItem("zeus_credentials");
    if (savedCreds) {
        try {
            const parsed = JSON.parse(savedCreds);
            if (parsed.username) {
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
    async function fetchSchedule() {
        const patterns = Array.from(selectedGroupPatterns);
        if (patterns.length === 0) {
            activeCourses = [];
            renderTimetableGrid();
            return;
        }

        const monday = getMonday(currentCalendarDate);
        const sunday = new Date(monday);
        sunday.setDate(monday.getDate() + 6);

        const startDateStr = monday.toISOString().slice(0, 10);
        const endDateStr = sunday.toISOString().slice(0, 10);

        try {
            const res = await fetch("/api/groups/schedule", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    patterns: patterns,
                    start_date: startDateStr,
                    end_date: endDateStr
                })
            });

            const data = await res.json();
            if (data.success) {
                activeCourses = data.courses || [];
            }
        } catch (e) {
            console.error("Error fetching group schedule", e);
        }

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

    function renderTimetableGrid() {
        const monday = getMonday(currentCalendarDate);
        const weekNum = getIsoWeekNumber(monday);

        // Update Topbar Title
        weekTitleLabel.textContent = `Semaine ${weekNum}`;
        topbarDatePicker.value = currentCalendarDate.toISOString().slice(0, 10);

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
            dayCol.dataset.date = dayDate.toISOString().slice(0, 10);

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
            const dayCourses = activeCourses.filter(c => isSameDay(new Date(c.start), dayDate));

            dayCourses.forEach(c => {
                const dStart = new Date(c.start);
                const dEnd = new Date(c.end);

                const startMin = dStart.getHours() * 60 + dStart.getMinutes();
                const endMin = dEnd.getHours() * 60 + dEnd.getMinutes();
                const durationMin = Math.max(endMin - startMin, 30);

                if (startMin < earliestCourseMinutes) {
                    earliestCourseMinutes = startMin;
                }

                const cardTop = (startMin / 60) * HOUR_HEIGHT;
                const cardHeight = Math.max((durationMin / 60) * HOUR_HEIGHT - 2, 24);

                const card = document.createElement("div");
                card.className = `zeus-event-card ${getTypeCardClass(c.activity_type)}`;
                card.style.top = `${cardTop}px`;
                card.style.height = `${cardHeight}px`;

                const roomsStr = c.rooms && c.rooms.length > 0 ? c.rooms.join(", ") : "";
                const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.join(", ") : "";

                card.innerHTML = `
                    <div class="zeus-event-title" title="${c.course_name}">${c.course_name}</div>
                    <div class="zeus-event-meta">
                        ${roomsStr ? `<span>📍 ${roomsStr}</span><br>` : ""}
                        ${teachersStr ? `<span>👤 ${teachersStr}</span><br>` : ""}
                        <span>🕐 ${formatTime(c.start)} - ${formatTime(c.end)}</span>
                    </div>
                `;

                // Click to view details
                card.addEventListener("click", () => {
                    showCourseDetailModal(c);
                });

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

        const roomsStr = c.rooms && c.rooms.length > 0 ? c.rooms.join(", ") : "Non spécifiée";
        const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.join(", ") : "Non spécifié";
        const groupsStr = c.groups && c.groups.length > 0 ? c.groups.join(", ") : "Non spécifié";

        modalBody.innerHTML = `
            <div class="mb-3">
                <span class="badge bg-primary px-3 py-1 fs-6">${c.activity_type}</span>
                ${c.is_cancelled ? '<span class="badge bg-danger px-3 py-1 fs-6 ms-1">ANNULÉ</span>' : ''}
            </div>
            <dl class="row mb-0 small">
                <dt class="col-sm-4 text-muted">Horaires :</dt>
                <dd class="col-sm-8 fw-semibold">${formatTime(c.start)} - ${formatTime(c.end)} (${c.duration} min)</dd>

                <dt class="col-sm-4 text-muted">Date :</dt>
                <dd class="col-sm-8">${new Date(c.start).toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</dd>

                <dt class="col-sm-4 text-muted">Salle(s) :</dt>
                <dd class="col-sm-8 fw-bold text-danger">📍 ${roomsStr}</dd>

                <dt class="col-sm-4 text-muted">Enseignant(s) :</dt>
                <dd class="col-sm-8 text-primary">👤 ${teachersStr}</dd>

                <dt class="col-sm-4 text-muted">Groupe(s) :</dt>
                <dd class="col-sm-8">👥 ${groupsStr}</dd>

                <dt class="col-sm-4 text-muted">Statut :</dt>
                <dd class="col-sm-8"><span class="badge bg-light text-dark border">${c.status || "PLANIFIE"}</span></dd>
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
    // 7. Modals: Trouver une salle & Sync
    // =========================================================================
    btnFindRoom.addEventListener("click", () => {
        findRoomModal.show();
    });

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

    // Initial Load
    loadGroupTree();

});
