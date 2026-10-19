<!-- lib/components/settings/ShortcutRow.svelte -->
<script lang="ts">
    /*
     * One action's row in the shortcuts tab: its name on the left, the keys it
     * answers to on the right. SettingsRow's control slot cannot shrink, and a
     * list of chips has to be free to wrap, so the row is laid out here — on
     * the same metrics, since it sits in the same table.
     */
    import { onDestroy } from "svelte";
    import GlassButton from "$lib/components/inputs/GlassButton.svelte";
    import {
        bindingFromEvent,
        display,
        serialize,
        type ShortcutAction,
        type ShortcutId,
    } from "$lib/config/shortcutRegistry";
    import { shortcuts } from "$lib/settings/shortcuts.svelte";

    let { action }: { action: ShortcutAction } = $props();

    let id = $derived(action.id as ShortcutId);

    let recording = $state(false);
    /** Says where a stolen binding came from, until the next press. */
    let notice = $state("");

    let bindings = $derived(shortcuts.for(id));
    let custom = $derived(shortcuts.isCustom(id));

    function start() {
        if (recording) return;
        recording = true;
        shortcuts.recording = true;
        notice = "";
        window.addEventListener("keydown", capture, true);
        window.addEventListener("pointerdown", stop, true);
    }

    function stop() {
        if (!recording) return;
        recording = false;
        shortcuts.recording = false;
        window.removeEventListener("keydown", capture, true);
        window.removeEventListener("pointerdown", stop, true);
    }

    /* Capture phase, so the press never reaches the window handler that would
       otherwise run whatever is already bound to it. */
    function capture(e: KeyboardEvent) {
        e.preventDefault();
        e.stopPropagation();

        const binding = bindingFromEvent(e);
        if (!binding) return;

        // Escape always dismisses, so it is worth more as the way out than as
        // a binding anyone could set.
        if (binding.key === "Escape") {
            stop();
            return;
        }

        const stolen = shortcuts.add(id, serialize(binding));
        notice = stolen.length ? `Taken from ${stolen.join(", ")}.` : "";
        stop();
    }

    onDestroy(stop);
</script>

<div class="row flex items-start justify-between gap-5 px-3 py-2">
    <div class="text flex flex-col">
        <span class="label">{action.label}</span>
        {#if action.description}
            <span class="description">{action.description}</span>
        {/if}
        {#if notice}
            <span class="notice">{notice}</span>
        {/if}
    </div>

    <div class="keys flex flex-wrap items-center justify-end gap-1.5">
        {#each bindings as binding (binding)}
            <span class="chip inline-flex items-center gap-1 rounded-[6px] pl-2 pr-1">
                {display(binding)}
                <button
                    class="chip-clear inline-flex items-center justify-center border-0 rounded-[4px] cursor-auto"
                    aria-label="Remove {display(binding)} from {action.label}"
                    title="Remove"
                    onclick={() => shortcuts.remove(id, binding)}
                >
                    <svg width="10" height="10" viewBox="0 0 10 10">
                        <path
                            d="M2 2l6 6M8 2l-6 6"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.4"
                            stroke-linecap="round"
                        />
                    </svg>
                </button>
            </span>
        {/each}

        {#if recording}
            <span class="chip listening rounded-[6px] px-2">Press keys…</span>
        {:else if bindings.length === 0}
            <span class="unbound">Unbound</span>
        {/if}

        <GlassButton
            label="Add a shortcut for {action.label}"
            size={24}
            onclick={start}
        >
            <svg width="24" height="24" viewBox="0 0 24 24">
                <path
                    d="M12 8v8M8 12h8"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.4"
                    stroke-linecap="round"
                />
            </svg>
        </GlassButton>

        <!-- Kept mounted so the chips never shift sideways as it comes and
             goes; only its ink fades. -->
        <div class="revert" class:shown={custom} aria-hidden={!custom}>
            <GlassButton
                label="Reset {action.label} to its default"
                size={24}
                onclick={() => shortcuts.reset(id)}
            >
                <svg width="24" height="24" viewBox="0 0 24 24">
                    <path
                        d="M12.6 8.4A4 4 0 1 1 8 12.9"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.2"
                        stroke-linecap="round"
                    />
                    <path d="M9.5 8.4 12 6.5 12 10.3Z" fill="currentColor" />
                </svg>
            </GlassButton>
        </div>
    </div>
</div>

<style>
    /* SettingsRow's type, so the two read as one table. */
    .label {
        font-family: "Figtree", sans-serif;
        font-size: 13px;
        font-weight: 400;
        line-height: 1.5;
        color: var(--color2);
    }

    .description {
        font-family: "Figtree", sans-serif;
        font-size: 11.5px;
        line-height: 1.35;
        color: var(--color3);
    }

    .notice {
        font-family: "Figtree", sans-serif;
        font-size: 11.5px;
        line-height: 1.35;
        color: var(--color1s);
    }

    /* A key cap: the panel's own darker step, so it reads as recessed into the
       row rather than laid on top of it. */
    .chip {
        min-height: 22px;
        background: var(--bg5);
        border: 1px solid color-mix(in srgb, var(--glassRim) 8%, transparent);
        font-family: "Figtree", sans-serif;
        font-size: 12px;
        font-weight: 500;
        color: var(--color1s);
        white-space: nowrap;
    }

    .chip-clear {
        width: 14px;
        height: 14px;
        background: transparent;
        color: var(--color3);
        transition:
            background 0.12s ease,
            color 0.12s ease;
    }

    .chip-clear:hover {
        background: color-mix(in srgb, var(--glassRim) 10%, transparent);
        color: var(--color1);
    }

    /* The one thing in the panel that asks to be looked at. */
    .listening {
        border-color: color-mix(in srgb, var(--color1s) 45%, transparent);
        color: var(--color1s);
        animation: pulse 1.1s ease-in-out infinite;
    }

    @keyframes pulse {
        0%,
        100% {
            opacity: 1;
        }
        50% {
            opacity: 0.45;
        }
    }

    .unbound {
        font-family: "Figtree", sans-serif;
        font-size: 12px;
        font-style: italic;
        color: var(--color4);
        white-space: nowrap;
    }

    .revert {
        opacity: 0;
        visibility: hidden;
        transition:
            opacity 0.25s ease,
            visibility 0s linear 0.25s;
    }

    .revert.shown {
        opacity: 1;
        visibility: visible;
        transition:
            opacity 0.25s ease,
            visibility 0s linear 0s;
    }

    .keys :global(svg) {
        color: var(--color2);
    }
</style>
