// src/lib/gpu/workingSpace.ts
//
// The working space the render chain operates in, named on the frontend so the
// in-browser tests can mirror what the Rust suite does rather than assume the
// identity.
//
// At runtime the matrix comes from the payload: the backend reads the file's
// profile and sends the conversion out of its primaries. This constant is the
// one it sends for a file carrying no profile at all.

// Linear sRGB → linear Rec.2020, column-major. Must match SRGB_TO_WORKING in
// src-tauri/src/icc.rs.
export const SRGB_TO_WORKING = new Float32Array([
    0.6274039, 0.06909729, 0.01639144,
    0.32928304, 0.9195404, 0.08801331,
    0.043313067, 0.011362316, 0.89559525,
]);

// What the backend sends when the file is already in the working space, and
// what a pipeline starts with before any image is loaded.
export const IDENTITY = new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1]);
