<!-- lib/components/settings/GearRow.svelte -->
<script lang="ts">
    // One camera, lens, scanner or chemistry setup in the settings list.
    import EditRow from "./EditRow.svelte";
    import { library } from "$lib/library/library.svelte";
    import { STOCK_FORMATS, STOCK_PROCESSES } from "$lib/types/filmStock";
    import { type Gear, type GearKind, gearSummary } from "$lib/types/gear";

    let { gear, open = false, ontoggle }: { gear: Gear; open?: boolean; ontoggle: () => void } = $props();

    const PLACEHOLDERS: Record<GearKind, { maker: string; name: string; mount?: string }> = {
        camera: { maker: "Nikon", name: "FM2", mount: "F" },
        lens: { maker: "Nikon", name: "Nikkor 50mm f/1.8 AI-s", mount: "F" },
        scanner: { maker: "Hasselblad", name: "Flextight X1" },
        chemistry: { maker: "Adox", name: "Rodinal" },
    };
    const NOUNS: Record<GearKind, string> = { camera: "camera", lens: "lens", scanner: "scanner", chemistry: "setup" };

    let hints = $derived(PLACEHOLDERS[gear.kind]);
    let hasMount = $derived(gear.kind === "camera" || gear.kind === "lens");
    let hasFormats = $derived(gear.kind === "camera");

    const update = (patch: Partial<Gear>) => library.updateGear(gear.id, patch);

    function toggleFormat(format: string) {
        const formats = gear.formats.includes(format)
            ? gear.formats.filter((f) => f !== format)
            : STOCK_FORMATS.filter((f) => f === format || gear.formats.includes(f));
        update({ formats });
    }
</script>

<EditRow
    title={[gear.maker, gear.name].filter(Boolean).join(" ") || `Untitled ${NOUNS[gear.kind]}`}
    summary={gearSummary(gear)}
    {open}
    {ontoggle}
    deleteLabel={`Delete ${NOUNS[gear.kind]}`}
    ondelete={() => void library.deleteGear(gear.id)}
>
    <label class="field">
        <span>Maker</span>
        <input value={gear.maker} placeholder={hints.maker} oninput={(e) => update({ maker: e.currentTarget.value })} />
    </label>
    <label class="field">
        <span>{gear.kind === "camera" || gear.kind === "scanner" ? "Model" : "Name"}</span>
        <input value={gear.name} placeholder={hints.name} oninput={(e) => update({ name: e.currentTarget.value })} />
    </label>
    {#if hasMount}
        <label class="field">
            <span>Mount</span>
            <input value={gear.mount} placeholder={hints.mount} oninput={(e) => update({ mount: e.currentTarget.value })} />
        </label>
    {/if}
    {#if hasFormats}
        <div class="field" class:wide={!hasMount}>
            <span>Formats</span>
            <div class="choice" role="group" aria-label="Formats">
                {#each STOCK_FORMATS as format (format)}
                    <button aria-pressed={gear.formats.includes(format)} class:on={gear.formats.includes(format)} onclick={() => toggleFormat(format)}>
                        {format}
                    </button>
                {/each}
            </div>
        </div>
    {/if}
    {#if gear.kind === "chemistry"}
        <div class="field wide">
            <span>Process</span>
            <div class="choice" role="radiogroup" aria-label="Process">
                {#each STOCK_PROCESSES as process (process)}
                    <button
                        role="radio"
                        aria-checked={gear.process === process}
                        class:on={gear.process === process}
                        onclick={() => update({ process: gear.process === process ? "" : process })}
                    >
                        {process}
                    </button>
                {/each}
            </div>
        </div>
    {/if}
</EditRow>
