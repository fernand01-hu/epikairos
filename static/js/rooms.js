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
    const prevDayBtn = document.getElementById("prevDayBtn");
    const nextDayBtn = document.getElementById("nextDayBtn");

    const SOON_MINUTES = 60;
    const HOUR_PX = 44;      // height of one hour in the day timeline
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

        details.innerHTML = `
            <div class="fp-detail-title"><span class="room-dot is-${status.state}"></span>${escapeHtml(room.name)}</div>
            <div class="fp-detail-status is-${status.state}">${statusLine}</div>
            ${timeline(room.courses || [], ref)}`;
    }

    function typeClass(type) {
        const u = String(type || "").trim().toUpperCase();
        if (u === "CME" || u === "CM") return "cm";
        if (u === "TDE" || u === "TD") return "td";
        if (u === "TPE" || u === "TP") return "tp";
        if (u.startsWith("EX") || u.includes("EXAM") || u.includes("EVAL")) return "ex";
        if (u.includes("CONF")) return "conf";
        if (u.includes("EVN") || u.includes("EVEN")) return "evn";
        return "autre";
    }

    /** The room's day as a vertical timeline, like a one-day planning. */
    function timeline(courses, ref) {
        const items = courses
            .map(c => ({ c, start: toMinutes(c.start), end: toMinutes(c.end) }))
            .filter(i => i.start !== null && i.end !== null && i.end > i.start)
            .sort((a, b) => a.start - b.start || b.end - a.end);
        if (items.length === 0) return `<div class="small text-muted mt-2">${t("Aucun cours ce jour-là.")}</div>`;

        const first = Math.min(8, Math.floor(items[0].start / 60));
        const last = Math.max(19, Math.ceil(Math.max(...items.map(i => i.end)) / 60));
        const top = (min) => ((min - first * 60) / 60) * HOUR_PX;

        // Overlapping courses share the width side by side
        let group = [], groupEnd = -1;
        const flush = () => {
            const lanes = [];
            group.forEach(i => {
                let lane = lanes.findIndex(end => end <= i.start);
                if (lane === -1) { lane = lanes.length; lanes.push(0); }
                lanes[lane] = i.end;
                i.lane = lane;
            });
            group.forEach(i => { i.lanes = lanes.length; });
            group = [];
        };
        items.forEach(i => {
            if (group.length && i.start >= groupEnd) flush();
            group.push(i);
            groupEnd = Math.max(groupEnd, i.end);
        });
        flush();

        let hours = "";
        for (let h = first; h <= last; h++) {
            hours += `<div class="rd-hour" style="top:${top(h * 60)}px"><span>${pad(h)}:00</span></div>`;
        }
        const isToday = dayInput.value === todayStr();
        const now = isToday && ref >= first * 60 && ref <= last * 60
            ? `<div class="rd-now" style="top:${top(ref)}px"></div>` : "";

        const cards = items.map(i => {
            const c = i.c;
            const people = [c.teachers && c.teachers.join(", "), c.groups && c.groups.join(", ")].filter(Boolean).join(" · ");
            const past = isToday && i.end <= ref;
            const height = Math.max(top(i.end) - top(i.start) - 2, 22);
            return `
                <div class="rd-card type-${typeClass(c.type)}${past ? " is-past" : ""}"
                     style="top:${top(i.start)}px;height:${height}px;left:calc(${(i.lane / i.lanes) * 100}% + 2px);right:calc(${((i.lanes - i.lane - 1) / i.lanes) * 100}% + 2px)"
                     title="${escapeHtml(`${c.title || ""}\n${c.start} – ${c.end}${people ? "\n" + people : ""}`)}">
                    <div class="rd-title">${escapeHtml(c.title || t("Cours"))}</div>
                    <div class="rd-meta">${escapeHtml(c.start)} – ${escapeHtml(c.end)}</div>
                    ${people ? `<div class="rd-meta">${escapeHtml(people)}</div>` : ""}
                </div>`;
        }).join("");

        return `
            <div class="rd-timeline" style="height:${top(last * 60) + 8}px">
                ${hours}
                <div class="rd-events">${cards}</div>
                ${now}
            </div>`;
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
            const box = planContainer.querySelector(".fp-loading");
            if (box) {
                box.classList.add("text-danger");
                box.textContent = t("Impossible de charger les salles : {error}", { error: err.message });
            }
        }
    }

    dayBtn.addEventListener("click", () => {
        if (typeof dayInput.showPicker === "function") {
            try { dayInput.showPicker(); return; } catch (e) { /* fall through */ }
        }
        dayInput.focus();
        dayInput.click();
    });

    function shiftDay(days) {
        const [y, m, d] = (dayInput.value || todayStr()).split("-").map(Number);
        const next = new Date(y, m - 1, d + days);
        dayInput.value = `${next.getFullYear()}-${pad(next.getMonth() + 1)}-${pad(next.getDate())}`;
        load(dayInput.value);
    }
    prevDayBtn.addEventListener("click", () => shiftDay(-1));
    nextDayBtn.addEventListener("click", () => shiftDay(1));

    document.getElementById("zoomInBtn").addEventListener("click", () => window.FloorPlan.zoomIn(planContainer));
    document.getElementById("zoomOutBtn").addEventListener("click", () => window.FloorPlan.zoomOut(planContainer));
    document.getElementById("zoomResetBtn").addEventListener("click", () => window.FloorPlan.reset(planContainer));

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
