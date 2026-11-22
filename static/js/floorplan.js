/**
 * EpiKairos - interactive floor plan (3rd floor) for the free rooms finder.
 *
 * Room shapes are traced from the school's floor plan, in a 1606x1029 space.
 * `kind: "office"` rooms (Bureaux, staff room) never host classes and are
 * drawn as plain building space. Any other room is matched against the
 * free-rooms API by its 3-digit number ("Salle 301", "310 - Amphi Plat").
 */
(function () {
    const SVG_NS = "http://www.w3.org/2000/svg";

    const OUTLINE =
        "M390,4 L470,4 L557,60 L290,472 Q430,560 450,690 L448,718 L438,718 L425,770 " +
        "L1300,772 L1300,127 L1468,127 L1468,810 L1497,810 L1595,940 L225,940 " +
        "L135,770 L155,770 L155,722 L108,722 L25,567 Z";

    const ROOMS = [
        // West wing
        { id: "301", pts: "390,5 450,5 476,24 449,61 386,148 325,110" },
        { id: "302", pts: "478,27 551,70 467,198 404,160 464,72" },
        { id: "303", pts: "283,172 347,210 290,335 212,282" },
        { id: "304", label: "Bureau", kind: "office", pts: "366,246 416,264 380,325 343,298" },
        { id: "305", pts: "210,285 276,327 228,438 131,408" },
        { id: "308", pts: "68,505 190,585 71,648 27,567" },
        { id: "309", pts: "190,586 250,638 106,717 72,650" },
        { id: "310", label: "Amphi", pts: "106,719 250,640 247,660 310,783 327,780 340,803 341,832 320,845 320,939 226,939 136,771 155,770 155,722" },
        { id: "312", label: "Bureau", kind: "office", pts: "340,508 395,558 356,588 318,546" },
        { id: "313", label: "Bureau", kind: "office", pts: "396,560 418,590 360,620 343,589" },
        { id: "314", label: "Bureau", kind: "office", pts: "419,592 435,623 375,652 361,622" },
        { id: "315", label: "Bureau", kind: "office", pts: "436,625 447,668 393,686 376,654" },
        // South wing
        { id: "311", label: "Bureau", kind: "office", pts: "321,846 397,846 397,939 321,939" },
        { id: "316", label: "Bureau", kind: "office", pts: "473,872 501,848 528,848 528,939 473,939" },
        { id: "317", pts: "529,823 623,823 623,939 529,939" },
        { id: "318", label: "Machines", pts: "624,823 718,823 718,939 624,939" },
        { id: "319", label: "Bureau", kind: "office", pts: "719,868 775,868 775,939 719,939" },
        { id: "320", label: "Bureau", kind: "office", pts: "851,774 905,774 905,843 851,843" },
        { id: "321", label: "Machines", pts: "906,774 1000,774 1000,898 906,913" },
        { id: "322", label: "Machines", pts: "1001,774 1094,774 1094,895 1001,908" },
        { id: "323", label: "Machines", pts: "1095,774 1188,774 1188,892 1095,908" },
        { id: "324", label: "Bureau", kind: "office", pts: "1221,871 1301,871 1301,939 1209,939" },
        { id: "325", label: "Atelier", pts: "1378,875 1545,875 1594,939 1378,939" },
        // East wing
        { id: "326", label: "Salle de repos", kind: "office", pts: "1400,746 1465,740 1465,851 1400,852" },
        { id: "327", pts: "1333,688 1466,670 1466,739 1342,755" },
        { id: "328", pts: "1340,595 1466,578 1466,668 1350,684" },
        { id: "329", pts: "1348,498 1467,482 1467,576 1358,590" },
        { id: "330", label: "Bureau", kind: "office", pts: "1302,443 1370,443 1368,472 1302,480" },
        { id: "331", label: "Majeure IF", pts: "1401,236 1467,236 1467,333 1401,333" },
        { id: "332", label: "Majeure Santé", pts: "1302,236 1372,236 1372,367 1302,367" },
        { id: "333", label: "Majeure SCIA-G", pts: "1301,128 1467,128 1467,235 1301,235" },
    ];

    const PLAN_IDS = new Set(ROOMS.map(r => r.id));
    // Drawing area in plan units
    const PLAN = { x: 15, y: 0, w: 1595, h: 950 };
    // Rooms are drawn this far inside their walls, so neighbours never share a border
    const INSET = 4;

    function roomNumber(name) {
        const m = String(name || "").match(/\b(\d{3})\b/);
        return m ? m[1] : null;
    }

    function parsePts(pts) {
        return pts.split(" ").map(p => p.split(",").map(Number));
    }

    function centroid(pts) {
        const coords = parsePts(pts);
        // Area-weighted polygon centroid, so labels sit inside slanted rooms
        let a = 0, cx = 0, cy = 0;
        for (let i = 0; i < coords.length; i++) {
            const [x0, y0] = coords[i];
            const [x1, y1] = coords[(i + 1) % coords.length];
            const f = x0 * y1 - x1 * y0;
            a += f; cx += (x0 + x1) * f; cy += (y0 + y1) * f;
        }
        a *= 0.5;
        if (Math.abs(a) < 1e-6) return coords[0];
        return [cx / (6 * a), cy / (6 * a)];
    }

    /** Moves every edge of the polygon `d` units inwards (mitred corners, capped). */
    function inset(pts, d) {
        const c = parsePts(pts);
        const n = c.length;
        let area = 0;
        for (let i = 0; i < n; i++) {
            const [x0, y0] = c[i], [x1, y1] = c[(i + 1) % n];
            area += x0 * y1 - x1 * y0;
        }
        const sign = area > 0 ? 1 : -1;
        const normal = (a, b) => {
            const dx = b[0] - a[0], dy = b[1] - a[1];
            const len = Math.hypot(dx, dy) || 1;
            return [(-dy / len) * sign, (dx / len) * sign];
        };
        return c.map((p, i) => {
            const n1 = normal(c[(i - 1 + n) % n], p);
            const n2 = normal(p, c[(i + 1) % n]);
            const dot = n1[0] * n2[0] + n1[1] * n2[1];
            const k = Math.min(d / Math.max(1 + dot, 0.25), d * 3);
            return `${(p[0] + (n1[0] + n2[0]) * k).toFixed(1)},${(p[1] + (n1[1] + n2[1]) * k).toFixed(1)}`;
        }).join(" ");
    }

    function el(tag, attrs, text) {
        const node = document.createElementNS(SVG_NS, tag);
        for (const [k, v] of Object.entries(attrs || {})) node.setAttribute(k, v);
        if (text !== undefined) node.textContent = text;
        return node;
    }

    const STATE_LABELS = { free: t("libre"), soon: t("prise dans moins d'une heure"), busy: t("occupée"), unknown: t("pas de données") };

    // -------------------------------------------------------------------------
    // Zoom and pan (pinch, drag, Ctrl+wheel, double tap, buttons)
    // The view is a rectangle of the plan with the same shape as the container.
    // -------------------------------------------------------------------------
    const MIN_WIDTH = 260;

    function boxAspect(box) {
        const r = box.getBoundingClientRect();
        return r.width / Math.max(r.height, 1);
    }

    function fullWidth(aspect) {
        return Math.max(PLAN.w, PLAN.h * aspect);
    }

    function clampView(v, aspect) {
        const w = Math.min(Math.max(v.w, MIN_WIDTH), fullWidth(aspect));
        const h = w / aspect;
        const fit = (pos, size, start, total) =>
            size >= total ? start + (total - size) / 2 : Math.min(Math.max(pos, start), start + total - size);
        return { x: fit(v.x, w, PLAN.x, PLAN.w), y: fit(v.y, h, PLAN.y, PLAN.h), w, h };
    }

    /** Wide screens see the whole floor; tall ones start at full height on the west wing. */
    function defaultView(aspect) {
        if (aspect >= (PLAN.w / PLAN.h) * 0.9) return clampView({ x: 0, y: 0, w: fullWidth(aspect) }, aspect);
        return clampView({ x: PLAN.x, y: PLAN.y, w: PLAN.h * aspect }, aspect);
    }

    function applyView(box) {
        const svg = box.querySelector("svg");
        const aspect = boxAspect(box);
        box.fpView = clampView(box.fpView || defaultView(aspect), aspect);
        if (svg) {
            const v = box.fpView;
            svg.setAttribute("viewBox", `${v.x} ${v.y} ${v.w} ${v.h}`);
        }
        box.classList.toggle("is-zoomed", box.fpView.w < fullWidth(aspect) - 1);
    }

    /** Zooms by `factor` around the client point (px, py). */
    function zoomAt(box, factor, px, py) {
        const r = box.getBoundingClientRect();
        const v = box.fpView;
        const fx = (px - r.left) / r.width, fy = (py - r.top) / r.height;
        const w = v.w / factor;
        box.fpView = { x: v.x + fx * v.w - fx * w, y: v.y + fy * v.h - fy * (w / boxAspect(box)), w };
        applyView(box);
    }



    function enableZoom(box) {
        if (box.fpZoomReady) return;
        box.fpZoomReady = true;
        const pointers = new Map();
        let last = null;   // previous gesture state { x, y, dist }
        let moved = 0;

        const gesture = () => {
            const pts = [...pointers.values()];
            const x = pts.reduce((s, p) => s + p.x, 0) / pts.length;
            const y = pts.reduce((s, p) => s + p.y, 0) / pts.length;
            const dist = pts.length > 1 ? Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y) : 0;
            return { x, y, dist, count: pts.length };
        };

        box.addEventListener("pointerdown", e => {
            if (e.pointerType === "mouse" && e.button !== 0) return;
            if (e.target.closest(".fp-overlay")) return;
            pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
            last = gesture();
            if (pointers.size === 1) moved = 0;
        });

        box.addEventListener("pointermove", e => {
            if (!pointers.has(e.pointerId)) return;
            pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
            const g = gesture();
            if (!last || g.count !== last.count) { last = g; return; }
            const zoomed = box.classList.contains("is-zoomed");
            if (g.count === 1 && !zoomed) { last = g; return; }   // let the page scroll
            moved += Math.hypot(g.x - last.x, g.y - last.y) + Math.abs(g.dist - last.dist);
            if (moved < 6) return;
            if (!box.hasPointerCapture(e.pointerId)) {
                try { box.setPointerCapture(e.pointerId); } catch (err) { /* ignore */ }
            }
            const r = box.getBoundingClientRect();
            const v = box.fpView;
            const scale = v.w / r.width;
            box.fpView = { ...v, x: v.x - (g.x - last.x) * scale, y: v.y - (g.y - last.y) * scale };
            applyView(box);
            if (g.count > 1 && last.dist > 0) zoomAt(box, g.dist / last.dist, g.x, g.y);
            last = g;
            e.preventDefault();
        });

        const end = e => {
            if (!pointers.delete(e.pointerId)) return;
            last = pointers.size ? gesture() : null;
            if (moved >= 6) box.fpLastDrag = Date.now();
        };
        box.addEventListener("pointerup", end);
        box.addEventListener("pointercancel", end);

        // Mouse wheel over the map zooms it (trackpad pinch arrives as Ctrl+wheel too)
        box.addEventListener("wheel", e => {
            e.preventDefault();
            const step = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;   // lines -> pixels
            zoomAt(box, Math.exp(-step / 300), e.clientX, e.clientY);
        }, { passive: false });

        box.addEventListener("dblclick", e => {
            if (e.target.closest(".fp-overlay")) return;
            zoomAt(box, 2, e.clientX, e.clientY);
        });

        window.addEventListener("resize", () => applyView(box));
    }

    // -------------------------------------------------------------------------
    // Rendering
    // -------------------------------------------------------------------------

    /**
     * Renders the plan into `container` (keeps the current zoom).
     * states: { "301": { state: "free"|"soon"|"busy", name, note }, ... } — rooms
     * missing from it are drawn as "no data". onSelect(roomId) fires on click/Enter.
     */
    function render(container, { states = {}, selected = null, onSelect } = {}) {
        const svg = el("svg", {
            class: "floorplan-svg",
            role: "group",
            preserveAspectRatio: "xMidYMid meet",
            "aria-label": t("Plan du 3e étage"),
        });

        svg.appendChild(el("path", { d: OUTLINE, class: "fp-outline" }));

        ROOMS.forEach(def => {
            const entry = states[def.id] || null;
            const office = def.kind === "office";
            const state = office ? "office" : (entry ? entry.state : "unknown");

            const g = el("g", {
                class: `fp-room is-${state}${selected === def.id ? " is-selected" : ""}`,
                "data-room": def.id,
            });
            if (!office) {
                g.setAttribute("role", "button");
                g.setAttribute("aria-label", `${entry ? entry.name : t("Salle {room}", { room: def.id })}, ${STATE_LABELS[state]}`);
            }

            g.appendChild(el("polygon", { points: inset(def.pts, INSET) }));
            const [cx, cy] = centroid(def.pts);
            g.appendChild(el("text", { x: cx, y: cy - (def.label && !office ? 8 : 0), class: office ? "fp-num fp-num-sm" : "fp-num" }, def.id));
            if (def.label && !office) {
                g.appendChild(el("text", { x: cx, y: cy + 16, class: "fp-sub" }, t(def.label)));
            }
            if (entry && entry.note && !office) {
                g.appendChild(el("text", { x: cx, y: cy + (def.label ? 36 : 22), class: `fp-sub fp-note is-${state}` }, entry.note));
            }

            if (!office && onSelect) {
                const activate = () => {
                    // A drag or pinch that ends on a room is not a click
                    if (container.fpLastDrag && Date.now() - container.fpLastDrag < 350) return;
                    onSelect(def.id);
                };
                g.addEventListener("click", activate);
            }
            svg.appendChild(g);
        });

        const old = container.querySelector("svg");
        if (old) old.replaceWith(svg);
        else {
            container.querySelector(".fp-loading")?.remove();
            container.prepend(svg);
        }
        enableZoom(container);
        applyView(container);
    }

    window.FloorPlan = { render, roomNumber };
})();
