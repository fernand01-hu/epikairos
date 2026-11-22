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
    const roomsLoading = document.getElementById("roomsLoading");
    const roomsInitialPrompt = document.getElementById("roomsInitialPrompt");


    const floorPlanSection = document.getElementById("floorPlanSection");
    const floorPlanContainer = document.getElementById("floorPlanContainer");
    const floorPlanDetails = document.getElementById("floorPlanDetails");
    const offPlanCard = document.getElementById("offPlanCard");
    const offPlanList = document.getElementById("offPlanList");

    let lastData = null;
    let selectedRoom = null;

    function currentFilter() {
        return roomFilterInput ? roomFilterInput.value.trim().toLowerCase() : "";
    }

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
                renderView(lastData, currentFilter());
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
        resultsSummaryBar.classList.add("d-none");
        floorPlanSection.classList.add("d-none");

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

    function renderResults(data, filterQuery = "") {
        resultsSummaryBar.classList.remove("d-none");
        freeRoomsBadge.textContent = `${data.free_count} salles libres`;
        occupiedRoomsBadge.textContent = `${data.occupied_count} salles occupées`;
        searchIntervalLabel.textContent = `Créneau : ${data.date} de ${data.start_time} à ${data.end_time}`;

        renderView(data, filterQuery);
    }

    function renderView(data, filterQuery = "") {
        floorPlanSection.classList.remove("d-none");
        renderPlan(data, filterQuery);
    }

    // -------------------------------------------------------------------------
    // Floor plan view
    // -------------------------------------------------------------------------
    function findEntry(data, number) {
        const match = r => window.FloorPlan.roomNumber(r.room_name) === number;
        const free = (data.free_rooms || []).find(match);
        if (free) return { status: "free", room: free };
        const busy = (data.occupied_rooms || []).find(match);
        if (busy) return { status: "busy", room: busy };
        return null;
    }

    function renderPlan(data, filterQuery) {
        window.FloorPlan.render(floorPlanContainer, data, {
            filter: filterQuery,
            selected: selectedRoom,
            onSelect: (def) => {
                selectedRoom = def.id;
                renderPlan(data, currentFilter());
                const g = floorPlanContainer.querySelector(`[data-room="${def.id}"]`);
                if (g) g.focus({ preventScroll: true });
            },
        });
        renderDetails(data);
        renderOffPlan(data, filterQuery);
    }

    function renderDetails(data) {
        if (!selectedRoom) {
            floorPlanDetails.innerHTML = '<div class="small text-muted">Cliquez sur une salle pour voir son occupation.</div>';
            return;
        }
        const entry = findEntry(data, selectedRoom);
        if (!entry) {
            floorPlanDetails.innerHTML = `
                <div class="fp-detail-title"><span class="room-dot is-unknown"></span>Salle ${escapeHtml(selectedRoom)}</div>
                <div class="small text-muted mt-1">Aucun cours de cette salle n'est connu d'EpiKairos. Elle apparaîtra dès qu'un étudiant aura synchronisé un planning qui l'utilise.</div>`;
            return;
        }
        const r = entry.room;
        if (entry.status === "free") {
            const until = r.next_course && r.next_course.start ? `jusqu'à ${escapeHtml(r.next_course.start)}` : "jusqu'au soir";
            const next = r.next_course
                ? `<div class="fp-detail-row"><span class="text-muted">Prochain cours</span><span>${escapeHtml(r.next_course.title)}</span><span class="text-muted">${escapeHtml(r.next_course.start)} – ${escapeHtml(r.next_course.end)}</span></div>`
                : '<div class="small text-muted mt-2">Aucun autre cours ce jour-là.</div>';
            floorPlanDetails.innerHTML = `
                <div class="fp-detail-title"><span class="room-dot"></span>${escapeHtml(r.room_name)}</div>
                <div class="fp-detail-status is-free">Libre ${until}</div>
                ${next}`;
        } else {
            const c = r.current_course || {};
            const teachers = c.teachers && c.teachers.length ? c.teachers.join(", ") : "—";
            const groups = c.groups && c.groups.length ? c.groups.join(", ") : "—";
            floorPlanDetails.innerHTML = `
                <div class="fp-detail-title"><span class="room-dot is-busy"></span>${escapeHtml(r.room_name)}</div>
                <div class="fp-detail-status is-busy">Occupée ${c.start ? `de ${escapeHtml(c.start)} ` : ""}jusqu'à ${escapeHtml(c.end || "—")}</div>
                <div class="fp-detail-row"><span class="text-muted">Cours</span><span>${escapeHtml(c.title || "Cours")}</span></div>
                <div class="fp-detail-row"><span class="text-muted">Enseignants</span><span>${escapeHtml(teachers)}</span></div>
                <div class="fp-detail-row"><span class="text-muted">Groupes</span><span>${escapeHtml(groups)}</span></div>`;
        }
    }

    function renderOffPlan(data, filterQuery) {
        const rooms = window.FloorPlan.offPlanRooms(data)
            .filter(e => !filterQuery || (e.room.room_name || "").toLowerCase().includes(filterQuery));
        offPlanCard.classList.toggle("d-none", rooms.length === 0);
        offPlanList.innerHTML = rooms.map(e => {
            const hint = e.status === "free"
                ? (e.room.next_course ? `Libre jusqu'à ${e.room.next_course.start}` : "Libre jusqu'au soir")
                : `Occupée jusqu'à ${(e.room.current_course || {}).end || "—"}`;
            return `<span class="fp-chip is-${e.status}" title="${escapeHtml(hint)}"><span class="room-dot${e.status === "busy" ? " is-busy" : ""}"></span>${escapeHtml(e.room.room_name)}</span>`;
        }).join("");
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
