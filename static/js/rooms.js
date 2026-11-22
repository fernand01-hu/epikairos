/**
 * EpiKairos - Salles libres: floor plan with each room's status for a day.
 *
 * Status compares the room's courses with the current time of day:
 *   busy  a course is running now
 *   soon  free, but a course starts in less than an hour
 *   free  otherwise
 * Picking another day keeps the current time of day as the reference.
 */

document.addEventListener("DOMContentLoaded", () => {
    const planContainer = document.getElementById("floorPlanContainer");
    const details = document.getElementById("floorPlanDetails");
    const dayBtn = document.getElementById("dayBtn");
    const dayInput = document.getElementById("dayInput");
    const dayClock = document.getElementById("dayClock");
    const offPlanCard = document.getElementById("offPlanCard");
    const offPlanList = document.getElementById("offPlanList");

    const SOON_MINUTES = 60;
    let rooms = [];          // [{ name, courses: [{title,type,start,end,teachers,groups}] }]
    let selectedRoom = null; // "301"
    let loadedDate = null;

    function escapeHtml(str) {
        if (str === null || str === undefined) return "";
        return String(str)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
            .replace(/'/g, "&#039;");
    }

    function pad(n) { return String(n).padStart(2, "0"); }

    function todayStr() {
        const d = new Date();
        return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
    }

    function nowMinutes() {
        const d = new Date();
        return d.getHours() * 60 + d.getMinutes();
    }

    function toMinutes(hhmm) {
        const [h, m] = String(hhmm || "").split(":").map(Number);
        return Number.isFinite(h) ? h * 60 + (m || 0) : null;
    }

    function formatDay(dateStr) {
        if (dateStr === todayStr()) return t("Aujourd'hui");
        const [y, m, d] = dateStr.split("-").map(Number);
        const label = new Date(y, m - 1, d).toLocaleDateString(LOCALE, { weekday: "long", day: "numeric", month: "long" });
        return label.charAt(0).toUpperCase() + label.slice(1);
    }

    /** Status of one room at the reference minute. */
    function roomStatus(room, ref) {
        const courses = room.courses || [];
        const current = courses.find(c => toMinutes(c.start) <= ref && ref < toMinutes(c.end));
        if (current) return { state: "busy", current, note: `→ ${current.end}` };

        const next = courses.find(c => toMinutes(c.start) > ref);
        if (next && toMinutes(next.start) - ref < SOON_MINUTES) {
            return { state: "soon", next, note: t("à {time}", { time: next.start }) };
        }
        return { state: "free", next, note: "" };
    }

    // -------------------------------------------------------------------------
    // Rendering
    // -------------------------------------------------------------------------
    function render() {
        const ref = nowMinutes();
        const states = {};
        rooms.forEach(room => {
            const n = window.FloorPlan.roomNumber(room.name);
            if (n) states[n] = { ...roomStatus(room, ref), name: room.name };
        });

        window.FloorPlan.render(planContainer, {
            states,
            selected: selectedRoom,
            onSelect: (id) => {
                selectedRoom = id;
                render();
                const g = planContainer.querySelector(`[data-room="${id}"]`);
                if (g) g.focus({ preventScroll: true });
            },
        });

        dayBtn.textContent = formatDay(dayInput.value);
        dayClock.textContent = t("Statut calculé à {time}", { time: `${pad(Math.floor(ref / 60))}:${pad(ref % 60)}` });
        renderDetails(ref);
        renderOffPlan(ref);
    }

    function renderDetails(ref) {
        if (!selectedRoom) {
            details.innerHTML = `<div class="small text-muted">${t("Cliquez sur une salle pour voir ses cours de la journée.")}</div>`;
            return;
        }
        const room = rooms.find(r => window.FloorPlan.roomNumber(r.name) === selectedRoom);
        if (!room) {
            details.innerHTML = `
                <div class="fp-detail-title"><span class="room-dot is-unknown"></span>${t("Salle {room}", { room: escapeHtml(selectedRoom) })}</div>
                <div class="small text-muted mt-1">${t("Aucune donnée pour cette salle.")}</div>`;
            return;
        }

        const status = roomStatus(room, ref);
        let statusLine;
        if (status.state === "busy") {
            statusLine = t("Occupée jusqu'à {time}", { time: escapeHtml(status.current.end) });
        } else if (status.state === "soon") {
            statusLine = t("Libre, cours à {time}", { time: escapeHtml(status.next.start) });
        } else {
            statusLine = status.next ? t("Libre jusqu'à {time}", { time: escapeHtml(status.next.start) }) : t("Libre jusqu'au soir");
        }

        const courses = room.courses || [];
        const list = courses.length === 0
            ? `<div class="small text-muted">${t("Aucun cours ce jour-là.")}</div>`
            : courses.map(c => {
                const isNow = c === status.current;
                const isPast = toMinutes(c.end) <= ref;
                const people = [c.teachers && c.teachers.join(", "), c.groups && c.groups.join(", ")].filter(Boolean).join(" · ");
                return `
                    <div class="fp-course${isNow ? " is-now" : ""}${isPast ? " is-past" : ""}">
                        <div class="fp-course-time">${escapeHtml(c.start)} – ${escapeHtml(c.end)}</div>
                        <div class="fp-course-title">${escapeHtml(c.title || t("Cours"))}</div>
                        ${people ? `<div class="fp-course-meta">${escapeHtml(people)}</div>` : ""}
                    </div>`;
            }).join("");

        details.innerHTML = `
            <div class="fp-detail-title"><span class="room-dot is-${status.state}"></span>${escapeHtml(room.name)}</div>
            <div class="fp-detail-status is-${status.state}">${statusLine}</div>
            <div class="section-label mb-1">${escapeHtml(formatDay(dayInput.value))}</div>
            ${list}`;
    }

    function renderOffPlan(ref) {
        const others = rooms.filter(r => !window.FloorPlan.isOnPlan(r.name));
        offPlanCard.classList.toggle("d-none", others.length === 0);
        offPlanList.innerHTML = others.map(room => {
            const s = roomStatus(room, ref);
            return `<span class="fp-chip"><span class="room-dot is-${s.state}"></span>${escapeHtml(room.name)}</span>`;
        }).join("");
    }

    // -------------------------------------------------------------------------
    // Data
    // -------------------------------------------------------------------------
    async function load(date) {
        try {
            const res = await fetch(`/api/database/rooms-day?date=${encodeURIComponent(date)}`);
            const data = await res.json();
            if (!res.ok) throw new Error((data.error && t(data.error)) || t("Erreur serveur ({status})", { status: res.status }));
            rooms = data.rooms || [];
            loadedDate = date;
            render();
        } catch (err) {
            planContainer.innerHTML = `<div class="small text-danger p-4">${t("Impossible de charger les salles : {error}", { error: escapeHtml(err.message) })}</div>`;
        }
    }

    dayBtn.addEventListener("click", () => {
        if (typeof dayInput.showPicker === "function") {
            try { dayInput.showPicker(); return; } catch (e) { /* fall through */ }
        }
        dayInput.focus();
        dayInput.click();
    });

    dayInput.addEventListener("change", () => {
        if (!dayInput.value) dayInput.value = todayStr();
        load(dayInput.value);
    });

    // Recompute colors every minute; reload data every 5 minutes.
    setInterval(() => {
        if (loadedDate && dayInput.value === loadedDate) render();
    }, 60 * 1000);
    setInterval(() => load(dayInput.value), 5 * 60 * 1000);

    dayInput.value = todayStr();
    load(dayInput.value);
});
