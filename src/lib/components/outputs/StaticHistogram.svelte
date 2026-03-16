<!-- src/lib/components/StaticHistogram.svelte -->
<!--
  Static single-channel histogram.
  Receives precomputed bin data (e.g. from backend when image is loaded).
  No GPU usage; draw-only, does not update with image adjustments.
-->
<script lang="ts">
	let {
		data = null,
		channel = "r",
	}: {
		data?: Uint32Array | null;
		channel?: "r" | "g" | "b";
	} = $props();

	let canvas: HTMLCanvasElement;

	const channelColor: Record<"r" | "g" | "b", string> = {
		r: "rgb(255,0,0)",
		g: "rgb(0,255,0)",
		b: "rgb(0,0,255)",
	};

	function draw() {
		if (!canvas || !data || data.length < 256) return;
		const ctx = canvas.getContext("2d");
		if (!ctx) return;

		const { width, height } = canvas;
		const max = Math.max(...data.subarray(0, 256));
		if (max === 0) {
			ctx.clearRect(0, 0, width, height);
			return;
		}

		ctx.clearRect(0, 0, width, height);
		ctx.fillStyle = channelColor[channel];
		ctx.beginPath();
		ctx.moveTo(0, height);
		for (let i = 0; i < 256; i++) {
			const x = (i / 255) * width;
			const y = height - (data[i] / max) * height;
			ctx.lineTo(x, y);
		}
		ctx.lineTo(width, height);
		ctx.closePath();
		ctx.fill();
	}

	$effect(() => {
		if (!canvas) return;
		draw();
	});

	$effect(() => {
		if (!canvas) return;
		const ro = new ResizeObserver((entries) => {
			for (const entry of entries) {
				let w: number, h: number;
				if (entry.devicePixelContentBoxSize) {
					w = entry.devicePixelContentBoxSize[0].inlineSize;
					h = entry.devicePixelContentBoxSize[0].blockSize;
				} else {
					const dpr = window.devicePixelRatio || 1;
					w = Math.round(entry.contentBoxSize[0].inlineSize * dpr);
					h = Math.round(entry.contentBoxSize[0].blockSize * dpr);
				}
				if (canvas.width !== w || canvas.height !== h) {
					canvas.width = w;
					canvas.height = h;
					draw();
				}
			}
		});
		try {
			ro.observe(canvas, { box: "device-pixel-content-box" });
		} catch {
			ro.observe(canvas, { box: "content-box" });
		}
		return () => ro.disconnect();
	});
</script>

<canvas bind:this={canvas}></canvas>

<style>
	canvas {
		width: 100%;
		aspect-ratio: 4 / 1;
		border-bottom-left-radius: 4px;
		border-bottom-right-radius: 4px;
		border-top-right-radius: 8px;
		border-top-left-radius: 8px;
		background: var(--bg1);
		margin-bottom: 4px;
	}
</style>
