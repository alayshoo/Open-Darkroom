<!-- lib/components/library/DetailsPanel.svelte -->
<script lang="ts">
    import { library } from "$lib/library/library.svelte";
    import StockPicker from "./StockPicker.svelte";
    import EditableName from "./EditableName.svelte";
    import GearPicker from "./GearPicker.svelte";
    import LensPicker from "./LensPicker.svelte";
    import { metadataRows } from "$lib/types/imageMetadata";
    import {
        FRAME_FIELDS,
        ROLL_SECTIONS,
        type FieldSpec,
        type FrameField,
        type RollField,
        framePlaceholder,
    } from "$lib/types/rollInfo";

    let roll = $derived(library.currentRoll);
    let single = $derived(library.selectedImages.length === 1 ? library.selectedImages[0] : null);
    // Dimensions are not known until an image is opened, so the info panel's row for them is left out.
    // Roll fields that pick from the catalog's gear instead of taking free text.
    const GEAR_SLOTS = { camera: "camera", scanner: "scanner", developer: "chemistry" } as const;
    const gearSlot = (key: RollField) => (key in GEAR_SLOTS ? GEAR_SLOTS[key as keyof typeof GEAR_SLOTS] : null);

    let shootingRows = $derived(single ? metadataRows(single.metadata, 0, 0).filter((r) => r.label !== "Dimensions") : []);
</script>

