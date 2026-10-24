/**
 * EpiKairos - Free Rooms Finder & Live Classroom Search
 */

document.addEventListener("DOMContentLoaded", () => {
    const roomsRoot = document.getElementById("roomsRoot");
    const roomSearchForm = document.getElementById("roomSearchForm");
    const roomDateInput = document.getElementById("roomDateInput");
    const roomStartTimeInput = document.getElementById("roomStartTimeInput");
    const roomEndTimeInput = document.getElementById("roomEndTimeInput");
    const nowBtn = document.getElementById("nowBtn");
    const heroInstantNowBtn = document.getElementById("heroInstantNowBtn");
    const refreshBtn = document.getElementById("refreshBtn");
    const roomFilterInput = document.getElementById("roomFilterInput");
    const searchRoomsBtn = document.getElementById("searchRoomsBtn");

    const resultsSummaryBar = document.getElementById("resultsSummaryBar");
    const freeRoomsBadge = document.getElementById("freeRoomsBadge");
    const occupiedRoomsBadge = document.getElementById("occupiedRoomsBadge");
    const searchIntervalLabel = document.getElementById("searchIntervalLabel");
    const freeFilteredCount = document.getElementById("freeFilteredCount");
    const occupiedFilteredCount = document.getElementById("occupiedFilteredCount");
    const roomsLoading = document.getElementById("roomsLoading");
    const roomsInitialPrompt = document.getElementById("roomsInitialPrompt");

    const freeRoomsSection = document.getElementById("freeRoomsSection");
    const freeRoomsGrid = document.getElementById("freeRoomsGrid");
    const occupiedRoomsSection = document.getElementById("occupiedRoomsSection");
    const occupiedRoomsGrid = document.getElementById("occupiedRoomsGrid");

    let lastData = null;

    function escapeHtml(str) {
        if (!str) return "";
        return String(str)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
            .replace(/'/g, "&#039;");
    }

    function setNow() {
        const now = new Date();
        const year = now.getFullYear();
        const month = String(now.getMonth() + 1).padStart(2, "0");
        const day = String(now.getDate()).padStart(2, "0");
        roomDateInput.value = `${year}-${month}-${day}`;

        const startH = String(now.getHours()).padStart(2, "0");
        const startM = String(now.getMinutes()).padStart(2, "0");

        const endD = new Date(now.getTime() + 2 * 60 * 60 * 1000);
        const endH = String(endD.getHours()).padStart(2, "0");
        const endM = String(endD.getMinutes()).padStart(2, "0");

        roomStartTimeInput.value = `${startH}:${startM}`;
        roomEndTimeInput.value = `${endH}:${endM}`;
    }

    if (nowBtn) {
        nowBtn.addEventListener("click", () => {
            setNow();
            executeSearch();
        });
    }

    if (heroInstantNowBtn) {
        heroInstantNowBtn.addEventListener("click", () => {
            setNow();
            executeSearch();
        });
    }

    if (refreshBtn) {
        refreshBtn.addEventListener("click", () => {
            executeSearch();
        });
    }

    // Preset buttons (Matinée, Midi, Après-midi, Soirée)
    document.querySelectorAll(".slot-preset-btn").forEach(btn => {
        btn.addEventListener("click", () => {
            roomStartTimeInput.value = btn.dataset.start;
            roomEndTimeInput.value = btn.dataset.end;
            executeSearch();
        });
    });

    // Real-time client-side filter
    if (roomFilterInput) {
        roomFilterInput.addEventListener("input", () => {
            if (lastData) {
                renderCards(lastData, roomFilterInput.value.trim().toLowerCase());
            }
        });
    }

    async function executeSearch() {
        const date = roomDateInput.value;
        const startTime = roomStartTimeInput.value;
        const endTime = roomEndTimeInput.value;

        if (!date || !startTime || !endTime) {
            alert("Veuillez renseigner la date et les horaires.");
            return;
        }

        if (startTime >= endTime) {
            alert("L'heure de début doit être antérieure à l'heure de fin.");
            return;
        }

        roomsLoading.classList.remove("d-none");
        roomsInitialPrompt.classList.add("d-none");
        freeRoomsSection.classList.add("d-none");
        occupiedRoomsSection.classList.add("d-none");
        resultsSummaryBar.classList.add("d-none");

        try {
            const res = await fetch("/api/database/free-rooms", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    date: date,
                    start_time: startTime,
                    end_time: endTime,
                })
            });

            const data = await res.json();
            if (!res.ok) throw new Error(data.error || "Erreur de recherche");

            lastData = data;
            const filterQuery = roomFilterInput ? roomFilterInput.value.trim().toLowerCase() : "";
            renderResults(data, filterQuery);

        } catch (err) {
            alert(`Erreur : ${err.message}`);
        } finally {
            roomsLoading.classList.add("d-none");
        }
    }

    roomSearchForm.addEventListener("submit", (e) => {
        e.preventDefault();
        executeSearch();
    });

    function calculateRemainingBadge(nextCourse, searchEndTime) {
        if (!nextCourse || !nextCourse.start) {
            return `<span class="room-until">Libre jusqu'au soir</span>`;
        }
        return `<span class="room-until">Libre jusqu'à ${escapeHtml(nextCourse.start)}</span>`;
    }

    function renderCards(data, filterQuery = "") {
        const freeRooms = (data.free_rooms || []).filter(r => {
            if (!filterQuery) return true;
            return (r.room_name || "").toLowerCase().includes(filterQuery);
        });

        const occupiedRooms = (data.occupied_rooms || []).filter(r => {
            if (!filterQuery) return true;
            const roomMatch = (r.room_name || "").toLowerCase().includes(filterQuery);
            const courseTitle = ((r.current_course && r.current_course.title) || "").toLowerCase();
            return roomMatch || courseTitle.includes(filterQuery);
        });

        // Free Rooms
        freeRoomsGrid.innerHTML = "";
        if (freeRooms.length > 0) {
            freeRoomsSection.classList.remove("d-none");
            if (freeFilteredCount) {
                freeFilteredCount.textContent = `${freeRooms.length} affichée(s)`;
            }

            freeRooms.forEach(r => {
                const col = document.createElement("div");
                col.className = "col-md-6 col-lg-4";

                const remainingBadge = calculateRemainingBadge(r.next_course, data.end_time);

                const nextInfo = r.next_course
                    ? `<div class="room-meta">Prochain cours : ${escapeHtml(r.next_course.title)} (${escapeHtml(r.next_course.start)} - ${escapeHtml(r.next_course.end)})</div>`
                    : '<div class="room-meta">Aucun autre cours aujourd\'hui</div>';

                col.innerHTML = `
                    <div class="room-card is-free">
                        <div class="d-flex align-items-center justify-content-between gap-2">
                            <span class="room-name"><span class="room-dot"></span>${escapeHtml(r.room_name)}</span>
                            ${remainingBadge}
                        </div>
                        ${nextInfo}
                    </div>
                `;
                freeRoomsGrid.appendChild(col);
            });
        } else {
            freeRoomsSection.classList.remove("d-none");
            if (freeFilteredCount) {
                freeFilteredCount.textContent = "0 salle";
            }
            freeRoomsGrid.innerHTML = `
                <div class="col-12 text-center py-4 text-muted small">
                    ${filterQuery ? "Aucune salle libre ne correspond à votre filtre." : "Aucune salle libre répertoriée sur ce créneau."}
                </div>
            `;
        }

        // Occupied Rooms
        occupiedRoomsGrid.innerHTML = "";
        if (occupiedRooms.length > 0) {
            occupiedRoomsSection.classList.remove("d-none");
            if (occupiedFilteredCount) {
                occupiedFilteredCount.textContent = `${occupiedRooms.length} affichée(s)`;
            }

            occupiedRooms.forEach(r => {
                const col = document.createElement("div");
                col.className = "col-md-6 col-lg-4";

                const c = r.current_course || {};
                const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.join(", ") : "-";
                const groupsStr = c.groups && c.groups.length > 0 ? c.groups.join(", ") : "-";

                col.innerHTML = `
                    <div class="room-card is-busy">
                        <div class="d-flex align-items-center justify-content-between gap-2">
                            <span class="room-name"><span class="room-dot"></span>${escapeHtml(r.room_name)}</span>
                            <span class="room-until">Jusqu'à ${escapeHtml(c.end || "-")}</span>
                        </div>
                        <div class="room-meta text-body">${escapeHtml(c.title || "Cours")}</div>
                        <div class="room-meta">${escapeHtml(teachersStr)} · ${escapeHtml(groupsStr)}</div>
                    </div>
                `;
                occupiedRoomsGrid.appendChild(col);
            });
        } else {
            occupiedRoomsSection.classList.add("d-none");
        }
    }

    function renderResults(data, filterQuery = "") {
        resultsSummaryBar.classList.remove("d-none");
        freeRoomsBadge.textContent = `${data.free_count} salles libres`;
        occupiedRoomsBadge.textContent = `${data.occupied_count} salles occupées`;
        searchIntervalLabel.textContent = `Créneau : ${data.date} de ${data.start_time} à ${data.end_time}`;

        renderCards(data, filterQuery);
    }

    // Check if ?now=1 was passed or page opened with is_now
    const urlParams = new URLSearchParams(window.location.search);
    const isNowRequested = urlParams.get("now") === "1" || (roomsRoot && roomsRoot.dataset.isNow === "1");

    if (isNowRequested) {
        setNow();
    }

    // Auto-search on page load
    executeSearch();
});
