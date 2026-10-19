<!-- lib/components/settings/sections/ToneSection.svelte -->
<script lang="ts">
    import SettingsGroup from "../SettingsGroup.svelte";
    import SettingsRow from "../SettingsRow.svelte";
    import SingleValSliderInt from "$lib/components/inputs/values/SingleValSliderInt.svelte";

    import { settings } from "$lib/settings/settings.svelte";
    import { defaultToneMap, type RolloffMode } from "$lib/types/toneMap";

    const MODES: { id: RolloffMode; label: string }[] = [
        { id: "luminance", label: "Luminance" },
        { id: "perChannel", label: "Per channel" },
    ];
</script>

<div class="section flex flex-col gap-4">
    <SettingsGroup
        title="Highlight roll-off"
        note="Without a roll-off, anything brighter than white is clipped, so blown areas merge into one flat white patch. The roll-off gently compresses the brightest tones instead, so highlights keep some gradation and fade to white smoothly, reaching it three stops above white. Higher strength starts the compression lower in the tonal range, which also dims the brightest tones that were already white. Applies to the preview, the histogram and every export."
    >
        <SettingsRow label="Strength" value={`${settings.rolloff}`} />
        <div class="slider-row px-3 pb-2.5 pt-1">
            <SingleValSliderInt
                min={0}
                max={100}
                step={1}
                defaultValue={defaultToneMap.rolloff}
                bind:value={settings.rolloff}
                onCommit={() => settings.saveRolloff()}
                stepAnimation={false}
                springy={false}
            />
        </div>
    </SettingsGroup>

    <SettingsGroup
        title="Roll-off method"
        note="Luminance compresses the overall brightness and keeps each colour's hue, which then fades to white as it nears the top. Per channel compresses red, green and blue separately, like film: bright colours desaturate sooner, but their hues can drift, so a bright orange may turn yellow."
    >
        <div class="mode-row flex flex-row gap-2 p-2">
            {#each MODES as mode (mode.id)}
                <button
                    class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
                    class:active={settings.rolloffMode === mode.id}
                    onclick={() => (settings.rolloffMode = mode.id)}
                >
                    {mode.label}
                </button>
            {/each}
        </div>
    </SettingsGroup>
</div>

<style>
    /* Matches the export menu's two-option pills. */
    .pill {
        font-family: "Figtree", sans-serif;
        font-size: 13px;
        background: var(--bg5);
        color: var(--color2);
        transition:
            background 0.2s cubic-bezier(0.2, 0, 0, 1),
            color 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .pill:hover {
        color: var(--color1);
    }

    .pill.active {
        background: var(--color1);
        color: var(--bg1);
        font-weight: 600;
    }
</style>
