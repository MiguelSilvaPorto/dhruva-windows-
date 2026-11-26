export type ThemeMode = 'auto' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

export interface Settings {
    schemaVersion: number;
    theme: ThemeMode;
    iconSize: number;
    iconSpacing: number;
    dockMargin: number;
    dockPaddingX: number;
    dockPaddingY: number;
    zoomFactor: number;
    zoomRise: number;
    autoHide: boolean;
    hideDelay: number;
    revealDelay: number;
    hideTaskbar: boolean;
    showTrash: boolean;
    pinned: string[];
    pinnedInitialized: boolean;
    dockOrder: string[];
    folders: string[];
    dockTemplate: string;
    launcherTemplate: string;
    animations: boolean;
    animationSpeed: number;
    zoomSmoothness: number;
    dockRadius: number;
    launcherWidth: number;
    launcherTop: number;
    launcherRadius: number;
    launcherBlur: number;
}

export interface AppEntry {
    id: string;
    name: string;
    path: string;
    target: string;
    args: string;
    aumid: string | null;
    iconSource: string;
    builtin: boolean;
}

export interface WindowInfo {
    hwnd: number;
    title: string;
    exe: string;
    pid: number;
    minimized: boolean;
    focused: boolean;
}

export interface Bootstrap {
    settings: Settings;
    apps: AppEntry[];
    theme: ResolvedTheme;
    taskbarHidden: boolean;
    version: string;
}

export interface TrashIcon {
    icon: string | null;
    full: boolean;
}

export interface SystemStatus {
    batteryPercent: number | null;
    charging: boolean;
}

export interface BackgroundApp {
    key: string;
    name: string;
    exe: string;
    pid: number;
    hwnd: number | null;
    title: string | null;
    windows: number;
    processes: number;
    memory: number;
    state: string;
}

export interface MediaStatus {
    hasSession: boolean;
    playing: boolean;
    title: string | null;
    artist: string | null;
    album: string | null;
    app: string | null;
    positionMs: number;
    durationMs: number;
    artKey: string | null;
    canPlayPause: boolean;
    canNext: boolean;
    canPrevious: boolean;
    canSeek: boolean;
}

export interface MediaPreviewTab {
    hwnd: number;
    mediaTitle: string;
    previousTitle: string;
    switched: boolean;
}

export interface MediaPreviewSession {
    hwnd: number;
    title: string;
    tab: MediaPreviewTab | null;
}

export interface MediaAudio {
    volume: number;
    muted: boolean;
}

export interface MasterAudio {
    volume: number;
    muted: boolean;
}

export interface MediaTimeline {
    positionMs: number;
    durationMs: number;
}

export interface WifiNetwork {
    ssid: string;
    signal: number;
    secured: boolean;
    hasProfile: boolean;
    connected: boolean;
    auth: string;
    cipher: string;
    profileName: string | null;
    connectable: boolean;
}

export interface WiredInfo {
    name: string;
    ipv4: string;
    gateway: string;
}

export interface WifiStatus {
    adapter: boolean;
    enabled: boolean;
    hardwareEnabled: boolean;
    connected: boolean;
    ssid: string | null;
    signal: number | null;
    auth: string | null;
    state: string;
    message: string | null;
    interfaceName: string | null;
    networks: WifiNetwork[];
    wired: WiredInfo | null;
}

export interface DockRect {
    x: number;
    y: number;
    w: number;
    h: number;
    radius: number;
}

export interface VirtualDesktopInfo {
    index: number;
    name: string;
    current: boolean;
}
