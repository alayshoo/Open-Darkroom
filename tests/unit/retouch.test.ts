import { describe, expect, it } from "vitest";
import {
    RETOUCH_TILE,
    RETOUCH_UNIFORM_SIZE,
    blurSpan,
    placeRetouch,
    readRetouch,
    retouchEqual,
    retouchTiles,
    retouchUniform,
    tileReads,
    type Retouch,
} from "$lib/types/retouch";
import { bestCandidate, resampleStroke, ringSamples, sourceCandidates } from "$lib/utils/retouchSource";
import {
    cloneImageEdit,
    editFitsSource,
    newImageEdit,
    parseImageEdit,
    serializeImageEdit,
} from "$lib/types/imageEdit";
import { applyAction, undoAction, type StateAccessors } from "$lib/history/historyDispatch";

const spot = (overrides: Partial<Retouch> = {}): Retouch => ({
    mode: "heal",
    points: [[0.5, 0.5]],
    radius: 0.01,
    feather: 0.5,
    opacity: 1,
    offset: [0.1, 0],
    ...overrides,
});

describe("placeRetouch", () => {
    it("lays fractions on the image's texels, the radius on its long edge", () => {
        const placed = placeRetouch(spot({ offset: [-0.025, 0.025] }), 1000, 500);
        expect(placed.points).toEqual([[500, 250]]);
        expect(placed.radius).toBe(10);
        // Whole texels, halves rounded up, as retouch.rs rounds them.
        expect(placed.offset).toEqual([-25, 13]);
    });
});

describe("retouchTiles", () => {
    it("cuts a long stroke into tiles only where it passes", () => {
        const placed = placeRetouch(spot({ mode: "clone", points: [[0, 0], [1, 1]], radius: 4 / 2048 }), 2048, 2048);
        const tiles = retouchTiles(placed, 2048, 2048);
        const perSide = 2048 / RETOUCH_TILE;
        expect(tiles.length).toBeGreaterThanOrEqual(perSide);
        expect(tiles.length).toBeLessThan(perSide * perSide);
    });

    it("gives a heal the blur's reach past each tile, and a clone none", () => {
        const heal = placeRetouch(spot(), 1000, 1000);
        const [healTile] = retouchTiles(heal, 1000, 1000);
        expect(healTile.target[0] - healTile.work[0]).toBe(blurSpan(heal.radius).reach);

        const clone = placeRetouch(spot({ mode: "clone" }), 1000, 1000);
        const [cloneTile] = retouchTiles(clone, 1000, 1000);
        expect(cloneTile.work).toEqual(cloneTile.target);
    });

    it("keeps every box on the image", () => {
        const placed = placeRetouch(spot({ points: [[0, 0]], offset: [-0.5, -0.5] }), 200, 100);
        for (const tile of retouchTiles(placed, 200, 100)) {
            expect(Math.min(...tile.target, ...tile.work)).toBeGreaterThanOrEqual(0);
            const reads = tileReads(tile, placed.offset, 200, 100);
            expect(reads === null || (reads[2] <= 200 && reads[3] <= 100)).toBe(true);
        }
    });
});

describe("blurSpan", () => {
    it("strides a wide blur rather than tapping every texel", () => {
        const span = blurSpan(300);
        expect(span.taps).toBeLessThanOrEqual(64);
        expect(span.taps * span.step).toBe(span.reach);
        expect(span.reach).toBeGreaterThanOrEqual(Math.ceil(3 * span.sigma));
    });
});

describe("retouchUniform", () => {
    it("is laid out as retouch.wgsl declares it", () => {
        const placed = placeRetouch(spot({ offset: [0.1, -0.1] }), 1000, 500);
        const [tile] = retouchTiles(placed, 1000, 500);
        const buffer = retouchUniform(placed, tile, [7, 9], 1000, 500);
        const f = new Float32Array(buffer);
        const u = new Uint32Array(buffer);
        expect(buffer.byteLength).toBe(RETOUCH_UNIFORM_SIZE);
        expect([f[6], f[7], f[8], f[9], f[10], f[11], f[12]]).toEqual([7, 9, 1000, 500, 100, -50, 10]);
        expect([u[18], u[19]]).toEqual([1, 1]);
    });
});

describe("readRetouch", () => {
    it("reads back a stroke and refuses anything it cannot apply", () => {
        expect(readRetouch(JSON.parse(JSON.stringify(spot())))).toEqual(spot());
        expect(readRetouch({ ...spot(), mode: "smudge" })).toBeNull();
        expect(readRetouch({ ...spot(), points: [] })).toBeNull();
        expect(readRetouch({ ...spot(), radius: -1 })).toBeNull();
        expect(readRetouch({ ...spot(), feather: 4 })?.feather).toBe(1);
    });
});

