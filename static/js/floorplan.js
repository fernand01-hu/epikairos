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
        { id: "301", pts: "392,6 470,10 385,147 328,108" },
        { id: "302", pts: "478,24 553,64 468,198 406,158" },
        { id: "303", pts: "284,174 344,211 290,333 212,284" },
        { id: "304", label: "Bureau", kind: "office", pts: "366,246 416,264 380,325 343,298" },
        { id: "305", pts: "210,285 276,327 228,438 131,408" },
        { id: "308", pts: "68,505 190,585 71,648 27,567" },
        { id: "309", pts: "190,586 250,638 106,717 72,650" },
        { id: "310", label: "Amphi", pts: "106,719 250,640 340,800 341,832 320,845 320,939 226,939 136,771 155,770 155,722" },
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

    function roomNumber(name) {
        const m = String(name || "").match(/\b(\d{3})\b/);
        return m ? m[1] : null;
    }

    function centroid(pts) {
        const coords = pts.split(" ").map(p => p.split(",").map(Number));
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

    function el(tag, attrs, text) {
        const node = document.createElementNS(SVG_NS, tag);
        for (const [k, v] of Object.entries(attrs || {})) node.setAttribute(k, v);
        if (text !== undefined) node.textContent = text;
        return node;
    }

    const STATE_LABELS = { free: "libre", soon: "prise dans moins d'une heure", busy: "occupée", unknown: "pas de données" };

    /**
     * Renders the plan into `container`.
     * states: { "301": { state: "free"|"soon"|"busy", name, note }, ... } — rooms
     * missing from it are drawn as "no data". onSelect(roomId) fires on click/Enter.
     */
    function render(container, { states = {}, selected = null, onSelect } = {}) {
        const svg = el("svg", {
            viewBox: "15 0 1595 950",
            class: "floorplan-svg",
            role: "group",
            "aria-label": "Plan du 3e étage",
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
                g.setAttribute("tabindex", "0");
                g.setAttribute("role", "button");
                g.setAttribute("aria-label", `${entry ? entry.name : "Salle " + def.id}, ${STATE_LABELS[state]}`);
            }

            g.appendChild(el("polygon", { points: def.pts }));
            const [cx, cy] = centroid(def.pts);
            g.appendChild(el("text", { x: cx, y: cy - (def.label && !office ? 8 : 0), class: office ? "fp-num fp-num-sm" : "fp-num" }, def.id));
            if (def.label && !office) {
                g.appendChild(el("text", { x: cx, y: cy + 16, class: "fp-sub" }, def.label));
            }
            if (entry && entry.note && !office) {
                g.appendChild(el("text", { x: cx, y: cy + (def.label ? 36 : 22), class: `fp-sub fp-note is-${state}` }, entry.note));
            }

            if (!office && onSelect) {
                const activate = () => onSelect(def.id);
                g.addEventListener("click", activate);
                g.addEventListener("keydown", e => {
                    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); activate(); }
                });
            }
            svg.appendChild(g);
        });

        container.replaceChildren(svg);
    }

    function isOnPlan(roomName) {
        return PLAN_IDS.has(roomNumber(roomName));
    }

    window.FloorPlan = { render, roomNumber, isOnPlan };
})();
