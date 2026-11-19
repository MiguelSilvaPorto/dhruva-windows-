import type { ResolvedTheme, ThemeMode } from './types';

export function resolveTheme(mode: ThemeMode, system: ResolvedTheme): ResolvedTheme {
    return mode === 'auto' ? system : mode;
}

export function applyTheme(theme: ResolvedTheme): void {
    const root = document.documentElement;
    root.dataset.theme = theme;
    root.style.colorScheme = theme;
}
