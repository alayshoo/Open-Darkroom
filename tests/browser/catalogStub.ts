// tests/browser/catalogStub.ts
//
// The backend's catalog, stubbed for tests that mount the whole page: one
// library, and whatever File > Open picks goes into it as image 1.

const LIBRARY = { id: 1, name: "Personal", created: 1 };

export const PICKED = {
    id: 1,
    libraryId: 1,
    rollId: null,
    frame: null,
    fileName: "picked.png",
    path: "/scans/picked.png",
    inPhotoFolder: false,
    missing: false,
    metadata: { fileName: "picked.png", make: null, model: null, lens: null, focalLength: null, aperture: null, exposureTime: null, iso: null, exposureBias: null, dateTaken: null },
    frameInfo: {},
    edited: null,
    added: 1,
};

/**
 * Answers a catalog command the way catalog_commands.rs would, opening by path
 * with `openPayload`. Undefined for any other command.
 */
export function catalogCommand(cmd: string, openPayload: () => ArrayBuffer): unknown {
    switch (cmd) {
        case "catalog_load":
            return { libraries: [LIBRARY], rolls: [], images: [], filmStocks: [], photoFolder: null };
        case "catalog_open_file":
            return { ...PICKED };
        case "open_image_path":
            return openPayload();
        case "catalog_edit":
            return null;
        case "catalog_save_edit":
            return Date.now();
        case "catalog_thumbnail":
        case "catalog_render_thumbnail":
            return new ArrayBuffer(0);
        default:
            return undefined;
    }
}
