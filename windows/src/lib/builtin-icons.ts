import calculatorSvg from '../assets/icons/calculator.svg?raw';
import explorerSvg from '../assets/icons/explorer.svg?raw';
import notepadSvg from '../assets/icons/notepad.svg?raw';
import settingsSvg from '../assets/icons/settings.svg?raw';
import type { AppEntry } from './types';

const dataUrl = (svg: string) =>
    `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;

const icons = {
    explorer: dataUrl(explorerSvg),
    settings: dataUrl(settingsSvg),
    notepad: dataUrl(notepadSvg),
    calculator: dataUrl(calculatorSvg),
};

/**
 * Windows 10's built-in executables still ship the Windows 7-era artwork.
 * These modern tiles are used for the stock apps the dock pins, while every
 * third-party app keeps its own real icon. The Start Menu can provide the
 * same app either as a shortcut or as a builtin, so the executable name is
 * checked as well.
 */
export function builtinIcon(app: AppEntry): string | null {
    const name = app.name.toLowerCase();
    const target = app.target.replace(/\//g, '\\').toLowerCase();
    const exe = target.split('\\').pop() ?? '';

    if (
        exe === 'explorer.exe' ||
        (app.builtin &&
            (name.includes('explorer') || name.includes('gerenciador de arquivos')))
    ) {
        return icons.explorer;
    }

    if (
        exe === 'systemsettings.exe' ||
        (app.builtin && (name.includes('configura') || name.includes('settings')))
    ) {
        return icons.settings;
    }

    if (
        exe === 'notepad.exe' ||
        (app.builtin && (name.includes('notas') || name.includes('notepad')))
    ) {
        return icons.notepad;
    }

    if (
        exe === 'calc.exe' ||
        (app.builtin && (name.includes('calculadora') || name.includes('calculator')))
    ) {
        return icons.calculator;
    }

    return null;
}
