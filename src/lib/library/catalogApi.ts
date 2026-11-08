// $lib/library/catalogApi.ts
//
// The backend's catalog (catalog.rs), one function per command. Must match
// the records and commands there.

import { invoke } from "@tauri-apps/api/core";
import type { ImageMetadata } from "$lib/types/imageMetadata";
import type { FrameInfo, RollInfo } from "$lib/types/rollInfo";
import type { ImageEdit } from "$lib/types/imageEdit";
import type { ToneMap } from "$lib/types/toneMap";
import { bakeCurveLut } from "$lib/types/curves";
import { cloneRetouch } from "$lib/types/retouch";
import type { FilmStock, StockFields } from "$lib/types/filmStock";
import type { Gear, GearFields, RollGear } from "$lib/types/gear";

// Returned when an image has no stored thumbnail yet. Must match NO_THUMBNAIL
// in catalog_commands.rs.
export const NO_THUMBNAIL = "thumbnail:none";

export interface LibraryRecord {
    id: number;
    name: string;
    created: number;
}

export interface RollRecord {
    id: number;
    libraryId: number;
    name: string;
    info: RollInfo;
    created: number;
    /** The stock it was shot on, when it is one the catalog knows. */
    stockId: number | null;
    gear: RollGear;
}

export interface ImageRecord {
    id: number;
    libraryId: number;
    rollId: number | null;
    frame: number | null;
    fileName: string;
    path: string;
    inPhotoFolder: boolean;
    missing: boolean;
    metadata: ImageMetadata;
    frameInfo: FrameInfo;
    edited: number | null;
    added: number;
}

export interface CatalogSnapshot {
    libraries: LibraryRecord[];
    rolls: RollRecord[];
    images: ImageRecord[];
    filmStocks: FilmStock[];
    gear: Gear[];
    photoFolder: string | null;
}

/** A library, or a roll in it. */
export interface Place {
    libraryId: number;
    rollId: number | null;
}

export interface Imported {
    roll: RollRecord | null;
    images: ImageRecord[];
}

export const loadCatalog = () => invoke<CatalogSnapshot>("catalog_load");

export const createLibrary = (name: string) => invoke<LibraryRecord>("catalog_create_library", { name });
export const renameLibrary = (id: number, name: string) => invoke<void>("catalog_rename_library", { id, name });
export const deleteLibrary = (id: number) => invoke<void>("catalog_delete_library", { id });

export const createRoll = (libraryId: number, name: string) =>
    invoke<RollRecord>("catalog_create_roll", { libraryId, name });
export const updateRoll = (id: number, name: string, info: RollInfo, stockId: number | null, gear: RollGear) =>
    invoke<void>("catalog_update_roll", { id, name, info, stockId, gear });
export const deleteRoll = (id: number) => invoke<void>("catalog_delete_roll", { id });

export const createStock = (stock: StockFields) => invoke<FilmStock>("catalog_create_stock", { stock });
export const updateStock = (id: number, stock: StockFields) => invoke<void>("catalog_update_stock", { id, stock });
export const deleteStock = (id: number) => invoke<void>("catalog_delete_stock", { id });
export const createGear = (gear: GearFields) => invoke<Gear>("catalog_create_gear", { gear });
export const updateGear = (id: number, gear: GearFields) => invoke<void>("catalog_update_gear", { id, gear });
export const deleteGear = (id: number) => invoke<void>("catalog_delete_gear", { id });

export const importFiles = (place: Place, copy: boolean) => invoke<Imported>("catalog_import_files", { place, copy });
export const importFolder = (libraryId: number, copy: boolean) =>
    invoke<Imported>("catalog_import_folder", { libraryId, copy });
export const openFile = (place: Place, copy: boolean) => invoke<ImageRecord | null>("catalog_open_file", { place, copy });

export const moveImages = (ids: number[], place: Place) => invoke<void>("catalog_move_images", { ids, place });
export const removeImages = (ids: number[]) => invoke<void>("catalog_remove_images", { ids });
export const setFrameInfo = (id: number, info: FrameInfo) => invoke<void>("catalog_set_frame_info", { id, info });
export const locateImage = (id: number) => invoke<ImageRecord | null>("catalog_locate", { id });

export const fetchEdit = (id: number) => invoke<string | null>("catalog_edit", { id });
export const saveEdit = (id: number, edit: string) => invoke<number>("catalog_save_edit", { id, edit });

export const choosePhotoFolder = () => invoke<string | null>("catalog_choose_photo_folder");

/** The stored thumbnail's JPEG bytes, or null when there is none yet. */
export async function fetchThumbnail(id: number): Promise<ArrayBuffer | null> {
    try {
        return await invoke<ArrayBuffer>("catalog_thumbnail", { id });
    } catch (e) {
        if (e === NO_THUMBNAIL) return null;
        throw e;
    }
}

/** Renders and stores a thumbnail with an edit, the way export renders it. */
export function renderThumbnail(id: number, edit: ImageEdit, toneMap: ToneMap): Promise<ArrayBuffer> {
    const lut = bakeCurveLut(edit.curves, edit.isRgb);
    return invoke<ArrayBuffer>("catalog_render_thumbnail", {
        id,
        sliders: { ...edit.sliders, toneMap, curveLut: lut ? Array.from(lut) : [] },
        crop: { ...edit.crop },
        retouch: edit.retouch.map(cloneRetouch),
    });
}
