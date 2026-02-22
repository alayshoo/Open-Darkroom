// $lib/utils/animateObject.ts

export function animateObject<T extends Record<string, number>>(
    obj: T,
    targets: Partial<T>,
    duration = 200
) {
    const keys = Object.keys(targets) as (keyof T & string)[];
    const starts: Record<string, number> = {};
    for (const k of keys) starts[k] = obj[k];
    const startTime = performance.now();

    function tick(now: number) {
        const t = Math.min((now - startTime) / duration, 1);
        const eased = 1 - (1 - t) ** 3;
        for (const k of keys) {
            (obj as any)[k] = starts[k] + (targets[k]! - starts[k]) * eased;
        }
        if (t < 1) requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
}