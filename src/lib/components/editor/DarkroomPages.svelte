<!-- lib/components/editor/DarkroomPages.svelte -->
<script lang="ts">
    import TripleValSliderHistGroup from "$lib/components/inputs/values/TripleValSliderHistGroup.svelte";
    import DoubleValSliderGroup from "$lib/components/inputs/values/DoubleValSliderGroup.svelte";
    import { editor } from "$lib/editor/editor.svelte";
    import ControlsPage from "./ControlsPage.svelte";
    import UnsharpMask from "./UnsharpMask.svelte";
</script>

<!-- Darkroom Sharpness Panel -->
<ControlsPage>
    <UnsharpMask />
</ControlsPage>
<!-- Input Levels Panel -->
<ControlsPage>
    <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
        <TripleValSliderHistGroup
            name="Red Input"
            bind:valueA={editor.edit.sliders.redBlackPoint}
            bind:valueB={editor.edit.sliders.redGamma}
            bind:valueC={editor.edit.sliders.redWhitePoint}
            onCommitA={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "redBlackPoint", oldValue, newValue })}
            onCommitB={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "redGamma", oldValue, newValue })}
            onCommitC={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "redWhitePoint", oldValue, newValue })}
            defaultValueA={0}
            defaultValueB={1}
            defaultValueC={255}
            min={0}
            max={255}
            minB={0.1}
            maxB={2}
            innerColor={"#430000"}
            outerColor={"#000000"}
            histogramData={editor.inputHistograms?.r ?? null}
            histogramChannel="r"
        ></TripleValSliderHistGroup>
        <TripleValSliderHistGroup
            name="Green Input"
            bind:valueA={editor.edit.sliders.greenBlackPoint}
            bind:valueB={editor.edit.sliders.greenGamma}
            bind:valueC={editor.edit.sliders.greenWhitePoint}
            onCommitA={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "greenBlackPoint", oldValue, newValue })}
            onCommitB={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "greenGamma", oldValue, newValue })}
            onCommitC={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "greenWhitePoint", oldValue, newValue })}
            defaultValueA={0}
            defaultValueB={1}
            defaultValueC={255}
            min={0}
            max={255}
            minB={0.1}
            maxB={2}
            innerColor={"#430000"}
            outerColor={"#000000"}
            histogramData={editor.inputHistograms?.g ?? null}
            histogramChannel="g"
        ></TripleValSliderHistGroup>
        <TripleValSliderHistGroup
            name="Blue Input"
            bind:valueA={editor.edit.sliders.blueBlackPoint}
            bind:valueB={editor.edit.sliders.blueGamma}
            bind:valueC={editor.edit.sliders.blueWhitePoint}
            onCommitA={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "blueBlackPoint", oldValue, newValue })}
            onCommitB={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "blueGamma", oldValue, newValue })}
            onCommitC={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "blueWhitePoint", oldValue, newValue })}
            defaultValueA={0}
            defaultValueB={1}
            defaultValueC={255}
            min={0}
            max={255}
            minB={0.1}
            maxB={2}
            innerColor={"#430000"}
            outerColor={"#000000"}
            histogramData={editor.inputHistograms?.b ?? null}
            histogramChannel="b"
        ></TripleValSliderHistGroup>
        <TripleValSliderHistGroup
            name="RGB Input"
            bind:valueA={editor.edit.sliders.masterBlackPoint}
            bind:valueB={editor.edit.sliders.masterGamma}
            bind:valueC={editor.edit.sliders.masterWhitePoint}
            onCommitA={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "masterBlackPoint", oldValue, newValue })}
            onCommitB={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "masterGamma", oldValue, newValue })}
            onCommitC={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "masterWhitePoint", oldValue, newValue })}
            defaultValueA={0}
            defaultValueB={1}
            defaultValueC={255}
            min={0}
            max={255}
            minB={0.1}
            maxB={2}
            innerColor={"#430000"}
            outerColor={"#000000"}
            liveHistogram
            bind:histogramCanvas={editor.masterHistogramCanvas}
        ></TripleValSliderHistGroup>
        <DoubleValSliderGroup
            name="RGB Output"
            bind:valueA={editor.edit.sliders.rgbOutputBlack}
            bind:valueB={editor.edit.sliders.rgbOutputWhite}
            onCommitA={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "rgbOutputBlack", oldValue, newValue })}
            onCommitB={(oldValue, newValue) =>
                editor.commit({ type: "slider", key: "rgbOutputWhite", oldValue, newValue })}
            defaultValueA={0}
            defaultValueB={255}
            min={0}
            max={255}
            innerColor={"#430000"}
            outerColor={"#000000"}
        ></DoubleValSliderGroup>
    </div>
</ControlsPage>
