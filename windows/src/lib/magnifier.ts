export interface MagnifyOptions {
    iconSize: number;
    gap: number;
    factor: number;
    radiusFactor: number;
    rise: number;
}

export interface MagnifyLayout {
    scales: number[];
    translates: number[];
    rises: number[];
    extra: number;
    total: number;
}

/**
 * Dhruva's magnification curve: a cosine bell that spans `radiusFactor`
 * icons on each side. The factor is doubled around 1, which is what makes
 * the 1.6 setting peak at roughly 2.2x in the centre, like macOS.
 */
export function maxScale(factor: number): number {
    return 1 + (factor - 1) * 2;
}

export function scaleAt(distance: number, options: MagnifyOptions): number {
    const radius = options.iconSize * options.radiusFactor;
    if (radius <= 0 || distance >= radius) return 1;

    const ratio = distance / radius;
    const bell = (Math.cos(ratio * Math.PI) + 1) * 0.5;
    const peak = maxScale(options.factor);
    return 1 + (peak - 1) * bell;
}

export function targetScales(
    baseCenters: number[],
    pointerX: number | null,
    baseWidth: number,
    options: MagnifyOptions,
): number[] {
    if (pointerX === null) return baseCenters.map(() => 1);

    const reach = options.iconSize * 1.2;
    if (pointerX < -reach || pointerX > baseWidth + reach) {
        return baseCenters.map(() => 1);
    }

    return baseCenters.map(center =>
        scaleAt(Math.abs(pointerX - center), options),
    );
}

export function layoutForScales(
    baseCenters: number[],
    baseWidth: number,
    baseWidths: number[],
    scales: number[],
    options: MagnifyOptions,
): MagnifyLayout {
    const count = baseCenters.length;
    if (count === 0) {
        return { scales: [], translates: [], rises: [], extra: 0, total: baseWidth };
    }

    const widths = scales.map(
        (scale, index) => (baseWidths[index] ?? options.iconSize) * scale,
    );
    const total =
        widths.reduce((sum, width) => sum + width, 0) +
        options.gap * Math.max(0, count - 1);

    // Scale the icons in their own slots instead of sliding the whole row.
    // Sliding made the dock look like it was rewinding and pushed the first
    // icons underneath the media pill.
    const peak = maxScale(options.factor);
    const span = Math.max(0.0001, peak - 1);
    const translates = baseCenters.map(() => 0);
    const rises = scales.map(scale => options.rise * ((scale - 1) / span));

    return { scales, translates, rises, extra: total - baseWidth, total };
}

export function approach(current: number, target: number, factor: number): number {
    if (Math.abs(target - current) < 0.0008) return target;
    return current + (target - current) * factor;
}
