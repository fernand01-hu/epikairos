/**
 * Zeus Auriga - Free Rooms Finder
 */

document.addEventListener("DOMContentLoaded", () => {
    const roomSearchForm = document.getElementById("roomSearchForm");
    const roomDateInput = document.getElementById("roomDateInput");
    const roomStartTimeInput = document.getElementById("roomStartTimeInput");
    const roomEndTimeInput = document.getElementById("roomEndTimeInput");
    const nowBtn = document.getElementById("nowBtn");
    const searchRoomsBtn = document.getElementById("searchRoomsBtn");

    const resultsSummaryBar = document.getElementById("resultsSummaryBar");
    const freeRoomsBadge = document.getElementById("freeRoomsBadge");
    const occupiedRoomsBadge = document.getElementById("occupiedRoomsBadge");
    const searchIntervalLabel = document.getElementById("searchIntervalLabel");
    const roomsLoading = document.getElementById("roomsLoading");
    const roomsInitialPrompt = document.getElementById("roomsInitialPrompt");

    const freeRoomsSection = document.getElementById("freeRoomsSection");
    const freeRoomsGrid = document.getElementById("freeRoomsGrid");
    const occupiedRoomsSection = document.getElementById("occupiedRoomsSection");
    const occupiedRoomsGrid = document.getElementById("occupiedRoomsGrid");

    function setNow() {
        const now = new Date();
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

    // Preset buttons
    document.querySelectorAll(".slot-preset-btn").forEach(btn => {
        btn.addEventListener("click", () => {
            roomStartTimeInput.value = btn.dataset.start;
            roomEndTimeInput.value = btn.dataset.end;
            executeSearch();
        });
    });

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

            renderResults(data);

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

    function renderResults(data) {
        resultsSummaryBar.classList.remove("d-none");
        freeRoomsBadge.textContent = `${data.free_count} salles libres`;
        occupiedRoomsBadge.textContent = `${data.occupied_count} salles occupées`;
        searchIntervalLabel.textContent = `Créneau : ${data.date} de ${data.start_time} à ${data.end_time}`;

        // Render Free Rooms
        freeRoomsGrid.innerHTML = "";
        if (data.free_rooms && data.free_rooms.length > 0) {
            freeRoomsSection.classList.remove("d-none");

            data.free_rooms.forEach(r => {
                const col = document.createElement("div");
                col.className = "col-md-6 col-lg-4";

                const nextInfo = r.next_course
                    ? `<div class="small text-muted mt-2 border-top pt-2">
                         <i class="bi bi-clock me-1 text-primary"></i> Prochain cours : <strong>${r.next_course.title}</strong> (${r.next_course.start} - ${r.next_course.end})
                       </div>`
                    : '<div class="small text-muted mt-2 border-top pt-2"><i class="bi bi-check2-all text-success me-1"></i> Libre jusqu\'à la fin de la journée</div>';

                col.innerHTML = `
                    <div class="card card-custom p-3 h-100 border-start border-4 border-success shadow-sm">
                        <div class="d-flex align-items-center justify-content-between mb-2">
                            <span class="badge bg-success-subtle text-success border border-success fw-bold px-2 py-1">
                                <i class="bi bi-check-circle me-1"></i> LIBRE
                            </span>
                            <span class="badge bg-light text-dark border small">
                                ${r.free_until}
                            </span>
                        </div>
                        <h6 class="fw-bold mb-1 text-dark d-flex align-items-center gap-2">
                            <i class="bi bi-door-open-fill text-success"></i>
                            ${r.room_name}
                        </h6>
                        ${nextInfo}
                    </div>
                `;
                freeRoomsGrid.appendChild(col);
            });
        } else {
            freeRoomsSection.classList.remove("d-none");
            freeRoomsGrid.innerHTML = `
                <div class="col-12 text-center py-4 text-muted small">
                    <i class="bi bi-emoji-frown fs-4 d-block mb-1"></i>
                    Aucune salle libre répertoriée sur ce créneau.
                </div>
            `;
        }

        // Render Occupied Rooms
        occupiedRoomsGrid.innerHTML = "";
        if (data.occupied_rooms && data.occupied_rooms.length > 0) {
            occupiedRoomsSection.classList.remove("d-none");

            data.occupied_rooms.forEach(r => {
                const col = document.createElement("div");
                col.className = "col-md-6 col-lg-4";

                const c = r.current_course || {};
                const teachersStr = c.teachers && c.teachers.length > 0 ? c.teachers.join(", ") : "-";
                const groupsStr = c.groups && c.groups.length > 0 ? c.groups.join(", ") : "-";

                col.innerHTML = `
                    <div class="card card-custom p-3 h-100 border-start border-4 border-danger shadow-sm opacity-90">
                        <div class="d-flex align-items-center justify-content-between mb-2">
                            <span class="badge bg-danger-subtle text-danger border border-danger fw-bold px-2 py-1">
                                <i class="bi bi-x-circle me-1"></i> OCCUPÉE
                            </span>
                            <span class="badge bg-light text-muted border small">
                                Jusqu'à ${c.end || "-"}
                            </span>
                        </div>
                        <h6 class="fw-bold mb-1 text-dark d-flex align-items-center gap-2">
                            <i class="bi bi-door-closed-fill text-danger"></i>
                            ${r.room_name}
                        </h6>
                        <div class="small fw-semibold text-navy mt-1">${c.title || "Cours"}</div>
                        <div class="small text-muted mt-2 border-top pt-2">
                            <div><i class="bi bi-person me-1"></i> ${teachersStr}</div>
                            <div><i class="bi bi-people me-1"></i> ${groupsStr}</div>
                        </div>
                    </div>
                `;
                occupiedRoomsGrid.appendChild(col);
            });
        }
    }

    // Auto-search on page load
    executeSearch();

});