{#snippet field(spec: FieldSpec<string>, value: string | undefined, placeholder: string | undefined, onchange: (v: string) => void)}
    <label class="field flex flex-col" class:half={spec.half} class:third={spec.third}>
        <span class="label">{spec.label}</span>
        {#if spec.options}
            <div class="choice flex" role="radiogroup" aria-label={spec.label}>
                {#each spec.options as option (option)}
                    <button
                        role="radio"
                        aria-checked={value === option}
                        class:on={value === option}
                        onclick={() => onchange(value === option ? "" : option)}>{option}</button
                    >
                {/each}
            </div>
        {:else if spec.multiline}
            <textarea rows="3" {placeholder} value={value ?? ""} oninput={(e) => onchange(e.currentTarget.value)}></textarea>
        {:else}
            <input {placeholder} value={value ?? ""} oninput={(e) => onchange(e.currentTarget.value)} />
        {/if}
    </label>
{/snippet}

<aside class="details glass flex flex-col rounded-[12px]">
    <div class="scroll flex flex-col">
        {#if single}
            <section class="flex flex-col">
                {#if roll && single.frame !== null}
                    <h2>Frame {single.frame}</h2>
                    <div class="fields flex flex-wrap">
                        {#each FRAME_FIELDS as spec (spec.key)}
                            {@render field(
                                spec,
                                single.frameInfo[spec.key],
                                framePlaceholder(spec.key, single.metadata),
                                (v) => library.setFrameInfo(single.id, { [spec.key as FrameField]: v }),
                            )}
                        {/each}
                    </div>
                {:else}
                    <h2>{single.fileName}</h2>
                {/if}
                {#if shootingRows.length > 0}
                    <h3>From the file</h3>
                    <dl class="m-0 flex flex-col">
                        {#each shootingRows as row (row.label)}
                            <div class="row flex justify-between">
                                <dt>{row.label}</dt>
                                <dd class="m-0">{row.value}</dd>
                            </div>
                        {/each}
                    </dl>
                {/if}
                <p class="path" title={single.path}>{single.inPhotoFolder ? "In the photo folder" : single.path}</p>
            </section>
        {:else if library.selectedImages.length > 1}
            <p class="note">{library.selectedImages.length} images selected.</p>
        {:else if !roll}
            <p class="note">Select an image to see how it was shot.</p>
        {/if}
        {#if roll}
            <section class="flex flex-col">
                <h2 class="roll-name">
                    <EditableName value={roll.name} label="Roll name" onrename={(name) => library.updateRoll(roll.id, { name })} />
                </h2>
                {#each ROLL_SECTIONS as section (section.title)}
                    <h3>{section.title}</h3>
                    <div class="fields flex flex-wrap">
                        {#each section.fields.filter((f) => !f.when || f.when(roll.info)) as spec (spec.key)}
                            {@const slot = gearSlot(spec.key)}
                            {#if spec.key === "filmStock" || spec.key === "lenses" || slot}
                                <div class="field flex flex-col">
                                    <span class="label">{spec.label}</span>
                                    {#if spec.key === "filmStock"}
                                        <StockPicker {roll} />
                                    {:else if spec.key === "lenses"}
                                        <LensPicker {roll} />
                                    {:else if slot}
                                        <GearPicker {roll} {slot} label={spec.label} />
                                    {/if}
                                </div>
                            {:else if spec.until}
                                {@const until = spec.until}
                                <div class="field flex flex-col">
                                    <span class="label">{spec.label}</span>
                                    <div class="range flex items-center">
                                        <input
                                            aria-label={`${spec.label} from`}
                                            placeholder={spec.placeholder}
                                            value={roll.info[spec.key] ?? ""}
                                            oninput={(e) => library.updateRoll(roll.id, { info: { [spec.key]: e.currentTarget.value } })}
                                        />
                                        <span class="dash" aria-hidden="true">–</span>
                                        <input
                                            aria-label={`${spec.label} until`}
                                            placeholder="Optional"
                                            value={roll.info[until] ?? ""}
                                            oninput={(e) => library.updateRoll(roll.id, { info: { [until]: e.currentTarget.value } })}
                                        />
                                    </div>
                                </div>
                            {:else}
                                {@render field(spec, roll.info[spec.key], spec.placeholder, (v) =>
                                    library.updateRoll(roll.id, { info: { [spec.key as RollField]: v } }),
                                )}
                            {/if}
                        {/each}
                    </div>
                {/each}
            </section>
        {/if}
    </div>
</aside>

<style>
    .details {
        height: 100%;
        box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
        text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
    }

    .scroll {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        scrollbar-width: none;
        /* Room below the last field for a picker's list to open into. */
        padding: 14px 16px 290px;
        gap: 22px;
    }

    .scroll::-webkit-scrollbar {
        display: none;
    }

    section + section {
        padding-top: 18px;
        border-top: 1px solid rgba(255, 255, 255, 0.07);
    }

    .roll-name {
        margin: 0 0 4px;
        font-size: 20px;
    }

    h2 {
        margin: 0 0 4px;
        font-family: var(--font-ui);
        font-size: 17px;
        font-weight: 600;
        color: var(--color1);
        overflow-wrap: anywhere;
    }

    h3 {
        margin: 14px 0 6px;
        font-family: var(--font-ui);
        font-size: 14px;
        font-weight: 600;
        color: var(--color1);
    }

    .fields {
        gap: 8px;
    }

    .field {
        flex: 1 1 100%;
        gap: 3px;
        min-width: 0;
    }

    .field.half {
        flex-basis: calc(50% - 4px);
    }

    .field.third {
        flex-basis: calc((100% - 16px) / 3);
    }

    .range {
        gap: 6px;
    }

    .dash {
        color: var(--color3);
    }

    .label {
        font-size: 12px;
        color: var(--color2);
    }

    input,
    textarea {
        width: 100%;
        margin: 0;
        padding: 5px 8px;
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 7px;
        background: rgba(0, 0, 0, 0.3);
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 13px;
        outline: none;
        resize: vertical;
        transition: border-color 0.12s ease;
    }

    input:focus,
    textarea:focus {
        border-color: rgba(255, 255, 255, 0.28);
    }

    input::placeholder,
    textarea::placeholder {
        color: var(--color4);
    }

    .choice {
        gap: 4px;
        padding: 3px;
        border-radius: 8px;
        background: rgba(0, 0, 0, 0.3);
    }

    .choice button {
        flex: 1;
        height: 24px;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--color2);
        font-size: 12px;
        cursor: default;
    }

    .choice button.on {
        background: var(--bg5);
        color: var(--color1);
    }

    dl {
        gap: 6px;
    }

    .row {
        gap: 12px;
        font-size: 13px;
    }

    dt {
        color: var(--color3);
        flex-shrink: 0;
    }

    dd {
        color: var(--color1);
        text-align: right;
        overflow-wrap: anywhere;
        user-select: text;
        -webkit-user-select: text;
    }

    .path {
        margin: 12px 0 0;
        font-size: 11px;
        color: var(--color4);
        overflow-wrap: anywhere;
        user-select: text;
        -webkit-user-select: text;
    }

    .note {
        margin: 0;
        font-size: 13px;
        line-height: 1.45;
        color: var(--color3);
    }
</style>