describe("the automatic source", () => {
    const placed = placeRetouch(spot({ points: [[0.3, 0.5], [0.7, 0.5]], radius: 5 / 400 }), 400, 400);

    it("only offers offsets that clear the stroke and stay on the image", () => {
        const candidates = sourceCandidates(placed, 400, 400);
        expect(candidates.length).toBeGreaterThan(0);
        const along = resampleStroke(placed.points, 1);
        for (const [dx, dy] of candidates) {
            for (const [x, y] of along) {
                const gap = Math.min(...along.map(([px, py]) => Math.hypot(x + dx - px, y + dy - py)));
                expect(gap).toBeGreaterThanOrEqual(2 * placed.radius);
                expect(x + dx).toBeGreaterThanOrEqual(0);
                expect(x + dx).toBeLessThanOrEqual(400);
            }
        }
    });

    it("judges them on texels clear of the stroke", () => {
        for (const [x, y] of ringSamples(placed, 400, 400)) {
            const d = Math.min(...resampleStroke(placed.points, 0.5).map(([px, py]) => Math.hypot(x - px, y - py)));
            expect(d).toBeGreaterThan(placed.radius);
        }
    });

    it("prefers the best score, and the nearer of two alike", () => {
        expect(bestCandidate([[10, 0], [50, 0]], [1, 0.5], 5)).toEqual([50, 0]);
        expect(bestCandidate([[10, 0], [50, 0]], [1, 1], 5)).toEqual([10, 0]);
        expect(bestCandidate([[10, 0]], [1e30], 5)).toBeNull();
    });
});

describe("the image edit document", () => {
    it("survives a round trip", () => {
        const edit = newImageEdit(6000, 4000);
        edit.sliders.exposure = 0.5;
        edit.isRgb = false;
        edit.heldColour = { saturation: 20 };
        edit.crop = { ...edit.crop, w: 0.5, quarterTurns: 1 };
        edit.retouch = [spot(), spot({ mode: "clone", points: [[0.1, 0.2], [0.3, 0.4]] })];
        expect(parseImageEdit(serializeImageEdit(edit))).toEqual(edit);
    });

    it("keeps what it can of a document from another build", () => {
        const text = JSON.stringify({
            version: 99,
            source: { width: 10, height: 20 },
            sliders: { exposure: 1, notASlider: 3, contrast: "high" },
            retouch: [spot(), { mode: "smudge" }],
            crop: { quarterTurns: 7 },
        });
        const edit = parseImageEdit(text)!;
        expect(edit.sliders.exposure).toBe(1);
        expect(edit.sliders.contrast).toBe(newImageEdit(1, 1).sliders.contrast);
        expect(edit.retouch).toEqual([spot()]);
        expect(edit.crop).toEqual(newImageEdit(1, 1).crop);
        expect(editFitsSource(edit, 10, 20)).toBe(true);
        expect(editFitsSource(edit, 20, 10)).toBe(false);
    });

    it("refuses what is not an edit at all", () => {
        expect(parseImageEdit("not json")).toBeNull();
        expect(parseImageEdit("{}")).toBeNull();
        expect(parseImageEdit(JSON.stringify({ version: 1, source: { width: 0, height: 5 } }))).toBeNull();
    });

    it("copies deeply", () => {
        const edit = newImageEdit(10, 10);
        edit.retouch = [spot()];
        const copy = cloneImageEdit(edit);
        copy.retouch[0].points[0][0] = 0.9;
        copy.curves.master[0].y = 0.5;
        expect(edit.retouch[0].points[0][0]).toBe(0.5);
        expect(edit.curves.master[0].y).toBe(0);
    });
});

describe("retouch history", () => {
    function accessors() {
        let strokes: Retouch[] = [];
        const state = {
            getRetouch: () => strokes,
            setRetouch: (next: Retouch[]) => (strokes = next),
        } as unknown as StateAccessors;
        return { state, strokes: () => strokes };
    }

    it("adds a stroke at the end and takes it off again", () => {
        const { state, strokes } = accessors();
        const a = spot();
        const b = spot({ mode: "clone" });
        applyAction({ type: "retouchAdd", stroke: a }, state);
        applyAction({ type: "retouchAdd", stroke: b }, state);
        expect(strokes()).toEqual([a, b]);
        undoAction({ type: "retouchAdd", stroke: b }, state);
        expect(strokes()).toEqual([a]);
    });

    it("clears every stroke and puts them back", () => {
        const { state, strokes } = accessors();
        const all = [spot(), spot({ opacity: 0.5 })];
        state.setRetouch(all);
        applyAction({ type: "retouchClear", strokes: all }, state);
        expect(strokes()).toEqual([]);
        undoAction({ type: "retouchClear", strokes: all }, state);
        expect(strokes().every((s, i) => retouchEqual(s, all[i]))).toBe(true);
    });
});
