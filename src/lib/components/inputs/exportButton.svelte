<script lang="ts">

</script>

<div class="container">
    <div class="export-button">Export</div>
    <div class="arrow-button">A</div>
</div>

<style>
    .container {
        display: flex;
        flex-direction: row;
        height: 28px;
        gap: 1px;
    }
    
    .export-button {
        display: flex;
        width: 63px;
        height: 28px;
        justify-content: center;
        align-items: center;

        background: var(--bg2);
        color: var(--color2);

        font-size: 16px;
        font-weight: 500;

        border-top-left-radius: 6px;
        border-bottom-left-radius: 6px;
        border-top-right-radius: 3px;
        border-bottom-right-radius: 3px;
    }

    .arrow-button {
        display: flex;
        width: 27px;
        height: 28px;
        justify-content: center;
        align-items: center;

        background: var(--bg2);
        color: var(--color2);

        font-size: 16px;
        font-weight: 500;

        border-top-left-radius: 3px;
        border-bottom-left-radius: 3px;
        border-top-right-radius: 6px;
        border-bottom-right-radius: 6px;
    }
</style>