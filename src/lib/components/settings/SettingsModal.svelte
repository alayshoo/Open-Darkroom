<!-- lib/components/settings/SettingsModal.svelte -->
<script lang="ts">
    /*
     * The settings window. Categories on the left, the active category's panel
     * on the right; both come from the section registry, so this file never
     * learns what any individual setting is.
     */
    import { settings } from "$lib/settings/settings.svelte";
    import { settingsSections } from "./sections";
    import GlassButton from "$lib/components/inputs/GlassButton.svelte";

    let active = $derived(
        settingsSections.find((s) => s.id === settings.section) ??
            settingsSections[0],
    );

    let dialogEl: HTMLElement | undefined = $state();

    /* The scrim is a sibling rather than an ancestor, so the test is what the
       click landed outside of rather than what it landed on. */
    function handleOverlayClick(e: MouseEvent) {
        if (dialogEl && !dialogEl.contains(e.target as Node)) settings.close();
    }
</script>

{#if settings.open}
    <!-- The overlay itself paints nothing and filters nothing: it only centres
         the dialog and sits above the title bar's z 999. Anything it did to
         itself — a filter, a fade — would make it a backdrop root and cut the
         sidebar's glass off from the app behind it. -->
    <div
        class="overlay fixed inset-0 z-1000 flex items-center justify-center"
        onclick={handleOverlayClick}
        role="presentation"
    >
        <div class="scrim absolute inset-0"></div>
        <div
            bind:this={dialogEl}
            class="dialog relative flex rounded-[14px]"
            role="dialog"
            aria-modal="true"
            aria-label="Settings"
        >
            <nav class="sidebar glass flex flex-col shrink-0 gap-0.5 p-2">
                <h2 class="sidebar-title m-0 mt-1.5 mb-2.5 px-2">Settings</h2>
                {#each settingsSections as section (section.id)}
                    <button
                        class="nav-item flex items-center gap-2.5 h-8 px-2 rounded-[8px] border-0 cursor-app"
                        class:active={section.id === active.id}
                        class:glass={section.id === active.id}
                        onclick={() => (settings.section = section.id)}
                    >
                        <svg
                            class="nav-icon shrink-0"
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <path d={section.icon} />
                        </svg>
                        <span>{section.label}</span>
                    </button>
                {/each}
            </nav>

            <!-- Rounded on all four corners, not just the outer two: the left
                 pair cut back to show the sidebar's glass behind them. -->
            <div
                class="panel relative flex flex-col flex-1 min-w-0 rounded-[14px] overflow-hidden"
            >
                <!-- In flow, so the list below it owns its own height and its
                     scrollbar starts under the bar. What overlaps the list is
                     the gradient alone, which hangs past the bar's edge. -->
                <header
                    class="panel-header relative z-1 shrink-0 flex items-center justify-between pl-5 pr-2.5"
                >
                    <h2 class="panel-title m-0">{active.label}</h2>
                    <!-- Top-aligned rather than centred in the bar: its inset
                         from the top then matches its inset from the right. -->
                    <div class="close-slot self-start mt-2.5">
                        <GlassButton
                            label="Close settings"
                            onclick={() => settings.close()}
                        >
                            <svg width="28" height="28" viewBox="0 0 24 24">
                                <path
                                    d="M8.6 8.6l6.8 6.8M15.4 8.6l-6.8 6.8"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="1.4"
                                    stroke-linecap="round"
                                />
                            </svg>
                        </GlassButton>
                    </div>
                </header>
                <div class="panel-body flex-1 pl-5 pr-1.5 pb-5">
                    {#key active.id}
                        {@const Section = active.component}
                        <Section />
                    {/key}
                </div>
            </div>
        </div>
    </div>
{/if}

<style>
    .overlay {
        padding: 44px 12px 12px;
    }

    /* Everything the background gets: one dim, one blur, under the dialog
       rather than around it. */
    .scrim {
        background: rgba(0, 0, 0, 0.6);
        backdrop-filter: blur(4px);
        animation: scrimIn 0.15s ease forwards;
    }

    /* No fill of its own — the panel is opaque and the sidebar is not, which is
       what lets the dimmed image read faintly through the left column. The
       radius is here for the shadow's sake; the two halves round themselves. */
    .dialog {
        /* How far the panel laps over the sidebar. Its own corner radius, so
           the rounded corners cut back onto glass — see .panel. */
        --panel-lap: 14px;
        width: min(1100px, 92vw);
        height: min(800px, 90vh);
        background: transparent;
        box-shadow: 0 20px 60px rgba(0, 0, 0, 0.65);
        font-family: "Figtree", sans-serif;
        animation: dialogIn 0.18s cubic-bezier(0.2, 0, 0, 1) forwards;
    }

    @keyframes scrimIn {
        from {
            opacity: 0;
        }
        to {
            opacity: 1;
        }
    }

    /* Transform only. Fading the dialog would make it a backdrop root for as
       long as the animation ran, and the sidebar's glass would pop when it
       ended. */
    @keyframes dialogIn {
        from {
            transform: translateY(-6px) scale(0.985);
        }
        to {
            transform: translateY(0) scale(1);
        }
    }

    /* The app's material at a much heavier tint — dark enough to sit beside an
       opaque panel, sheer enough that the dimmed image still shows through. */
    /* 190px on show. The lap is dead width tucked under the panel, so the
       right padding has to cover it too — otherwise the tabs run on under the
       panel's edge and the selected one looks folded beneath it. */
    .sidebar {
        --panel-tint: rgba(0, 0, 0, 0.78);
        --panel-blur: 40px;
        width: calc(190px + var(--panel-lap));
        padding-right: calc(0.5rem + var(--panel-lap));
        border-radius: 14px 0 0 14px;
    }

    /* No rim or gloss: the sidebar shares three of the dialog's four edges, so
       a lit outline of its own would read as a border down one side. */
    .sidebar::before,
    .sidebar::after {
        display: none;
    }

    /* Laps over the sidebar by exactly its own radius, so the rounded left
       corners cut back onto glass instead of biting a hole through to the
       scrim. Both halves have to be positioned for the lap to stack in DOM
       order — .glass positions the sidebar, so the panel says so too. */
    .panel {
        /* The heading bar, and how far its fade hangs below it. The body's top
           padding is measured off both, so the rows start clear of the fade
           rather than under the tail of it. */
        --header-h: 56px;
        --header-fade: 32px;
        /* Width of the body's scrollbar, which the body's right padding is
           measured against so the content keeps an even margin either side. */
        --scroll-gutter: 14px;
        margin-left: calc(-1 * var(--panel-lap));
        background: var(--bg3);
    }

    /* Inert apart from the close button — the wheel belongs to the list
       underneath. The two lengths below are what the body's top padding is
       measured from, so the rows start clear of the fade rather than under
       the tail of it. */
    .panel-header {
        height: var(--header-h);
        pointer-events: none;
    }

    /* The title bar's arrangement, over the panel's own rows. What makes the
       heading legible is the cover rather than the shade: it holds the panel's
       own colour, barely darkened, until it reaches the bar's edge. Full width,
       since the list and its scrollbar both start below the bar. */
    .panel-header::before {
        content: "";
        position: absolute;
        inset: 0;
        background: linear-gradient(
            to bottom,
            color-mix(in srgb, var(--bg3) 88%, #000) 0%,
            var(--bg3) 100%
        );
        z-index: -1;
    }

    /* The fall-off, hung past the bar so it finishes in open space instead of
       on the bar's edge — opaque where the list's top edge cuts across it, so a
       row scrolling out vanishes under cover rather than at a visible line. The
       one length held back from the gutter: it is the only part low enough to
       reach the scrollbar, and there are no rows behind the bar to cover. It
       starts on the panel's own colour, so stopping short leaves no seam. */
    .panel-header::after {
        content: "";
        position: absolute;
        top: 100%;
        left: 0;
        right: var(--scroll-gutter);
        height: var(--header-fade);
        background: linear-gradient(to bottom, var(--bg3) 0%, transparent 100%);
        z-index: -1;
    }

    .close-slot {
        pointer-events: auto;
    }

    /* A weight lighter than the panel's heading, which is the loud one. */
    .sidebar-title {
        font-size: 18px;
        font-weight: 500;
        color: var(--color1);
    }

    .nav-item {
        color: var(--color2);
        font-family: inherit;
        font-size: 13px;
        text-align: left;
        transition:
            background 0.12s ease,
            color 0.2s ease;
    }

    .nav-item:not(.active) {
        background: transparent;
    }

    .nav-item:not(.active):hover {
        background: color-mix(in srgb, var(--glassRim) 6%, transparent);
        color: var(--color1);
    }

    /* The selected tab is the one thing in here that lifts: it takes the glass
       at a light tint, minus the blur — the sidebar behind it is flat, so the
       frost and the rim carry it on their own. */
    .nav-item.active {
        --panel-tint: color-mix(in srgb, var(--glassRim) 8%, transparent);
        --rim-hi: 0.2;
        --rim-mid: 0.09;
        --rim-lo: 0.05;
        --rim-end: 0.1;
        backdrop-filter: none;
        -webkit-backdrop-filter: none;
        color: var(--color1);
    }

    .panel-title {
        font-family: "Outfit Variable", "Figtree", sans-serif;
        font-size: 25px;
        font-weight: 700;
        color: var(--color1);
    }

    /* Same ink as the side bar's revert button, so it swings with the theme. */
    .close-slot :global(svg) {
        color: var(--color2);
    }

    /* The bar sits outside the padding box, so the right padding is short by
       its width and the gutter is held open whether it is showing or not —
       between them the content keeps an even 20px margin either side.

       No scrollbar-width/scrollbar-color here: setting either makes Chromium
       drop the ::-webkit-scrollbar rules below entirely. */
    .panel-body {
        overflow-y: auto;
        scrollbar-gutter: stable;
        /* Clear of the part of the gradient that hangs into this box — the
           first row would sit in the tail of the fade otherwise. */
        padding-top: calc(var(--header-fade) + 4px);
    }

    .panel-body::-webkit-scrollbar {
        width: var(--scroll-gutter);
    }

    /* Nothing behind the thumb: every piece of the widget stays clear, so the
       panel shows through the gutter rather than a track beside the fade. */
    .panel-body::-webkit-scrollbar,
    .panel-body::-webkit-scrollbar-track,
    .panel-body::-webkit-scrollbar-track-piece,
    .panel-body::-webkit-scrollbar-corner {
        background: transparent;
    }

    /* Styling the bar at all brings back the stepper arrows on Windows. */
    .panel-body::-webkit-scrollbar-button {
        display: none;
        height: 0;
        width: 0;
    }

    /* Inset by a transparent border rather than by width, so the thumb keeps
       its full hit area while reading as a 3px sliver held clear of the edge.
       The border has to stay even: the radius is resolved on the border box,
       and the inner one it leaves behind is that minus the border on each
       side — uneven borders round one flank and square off the other. */
    .panel-body::-webkit-scrollbar-thumb {
        background: var(--sliderTrack);
        background-clip: content-box;
        border: 5.5px solid transparent;
        border-radius: 999px;
    }

    .panel-body::-webkit-scrollbar-thumb:hover {
        background: var(--color3);
        background-clip: content-box;
    }
</style>
