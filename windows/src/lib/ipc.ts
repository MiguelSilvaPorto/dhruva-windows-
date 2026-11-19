import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
    BackgroundApp,
    Bootstrap,
    DockRect,
    Settings,
    MediaStatus,
    MediaPreviewSession,
    MediaPreviewTab,
    MediaAudio,
    MediaTimeline,
    SystemStatus,
    TrashIcon,
    WifiStatus,
    WindowInfo,
    VirtualDesktopInfo,
} from './types';

export const api = {
    bootstrap: () => invoke<Bootstrap>('bootstrap'),
    getIcon: (appId: string) => invoke<string | null>('get_icon', { appId }),
    getExeIcon: (path: string) => invoke<string | null>('get_exe_icon', { path }),
    getTrashIcon: () => invoke<TrashIcon>('get_trash_icon'),
    getSystemStatus: () => invoke<SystemStatus>('system_status'),
    getBackgroundApps: () => invoke<BackgroundApp[]>('background_apps'),
    backgroundActivate: (hwnd: number | null, exe: string) =>
        invoke<void>('background_activate', { hwnd, exe }),
    backgroundTerminate: (pid: number) =>
        invoke<void>('background_terminate', { pid }),
    getMediaStatus: () => invoke<MediaStatus>('media_status'),
    getMediaArt: () => invoke<string | null>('media_art'),
    mediaPreviewStart: () =>
        invoke<MediaPreviewSession | null>('media_preview_start'),
    mediaPreviewStop: (selection: MediaPreviewTab) =>
        invoke<boolean>('media_preview_stop', { selection }),
    getMediaAudio: () => invoke<MediaAudio | null>('media_audio'),
    getMediaTimeline: (allowSwitch = false) =>
        invoke<MediaTimeline | null>('media_timeline', { allowSwitch }),
    setMediaVolume: (volume: number) =>
        invoke<void>('set_media_volume', { volume }),
    setMediaMuted: (muted: boolean) =>
        invoke<void>('set_media_muted', { muted }),
    mediaControl: (action: string, positionMs?: number) =>
        invoke<void>('media_control', {
            action,
            positionMs: positionMs ?? null,
        }),
    getWifiStatus: (includeNetworks = true) =>
        invoke<WifiStatus>('wifi_status', { includeNetworks }),
    setWifiEnabled: (enabled: boolean) =>
        invoke<void>('wifi_set_enabled', { enabled }),
    connectWifi: (ssid: string, password?: string) =>
        invoke<void>('wifi_connect', { ssid, password: password ?? null }),
    disconnectWifi: () => invoke<void>('wifi_disconnect'),
    forgetWifi: (profile: string) => invoke<void>('wifi_forget', { profile }),
    focusDock: () => invoke<void>('focus_dock'),
    releaseDockFocus: () => invoke<void>('release_dock_focus'),
    openStartMenu: () => invoke<void>('open_start_menu'),
    openLauncher: () => invoke<void>('open_launcher'),
    closeLauncher: (restoreFocus = true) =>
        invoke<void>('close_launcher', { restoreFocus }),
    openTaskView: () => invoke<void>('open_task_view'),
    newVirtualDesktop: () => invoke<void>('new_virtual_desktop'),
    closeVirtualDesktop: () => invoke<void>('close_virtual_desktop'),
    switchVirtualDesktop: (direction: 'left' | 'right') =>
        invoke<void>('switch_virtual_desktop', { direction }),
    getVirtualDesktops: () => invoke<VirtualDesktopInfo[]>('virtual_desktops'),
    switchVirtualDesktopTo: (index: number) =>
        invoke<void>('switch_virtual_desktop_to', { index }),
    openActionCenter: () => invoke<void>('open_action_center'),
    openSettingsPage: (page: string) =>
        invoke<void>('open_settings_page', { page }),
    launchApp: (appId: string) => invoke<void>('launch_app', { appId }),
    focusWindow: (hwnd: number) => invoke<boolean>('focus_window', { hwnd }),
    pickFolder: () => invoke<string | null>('pick_folder'),
    openFolder: (path: string, newWindow = false) =>
        invoke<void>('open_folder', { path, newWindow }),
    openPath: (path: string) => invoke<void>('open_path', { path }),
    minimizeWindow: (hwnd: number) => invoke<void>('minimize_window', { hwnd }),
    closeWindow: (hwnd: number) => invoke<void>('close_window', { hwnd }),
    listWindows: () => invoke<WindowInfo[]>('list_windows'),
    windowPreview: (hwnd: number, width: number, height: number) =>
        invoke<string | null>('window_preview', { hwnd, width, height }),
    windowPreviewJpeg: (hwnd: number, width: number, height: number) =>
        invoke<string | null>('window_preview_jpeg', { hwnd, width, height }),
    openTrash: () => invoke<void>('open_trash'),
    emptyTrash: () => invoke<void>('empty_trash'),
    saveSettings: (settings: Settings) =>
        invoke<Settings>('save_settings', { settings }),
    resetSettings: () => invoke<Settings>('reset_settings'),
    setTaskbarHidden: (hidden: boolean) =>
        invoke<Settings>('set_taskbar_hidden', { hidden }),
    openSettings: () => invoke<void>('open_settings'),
    closeSettings: () => invoke<void>('close_settings'),
    resizeDock: (width: number, height: number) =>
        invoke<void>('resize_dock', { width, height }),
    setDockReservation: (height: number, enabled: boolean) =>
        invoke<void>('set_dock_reservation', { height, enabled }),
    setDockRegion: (rects: DockRect[], full = false) =>
        invoke<void>('set_dock_region', { rects, full }),
    dockReady: () => invoke<void>('dock_ready'),
    quit: () => invoke<void>('quit_app'),
};

export function onWindowsChanged(callback: (windows: WindowInfo[]) => void): Promise<UnlistenFn> {
    return listen<WindowInfo[]>('windows-changed', event => callback(event.payload));
}

export function onThemeChanged(callback: (theme: 'light' | 'dark') => void): Promise<UnlistenFn> {
    return listen<'light' | 'dark'>('theme-changed', event => callback(event.payload));
}

export function onSettingsChanged(callback: (settings: Settings) => void): Promise<UnlistenFn> {
    return listen<Settings>('settings-changed', event => callback(event.payload));
}

export function onTrashChanged(callback: (full: boolean) => void): Promise<UnlistenFn> {
    return listen<boolean>('trash-changed', event => callback(event.payload));
}

export function onRelayout(callback: () => void): Promise<UnlistenFn> {
    return listen('relayout', () => callback());
}

export function onSessionUnlocked(callback: () => void): Promise<UnlistenFn> {
    return listen('session-unlocked', () => callback());
}

export function onDismissOverlays(
    callback: (source: string) => void,
): Promise<UnlistenFn> {
    return listen<string>('dismiss-overlays', event => callback(event.payload));
}

export function onLauncherOpen(callback: () => void): Promise<UnlistenFn> {
    return listen('launcher-open', () => callback());
}
