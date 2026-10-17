// Bin layout shared by the three histogram shaders: three channels of 256
// laid out end to end in one buffer.
const BINS_PER_CHANNEL: u32 = 256u;
const BIN_TOTAL:        u32 = 768u;
const R_BASE:           u32 = 0u;
const G_BASE:           u32 = 256u;
const B_BASE:           u32 = 512u;
