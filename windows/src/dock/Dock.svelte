<script lang="ts">
    import { getCurrentWindow } from '@tauri-apps/api/window';
    import { onMount, tick } from 'svelte';
    import { builtinIcon } from '../lib/builtin-icons';
    import {
        api,
        onDismissOverlays,
        onSessionUnlocked,
        onTrashChanged,
    } from '../lib/ipc';
    import {
        approach,
        layoutForScales,
        maxScale,
        targetScales,
    } from '../lib/magnifier';
    import type {
        AppEntry,
        BackgroundApp,
        MasterAudio,
        MediaStatus,
        MediaPreviewTab,
        Settings,
        TrashIcon,
        WifiNetwork,
        WifiStatus,
        WindowInfo,
        VirtualDesktopInfo,
    } from '../lib/types';

    let {
        apps,
        settings,
        windows,
        relayout,
        refreshWindows,
        version,
    }: {
        apps: AppEntry[];
        settings: Settings;
        windows: WindowInfo[];
        relayout: number;
        refreshWindows: () => Promise<void>;
        version: string;
    } = $props();

    type DisplayItem =
        | {
              kind: 'app';
              key: string;
              app: AppEntry;
              pinned: boolean;
              running: boolean;
              focused: boolean;
              count: number;
              focusedIndex: number;
          }
        | {
              kind: 'dynamic';
              key: string;
              name: string;
              exe: string;
              windows: WindowInfo[];
              focused: boolean;
              count: number;
              focusedIndex: number;
          }
        | {
              kind: 'folder';
              key: string;
              name: string;
              path: string;
          }
        | { kind: 'trash'; key: string };

    type MenuState =
        | { kind: 'app'; app: AppEntry; pinned: boolean }
        | {
              kind: 'dynamic';
              name: string;
              exe: string;
              windows: WindowInfo[];
          }
        | { kind: 'trash' }
        | { kind: 'workspace' }
        | { kind: 'folder'; path: string; name: string }
        | { kind: 'dock' };

    const WINDOW_HEIGHT = 640;
    const iconCache = new Map<string, string>();
    const requestedIcons = new Set<string>();
    const requestedFolderIcons = new Set<string>();

    let rowEl = $state<HTMLDivElement | null>(null);
    let barEl = $state<HTMLDivElement | null>(null);
    let bgEl = $state<HTMLDivElement | null>(null);
    let menuEl = $state<HTMLDivElement | null>(null);

    let icons = $state<Record<string, string>>({});
    let trashIcon = $state<string | null>(null);
    let trashFull = $state(false);
    let hidden = $state(false);
    let menu = $state<MenuState | null>(null);
    let menuLeft = $state(0);

    let wifiPanelOpen = $state(false);
    let wifiPanelEl = $state<HTMLDivElement | null>(null);
    let wifiPanelLeft = $state(0);
    let wifiStatus = $state<WifiStatus | null>(null);
    let wifiLoading = $state(false);
    let wifiBusy = $state<string | null>(null);
    let wifiError = $state<string | null>(null);
    let wifiPasswordFor = $state<string | null>(null);
    let wifiPassword = $state('');
    let wifiPasswordVisible = $state(false);
    let wifiForgetFor = $state<string | null>(null);
    let wifiSeq = 0;
    let wifiForgetTimer: ReturnType<typeof setTimeout> | undefined;

    let volumePanelOpen = $state(false);
    let volumePanelEl = $state<HTMLDivElement | null>(null);
    let volumePanelLeft = $state(0);
    let masterVolume = $state<number | null>(null);
    let masterMuted = $state(false);
    let masterError = $state<string | null>(null);
    let masterLoading = $state(true);
    let masterBusy = $state<string | null>(null);
    let masterVolumeSyncAt = 0;
    let masterVolumeApplyTimer: ReturnType<typeof setTimeout> | undefined;

    let media = $state<MediaStatus | null>(null);
    let mediaArt = $state<string | null>(null);
    let mediaArtKey = '';
    let mediaArtSeq = 0;
    let mediaOpen = $state(false);
    let mediaPinned = $state(false);
    let mediaDotEl = $state<HTMLButtonElement | null>(null);
    let mediaPanelEl = $state<HTMLDivElement | null>(null);
    let mediaPanelLeft = $state(0);
    let mediaPosition = $state(0);
    let mediaBasePosition = 0;
    let mediaBaseAt = 0;
    let mediaRawPosition = -1;
    let mediaSeekUntil = 0;
    let mediaTimelineKey = '';
    let mediaTimelineDuration = $state(0);
    let mediaSyncing = $state(false);
    let mediaBusy = $state<string | null>(null);
    let mediaSeeking = $state(false);
    let mediaSourceIcon = $state<string | null>(null);
    let mediaVolume = $state<number | null>(null);
    let mediaMuted = $state(false);
    let mediaVolumeSyncAt = 0;
    let mediaVolumeApplyTimer: ReturnType<typeof setTimeout> | undefined;
    let mediaPreviewOpen = $state(false);
    let mediaPreviewFrame = $state<string | null>(null);
    let mediaPreviewResolving = $state(false);
    let mediaPreviewMissing = $state(false);
    let mediaPreviewHwnd: number | null = null;
    let mediaPreviewTab: MediaPreviewTab | null = null;
    let mediaPreviewArtKey = '';
    let mediaPreviewSeq = 0;
    let mediaPreviewTimer: ReturnType<typeof setTimeout> | undefined;
    let mediaHoverTimer: ReturnType<typeof setTimeout> | undefined;
    let mediaHoverCloseTimer: ReturnType<typeof setTimeout> | undefined;
    let mediaPanelHover = false;

    let bgPanelOpen = $state(false);
    let bgPanelEl = $state<HTMLDivElement | null>(null);
    let bgPanelLeft = $state(0);
    let bgApps = $state<BackgroundApp[]>([]);
    let bgIcons = $state<Record<string, string>>({});
    let bgLoading = $state(false);
    let bgBusy = $state<string | null>(null);
    let bgConfirmPid = $state<number | null>(null);
    let bgError = $state<string | null>(null);
    let bgSeq = 0;
    const bgRequested = new Map<string, number>();
    let bgConfirmTimer: ReturnType<typeof setTimeout> | undefined;

    let draggingKey = $state<string | null>(null);
    let dragOverKey = $state<string | null>(null);
    let dragAfter = $state(false);
    let suppressClick = false;

    let workspacePulse = $state<'left' | 'right' | null>(null);
    let workspacePulseTimer: ReturnType<typeof setTimeout> | undefined;
    let lastWorkspaceSwitch = 0;

    let workspacePanelOpen = $state(false);
    let workspacePanelEl = $state<HTMLDivElement | null>(null);
    let workspaceBtnEl = $state<HTMLButtonElement | null>(null);
    let workspacePanelLeft = $state(0);
    let workspacePanelBottom = $state(0);
    let workspaceDesktops = $state<VirtualDesktopInfo[]>([]);
    let workspaceLoading = $state(false);
    let workspaceError = $state<string | null>(null);
    let workspaceOpenTimer: ReturnType<typeof setTimeout> | undefined;
    let workspaceHideTimer: ReturnType<typeof setTimeout> | undefined;
    let workspacePanelHover = false;
    let workspaceRequest = 0;
    let tooltipVisible = $state(false);
    let tooltipLabel = $state('');
    let tooltipLeft = $state(0);
    let tooltipBottom = $state(0);

    let previewOpen = $state(false);
    let previewName = $state('');
    let previewIcon = $state<string | null>(null);
    let previewWindows = $state<WindowInfo[]>([]);
    let previewThumbs = $state<Record<number, string | null>>({});
    let previewLoading = $state<Record<number, boolean>>({});
    let previewLeft = $state(0);
    let previewBottom = $state(0);
    let previewEl = $state<HTMLDivElement | null>(null);
    let previewKey = '';
    let previewSeq = 0;
    let previewHover = false;
    let previewHoverHwnd = $state<number | null>(null);
    let previewHideTimer: ReturnType<typeof setTimeout> | undefined;
    let previewPrefetchTimer: ReturnType<typeof setTimeout> | undefined;
    let previewRowEl = $state<HTMLDivElement | null>(null);
    let previewScrollLeft = $state(0);
    let previewScrollWidth = $state(0);
    let previewScrollClientWidth = $state(0);
    let previewDragging = $state(false);
    let previewJustDragged = false;
    let previewDragResetTimer: ReturnType<typeof setTimeout> | undefined;
    let previewDrag: {
        pointerId: number;
        startX: number;
        startScroll: number;
        moved: boolean;
    } | null = null;
    const previewCanScrollLeft = $derived(previewScrollLeft > 1);
    const previewCanScrollRight = $derived(
        previewScrollLeft + previewScrollClientWidth <
            previewScrollWidth - 1,
    );
    const previewCache = new Map<
        number,
        { url: string | null; at: number }
    >();
    const previewInflight = new Map<number, Promise<string | null>>();

    // The preview opens after this delay; captures start a little before it,
    // so the thumbnails are usually ready before the panel appears.
    const PREVIEW_SHOW_DELAY = 320;
    const PREFETCH_DELAY = 70;
    // Thumbnails stay usable while a fresh frame is captured in background.
    const THUMB_FRESH_MS = 10_000;
    const THUMB_KEEP_MS = 120_000;
    const THUMB_MAX_WIDTH = 240;
    const THUMB_MAX_HEIGHT = 132;

    let baseWidth = $state(0);
    let baseCenters: number[] = [];
    let baseWidths: number[] = [];
    let currentScales: number[] = [];
    let pointerX = -9999;
    let hovering = $state(false);
    let rafId = 0;
    let lastRelayout = -1;
    let ready = false;
    let windowSized = false;
    let batteryPercent = $state<number | null>(null);
    let charging = $state(false);

    let hideTimer: ReturnType<typeof setTimeout> | undefined;
    let revealTimer: ReturnType<typeof setTimeout> | undefined;
    let tooltipTimer: ReturnType<typeof setTimeout> | undefined;
    let regionTimer: ReturnType<typeof setTimeout> | undefined;
    let tooltipIndex = -1;

    let bounces = $state<Record<string, number>>({});
    let bouncing = $state<Record<string, boolean>>({});
    let pressing = $state<Record<string, boolean>>({});
    let now = $state(new Date());

    const timeText = $derived(
        now.toLocaleTimeString('pt-BR', {
            hour: '2-digit',
            minute: '2-digit',
            hour12: false,
        }),
    );

    const dateText = $derived(
        now.toLocaleDateString('pt-BR', {
            day: '2-digit',
            month: '2-digit',
            year: 'numeric',
        }),
    );

    const wifiConnected = $derived(wifiStatus?.connected ?? false);
    const wifiEnabled = $derived(wifiStatus?.enabled ?? false);
    const wifiSignal = $derived(wifiStatus?.signal ?? 0);
    const wifiHasAdapter = $derived(wifiStatus?.adapter ?? false);
    const currentWifi = $derived(
        wifiStatus?.networks.find(network => network.connected) ?? null,
    );
    const mediaVisible = $derived(
        !!media?.hasSession && !!media.title,
    );
    const mediaDuration = $derived(
        mediaTimelineDuration > 0
            ? mediaTimelineDuration
            : (media?.durationMs ?? 0),
    );

    const mediaProgress = $derived(
        mediaDuration > 0
            ? Math.min(100, (mediaPosition / mediaDuration) * 100)
            : 0,
    );


    function signalLevel(signal: number): number {
        if (signal >= 75) return 4;
        if (signal >= 50) return 3;
        if (signal >= 25) return 2;
        return 1;
    }

    function wifiTitle(): string {
        if (!wifiStatus) return 'Rede';
        if (!wifiStatus.adapter) {
            return wifiStatus.wired
                ? `Ethernet: ${wifiStatus.wired.name}`
                : 'Rede';
        }
        if (!wifiStatus.enabled) return 'Wi-Fi desligado';
        if (wifiStatus.connected) {
            return `Wi-Fi: ${wifiStatus.ssid ?? 'conectado'}`;
        }
        return 'Wi-Fi: sem conexão';
    }

    const normalizePath = (value: string) =>
        value.replace(/\//g, '\\').trim().toLowerCase();

    const folderName = (value: string) => {
        const clean = value.replace(/[\\/]+$/, '');
        const parts = clean.split(/[\\/]/).filter(Boolean);
        return parts[parts.length - 1] ?? value;
    };

    const folderKey = (value: string) => `folder:${normalizePath(value)}`;

    const exeAliases: Record<string, string[]> = {
        'wt.exe': ['windowsterminal.exe'],
        'calc.exe': ['calculator.exe', 'calculatorapp.exe'],
    };

    function windowsFor(app: AppEntry): WindowInfo[] {
        if (!app.target) return [];
        const target = normalizePath(app.target);
        const targetBase = target.split('\\').pop() ?? target;
        const aliases = exeAliases[targetBase] ?? [];

        return windows.filter(window => {
            const exe = normalizePath(window.exe);
            if (!exe) return false;
            const base = exe.split('\\').pop() ?? exe;
            if (exe.includes('\\')) {
                return exe === target || aliases.includes(base);
            }
            return exe === targetBase || exe === target || aliases.includes(base);
        });
    }

    function dynamicName(windows: WindowInfo[], exe: string): string {
        const stem = (exe.split('\\').pop() ?? 'App').replace(/\.exe$/i, '');
        const title = windows[0]?.title?.trim() ?? '';
        if (title && title.length <= 32) return title;
        return stem.charAt(0).toUpperCase() + stem.slice(1);
    }

    const items = $derived.by((): DisplayItem[] => {
        const pinnedSet = new Set(settings.pinned);
        const byId = new Map(apps.map(app => [app.id, app]));
        const pinnedApps = settings.pinned
            .map(id => byId.get(id))
            .filter((app): app is AppEntry => Boolean(app));

        const pinnedTargets = new Set(
            pinnedApps
                .map(app => normalizePath(app.target))
                .filter(Boolean),
        );
        const seenTargets = new Set(pinnedTargets);
        const extras: AppEntry[] = [];

        for (const app of apps) {
            if (pinnedSet.has(app.id)) continue;
            const target = normalizePath(app.target);
            if (!target || seenTargets.has(target)) continue;
            if (windowsFor(app).length === 0) continue;
            seenTargets.add(target);
            extras.push(app);
        }

        const runningExtra = extras;

        const list: DisplayItem[] = [];

        for (const app of pinnedApps) {
            const matches = windowsFor(app);
            list.push({
                kind: 'app',
                key: app.id,
                app,
                pinned: true,
                running: matches.length > 0,
                focused: matches.some(window => window.focused),
                count: matches.length,
                focusedIndex: matches.findIndex(window => window.focused),
            });
        }

        for (const app of runningExtra) {
            const matches = windowsFor(app);
            list.push({
                kind: 'app',
                key: app.id,
                app,
                pinned: false,
                running: true,
                focused: matches.some(window => window.focused),
                count: matches.length,
                focusedIndex: matches.findIndex(window => window.focused),
            });
        }

        for (const path of settings.folders) {
            const trimmed = path.trim();
            if (!trimmed) continue;
            const key = folderKey(trimmed);
            if (list.some(item => item.key === key)) continue;
            list.push({
                kind: 'folder',
                key,
                name: folderName(trimmed),
                path: trimmed,
            });
        }

        // Anything still running outside the Start Menu catalog gets its own
        // entry, so the dock mirrors every real application, not only the
        // shortcuts it happened to index.
        const shownWindows = new Set<number>();
        for (const app of [...pinnedApps, ...runningExtra]) {
            for (const window of windowsFor(app)) {
                shownWindows.add(window.hwnd);
            }
        }

        const dynamic = new Map<string, WindowInfo[]>();
        for (const window of windows) {
            if (shownWindows.has(window.hwnd)) continue;
            const exe = normalizePath(window.exe);
            if (!exe) continue;
            const base = exe.split('\\').pop() ?? '';
            if (base === 'applicationframehost.exe') continue;
            if (!window.title.trim()) continue;
            const group = dynamic.get(exe) ?? [];
            group.push(window);
            dynamic.set(exe, group);
        }

        for (const [key, group] of [...dynamic.entries()].sort((a, b) =>
            dynamicName(a[1], a[1][0].exe).localeCompare(
                dynamicName(b[1], b[1][0].exe),
            ),
        )) {
            list.push({
                kind: 'dynamic',
                key: `dynamic:${key}`,
                name: dynamicName(group, group[0].exe),
                exe: group[0].exe,
                windows: group,
                focused: group.some(window => window.focused),
                count: group.length,
                focusedIndex: group.findIndex(window => window.focused),
            });
        }

        if (settings.dockOrder.length > 0) {
            const order = new Map(
                settings.dockOrder.map((key, index) => [key, index]),
            );
            const baseIndex = new Map(
                list.map((item, index) => [item.key, index]),
            );

            list.sort((a, b) => {
                const left = order.get(a.key) ?? Number.POSITIVE_INFINITY;
                const right = order.get(b.key) ?? Number.POSITIVE_INFINITY;
                if (left === right) {
                    return (
                        (baseIndex.get(a.key) ?? 0) -
                        (baseIndex.get(b.key) ?? 0)
                    );
                }
                return left - right;
            });
        }

        if (settings.showTrash) {
            list.push({ kind: 'trash', key: 'trash' });
        }

        return list;
    });

    const dockHeight = $derived(
        settings.iconSize + 6 + settings.dockPaddingY * 2,
    );

    const animationDuration = $derived(
        settings.animations ? Math.round(200 * settings.animationSpeed) : 0,
    );
    const zoomSmoothing = $derived(
        settings.animations ? settings.zoomSmoothness : 1,
    );

    const menuBottom = $derived(dockHeight + 8);

    $effect(() => {
        for (const item of items) {
            if (item.kind === 'dynamic') {
                const id = item.key;
                if (icons[id] || requestedIcons.has(id)) continue;
                requestedIcons.add(id);

                const cached = iconCache.get(id);
                if (cached) {
                    icons = { ...icons, [id]: cached };
                    continue;
                }

                api.getExeIcon(item.exe)
                    .then(data => {
                        if (!data) return;
                        iconCache.set(id, data);
                        icons = { ...icons, [id]: data };
                    })
                    .catch(() => {
                        // The letter tile stays as a fallback.
                    });
                continue;
            }

            if (item.kind !== 'app') continue;
            const id = item.app.id;
            if (icons[id] || requestedIcons.has(id)) continue;

            requestedIcons.add(id);

            const modern = builtinIcon(item.app);
            if (modern) {
                icons = { ...icons, [id]: modern };
                continue;
            }

            const cached = iconCache.get(id);
            if (cached) {
                icons = { ...icons, [id]: cached };
                continue;
            }

            api
                .getIcon(id)
                .then(data => {
                    if (!data) return;
                    iconCache.set(id, data);
                    icons = { ...icons, [id]: data };
                })
                .catch(() => {
                    // The letter tile stays as a fallback.
                });
        }
    });

    $effect(() => {
        for (const path of settings.folders) {
            if (!path.trim()) continue;
            const key = folderKey(path);
            if (icons[key] || requestedFolderIcons.has(key)) continue;
            requestedFolderIcons.add(key);

            api.getExeIcon(path)
                .then(icon => {
                    if (icon) icons = { ...icons, [key]: icon };
                })
                .catch(() => {
                    // The folder tile stays as a fallback.
                });
        }
    });

    $effect(() => {
        const appName = media?.app?.trim().toLowerCase();
        if (!appName) {
            mediaSourceIcon = null;
            return;
        }

        const match = apps.find(app => {
            const name = app.name.trim().toLowerCase();
            return (
                name === appName ||
                name.includes(appName) ||
                appName.includes(name)
            );
        });

        if (!match) {
            mediaSourceIcon = null;
            return;
        }

        if (icons[match.id]) {
            mediaSourceIcon = icons[match.id];
            return;
        }

        let cancelled = false;
        api.getIcon(match.id)
            .then(icon => {
                if (!cancelled && icon) mediaSourceIcon = icon;
            })
            .catch(() => {});

        return () => {
            cancelled = true;
        };
    });

    $effect(() => {
        // Track everything that changes the row geometry.
        items.length;
        items.map(item => item.key).join(',');
        settings.iconSize;
        settings.iconSpacing;
        settings.dockPaddingX;
        settings.dockPaddingY;
        settings.zoomFactor;
        relayout;

        const force = relayout !== lastRelayout;
        lastRelayout = relayout;

        tick().then(() => {
            if (!rowEl) return;
            measure();
            void resizeAndRegion(force);
        });
    });

    function applyDockReservation() {
        const height =
            settings.iconSize +
            6 +
            settings.dockPaddingY * 2 +
            settings.dockMargin;
        void api.setDockReservation(height, !settings.autoHide).catch(() => {});
    }

    $effect(() => {
        // Reserve the strip occupied by the bar so maximized windows stop
        // above it instead of extending underneath. Auto-hide gives the
        // screen back to windows and reveals the dock on demand.
        applyDockReservation();

        // The window is hidden while the first effect runs; the shell only
        // applies the appbar after it is visible, so reserve again once the
        // dock has settled.
        const timer = setTimeout(applyDockReservation, 900);
        return () => clearTimeout(timer);
    });

    $effect(() => {
        if (!wifiPanelOpen) return;
        const timer = setInterval(() => {
            void loadWifiStatus(true);
        }, 5000);
        return () => clearInterval(timer);
    });

    $effect(() => {
        // The panel grows and shrinks with its content; keep the native
        // click-through region in sync after every render.
        if (!wifiPanelOpen) return;
        wifiStatus;
        wifiBusy;
        wifiPasswordFor;
        wifiError;
        wifiForgetFor;
        void tick().then(() => sendRegion(true));
    });

    $effect(() => {
        if (!volumePanelOpen) return;
        void refreshMasterAudio();
        const timer = setInterval(() => void refreshMasterAudio(), 1200);
        return () => clearInterval(timer);
    });

    $effect(() => {
        if (!volumePanelOpen) return;
        masterError;
        void tick().then(() => sendRegion(true));
    });

    $effect(() => {
        if (!mediaOpen) return;
        mediaArt;
        media?.title;
        media?.album;
        mediaPreviewOpen;
        void tick().then(() => sendRegion(true));
    });

    $effect(() => {
        if (!mediaPreviewOpen) return;
        const artKey = media?.artKey ?? '';
        if (artKey === mediaPreviewArtKey) return;
        void resolveMediaPreview();
    });

    $effect(() => {
        if (!mediaOpen) return;
        const artKey = media?.artKey ?? '';
        if (artKey === mediaTimelineKey) return;
        mediaTimelineKey = artKey;
        void syncMediaTimeline(false);
    });

    $effect(() => {
        if (!mediaOpen) return;
        void refreshMediaVolume();
        const timer = setInterval(() => void refreshMediaVolume(), 1500);
        return () => clearInterval(timer);
    });

    $effect(() => {
        if (!bgPanelOpen) return;
        const timer = setInterval(() => {
            void loadBackgroundApps();
        }, 4000);
        return () => clearInterval(timer);
    });

    $effect(() => {
        if (!bgPanelOpen) return;
        bgApps;
        bgLoading;
        bgConfirmPid;
        bgError;
        void tick().then(() => sendRegion(true));
    });

    $effect(() => {
        if (!previewOpen) return;
        previewThumbs;
        previewLoading;
        void tick().then(() => sendRegion(hovering));
    });

    $effect(() => {
        if (!workspacePanelOpen) return;
        workspaceDesktops;
        workspaceLoading;
        workspaceError;
        void tick().then(() => sendRegion(hovering));
    });

    $effect(() => {
        if (settings.autoHide) return;
        clearTimeout(hideTimer);
        if (hidden) {
            hidden = false;
            setTimeout(() => {
                measure();
                sendRegion(hovering);
            }, 280);
        }
    });

    onMount(() => {
        let disposed = false;
        let unlistenTrash: (() => void) | undefined;
        let unlistenSession: (() => void) | undefined;
        let unlistenDismiss: (() => void) | undefined;

        const currentWindowLabel = getCurrentWindow().label;

        const dismissOverlays = (source?: string) => {
            if (source === currentWindowLabel) return;
            if (wifiPanelOpen) closeWifiPanel();
            if (volumePanelOpen) closeVolumePanel();
            if (mediaOpen) closeMediaPanel();
            if (bgPanelOpen) closeBackgroundPanel();
            if (menu) closeMenu();
            hidePreview();
            hideWorkspacePanel();
            hideTooltipNow();
        };

        onDismissOverlays(source => dismissOverlays(source)).then(unlisten => {
            if (disposed) unlisten();
            else unlistenDismiss = unlisten;
        });

        const onWindowBlur = () => dismissOverlays();
        window.addEventListener('blur', onWindowBlur);

        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key !== 'Escape') return;
            if (wifiPasswordFor) cancelWifiPassword();
            else if (wifiPanelOpen) closeWifiPanel();
            else if (volumePanelOpen) closeVolumePanel();
            else if (mediaOpen) closeMediaPanel();
            else if (bgPanelOpen) closeBackgroundPanel();
            else if (menu) closeMenu();
            else if (previewOpen) hidePreview();
            else if (workspacePanelOpen) hideWorkspacePanel();
        };
        window.addEventListener('keydown', onKeyDown);

        api
            .getTrashIcon()
            .then((trash: TrashIcon) => {
                if (disposed) return;
                trashIcon = trash.icon;
                trashFull = trash.full;
            })
            .catch(() => {});

        onTrashChanged(full => {
            trashFull = full;
        }).then(unlisten => {
            if (disposed) unlisten();
            else unlistenTrash = unlisten;
        });

        onSessionUnlocked(() => {
            applyDockReservation();
            setTimeout(applyDockReservation, 600);
            hideTooltipNow();
            clearHide();
            void loadWifiStatus(false);
            if (hidden) {
                hidden = false;
                setTimeout(() => {
                    measure();
                    sendRegion(hovering);
                }, 300);
            }
        }).then(unlisten => {
            if (disposed) unlisten();
            else unlistenSession = unlisten;
        });

        const refreshStatus = () => {
            api
                .getSystemStatus()
                .then(status => {
                    if (disposed) return;
                    batteryPercent = status.batteryPercent;
                    charging = status.charging;
                })
                .catch(() => {});
        };
        refreshStatus();
        void loadWifiStatus(false);
        void refreshMedia();
        void refreshMasterAudio();
        void refreshDesktopIndicator();
        const statusTimer = setInterval(refreshStatus, 30000);
        const desktopTimer = setInterval(refreshDesktopIndicator, 800);
        const wifiTimer = setInterval(() => {
            if (!wifiPanelOpen) void loadWifiStatus(false);
        }, 30000);
        const masterTimer = setInterval(() => {
            if (!volumePanelOpen && Date.now() - masterVolumeSyncAt > 1200) {
                void refreshMasterAudio();
            }
        }, 20000);
        const mediaTimer = setInterval(() => {
            void refreshMedia();
        }, 1000);
        const mediaProgressTimer = setInterval(() => {
            if (!media?.playing || !mediaDuration || mediaSeeking) return;
            mediaPosition = Math.min(
                mediaDuration,
                mediaBasePosition + (Date.now() - mediaBaseAt),
            );
        }, 250);
        const clockTimer = setInterval(() => {
            now = new Date();
        }, 1000);

        if (settings.autoHide) {
            hideTimer = setTimeout(() => {
                if (
                    !hovering &&
                    !menu &&
                    !wifiPanelOpen &&
                    !mediaOpen &&
                    !bgPanelOpen
                )
                    hideDock();
            }, 2600);
        }

        return () => {
            disposed = true;
            unlistenTrash?.();
            unlistenSession?.();
            unlistenDismiss?.();
            window.removeEventListener('blur', onWindowBlur);
            window.removeEventListener('keydown', onKeyDown);
            clearInterval(statusTimer);
            clearInterval(desktopTimer);
            clearInterval(wifiTimer);
            clearInterval(masterTimer);
            clearInterval(mediaTimer);
            clearInterval(mediaProgressTimer);
            clearInterval(clockTimer);
            clearTimeout(hideTimer);
            clearTimeout(revealTimer);
            clearTimeout(tooltipTimer);
            clearTimeout(regionTimer);
            clearTimeout(previewPrefetchTimer);
            clearTimeout(previewHideTimer);
            clearTimeout(previewDragResetTimer);
            clearTimeout(wifiForgetTimer);
            clearTimeout(bgConfirmTimer);
            clearTimeout(workspacePulseTimer);
            if (rafId) cancelAnimationFrame(rafId);
        };
    });

    function measure() {
        if (!rowEl) return;
        const elements = Array.from(
            rowEl.querySelectorAll<HTMLElement>('.dock-item'),
        );

        for (const element of elements) {
            element.style.transform = '';
            const icon = element.querySelector<HTMLElement>('.icon-scale');
            if (icon) icon.style.transform = '';
        }

        const rowRect = rowEl.getBoundingClientRect();
        baseWidth = rowRect.width;
        baseWidths = elements.map(element => {
            const rect = element.getBoundingClientRect();
            return rect.width;
        });
        baseCenters = elements.map(element => {
            const rect = element.getBoundingClientRect();
            return rect.left - rowRect.left + rect.width / 2;
        });
        currentScales = elements.map(() => 1);
    }

    async function resizeAndRegion(force: boolean) {
        if (!rowEl) return;

        if (!windowSized || force) {
            windowSized = true;
            // Width 0 means the full monitor: the host window spans the
            // screen while the bar itself stays compact and centered.
            try {
                await api.resizeDock(0, WINDOW_HEIGHT);
            } catch (error) {
                console.error('resize_dock failed', error);
            }
            await tick();
            await new Promise(resolve =>
                requestAnimationFrame(() => requestAnimationFrame(resolve)),
            );
            measure();
        }

        sendRegion(hovering);

        if (!ready) {
            ready = true;
            await api.dockReady().catch(() => {});
        }
    }

    function sendRegion(expanded: boolean) {
        if (hidden) {
            void api
                .setDockRegion([
                    {
                        x: 0,
                        y: window.innerHeight - 4,
                        w: window.innerWidth,
                        h: 4,
                        radius: 0,
                    },
                ])
                .catch(() => {});
            return;
        }

        const bar = barEl?.getBoundingClientRect();
        if (!bar) return;

        // The magnifier is capped to the bar geometry, so the hover area
        // never needs to grow outside the visible dock.
        const top = 10;

        const pad = 12;
        const rects = [
            {
                x: bar.left - pad,
                y: bar.top - top,
                w: bar.width + pad * 2,
                h: bar.height + top + (window.innerHeight - bar.bottom),
                radius: Math.max(0, settings.dockRadius),
            },
        ];

        const menuRect = menuEl?.getBoundingClientRect();
        if (menu && menuRect && menuRect.width > 0) {
            rects.push({
                x: menuRect.left - 8,
                y: menuRect.top - 8,
                w: menuRect.width + 16,
                h: menuRect.height + 16,
                radius: 18,
            });
        }

        const panelRect = wifiPanelEl?.getBoundingClientRect();
        if (wifiPanelOpen && panelRect && panelRect.width > 0) {
            rects.push({
                x: panelRect.left - 10,
                y: panelRect.top - 10,
                w: panelRect.width + 20,
                h: panelRect.height + 20,
                radius: 22,
            });
        }

        const volumeRect = volumePanelEl?.getBoundingClientRect();
        if (volumePanelOpen && volumeRect && volumeRect.width > 0) {
            rects.push({
                x: volumeRect.left - 10,
                y: volumeRect.top - 10,
                w: volumeRect.width + 20,
                h: volumeRect.height + 20,
                radius: 22,
            });
        }

        const mediaRect = mediaPanelEl?.getBoundingClientRect();
        if (mediaOpen && mediaRect && mediaRect.width > 0) {
            rects.push({
                x: mediaRect.left - 10,
                y: mediaRect.top - 10,
                w: mediaRect.width + 20,
                h: mediaRect.height + 20,
                radius: 22,
            });
        }

        const bgRect = bgPanelEl?.getBoundingClientRect();
        if (bgPanelOpen && bgRect && bgRect.width > 0) {
            rects.push({
                x: bgRect.left - 10,
                y: bgRect.top - 10,
                w: bgRect.width + 20,
                h: bgRect.height + 20,
                radius: 22,
            });
        }

        const previewRect = previewEl?.getBoundingClientRect();
        if (previewOpen && previewRect && previewRect.width > 0) {
            rects.push({
                x: previewRect.left - 10,
                y: previewRect.top - 10,
                w: previewRect.width + 20,
                h: previewRect.height + 20,
                radius: 18,
            });
        }

        const workspaceRect = workspacePanelEl?.getBoundingClientRect();
        if (workspacePanelOpen && workspaceRect && workspaceRect.width > 0) {
            rects.push({
                x: workspaceRect.left - 10,
                y: workspaceRect.top - 10,
                w: workspaceRect.width + 20,
                h: workspaceRect.height + 20,
                radius: 18,
            });
        }

        void api.setDockRegion(rects).catch(() => {});
    }

    function refreshMenuRegion() {
        void tick().then(() => sendRegion(true));
    }

    function scheduleRegion(expanded: boolean) {
        clearTimeout(regionTimer);
        regionTimer = setTimeout(() => sendRegion(expanded), 30);
    }

    function clearHide() {
        clearTimeout(hideTimer);
    }

    function scheduleHide(delay = settings.hideDelay) {
        clearHide();
        if (
            !settings.autoHide ||
            menu ||
            wifiPanelOpen ||
            volumePanelOpen ||
            mediaOpen ||
            bgPanelOpen ||
            draggingKey
        )
            return;
        hideTimer = setTimeout(() => {
            if (
                !hovering &&
                !menu &&
                !wifiPanelOpen &&
                !volumePanelOpen &&
                !mediaOpen &&
                !bgPanelOpen
            )
                hideDock();
        }, delay);
    }

    function hideDock() {
        if (hidden) return;
        hidden = true;
        tooltipVisible = false;
        hidePreview();
        hideWorkspacePanel();
        menu = null;
        wifiPanelOpen = false;
        wifiPasswordFor = null;
        wifiPassword = '';
        wifiForgetFor = null;
        volumePanelOpen = false;
        masterError = null;
        mediaOpen = false;
        bgPanelOpen = false;
        bgConfirmPid = null;
        bgError = null;
        if (wifiBusy === null) void api.releaseDockFocus().catch(() => {});
        sendRegion(false);
    }

    function revealDock() {
        if (!hidden) return;
        hidden = false;
        setTimeout(() => {
            measure();
            sendRegion(hovering);
        }, 280);
    }

    function onAnchorEnter() {
        clearHide();
        cancelPreviewHide();
        revealDock();
        if (!hovering) {
            hovering = true;
            sendRegion(true);
        }
    }

    function onAnchorLeave() {
        hovering = false;
        tooltipIndex = -1;
        hideTooltipNow();
        schedulePreviewHide();
        scheduleHide();
        ensureLoop();
    }

    function onRevealStripEnter() {
        clearTimeout(revealTimer);
        if (!hidden) {
            clearHide();
            scheduleHide(1400);
            return;
        }
        revealTimer = setTimeout(() => {
            revealDock();
            scheduleHide(2000);
        }, settings.revealDelay);
    }

    function onRevealStripLeave() {
        clearTimeout(revealTimer);
        if (!hidden) scheduleHide();
    }

    function ensureLoop() {
        if (!rafId) rafId = requestAnimationFrame(stepMagnifier);
    }

    function stepMagnifier() {
        rafId = 0;
        if (!rowEl) return;

        const elements = Array.from(
            rowEl.querySelectorAll<HTMLElement>('.dock-item'),
        );
        if (
            elements.length !== baseCenters.length ||
            baseWidths.length !== elements.length
        ) {
            measure();
            ensureLoop();
            return;
        }
        if (elements.length === 0) return;

        const preferredPeak = maxScale(settings.zoomFactor);
        const iconSize = Math.max(1, settings.iconSize);
        const slotPeak = (iconSize + settings.iconSpacing) / iconSize;
        const heightPeak = 1 + settings.dockPaddingY / iconSize;
        const peak = Math.max(1, Math.min(preferredPeak, slotPeak, heightPeak));
        const factor = 1 + (peak - 1) / 2;

        const options = {
            iconSize: settings.iconSize,
            gap: settings.iconSpacing,
            factor,
            radiusFactor: 3.5,
            rise: 0,
        };

        const targets = targetScales(
            baseCenters,
            hovering ? pointerX : null,
            baseWidth,
            options,
        );

        if (currentScales.length !== targets.length) {
            currentScales = targets.slice();
        }

        let moving = hovering;
        for (let index = 0; index < targets.length; index++) {
            currentScales[index] = approach(
                currentScales[index] ?? 1,
                targets[index],
                zoomSmoothing,
            );
            if (Math.abs(currentScales[index] - targets[index]) > 0.001) {
                moving = true;
            }
        }

        const layout = layoutForScales(
            baseCenters,
            baseWidth,
            baseWidths,
            currentScales,
            options,
        );

        for (let index = 0; index < elements.length; index++) {
            const element = elements[index];
            element.style.transform = `translateX(${layout.translates[index].toFixed(3)}px)`;
            element.style.zIndex = String(
                1 + Math.round(currentScales[index] * 10),
            );
            const icon = element.querySelector<HTMLElement>('.icon-scale');
            if (icon) {
                icon.style.transform = `translate3d(0, ${(-layout.rises[index]).toFixed(3)}px, 0) scale(${currentScales[index].toFixed(4)})`;
            }
        }

        if (moving) {
            ensureLoop();
        } else {
            sendRegion(false);
        }
    }

    function onAnchorPointerMove(event: PointerEvent) {
        if (hidden || !rowEl || draggingKey) return;
        const rowRect = rowEl.getBoundingClientRect();
        pointerX = event.clientX - rowRect.left;

        if (!hovering) {
            hovering = true;
            sendRegion(true);
        }

        ensureLoop();
        updateTooltip(event);
    }

    function closestItemIndex(): number {
        if (baseCenters.length === 0) return -1;
        let best = -1;
        let distance = Infinity;
        for (let index = 0; index < baseCenters.length; index++) {
            const current = Math.abs(pointerX - baseCenters[index]);
            if (current < distance) {
                distance = current;
                best = index;
            }
        }
        return distance <= settings.iconSize * 0.85 ? best : -1;
    }

    function updateTooltip(_event: PointerEvent) {
        const index = closestItemIndex();
        if (index < 0) {
            hideTooltipNow();
            tooltipIndex = -1;
            schedulePreviewHide(120);
            return;
        }

        const item = items[index];
        if (!item) return;
        const label =
            item.kind === 'app'
                ? item.app.name
                : item.kind === 'dynamic'
                  ? item.name
                  : item.kind === 'folder'
                    ? item.name
                    : 'Lixeira';

        if (index !== tooltipIndex) {
            tooltipIndex = index;
            clearTimeout(tooltipTimer);
            tooltipVisible = false;
            if (previewOpen && previewKey !== item.key) hidePreview();
            schedulePreviewPrefetch(index);
        }

        tooltipLabel = label;
        clearTimeout(tooltipTimer);
        tooltipTimer = setTimeout(() => {
            const element =
                rowEl?.querySelectorAll<HTMLElement>('.dock-item')[index];
            if (!element) return;
            const icon =
                element.querySelector<HTMLElement>('.icon-scale') ?? element;
            const rect = icon.getBoundingClientRect();
            tooltipLabel = label;
            tooltipLeft = Math.max(
                90,
                Math.min(window.innerWidth - 90, rect.left + rect.width / 2),
            );
            tooltipBottom = window.innerHeight - rect.top + 10;
            tooltipVisible = true;
            openPreviewForIndex(index);
        }, PREVIEW_SHOW_DELAY);
    }

    function hideTooltipNow() {
        clearTimeout(tooltipTimer);
        clearTimeout(previewPrefetchTimer);
        tooltipVisible = false;
    }

    function previewItem(index: number) {
        const item = items[index];
        if (!item) return null;
        if (item.kind === 'app') {
            return {
                key: item.key,
                name: item.app.name,
                icon: icons[item.key] ?? null,
                windows: windowsFor(item.app),
            };
        }
        if (item.kind === 'dynamic') {
            return {
                key: item.key,
                name: item.name,
                icon: icons[item.key] ?? null,
                windows: item.windows,
            };
        }
        return null;
    }

    function schedulePreviewHide(delay = 300) {
        clearTimeout(previewHideTimer);
        previewHideTimer = setTimeout(() => {
            if (!previewHover) hidePreview();
        }, delay);
    }

    function cancelPreviewHide() {
        clearTimeout(previewHideTimer);
    }

    function hidePreview() {
        clearTimeout(previewHideTimer);
        clearTimeout(previewDragResetTimer);
        previewHoverHwnd = null;
        previewDrag = null;
        previewDragging = false;
        previewJustDragged = false;
        if (!previewOpen) return;
        previewOpen = false;
        previewKey = '';
        previewWindows = [];
        previewHover = false;
        previewScrollLeft = 0;
        previewScrollWidth = 0;
        previewScrollClientWidth = 0;
        previewSeq++;
        scheduleRegion(hovering);
    }

    function onPreviewEnter() {
        previewHover = true;
        cancelPreviewHide();
    }

    function onPreviewLeave() {
        if (previewDragging) return;
        previewHover = false;
        previewHoverHwnd = null;
        schedulePreviewHide();
    }

    function onPreviewCardEnter(window: WindowInfo) {
        if (previewDragging) return;
        previewHoverHwnd = window.hwnd;
    }

    function onPreviewCardLeave(window: WindowInfo) {
        if (previewHoverHwnd !== window.hwnd) return;
        previewHoverHwnd = null;
    }

    function updatePreviewScroll() {
        const row = previewRowEl;
        if (!row) return;
        previewScrollLeft = row.scrollLeft;
        previewScrollWidth = row.scrollWidth;
        previewScrollClientWidth = row.clientWidth;
    }

    function scrollPreview(direction: -1 | 1) {
        const row = previewRowEl;
        if (!row) return;
        previewHoverHwnd = null;
        const page = Math.max(200, row.clientWidth - 200);
        const max = row.scrollWidth - row.clientWidth;
        const target = Math.max(
            0,
            Math.min(max, row.scrollLeft + direction * page),
        );
        row.scrollTo({ left: target, behavior: 'smooth' });
    }

    function onPreviewWheel(event: WheelEvent) {
        event.stopPropagation();
        const row = previewRowEl;
        if (!row) return;
        const max = row.scrollWidth - row.clientWidth;
        if (max <= 1) return;
        event.preventDefault();
        previewHoverHwnd = null;
        const unit =
            event.deltaMode === 1
                ? 16
                : event.deltaMode === 2
                  ? row.clientWidth
                  : 1;
        const delta =
            Math.abs(event.deltaX) > Math.abs(event.deltaY)
                ? event.deltaX
                : event.deltaY;
        row.scrollLeft = Math.max(
            0,
            Math.min(max, row.scrollLeft + delta * unit),
        );
    }

    function onPreviewPointerDown(event: PointerEvent) {
        const row = previewRowEl;
        if (!row || event.button !== 0) return;
        if (row.scrollWidth <= row.clientWidth + 1) return;
        if ((event.target as Element | null)?.closest('.preview-nav')) return;
        previewDrag = {
            pointerId: event.pointerId,
            startX: event.clientX,
            startScroll: row.scrollLeft,
            moved: false,
        };
    }

    function onPreviewPointerMove(event: PointerEvent) {
        const drag = previewDrag;
        const row = previewRowEl;
        if (!drag || !row || drag.pointerId !== event.pointerId) return;
        const dx = event.clientX - drag.startX;
        if (!drag.moved) {
            if (Math.abs(dx) < 5) return;
            drag.moved = true;
            previewDragging = true;
            previewHoverHwnd = null;
            row.setPointerCapture(event.pointerId);
        }
        event.preventDefault();
        row.scrollLeft = drag.startScroll - dx;
    }

    function onPreviewPointerUp(event: PointerEvent) {
        const drag = previewDrag;
        if (!drag || drag.pointerId !== event.pointerId) return;
        previewDrag = null;
        if (!drag.moved) return;
        previewDragging = false;
        previewJustDragged = true;
        clearTimeout(previewDragResetTimer);
        previewDragResetTimer = setTimeout(() => {
            previewJustDragged = false;
        }, 350);
    }

    function onPreviewPointerCancel(event: PointerEvent) {
        if (previewDrag?.pointerId !== event.pointerId) return;
        previewDrag = null;
        previewDragging = false;
    }

    function onPreviewKeyDown(event: KeyboardEvent) {
        if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
        const row = previewRowEl;
        if (!row) return;
        const cards = Array.from(
            row.querySelectorAll<HTMLButtonElement>('.preview-card'),
        );
        if (cards.length === 0) return;
        const active = document.activeElement as HTMLElement | null;
        const current = cards.findIndex(card => card === active);
        if (current < 0 && (!active || !row.contains(active))) return;
        event.preventDefault();
        const step = event.key === 'ArrowRight' ? 1 : -1;
        const next =
            current < 0
                ? step > 0
                    ? 0
                    : cards.length - 1
                : Math.max(0, Math.min(cards.length - 1, current + step));
        const card = cards[next];
        card.focus({ preventScroll: true });
        card.scrollIntoView({
            block: 'nearest',
            inline: 'nearest',
            behavior: 'smooth',
        });
    }

    function bringPreviewWindow(window: WindowInfo) {
        previewHoverHwnd = null;
        hidePreview();
        hideTooltipNow();
        void api
            .focusWindow(window.hwnd)
            .then(focused => {
                if (focused) hidePreview();
            })
            .catch(error => console.error('focus_window failed', error));
    }

    function previewThumbRequest(): [number, number] {
        // Capture only the pixels the card actually shows, even on HiDPI.
        const dpr = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
        return [
            Math.round(THUMB_MAX_WIDTH * dpr),
            Math.round(THUMB_MAX_HEIGHT * dpr),
        ];
    }

    function cachedPreviewThumb(hwnd: number) {
        const cached = previewCache.get(hwnd);
        if (!cached) return undefined;
        if (Date.now() - cached.at > THUMB_KEEP_MS) {
            previewCache.delete(hwnd);
            return undefined;
        }
        return cached;
    }

    function capturePreviewThumb(hwnd: number): Promise<string | null> {
        // Prefetch and the open panel share the same capture.
        const inflight = previewInflight.get(hwnd);
        if (inflight) return inflight;

        const [width, height] = previewThumbRequest();
        const promise = api
            .windowPreviewJpeg(hwnd, width, height)
            .then(url => {
                previewCache.set(hwnd, { url, at: Date.now() });
                return url;
            })
            .catch(() => {
                previewCache.set(hwnd, { url: null, at: Date.now() });
                return null;
            })
            .finally(() => {
                previewInflight.delete(hwnd);
            });

        previewInflight.set(hwnd, promise);
        return promise;
    }

    function schedulePreviewPrefetch(index: number) {
        clearTimeout(previewPrefetchTimer);
        const item = previewItem(index);
        if (!item || item.windows.length === 0) return;

        const windows = item.windows;
        previewPrefetchTimer = setTimeout(() => {
            const now = Date.now();
            for (const window of windows) {
                if (window.minimized) continue;
                const cached = cachedPreviewThumb(window.hwnd);
                if (cached?.url && now - cached.at < THUMB_FRESH_MS) continue;
                void capturePreviewThumb(window.hwnd);
            }
        }, PREFETCH_DELAY);
    }

    function loadPreviewThumbs(windows: WindowInfo[], seq: number) {
        for (const window of windows) {
            if (window.minimized) {
                if (seq === previewSeq) {
                    previewThumbs = { ...previewThumbs, [window.hwnd]: null };
                }
                continue;
            }

            const cached = cachedPreviewThumb(window.hwnd);
            if (cached?.url) {
                if (seq === previewSeq) {
                    previewThumbs = {
                        ...previewThumbs,
                        [window.hwnd]: cached.url,
                    };
                }
                if (Date.now() - cached.at < THUMB_FRESH_MS) continue;

                // Show the old frame right away and refresh behind the scenes.
                void capturePreviewThumb(window.hwnd).then(url => {
                    if (seq !== previewSeq || !url || url === cached.url) return;
                    previewThumbs = { ...previewThumbs, [window.hwnd]: url };
                });
                continue;
            }

            previewLoading = { ...previewLoading, [window.hwnd]: true };
            void capturePreviewThumb(window.hwnd).then(url => {
                if (seq !== previewSeq) return;
                previewThumbs = { ...previewThumbs, [window.hwnd]: url };
                previewLoading = { ...previewLoading, [window.hwnd]: false };
            });
        }
    }

    function openPreviewForIndex(index: number) {
        const item = previewItem(index);
        if (!item || item.windows.length === 0) {
            hidePreview();
            return;
        }

        if (previewOpen && previewKey === item.key) {
            cancelPreviewHide();
            hideTooltipNow();
            return;
        }

        const element =
            rowEl?.querySelectorAll<HTMLElement>('.dock-item')[index];
        if (!element) return;
        const icon = element.querySelector<HTMLElement>('.icon-scale') ?? element;
        const rect = icon.getBoundingClientRect();
        const estimatedWidth = Math.min(
            window.innerWidth * 0.94,
            item.windows.length === 1
                ? 260
                : item.windows.length * 192 + 20,
        );
        const half = estimatedWidth / 2;

        const ordered = [...item.windows].sort(
            (a, b) => Number(b.focused) - Number(a.focused),
        );

        // Paint whatever the prefetch already has before the panel opens,
        // instead of flashing empty placeholders.
        const knownThumbs: Record<number, string | null> = {};
        for (const window of ordered) {
            if (window.minimized) {
                knownThumbs[window.hwnd] = null;
                continue;
            }
            const cached = cachedPreviewThumb(window.hwnd);
            if (cached?.url) knownThumbs[window.hwnd] = cached.url;
        }

        previewKey = item.key;
        previewName = item.name;
        previewIcon = item.icon;
        previewWindows = ordered;
        previewThumbs = knownThumbs;
        previewLoading = {};
        previewLeft = Math.max(
            half + 10,
            Math.min(window.innerWidth - half - 10, rect.left + rect.width / 2),
        );
        previewBottom = window.innerHeight - rect.top + 10;
        previewOpen = true;
        previewSeq++;
        hideTooltipNow();

        if (previewHover) previewHover = false;
        cancelPreviewHide();
        void tick().then(() => {
            updatePreviewScroll();
            sendRegion(hovering);
        });
        loadPreviewThumbs(ordered, previewSeq);
    }

    function openPreviewWindow(window: WindowInfo) {
        if (previewJustDragged) {
            previewJustDragged = false;
            return;
        }
        bringPreviewWindow(window);
    }

    async function activateApp(app: AppEntry, newWindow = false) {
        hidePreview();
        hideTooltipNow();
        const matches = windowsFor(app);
        if (newWindow || matches.length === 0) {
            startBounce(app.id);
            try {
                await api.launchApp(app.id);
            } catch (error) {
                console.error('launch_app failed', error);
            }
            setTimeout(() => void refreshWindows(), 500);
            setTimeout(() => void refreshWindows(), 1400);
            return;
        }

        startPress(app.id);
        const target =
            matches.find(window => !window.minimized) ?? matches[0];
        try {
            await api.focusWindow(target.hwnd);
        } catch (error) {
            console.error('focus_window failed', error);
        }
    }

    function startBounce(id: string) {
        bounces = { ...bounces, [id]: (bounces[id] ?? 0) + 1 };
        bouncing = { ...bouncing, [id]: true };
        setTimeout(() => {
            bouncing = { ...bouncing, [id]: false };
        }, 1050);
    }

    function startPress(id: string) {
        pressing = { ...pressing, [id]: true };
        setTimeout(() => {
            pressing = { ...pressing, [id]: false };
        }, 190);
    }

    function onAppClick(event: MouseEvent, app: AppEntry) {
        if (suppressClick) return;
        if (menu) closeMenu();
        void activateApp(app, event.ctrlKey);
    }

    function onAppAuxClick(event: MouseEvent, app: AppEntry) {
        if (event.button !== 1 || suppressClick) return;
        event.preventDefault();
        void activateApp(app, true);
    }

    function onDockDragStart(event: DragEvent, item: DisplayItem) {
        if (item.kind === 'trash') {
            event.preventDefault();
            return;
        }

        draggingKey = item.key;
        dragOverKey = null;
        dragAfter = false;
        suppressClick = true;
        hovering = false;
        pointerX = -9999;
        hideTooltipNow();
        clearHide();

        if (event.dataTransfer) {
            event.dataTransfer.effectAllowed = 'move';
            event.dataTransfer.setData('text/plain', item.key);
        }

        ensureLoop();
    }

    function onDockDragOver(event: DragEvent, item: DisplayItem) {
        if (
            !draggingKey ||
            item.kind === 'trash' ||
            item.key === draggingKey
        ) {
            return;
        }

        event.preventDefault();
        if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';

        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        dragOverKey = item.key;
        dragAfter = event.clientX > rect.left + rect.width / 2;
    }

    function onDockDrop(event: DragEvent, item: DisplayItem) {
        event.preventDefault();
        const from = draggingKey;
        const after = dragAfter;
        clearDockDrag();

        if (!from || from === item.key || item.kind === 'trash') return;
        reorderDock(from, item.key, after);
    }

    function onDockDragEnd() {
        clearDockDrag();
        setTimeout(() => {
            suppressClick = false;
        }, 60);
    }

    function clearDockDrag() {
        draggingKey = null;
        dragOverKey = null;
        dragAfter = false;
    }

    function reorderDock(fromKey: string, toKey: string, after: boolean) {
        const appItems = items.filter(item => item.kind !== 'trash');
        const keys = appItems.map(item => item.key);
        const fromIndex = keys.indexOf(fromKey);
        if (fromIndex < 0) return;

        keys.splice(fromIndex, 1);
        let targetIndex = keys.indexOf(toKey);
        if (targetIndex < 0) return;
        if (after) targetIndex += 1;
        keys.splice(targetIndex, 0, fromKey);

        const byKey = new Map(appItems.map(item => [item.key, item]));
        const dragged = byKey.get(fromKey);
        const pinned = keys
            .map(key => byKey.get(key))
            .filter(
                (item): item is Extract<DisplayItem, { kind: 'app' }> =>
                    item?.kind === 'app' &&
                    (item.pinned ||
                        (item.key === fromKey && dragged?.kind === 'app')),
            )
            .map(item => item.app.id);

        void api
            .saveSettings({
                ...settings,
                pinned,
                pinnedInitialized: true,
                dockOrder: keys,
            })
            .catch(error => console.error('save dock order failed', error));
    }

    async function addDockFolder() {
        closeMenu();
        hideTooltipNow();

        try {
            const path = await api.pickFolder();
            if (!path) return;
            const normalized = normalizePath(path);
            if (settings.folders.some(folder => normalizePath(folder) === normalized)) {
                return;
            }
            await api.saveSettings({
                ...settings,
                folders: [...settings.folders, path],
            });
        } catch (error) {
            console.error('add dock folder failed', error);
        }
    }

    function openDockFolder(path: string, newWindow = false) {
        if (suppressClick) return;
        if (menu) closeMenu();
        hideTooltipNow();
        void api
            .openFolder(path, newWindow)
            .then(() => {
                setTimeout(() => void refreshWindows(), 600);
            })
            .catch(error => console.error('open_folder failed', error));
    }

    function openFolderMenu(
        event: MouseEvent,
        item: Extract<DisplayItem, { kind: 'folder' }>,
    ) {
        event.preventDefault();
        event.stopPropagation();
        closeWifiPanel();
        closeVolumePanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideTooltipNow();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        menuLeft = Math.max(
            140,
            Math.min(window.innerWidth - 140, rect.left + rect.width / 2),
        );
        menu = { kind: 'folder', path: item.path, name: item.name };
        refreshMenuRegion();
    }

    function removeDockFolder(path: string) {
        closeMenu();
        const key = folderKey(path);
        const normalized = normalizePath(path);
        void api
            .saveSettings({
                ...settings,
                folders: settings.folders.filter(
                    folder => normalizePath(folder) !== normalized,
                ),
                dockOrder: settings.dockOrder.filter(orderKey => orderKey !== key),
            })
            .catch(error => console.error('remove dock folder failed', error));
    }

    function openMenuFolder(newWindow = false) {
        const current = menu;
        if (!current || current.kind !== 'folder') return;
        openDockFolder(current.path, newWindow);
    }

    function removeMenuFolder() {
        const current = menu;
        if (!current || current.kind !== 'folder') return;
        removeDockFolder(current.path);
    }

    function openAppMenu(event: MouseEvent, app: AppEntry) {
        event.preventDefault();
        event.stopPropagation();
        closeWifiPanel();
        closeVolumePanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideTooltipNow();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        menuLeft = Math.max(
            140,
            Math.min(window.innerWidth - 140, rect.left + rect.width / 2),
        );
        menu = {
            kind: 'app',
            app,
            pinned: settings.pinned.includes(app.id),
        };
        refreshMenuRegion();
    }

    function focusDynamic(item: { key: string; windows: WindowInfo[] }) {
        if (suppressClick) return;
        if (menu) closeMenu();
        hidePreview();
        const target =
            item.windows.find(window => !window.minimized) ?? item.windows[0];
        if (!target) return;
        startPress(item.key);
        void api
            .focusWindow(target.hwnd)
            .catch(error => console.error('focus_window failed', error));
    }

    function openDynamicMenu(
        event: MouseEvent,
        item: { name: string; exe: string; windows: WindowInfo[] },
    ) {
        event.preventDefault();
        event.stopPropagation();
        closeWifiPanel();
        closeVolumePanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideTooltipNow();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        menuLeft = Math.max(
            140,
            Math.min(window.innerWidth - 140, rect.left + rect.width / 2),
        );
        menu = {
            kind: 'dynamic',
            name: item.name,
            exe: item.exe,
            windows: item.windows,
        };
        refreshMenuRegion();
    }

    async function closeDynamicWindows(windows: WindowInfo[]) {
        closeMenu();
        for (const window of windows) {
            await api.closeWindow(window.hwnd).catch(() => {});
        }
        setTimeout(() => void refreshWindows(), 350);
    }

    function openDockMenu(event: MouseEvent) {
        if ((event.target as HTMLElement).closest('button.dock-item')) return;
        event.preventDefault();
        closeWifiPanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hideTooltipNow();
        menuLeft = window.innerWidth / 2;
        menu = { kind: 'dock' };
        refreshMenuRegion();
    }

    function openTrashMenu(event: MouseEvent) {
        event.preventDefault();
        event.stopPropagation();
        closeWifiPanel();
        closeVolumePanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideTooltipNow();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        menuLeft = Math.max(
            140,
            Math.min(window.innerWidth - 140, rect.left + rect.width / 2),
        );
        menu = { kind: 'trash' };
        refreshMenuRegion();
    }

    function closeMenu() {
        if (!menu) return;
        menu = null;
        scheduleRegion(hovering);
    }

    async function togglePin(app: AppEntry) {
        const pinned = settings.pinned.includes(app.id)
            ? settings.pinned.filter(id => id !== app.id)
            : [...settings.pinned, app.id];
        closeMenu();
        try {
            await api.saveSettings({
                ...settings,
                pinned,
                pinnedInitialized: true,
            });
        } catch (error) {
            console.error('save_settings failed', error);
        }
    }

    async function focusWindow(hwnd: number) {
        closeMenu();
        try {
            await api.focusWindow(hwnd);
        } catch (error) {
            console.error('focus_window failed', error);
        }
    }

    async function closeAppWindows(app: AppEntry) {
        closeMenu();
        for (const window of windowsFor(app)) {
            await api.closeWindow(window.hwnd).catch(() => {});
        }
        setTimeout(() => void refreshWindows(), 350);
    }

    async function openTrash() {
        closeMenu();
        await api.openTrash().catch(() => {});
    }

    async function emptyTrash() {
        closeMenu();
        await api.emptyTrash().catch(() => {});
        setTimeout(() => {
            api
                .getTrashIcon()
                .then(trash => {
                    trashIcon = trash.icon;
                    trashFull = trash.full;
                })
                .catch(() => {});
        }, 800);
    }

    async function restoreTaskbar() {
        closeMenu();
        try {
            await api.setTaskbarHidden(false);
        } catch (error) {
            console.error('set_taskbar_hidden failed', error);
        }
    }

    function openSettings() {
        closeMenu();
        void api.openSettings().catch(() => {});
    }

    function openLauncher() {
        hideTooltipNow();
        void api
            .openLauncher()
            .catch(error => console.error('open_launcher failed', error));
    }

    function closeAllDockPanels() {
        closeMenu();
        closeWifiPanel();
        closeVolumePanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideWorkspacePanel();
        hideTooltipNow();
    }

    function openTaskView() {
        closeAllDockPanels();
        void api
            .openTaskView()
            .catch(error => console.error('task_view failed', error));
    }

    function newVirtualDesktop() {
        hideWorkspacePanel();
        closeMenu();
        void api
            .newVirtualDesktop()
            .catch(error => console.error('new_desktop failed', error));
        setTimeout(() => void refreshDesktopIndicator(), 650);
    }

    function closeVirtualDesktop() {
        hideWorkspacePanel();
        closeMenu();
        void api
            .closeVirtualDesktop()
            .catch(error => console.error('close_desktop failed', error));
        setTimeout(() => void refreshDesktopIndicator(), 650);
    }

    function switchVirtualDesktop(direction: 'left' | 'right') {
        closeMenu();
        clearTimeout(workspacePulseTimer);
        workspacePulse = direction;
        workspacePulseTimer = setTimeout(() => {
            workspacePulse = null;
        }, 220);

        void api
            .switchVirtualDesktop(direction)
            .catch(error => console.error('switch_desktop failed', error));

        if (workspacePanelOpen) {
            setTimeout(() => void refreshWorkspaceDesktops(), 320);
        }
        setTimeout(() => void refreshDesktopIndicator(), 380);
    }

    function onWorkspaceWheel(event: WheelEvent) {
        event.preventDefault();
        const now = Date.now();
        if (now - lastWorkspaceSwitch < 180) return;
        lastWorkspaceSwitch = now;

        const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY)
            ? event.deltaX
            : event.deltaY;
        if (Math.abs(delta) < 1) return;
        switchVirtualDesktop(delta > 0 ? 'right' : 'left');
    }

    function scheduleWorkspaceHide(delay = 300) {
        clearTimeout(workspaceHideTimer);
        workspaceHideTimer = setTimeout(() => {
            if (!workspacePanelHover) hideWorkspacePanel();
        }, delay);
    }

    function hideWorkspacePanel() {
        clearTimeout(workspaceOpenTimer);
        clearTimeout(workspaceHideTimer);
        if (!workspacePanelOpen) return;
        workspacePanelOpen = false;
        workspacePanelHover = false;
        workspaceRequest++;
        scheduleRegion(hovering);
    }

    async function refreshWorkspaceDesktops() {
        const request = ++workspaceRequest;
        workspaceLoading = true;
        workspaceError = null;

        try {
            const desktops = await api.getVirtualDesktops();
            if (request !== workspaceRequest) return;
            workspaceDesktops = desktops;
        } catch (error) {
            if (request !== workspaceRequest) return;
            workspaceError = errorText(error);
        } finally {
            if (request === workspaceRequest) workspaceLoading = false;
        }
    }

    async function refreshDesktopIndicator() {
        try {
            const desktops = await api.getVirtualDesktops();
            workspaceDesktops = desktops;
        } catch {
            // Keep the last known indicator while Windows is busy.
        }
    }

    async function openWorkspacePanel() {
        if (!workspaceBtnEl) return;
        hidePreview();
        hideTooltipNow();

        const rect = workspaceBtnEl.getBoundingClientRect();
        const panelWidth = 252;
        const barTop = barEl?.getBoundingClientRect().top ?? rect.top;
        workspacePanelLeft = Math.max(
            panelWidth / 2 + 10,
            Math.min(
                window.innerWidth - panelWidth / 2 - 10,
                rect.left + rect.width / 2,
            ),
        );
        workspacePanelBottom = window.innerHeight - barTop + 10;
        workspacePanelOpen = true;
        workspacePanelHover = false;
        void tick().then(() => sendRegion(hovering));
        void refreshWorkspaceDesktops();
    }

    function onWorkspaceEnter() {
        cancelPreviewHide();
        clearTimeout(workspaceHideTimer);
        clearTimeout(workspaceOpenTimer);
        if (workspacePanelOpen) {
            void refreshWorkspaceDesktops();
            return;
        }
        workspaceOpenTimer = setTimeout(() => {
            void openWorkspacePanel();
        }, 160);
    }

    function onWorkspaceLeave() {
        clearTimeout(workspaceOpenTimer);
        scheduleWorkspaceHide();
    }

    function onWorkspacePanelEnter() {
        workspacePanelHover = true;
        clearTimeout(workspaceHideTimer);
    }

    function onWorkspacePanelLeave() {
        workspacePanelHover = false;
        scheduleWorkspaceHide();
    }

    function selectVirtualDesktop(desktop: VirtualDesktopInfo) {
        hideWorkspacePanel();
        if (desktop.current) return;
        void api
            .switchVirtualDesktopTo(desktop.index)
            .then(() => {
                setTimeout(() => void refreshDesktopIndicator(), 450);
            })
            .catch(error => console.error('switch desktop failed', error));
    }

    function openWorkspaceMenu(event: MouseEvent) {
        event.preventDefault();
        event.stopPropagation();
        closeWifiPanel();
        closeVolumePanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideWorkspacePanel();
        hideTooltipNow();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        menuLeft = Math.max(
            140,
            Math.min(window.innerWidth - 140, rect.left + rect.width / 2),
        );
        menu = { kind: 'workspace' };
        refreshMenuRegion();
    }

    function openStatusPage(page: string) {
        hidePreview();
        hideTooltipNow();
        void api.openSettingsPage(page).catch(() => {});
    }

    function openActionCenter() {
        closeMenu();
        closeWifiPanel();
        closeVolumePanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideTooltipNow();
        void api
            .openActionCenter()
            .catch(error => console.error('open_action_center failed', error));
    }

    function errorText(error: unknown): string {
        const message =
            typeof error === 'string'
                ? error
                : error instanceof Error
                  ? error.message
                  : String(error);
        return (
            message.replace(/^Error:\s*/i, '').trim() ||
            'O Windows não respondeu.'
        );
    }

    async function loadWifiStatus(
        includeNetworks = false,
    ): Promise<WifiStatus | null> {
        const wantNetworks = includeNetworks || wifiPanelOpen;
        const seq = ++wifiSeq;
        if (wantNetworks) wifiLoading = true;

        try {
            const status = await api.getWifiStatus(wantNetworks);
            if (seq !== wifiSeq) return wifiStatus;
            wifiStatus = status;
            return status;
        } catch (error) {
            if (seq === wifiSeq) wifiError = errorText(error);
            return null;
        } finally {
            if (seq === wifiSeq) wifiLoading = false;
        }
    }

    function refreshWifi() {
        wifiError = null;
        void loadWifiStatus(true);
    }

    function applyMasterAudio(audio: MasterAudio | null) {
        if (!audio) {
            masterVolume = null;
            return;
        }
        // The slider is the source of truth while the user drags it.
        if (Date.now() - masterVolumeSyncAt < 900) return;
        masterVolume = audio.volume;
        masterMuted = audio.muted;
    }

    async function refreshMasterAudio() {
        if (masterVolume == null) masterLoading = true;
        try {
            applyMasterAudio(await api.getMasterAudio());
        } catch (error) {
            if (volumePanelOpen) masterError = errorText(error);
        } finally {
            masterLoading = false;
        }
    }

    function masterStatusLabel(): string {
        if (masterMuted) return 'Silenciado';
        if (masterVolume != null) return 'Saída padrão do Windows';
        return masterLoading
            ? 'Procurando saída de áudio…'
            : 'Nenhuma saída de áudio encontrada';
    }

    function onMasterVolumeInput(event: Event) {
        const level =
            Number((event.currentTarget as HTMLInputElement).value) / 100;
        const wasMuted = masterMuted;
        masterVolume = level;
        masterMuted = false;
        masterVolumeSyncAt = Date.now();
        masterError = null;
        clearTimeout(masterVolumeApplyTimer);
        if (wasMuted) {
            void api.setMasterMuted(false).catch(error => {
                masterError = errorText(error);
            });
        }
        masterVolumeApplyTimer = setTimeout(() => {
            void api.setMasterVolume(level).catch(error => {
                masterError = errorText(error);
            });
        }, 40);
    }

    function onMasterVolumeCommit() {
        if (masterVolume == null) return;
        masterVolumeSyncAt = Date.now();
        clearTimeout(masterVolumeApplyTimer);
        void api.setMasterVolume(masterVolume).catch(error => {
            masterError = errorText(error);
        });
    }

    async function toggleMasterMute() {
        if (masterVolume == null || masterBusy) return;
        const target = !masterMuted;
        masterBusy = 'mute';
        masterMuted = target;
        masterError = null;
        try {
            await api.setMasterMuted(target);
        } catch (error) {
            masterMuted = !target;
            masterError = errorText(error);
        } finally {
            masterBusy = null;
        }
    }

    function openVolumePanel(event: MouseEvent) {
        event.preventDefault();
        event.stopPropagation();

        if (volumePanelOpen) {
            closeVolumePanel();
            return;
        }

        closeMenu();
        closeWifiPanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideWorkspacePanel();
        hideTooltipNow();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        volumePanelLeft = Math.min(
            Math.max(180, rect.left + rect.width / 2),
            window.innerWidth - 180,
        );
        volumePanelOpen = true;
        masterError = null;
        void refreshMasterAudio();
        void tick().then(() => sendRegion(true));
    }

    function closeVolumePanel() {
        if (!volumePanelOpen) return;
        volumePanelOpen = false;
        masterError = null;
        clearTimeout(masterVolumeApplyTimer);
        scheduleRegion(hovering);
    }

    function openWifiPanel(event: MouseEvent) {
        event.preventDefault();
        event.stopPropagation();

        if (wifiPanelOpen) {
            closeWifiPanel();
            return;
        }

        closeMenu();
        closeVolumePanel();
        closeMediaPanel();
        closeBackgroundPanel();
        hidePreview();
        hideTooltipNow();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        wifiPanelLeft = Math.min(
            Math.max(210, rect.left + rect.width / 2),
            window.innerWidth - 210,
        );
        wifiPanelOpen = true;
        wifiError = null;
        wifiPasswordFor = null;
        wifiForgetFor = null;
        void loadWifiStatus(true);
        void tick().then(() => sendRegion(true));
    }

    function closeWifiPanel() {
        if (!wifiPanelOpen) return;
        wifiPanelOpen = false;
        wifiPasswordFor = null;
        wifiPassword = '';
        wifiPasswordVisible = false;
        wifiForgetFor = null;
        wifiError = null;
        void api.releaseDockFocus().catch(() => {});
        scheduleRegion(hovering);
    }

    function cancelWifiPassword() {
        wifiPasswordFor = null;
        wifiPassword = '';
        wifiPasswordVisible = false;
        wifiError = null;
        void api.releaseDockFocus().catch(() => {});
    }

    async function toggleWifi() {
        if (!wifiStatus || wifiBusy) return;
        const target = !wifiStatus.enabled;
        wifiBusy = 'toggle';
        wifiError = null;

        try {
            await api.setWifiEnabled(target);
            for (let attempt = 0; attempt < 6; attempt++) {
                await new Promise(resolve => setTimeout(resolve, 650));
                const status = await loadWifiStatus(true);
                if (status && status.enabled === target) break;
            }
        } catch (error) {
            wifiError = errorText(error);
        } finally {
            wifiBusy = null;
            void tick().then(() => sendRegion(true));
        }
    }

    function connectNetwork(network: WifiNetwork) {
        if (wifiBusy || network.connected) return;
        wifiForgetFor = null;
        wifiError = null;

        if (!network.connectable) {
            wifiError = 'Esta rede não está aceitando conexões agora.';
            return;
        }

        if (!network.secured || network.hasProfile) {
            void doWifiConnect(network.ssid);
            return;
        }

        if (!network.auth.endsWith('Pessoal')) {
            wifiError = network.auth.includes('Enterprise')
                ? 'Rede corporativa: use o Windows para informar usuário e senha.'
                : `A dock ainda não configura redes ${network.auth}.`;
            return;
        }

        wifiPasswordFor = network.ssid;
        wifiPassword = '';
        wifiPasswordVisible = false;
        void api.focusDock().catch(() => {});
        void tick().then(() => sendRegion(true));
        setTimeout(() => {
            wifiPanelEl
                ?.querySelector<HTMLInputElement>('.wifi-password input')
                ?.focus();
        }, 80);
    }

    async function submitWifiPassword(network: WifiNetwork) {
        if (!wifiPassword || wifiBusy) return;
        if (wifiPassword.length < 8) {
            wifiError = 'A senha precisa ter pelo menos 8 caracteres.';
            return;
        }
        await doWifiConnect(network.ssid, wifiPassword);
    }

    async function doWifiConnect(ssid: string, password?: string) {
        if (wifiBusy) return;
        wifiBusy = `connect:${ssid}`;
        wifiError = null;

        try {
            await api.connectWifi(ssid, password);
            const connected = await waitForWifi(ssid);
            if (connected) {
                wifiPasswordFor = null;
                wifiPassword = '';
                wifiPasswordVisible = false;
                wifiForgetFor = null;
                void api.releaseDockFocus().catch(() => {});
            } else {
                wifiError =
                    'Não foi possível conectar. Confira a senha e tente novamente.';
            }
        } catch (error) {
            wifiError = errorText(error);
        } finally {
            wifiBusy = null;
            await loadWifiStatus(true);
        }
    }

    async function waitForWifi(ssid: string): Promise<boolean> {
        for (let attempt = 0; attempt < 20; attempt++) {
            await new Promise(resolve => setTimeout(resolve, 900));
            try {
                const status = await api.getWifiStatus(true);
                wifiStatus = status;
                if (
                    status.connected &&
                    (status.ssid ?? '').toLowerCase() === ssid.toLowerCase()
                ) {
                    return true;
                }
            } catch {
                // Keep polling until Windows settles on a state.
            }
        }
        return false;
    }

    async function disconnectWifi() {
        if (wifiBusy) return;
        wifiBusy = 'disconnect';
        wifiError = null;

        try {
            await api.disconnectWifi();
            await new Promise(resolve => setTimeout(resolve, 700));
            await loadWifiStatus(true);
        } catch (error) {
            wifiError = errorText(error);
        } finally {
            wifiBusy = null;
        }
    }

    function armForget(network: WifiNetwork | null) {
        if (!network || !network.hasProfile) return;
        clearTimeout(wifiForgetTimer);

        if (wifiForgetFor === network.ssid) {
            void forgetWifi(network);
            return;
        }

        wifiForgetFor = network.ssid;
        wifiForgetTimer = setTimeout(() => {
            wifiForgetFor = null;
        }, 3500);
    }

    async function forgetWifi(network: WifiNetwork) {
        wifiBusy = `forget:${network.ssid}`;
        wifiError = null;

        try {
            await api.forgetWifi(network.profileName ?? network.ssid);
            wifiForgetFor = null;
            await new Promise(resolve => setTimeout(resolve, 400));
            await loadWifiStatus(true);
        } catch (error) {
            wifiError = errorText(error);
        } finally {
            wifiBusy = null;
        }
    }

    function openNetworkSettings() {
        closeWifiPanel();
        openStatusPage('network');
    }

    async function refreshMedia() {
        try {
            const status = await api.getMediaStatus();
            const now = Date.now();
            const previous = mediaPosition;
            const wasPlaying = media?.playing ?? false;
            const sameTrack =
                media?.title === status.title &&
                (media?.artist ?? '') === (status.artist ?? '');
            const rawChanged = status.positionMs !== mediaRawPosition;

            media = status;

            if (!status.hasSession || !status.title) {
                mediaRawPosition = -1;
                mediaBasePosition = 0;
                mediaBaseAt = now;
                mediaPosition = 0;
                mediaTimelineDuration = 0;
                mediaArtKey = '';
                mediaArt = null;
                if (mediaOpen) closeMediaPanel();
                return;
            }

            if (!sameTrack) {
                mediaTimelineDuration = 0;
                mediaBasePosition = status.positionMs;
                mediaBaseAt = now;
                mediaPosition = status.positionMs;
            } else if (mediaSeeking) {
                // Keep the value the user is dragging.
            } else if (now < mediaSeekUntil) {
                // Trust the seek target until the player catches up.
            } else if (!status.playing) {
                if (rawChanged) {
                    mediaBasePosition = status.positionMs;
                    mediaBaseAt = now;
                    mediaPosition = status.positionMs;
                } else if (wasPlaying) {
                    mediaBasePosition = previous;
                    mediaBaseAt = now;
                    mediaPosition = previous;
                }
            } else if (!wasPlaying) {
                mediaBasePosition = rawChanged ? status.positionMs : previous;
                mediaBaseAt = now;
                mediaPosition = mediaBasePosition;
            } else if (rawChanged) {
                mediaBasePosition = status.positionMs;
                mediaBaseAt = now;
                mediaPosition = status.positionMs;
            } else {
                // Chrome often keeps the SMTC position frozen while playing.
                // Keep the local clock instead of rewinding on every poll.
                const duration =
                    mediaTimelineDuration > 0
                        ? mediaTimelineDuration
                        : status.durationMs;
                mediaPosition = Math.min(
                    duration,
                    mediaBasePosition + (now - mediaBaseAt),
                );
            }

            mediaRawPosition = status.positionMs;

            if (status.artKey && status.artKey !== mediaArtKey) {
                mediaArtKey = status.artKey;
                mediaArt = null;
                const seq = ++mediaArtSeq;
                api.getMediaArt()
                    .then(url => {
                        if (seq === mediaArtSeq) mediaArt = url;
                    })
                    .catch(() => {
                        if (seq === mediaArtSeq) mediaArt = null;
                    });
            }
        } catch {
            // The media bridge can be unavailable while Windows starts.
        }
    }

    function mediaDescription(): string {
        if (!media) return 'Mídia';
        const app = media.app?.trim();
        const artist = media.artist?.trim();
        const title = media.title?.trim();
        if (app) return artist ? `${app} · ${artist}` : app;
        return artist ?? title ?? 'Mídia';
    }

    function showMediaTooltip() {
        if (!mediaDotEl) return;
        const rect = mediaDotEl.getBoundingClientRect();
        tooltipLabel = mediaDescription();
        tooltipLeft = Math.max(
            90,
            Math.min(window.innerWidth - 90, rect.left + rect.width / 2),
        );
        tooltipBottom = window.innerHeight - rect.top + 10;
        tooltipVisible = true;
    }

    function formatTime(ms: number): string {
        const seconds = Math.max(0, Math.floor(ms / 1000));
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor((seconds % 3600) / 60);
        const rest = seconds % 60;

        if (hours > 0) {
            return `${hours}:${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`;
        }
        return `${minutes}:${String(rest).padStart(2, '0')}`;
    }

    function positionMediaPanel(): boolean {
        const rect = mediaDotEl?.getBoundingClientRect();
        if (!rect) return false;
        mediaPanelLeft = Math.min(
            Math.max(190, rect.left + rect.width / 2),
            window.innerWidth - 190,
        );
        return true;
    }

    function openMediaPanel(event: MouseEvent) {
        event.preventDefault();
        event.stopPropagation();

        if (mediaOpen) {
            if (!mediaPinned) {
                mediaPinned = true;
                cancelMediaHoverClose();
                void syncMediaTimeline(true);
                return;
            }
            closeMediaPanel();
            return;
        }

        closeMenu();
        closeWifiPanel();
        closeVolumePanel();
        closeBackgroundPanel();
        hidePreview();
        hideWorkspacePanel();
        hideTooltipNow();
        if (!positionMediaPanel()) return;
        clearTimeout(mediaHoverTimer);
        clearTimeout(mediaHoverCloseTimer);
        mediaPinned = true;
        mediaOpen = true;
        void syncMediaTimeline(true);
        void tick().then(() => sendRegion(true));
    }

    function openMediaHover() {
        if (!mediaVisible || !mediaDotEl) return;
        if (mediaOpen) {
            if (!mediaPinned) cancelMediaHoverClose();
            return;
        }
        if (!positionMediaPanel()) return;
        hidePreview();
        hideWorkspacePanel();
        hideTooltipNow();
        mediaPinned = false;
        mediaOpen = true;
        void syncMediaTimeline(false);
        void tick().then(() => sendRegion(hovering));
    }

    function onMediaDotEnter() {
        clearHide();
        clearTimeout(mediaHoverTimer);
        cancelMediaHoverClose();
        if (mediaPinned || mediaOpen) return;
        setTimeout(showMediaTooltip, 0);
        mediaHoverTimer = setTimeout(openMediaHover, 420);
    }

    function onMediaDotLeave() {
        clearTimeout(mediaHoverTimer);
        hideTooltipNow();
        if (!mediaPinned) scheduleMediaHoverClose();
    }

    function scheduleMediaHoverClose(delay = 260) {
        clearTimeout(mediaHoverCloseTimer);
        mediaHoverCloseTimer = setTimeout(() => {
            if (!mediaPinned && !mediaPanelHover) closeMediaPanel();
        }, delay);
    }

    function cancelMediaHoverClose() {
        clearTimeout(mediaHoverCloseTimer);
    }

    function onMediaPanelEnter() {
        clearHide();
        mediaPanelHover = true;
        cancelMediaHoverClose();
    }

    function onMediaPanelLeave() {
        mediaPanelHover = false;
        scheduleHide();
        if (!mediaPinned) scheduleMediaHoverClose();
    }

    function closeMediaPanel(skipPreviewRestore = false) {
        stopMediaPreview(!skipPreviewRestore);
        clearTimeout(mediaHoverTimer);
        clearTimeout(mediaHoverCloseTimer);
        mediaPinned = false;
        mediaPanelHover = false;
        if (!mediaOpen) return;
        mediaOpen = false;
        scheduleRegion(hovering);
    }

    async function captureMediaPreview(seq: number) {
        if (!mediaPreviewOpen || seq !== mediaPreviewSeq || mediaPreviewHwnd == null) {
            return;
        }

        const hwnd = mediaPreviewHwnd;
        try {
            const frame = await api.windowPreviewJpeg(hwnd, 328, 185);
            if (seq === mediaPreviewSeq && mediaPreviewOpen && frame) {
                mediaPreviewFrame = frame;
            }
        } catch {
            // A frame may fail while the window is being resized or redrawn.
        }

        if (seq === mediaPreviewSeq && mediaPreviewOpen && mediaPreviewHwnd != null) {
            mediaPreviewTimer = setTimeout(
                () => void captureMediaPreview(seq),
                110,
            );
        }
    }

    async function resolveMediaPreview() {
        const seq = ++mediaPreviewSeq;
        mediaPreviewArtKey = media?.artKey ?? '';
        mediaPreviewFrame = null;
        mediaPreviewHwnd = null;
        mediaPreviewMissing = false;
        mediaPreviewResolving = true;
        void tick().then(() => sendRegion(true));

        await restoreMediaPreviewTab();
        if (seq !== mediaPreviewSeq || !mediaPreviewOpen) return;

        const target = await api
            .mediaPreviewStart(media?.title ?? null, media?.app ?? null)
            .catch(() => null);
        if (seq !== mediaPreviewSeq || !mediaPreviewOpen) {
            if (target?.tab) {
                void api.mediaPreviewStop(target.tab).catch(() => false);
            }
            return;
        }

        mediaPreviewResolving = false;
        if (!target) {
            mediaPreviewMissing = true;
            return;
        }

        mediaPreviewHwnd = target.hwnd;
        mediaPreviewTab = target.tab;
        setTimeout(() => void syncMediaTimeline(false), 500);
        mediaPreviewTimer = setTimeout(
            () => void captureMediaPreview(seq),
            220,
        );
    }

    async function restoreMediaPreviewTab() {
        const tab = mediaPreviewTab;
        mediaPreviewTab = null;
        if (tab) {
            await api.mediaPreviewStop(tab).catch(() => false);
        }
    }

    function toggleMediaPreview() {
        if (mediaPreviewOpen) {
            stopMediaPreview();
            return;
        }

        mediaPinned = true;
        cancelMediaHoverClose();
        mediaPreviewOpen = true;
        void resolveMediaPreview();
    }

    function stopMediaPreview(restore = true) {
        if (restore) {
            void restoreMediaPreviewTab();
        } else {
            mediaPreviewTab = null;
        }

        if (mediaPreviewTimer) clearTimeout(mediaPreviewTimer);
        mediaPreviewTimer = undefined;
        mediaPreviewSeq++;
        mediaPreviewOpen = false;
        mediaPreviewResolving = false;
        mediaPreviewMissing = false;
        mediaPreviewFrame = null;
        mediaPreviewHwnd = null;
        mediaPreviewArtKey = '';
        void tick().then(() => sendRegion(true));
    }

    function openMediaPreviewWindow() {
        const hwnd = mediaPreviewHwnd;
        closeMediaPanel(true);
        if (hwnd != null) void focusWindow(hwnd);
    }

    async function refreshMediaVolume() {
        if (Date.now() - mediaVolumeSyncAt < 1000) return;
        const audio = await api.getMediaAudio().catch(() => null);
        if (!mediaOpen || Date.now() - mediaVolumeSyncAt < 1000) return;
        mediaVolume = audio ? audio.volume : null;
        mediaMuted = audio ? audio.muted : false;
    }

    function onMediaVolumeInput(event: Event) {
        const value = Number((event.currentTarget as HTMLInputElement).value);
        const level = value / 100;
        mediaVolume = level;
        mediaVolumeSyncAt = Date.now();

        if (level > 0 && mediaMuted) {
            mediaMuted = false;
            void api.setMediaMuted(false).catch(() => {});
        }

        clearTimeout(mediaVolumeApplyTimer);
        mediaVolumeApplyTimer = setTimeout(() => {
            void api.setMediaVolume(level).catch(() => {});
        }, 70);
    }

    function onMediaVolumeCommit() {
        if (mediaVolume == null) return;
        mediaVolumeSyncAt = Date.now();
        clearTimeout(mediaVolumeApplyTimer);
        void api.setMediaVolume(mediaVolume).catch(() => {});
    }

    function toggleMediaMute() {
        if (mediaVolume == null) return;
        mediaMuted = !mediaMuted;
        mediaVolumeSyncAt = Date.now();
        void api.setMediaMuted(mediaMuted).catch(() => {});
    }

    async function mediaAction(action: string) {
        if (mediaBusy) return;
        mediaBusy = action;

        try {
            await api.mediaControl(action);
            await new Promise(resolve => setTimeout(resolve, 250));
            await refreshMedia();
        } catch (error) {
            console.error('media_control failed', error);
        } finally {
            mediaBusy = null;
        }
    }

    async function syncMediaTimeline(allowSwitch: boolean) {
        if (!mediaOpen || !media || mediaSyncing) return;

        mediaSyncing = true;
        const key = media.artKey ?? '';
        mediaTimelineKey = key;
        try {
            const sample = await api.getMediaTimeline(allowSwitch);
            if (!mediaOpen || !sample || (media?.artKey ?? '') !== key) return;

            mediaBasePosition = sample.positionMs;
            mediaBaseAt = Date.now();
            mediaPosition = sample.positionMs;
            if (sample.durationMs > 0) mediaTimelineDuration = sample.durationMs;
            mediaRawPosition = media.positionMs;
            mediaSeekUntil = Date.now() + 1200;
        } finally {
            mediaSyncing = false;
        }
    }

    function applySeekTarget(target: number) {
        mediaPosition = target;
        mediaBasePosition = target;
        mediaBaseAt = Date.now();
        mediaSeekUntil = Date.now() + 1500;
    }

    async function seekRelative(delta: number) {
        if (!mediaDuration) return;

        const target = Math.max(
            0,
            Math.min(mediaDuration, mediaPosition + delta),
        );
        applySeekTarget(target);

        try {
            await api.mediaControl('seek', target);
            await new Promise(resolve => setTimeout(resolve, 300));
            await refreshMedia();
        } catch (error) {
            console.error('media seek failed', error);
        }
    }

    function onMediaSeekInput(event: Event) {
        const value = Number((event.currentTarget as HTMLInputElement).value);
        mediaPosition = value;
        mediaBasePosition = value;
        mediaBaseAt = Date.now();
        mediaSeekUntil = Date.now() + 1500;
        mediaSeeking = true;
    }

    async function onMediaSeekCommit(event: Event) {
        const value = Number((event.currentTarget as HTMLInputElement).value);
        const previous = mediaPosition;
        mediaPosition = value;
        mediaBasePosition = value;
        mediaBaseAt = Date.now();
        mediaSeeking = false;
        mediaSeekUntil = Date.now() + 1500;

        if (!mediaDuration) return;

        try {
            await api.mediaControl('seek', value);
            await new Promise(resolve => setTimeout(resolve, 350));
            await refreshMedia();
        } catch (error) {
            console.error('media seek failed', error);
            mediaBasePosition = previous;
            mediaBaseAt = Date.now();
            mediaPosition = previous;
            mediaSeekUntil = 0;
        }
    }

    async function loadBackgroundApps() {
        const seq = ++bgSeq;
        if (bgApps.length === 0) bgLoading = true;

        try {
            const list = await api.getBackgroundApps();
            if (seq !== bgSeq) return;
            bgApps = list;

            const catalogByExe = new Map<string, AppEntry>();
            for (const catalogApp of apps) {
                for (const source of [catalogApp.target, catalogApp.path]) {
                    const base = normalizePath(source).split('\\').pop() ?? '';
                    if (base) catalogByExe.set(base, catalogApp);
                }
            }

            for (const app of list) {
                if (bgIcons[app.key]) continue;

                const now = Date.now();
                const lastRequest = bgRequested.get(app.key) ?? 0;
                if (now - lastRequest < 8000) continue;
                bgRequested.set(app.key, now);

                const exeBase = normalizePath(app.exe).split('\\').pop() ?? '';
                const match = exeBase ? catalogByExe.get(exeBase) : undefined;

                if (match && icons[match.id]) {
                    bgIcons = { ...bgIcons, [app.key]: icons[match.id] };
                    continue;
                }

                const request = match
                    ? api.getIcon(match.id).catch(() => null)
                    : Promise.resolve(null);

                request
                    .then(icon => {
                        if (icon) return icon;
                        return app.exe
                            ? api.getExeIcon(app.exe).catch(() => null)
                            : null;
                    })
                    .then(icon => {
                        if (icon) {
                            bgIcons = { ...bgIcons, [app.key]: icon };
                        }
                    })
                    .catch(() => {});
            }

            if (
                bgConfirmPid !== null &&
                !list.some(app => app.pid === bgConfirmPid)
            ) {
                bgConfirmPid = null;
            }
        } catch (error) {
            if (seq === bgSeq) bgError = errorText(error);
        } finally {
            if (seq === bgSeq) bgLoading = false;
        }
    }

    function openBackgroundPanel(event: MouseEvent) {
        event.preventDefault();
        event.stopPropagation();

        if (bgPanelOpen) {
            closeBackgroundPanel();
            return;
        }

        closeMenu();
        closeWifiPanel();
        closeVolumePanel();
        closeMediaPanel();
        hidePreview();
        hideTooltipNow();
        const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
        bgPanelLeft = Math.min(
            Math.max(220, rect.left + rect.width / 2),
            window.innerWidth - 220,
        );
        bgPanelOpen = true;
        bgError = null;
        bgConfirmPid = null;
        void loadBackgroundApps();
        void tick().then(() => sendRegion(true));
    }

    function closeBackgroundPanel() {
        if (!bgPanelOpen) return;
        bgPanelOpen = false;
        bgError = null;
        bgConfirmPid = null;
        clearTimeout(bgConfirmTimer);
        scheduleRegion(hovering);
    }

    async function openBackgroundApp(app: BackgroundApp) {
        if (bgBusy) return;
        bgBusy = app.key;
        bgError = null;

        try {
            await api.backgroundActivate(app.hwnd, app.exe);
            closeBackgroundPanel();
        } catch (error) {
            bgError = errorText(error);
        } finally {
            bgBusy = null;
        }
    }

    function armBackgroundTerminate(app: BackgroundApp) {
        clearTimeout(bgConfirmTimer);

        if (bgConfirmPid === app.pid) {
            void terminateBackgroundApp(app);
            return;
        }

        bgConfirmPid = app.pid;
        bgConfirmTimer = setTimeout(() => {
            bgConfirmPid = null;
        }, 3500);
    }

    async function terminateBackgroundApp(app: BackgroundApp) {
        if (bgBusy) return;
        bgBusy = `kill:${app.key}`;
        bgError = null;

        try {
            await api.backgroundTerminate(app.pid);
            bgConfirmPid = null;
            await new Promise(resolve => setTimeout(resolve, 450));
            await loadBackgroundApps();
        } catch (error) {
            bgError = errorText(error);
        } finally {
            bgBusy = null;
        }
    }

    function backgroundStateLabel(state: string): string {
        if (state === 'minimized') return 'Minimizado';
        if (state === 'hidden') return 'Oculto';
        if (state === 'no-window') return 'Sem janela';
        return state;
    }

    function formatMemory(bytes: number): string {
        if (!bytes) return '—';
        const mb = bytes / (1024 * 1024);
        return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
    }

    function openTaskManager() {
        closeBackgroundPanel();
        void api
            .backgroundActivate(null, 'C:\\Windows\\System32\\Taskmgr.exe')
            .catch(error => console.error('task manager failed', error));
    }

    function batteryFillWidth(): number {
        if (batteryPercent === null) return 1;
        return Math.max(1, Math.min(14, batteryPercent * 0.14));
    }

    function quit() {
        void api.quit().catch(() => {});
    }

    function initial(name: string): string {
        return (name.trim()[0] ?? '?').toUpperCase();
    }
</script>

<div
    class="dock-stage"
    class:is-hidden={hidden}
    style={`--anim-duration:${animationDuration}ms; --anim-speed:${settings.animations ? settings.animationSpeed : 0};`}
>
    <div
        class="dock-bar"
        class:is-zooming={hovering}
        bind:this={barEl}
        style={`--icon:${settings.iconSize}px; --gap:${settings.iconSpacing}px; --pad-x:${settings.dockPaddingX}px; --pad-y:${settings.dockPaddingY}px; --margin:${settings.dockMargin}px; --dock-h:${dockHeight}px; --dock-radius:${settings.dockRadius}px;`}
        oncontextmenu={openDockMenu}
    >
        <div class="dock-bg" bind:this={bgEl}></div>

        <div class="dock-left">
            <button
                class="launcher-btn workspace-btn"
                tabindex="-1"
                bind:this={workspaceBtnEl}
                title="Áreas de trabalho — passe o mouse para ver a lista; clique para a visão de tarefas"
                onclick={openTaskView}
                oncontextmenu={openWorkspaceMenu}
                onwheel={onWorkspaceWheel}
                onpointerenter={onWorkspaceEnter}
                onpointerleave={onWorkspaceLeave}
            >
                <span
                    class="workspace-line"
                    class:pulse-left={workspacePulse === 'left'}
                    class:pulse-right={workspacePulse === 'right'}
                    aria-hidden="true"
                >
                    {#if workspaceDesktops.length > 0}
                        {#each workspaceDesktops as desktop (desktop.index)}
                            <i class:is-current={desktop.current}></i>
                        {/each}
                    {:else}
                        <i class="is-current"></i><i></i>
                    {/if}
                </span>
            </button>

            {#if mediaVisible && media}
                <button
                    class="media-dot"
                    class:is-playing={media.playing}
                    bind:this={mediaDotEl}
                    style={`--progress:${mediaProgress}`}
                    onclick={openMediaPanel}
                    onpointerenter={onMediaDotEnter}
                    onpointerleave={onMediaDotLeave}
                    aria-label={mediaDescription()}
                >
                    <span class="media-dot-inner">
                        {#if mediaSourceIcon}
                            <img src={mediaSourceIcon} alt="" draggable="false" />
                        {:else if mediaArt}
                            <img src={mediaArt} alt="" draggable="false" />
                        {:else}
                            <svg
                                viewBox="0 0 24 24"
                                width="15"
                                height="15"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.6"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                aria-hidden="true"
                            >
                                <path d="M9 18V6l10-2v12" />
                                <circle cx="6.5" cy="18" r="2.5" />
                                <circle cx="16.5" cy="16" r="2.5" />
                            </svg>
                        {/if}
                    </span>
                    <span class="media-dot-badge" aria-hidden="true">
                        <svg
                            viewBox="0 0 16 16"
                            width="7"
                            height="7"
                            fill="currentColor"
                            aria-hidden="true"
                        >
                            <path d="M6 2.2l6-1.1v7.2a2.1 2.1 0 1 1-1.2-1.9V3.4L7.2 4v6.8A2.1 2.1 0 1 1 6 8.9z" />
                        </svg>
                    </span>
                </button>
            {/if}

            <div
                class="dock-row"
                bind:this={rowEl}
                onpointerenter={onAnchorEnter}
                onpointerleave={onAnchorLeave}
                onpointermove={onAnchorPointerMove}
            >
                {#each items as item (item.key)}
                    {#if item.kind === 'app'}
                        <button
                            class="dock-item"
                            class:is-running={item.running}
                            class:is-focused={item.focused}
                            class:is-dragging={draggingKey === item.key}
                            class:drop-before={dragOverKey === item.key &&
                                !dragAfter}
                            class:drop-after={dragOverKey === item.key &&
                                dragAfter}
                            data-key={item.key}
                            tabindex="-1"
                            draggable={true}
                            ondragstart={event =>
                                onDockDragStart(event, item)}
                            ondragover={event =>
                                onDockDragOver(event, item)}
                            ondrop={event => onDockDrop(event, item)}
                            ondragend={onDockDragEnd}
                            onclick={event => onAppClick(event, item.app)}
                            onauxclick={event =>
                                onAppAuxClick(event, item.app)}
                            oncontextmenu={event =>
                                openAppMenu(event, item.app)}
                        >
                            <span class="icon-scale">
                                {#key bounces[item.app.id] ?? 0}
                                    <span
                                        class="bounce-wrap"
                                        class:is-bouncing={bouncing[
                                            item.app.id
                                        ]}
                                        class:is-pressed={pressing[item.app.id]}
                                    >
                                        {#if icons[item.app.id]}
                                            <img
                                                src={icons[item.app.id]}
                                                alt=""
                                                draggable="false"
                                            />
                                        {:else}
                                            <span class="letter"
                                                >{initial(item.app.name)}</span
                                            >
                                        {/if}
                                    </span>
                                {/key}
                            </span>
                            {#if item.running}
                                <span class="dots" aria-hidden="true">
                                    {#each Array(
                                        Math.min(item.count, 5),
                                    ) as _, index}
                                        <i
                                            class="dot"
                                            class:focused={index ===
                                                item.focusedIndex}
                                        ></i>
                                    {/each}
                                </span>
                            {/if}
                        </button>
                    {:else if item.kind === 'dynamic'}
                        <button
                            class="dock-item"
                            class:is-running={true}
                            class:is-focused={item.focused}
                            class:is-dragging={draggingKey === item.key}
                            class:drop-before={dragOverKey === item.key &&
                                !dragAfter}
                            class:drop-after={dragOverKey === item.key &&
                                dragAfter}
                            data-key={item.key}
                            tabindex="-1"
                            title={item.name}
                            draggable={true}
                            ondragstart={event =>
                                onDockDragStart(event, item)}
                            ondragover={event =>
                                onDockDragOver(event, item)}
                            ondrop={event => onDockDrop(event, item)}
                            ondragend={onDockDragEnd}
                            onclick={() => focusDynamic(item)}
                            oncontextmenu={event =>
                                openDynamicMenu(event, item)}
                        >
                            <span class="icon-scale">
                                <span
                                    class="bounce-wrap"
                                    class:is-pressed={pressing[item.key]}
                                >
                                    {#if icons[item.key]}
                                        <img
                                            src={icons[item.key]}
                                            alt=""
                                            draggable="false"
                                        />
                                    {:else}
                                        <span class="letter"
                                            >{initial(item.name)}</span
                                        >
                                    {/if}
                                </span>
                            </span>
                            <span class="dots" aria-hidden="true">
                                {#each Array(Math.min(item.count, 5)) as _, index}
                                    <i
                                        class="dot"
                                        class:focused={index ===
                                            item.focusedIndex}
                                    ></i>
                                {/each}
                            </span>
                        </button>
                    {:else if item.kind === 'folder'}
                        <button
                            class="dock-item folder"
                            class:is-dragging={draggingKey === item.key}
                            class:drop-before={dragOverKey === item.key &&
                                !dragAfter}
                            class:drop-after={dragOverKey === item.key &&
                                dragAfter}
                            data-key={item.key}
                            tabindex="-1"
                            title={item.name}
                            draggable={true}
                            ondragstart={event =>
                                onDockDragStart(event, item)}
                            ondragover={event =>
                                onDockDragOver(event, item)}
                            ondrop={event => onDockDrop(event, item)}
                            ondragend={onDockDragEnd}
                            onclick={() => openDockFolder(item.path)}
                            oncontextmenu={event =>
                                openFolderMenu(event, item)}
                        >
                            <span class="icon-scale">
                                <span
                                    class="bounce-wrap"
                                    class:is-pressed={pressing[item.key]}
                                >
                                    {#if icons[item.key]}
                                        <img
                                            src={icons[item.key]}
                                            alt=""
                                            draggable="false"
                                        />
                                    {:else}
                                        <span class="folder-tile">
                                            <svg
                                                viewBox="0 0 24 24"
                                                width="58%"
                                                height="58%"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="1.6"
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                                aria-hidden="true"
                                            >
                                                <path
                                                    d="M3 7.2A1.8 1.8 0 0 1 4.8 5.4h4.4l2.1 2.1h7.9A1.8 1.8 0 0 1 21 9.3v7.5a1.8 1.8 0 0 1-1.8 1.8H4.8A1.8 1.8 0 0 1 3 16.8z"
                                                />
                                            </svg>
                                        </span>
                                    {/if}
                                </span>
                            </span>
                        </button>
                    {:else}
                        <button
                            class="dock-item trash"
                            tabindex="-1"
                            onclick={openTrash}
                            oncontextmenu={openTrashMenu}
                        >
                            <span class="icon-scale">
                                {#if trashIcon}
                                    <img
                                        src={trashIcon}
                                        alt=""
                                        draggable="false"
                                    />
                                {:else}
                                    <span class="letter">🗑</span>
                                {/if}
                            </span>
                            {#if trashFull}
                                <span class="trash-badge"></span>
                            {/if}
                        </button>
                    {/if}
                {/each}
            </div>
        </div>

        <div class="dock-right">
            <button
                class="status-btn background"
                class:is-active={bgPanelOpen}
                tabindex="-1"
                title="Programas em segundo plano"
                onclick={openBackgroundPanel}
            >
                <svg
                    viewBox="0 0 16 16"
                    width="14"
                    height="14"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.35"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                >
                    <path d="M8 2.2l5.5 2.8L8 7.8 2.5 5z" />
                    <path d="M2.5 8l5.5 2.8L13.5 8" />
                    <path d="M2.5 11l5.5 2.8 5.5-2.8" />
                </svg>
            </button>

            <button
                class="status-btn network"
                class:is-active={wifiPanelOpen}
                class:is-connected={wifiConnected}
                class:is-muted={wifiHasAdapter && !wifiEnabled}
                tabindex="-1"
                title={wifiTitle()}
                onclick={openWifiPanel}
            >
                {#if wifiStatus && !wifiHasAdapter}
                    <svg
                        viewBox="0 0 16 16"
                        width="15"
                        height="15"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.35"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path d="M5 2.2v3" />
                        <path d="M11 2.2v3" />
                        <path d="M3.6 5.2h8.8v3.7a4.4 4.4 0 0 1-8.8 0z" />
                        <path d="M8 12.9v1.1" />
                    </svg>
                {:else}
                    <svg
                        viewBox="0 0 16 16"
                        width="14"
                        height="14"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.4"
                        stroke-linecap="round"
                        aria-hidden="true"
                    >
                        <path
                            d="M2.5 5.8a8 8 0 0 1 11 0"
                            style:opacity={wifiConnected
                                ? wifiSignal >= 55
                                    ? 1
                                    : 0.3
                                : 0.4}
                        />
                        <path
                            d="M4.8 8.2a5 5 0 0 1 6.4 0"
                            style:opacity={wifiConnected
                                ? wifiSignal >= 30
                                    ? 1
                                    : 0.3
                                : 0.4}
                        />
                        <path
                            d="M6.8 10.5a2.4 2.4 0 0 1 2.4 0"
                            style:opacity={wifiConnected ? 1 : 0.4}
                        />
                        <circle
                            cx="8"
                            cy="12.7"
                            r="0.8"
                            fill="currentColor"
                            stroke="none"
                            style:opacity={wifiConnected ? 1 : 0.4}
                        />
                        {#if wifiStatus && wifiHasAdapter && !wifiEnabled}
                            <line x1="2.8" y1="13.2" x2="13.2" y2="2.8" />
                        {/if}
                    </svg>
                {/if}
                {#if wifiConnected}
                    <span class="status-online-dot"></span>
                {/if}
            </button>

            <button
                class="status-btn sound"
                class:is-active={volumePanelOpen}
                class:is-muted={masterMuted || masterVolume === 0}
                tabindex="-1"
                title={masterMuted
                    ? 'Som silenciado'
                    : masterVolume == null
                      ? 'Som'
                      : `Som — ${Math.round(masterVolume * 100)}%`}
                aria-label="Controle de volume"
                onclick={openVolumePanel}
                oncontextmenu={event => {
                    event.preventDefault();
                    event.stopPropagation();
                    closeVolumePanel();
                    openStatusPage('sound');
                }}
            >
                <svg
                    viewBox="0 0 16 16"
                    width="14"
                    height="14"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.4"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                >
                    <path d="M3 6.2h2.2L8.4 3.7v8.6L5.2 9.8H3z" />
                    {#if masterMuted || masterVolume === 0}
                        <path d="M10.9 6.1l3.4 3.8M14.3 6.1l-3.4 3.8" />
                    {:else if (masterVolume ?? 0) < 0.45}
                        <path d="M10.8 6.4a2.6 2.6 0 0 1 0 3.2" />
                    {:else}
                        <path d="M10.5 5.9a3.6 3.6 0 0 1 0 4.2" />
                        <path d="M12.4 4.6a6.2 6.2 0 0 1 0 6.8" />
                    {/if}
                </svg>
            </button>

            {#if batteryPercent !== null}
                <button
                    class="status-btn battery"
                    tabindex="-1"
                    title={charging ? 'Bateria carregando' : 'Bateria'}
                    onclick={() => openStatusPage('battery')}
                >
                    <svg
                        viewBox="0 0 22 12"
                        width="20"
                        height="11"
                        fill="none"
                        aria-hidden="true"
                    >
                        <rect
                            x="1"
                            y="2"
                            width="18"
                            height="8"
                            rx="2"
                            stroke="currentColor"
                            stroke-width="1.2"
                        />
                        <rect
                            x="3"
                            y="4"
                            width={batteryFillWidth()}
                            height="4"
                            rx="1"
                            fill="currentColor"
                        />
                        <rect x="20" y="4" width="1.5" height="4" rx="0.7" fill="currentColor" />
                    </svg>
                    <span>{batteryPercent}%</span>
                    {#if charging}
                        <span class="battery-bolt">⚡</span>
                    {/if}
                </button>
            {/if}

            <span class="dock-divider clock-divider"></span>

            <button
                class="clock-block"
                tabindex="-1"
                title="Notificações e calendário"
                onclick={openActionCenter}
            >
                <span class="clock-time">{timeText}</span>
                <span class="clock-date">{dateText}</span>
            </button>
        </div>
    </div>

    <div
        class="tooltip"
        class:is-visible={tooltipVisible}
        style={`left:${tooltipLeft}px; bottom:${tooltipBottom}px`}
    >
        {tooltipLabel}
    </div>

    {#if previewOpen}
        <div
            class="window-preview"
            bind:this={previewEl}
            style={`left:${previewLeft}px; bottom:${previewBottom}px`}
            onpointerenter={onPreviewEnter}
            onpointerleave={onPreviewLeave}
            onwheel={onPreviewWheel}
        >
            <div class="preview-head">
                {#if previewIcon}
                    <img src={previewIcon} alt="" draggable="false" />
                {/if}
                <span class="preview-name">{previewName}</span>
                <span class="preview-count">
                    {previewWindows.length}
                    {previewWindows.length === 1 ? 'janela' : 'janelas'}
                </span>
            </div>

            <div class="preview-viewport">
                <div
                    class="preview-row"
                    class:is-dragging={previewDragging}
                    bind:this={previewRowEl}
                    role="toolbar"
                    aria-label={`Janelas de ${previewName}`}
                    aria-orientation="horizontal"
                    onscroll={updatePreviewScroll}
                    onkeydown={onPreviewKeyDown}
                    onpointerdown={onPreviewPointerDown}
                    onpointermove={onPreviewPointerMove}
                    onpointerup={onPreviewPointerUp}
                    onpointercancel={onPreviewPointerCancel}
                >
                    {#each previewWindows as window (window.hwnd)}
                        <button
                            class="preview-card"
                            class:is-hovered={previewHoverHwnd === window.hwnd}
                            title={window.title}
                            onclick={() => openPreviewWindow(window)}
                            onpointerenter={() => onPreviewCardEnter(window)}
                            onpointerleave={() => onPreviewCardLeave(window)}
                            onfocus={() => onPreviewCardEnter(window)}
                            onblur={() => onPreviewCardLeave(window)}
                        >
                            <span class="preview-thumb">
                                {#if previewThumbs[window.hwnd]}
                                    <img
                                        src={previewThumbs[window.hwnd]}
                                        alt=""
                                        draggable="false"
                                    />
                                {:else if previewLoading[window.hwnd]}
                                    <span class="preview-spinner"></span>
                                {:else if window.minimized}
                                    <span class="preview-placeholder">
                                        Minimizada
                                    </span>
                                {:else}
                                    <span class="preview-placeholder">
                                        Sem prévia
                                    </span>
                                {/if}
                            </span>
                            <span class="preview-title">
                                {window.title || 'Sem título'}
                            </span>
                            <span
                                class="preview-hover-hint"
                                class:is-visible={previewHoverHwnd ===
                                    window.hwnd}
                            >
                                Trazer para frente
                            </span>
                        </button>
                    {/each}
                </div>

                <span
                    class="preview-edge is-left"
                    class:is-visible={previewCanScrollLeft}
                    aria-hidden="true"
                ></span>
                <span
                    class="preview-edge is-right"
                    class:is-visible={previewCanScrollRight}
                    aria-hidden="true"
                ></span>

                <button
                    class="preview-nav is-left"
                    class:is-visible={previewCanScrollLeft}
                    tabindex="-1"
                    aria-label="Janelas anteriores"
                    title="Janelas anteriores"
                    onclick={() => scrollPreview(-1)}
                >
                    <svg
                        viewBox="0 0 16 16"
                        width="14"
                        height="14"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.7"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path d="M9.8 3.5 5.3 8l4.5 4.5" />
                    </svg>
                </button>
                <button
                    class="preview-nav is-right"
                    class:is-visible={previewCanScrollRight}
                    tabindex="-1"
                    aria-label="Próximas janelas"
                    title="Próximas janelas"
                    onclick={() => scrollPreview(1)}
                >
                    <svg
                        viewBox="0 0 16 16"
                        width="14"
                        height="14"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.7"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path d="M6.2 3.5 10.7 8l-4.5 4.5" />
                    </svg>
                </button>
            </div>
        </div>
    {/if}

    {#if workspacePanelOpen}
        <div
            class="workspace-panel"
            bind:this={workspacePanelEl}
            style={`left:${workspacePanelLeft}px; bottom:${workspacePanelBottom}px`}
            onpointerenter={onWorkspacePanelEnter}
            onpointerleave={onWorkspacePanelLeave}
            onwheel={event => event.stopPropagation()}
        >
            <div class="workspace-head">
                <span class="workspace-head-icon">
                    <svg
                        viewBox="0 0 16 16"
                        width="14"
                        height="14"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.35"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <rect x="1.6" y="2.4" width="12.8" height="9" rx="1.8" />
                        <path d="M5.4 13.6h5.2" />
                    </svg>
                </span>
                <span>Áreas de trabalho</span>
                {#if !workspaceLoading && !workspaceError}
                    <span class="workspace-count">
                        {workspaceDesktops.length}
                    </span>
                {/if}
            </div>

            {#if workspaceError}
                <div class="workspace-error">{workspaceError}</div>
            {:else if workspaceLoading && workspaceDesktops.length === 0}
                <div class="workspace-loading">
                    <span class="workspace-spinner"></span>
                    Lendo áreas de trabalho…
                </div>
            {:else}
                <div class="workspace-list">
                    {#each workspaceDesktops as desktop (desktop.index)}
                        <button
                            class="workspace-row"
                            class:is-current={desktop.current}
                            onclick={() => selectVirtualDesktop(desktop)}
                        >
                            <span class="workspace-row-icon">
                                <svg
                                    viewBox="0 0 16 16"
                                    width="13"
                                    height="13"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="1.35"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                    aria-hidden="true"
                                >
                                    <rect
                                        x="1.8"
                                        y="2.6"
                                        width="12.4"
                                        height="8.6"
                                        rx="1.6"
                                    />
                                    <path d="M5.6 13.4h4.8" />
                                </svg>
                            </span>
                            <span class="workspace-row-name">
                                {desktop.name}
                            </span>
                            {#if desktop.current}
                                <span class="workspace-current">Atual</span>
                            {/if}
                        </button>
                    {/each}
                </div>
            {/if}

            <div class="workspace-footer">
                <button class="workspace-action" onclick={newVirtualDesktop}>
                    <span>+ Nova área</span>
                </button>
                <button
                    class="workspace-action"
                    onclick={() => {
                        hideWorkspacePanel();
                        openTaskView();
                    }}
                >
                    Visão de tarefas
                </button>
            </div>
        </div>
    {/if}

    {#snippet signalBars(level: number)}
        <span class="signal-bars" data-level={level} aria-hidden="true">
            <i></i><i></i><i></i><i></i>
        </span>
    {/snippet}

    {#if volumePanelOpen}
        <div
            class="wifi-backdrop"
            onclick={closeVolumePanel}
            oncontextmenu={event => {
                event.preventDefault();
                closeVolumePanel();
            }}
        ></div>

        <div
            class="volume-panel"
            bind:this={volumePanelEl}
            style={`left:${volumePanelLeft}px; bottom:${menuBottom}px`}
            onpointerenter={clearHide}
            onpointerleave={() => scheduleHide()}
        >
            <div class="volume-header">
                <div class="volume-header-copy">
                    <h3>Som</h3>
                    <span>{masterStatusLabel()}</span>
                </div>
                <div class="volume-header-actions">
                    <button
                        class="icon-button"
                        onclick={closeVolumePanel}
                        title="Fechar"
                    >
                        <svg
                            viewBox="0 0 16 16"
                            width="13"
                            height="13"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.6"
                            stroke-linecap="round"
                            aria-hidden="true"
                        >
                            <path d="M4 4l8 8M12 4l-8 8" />
                        </svg>
                    </button>
                </div>
            </div>

            <div class="volume-control" class:is-muted={masterMuted}>
                <button
                    class="volume-mute"
                    onclick={() => void toggleMasterMute()}
                    disabled={masterVolume == null || !!masterBusy}
                    title={masterMuted ? 'Ativar som' : 'Silenciar'}
                    aria-label={masterMuted ? 'Ativar som' : 'Silenciar'}
                >
                    <svg
                        viewBox="0 0 24 24"
                        width="19"
                        height="19"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.7"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path
                            d="M4 9.5h3.2L12 5.6v12.8l-4.8-3.9H4z"
                            fill="currentColor"
                            stroke="none"
                        />
                        {#if masterMuted}
                            <path d="M15.8 9.4l4.4 5.2M20.2 9.4l-4.4 5.2" />
                        {:else if (masterVolume ?? 0) < 0.45}
                            <path d="M15.4 9.9a4 4 0 0 1 0 4.2" />
                        {:else}
                            <path d="M15.2 9.3a4.1 4.1 0 0 1 0 5.4" />
                            <path d="M17.8 6.9a7.7 7.7 0 0 1 0 10.2" />
                        {/if}
                    </svg>
                </button>

                <input
                    type="range"
                    min="0"
                    max="100"
                    step="1"
                    value={masterMuted
                        ? 0
                        : Math.round((masterVolume ?? 0) * 100)}
                    disabled={masterVolume == null}
                    oninput={onMasterVolumeInput}
                    onchange={onMasterVolumeCommit}
                    style={`--volume:${masterMuted ? 0 : (masterVolume ?? 0) * 100}%`}
                    aria-label="Volume do sistema"
                />

                <span class="volume-value">
                    {masterMuted
                        ? 'Mudo'
                        : masterVolume == null
                          ? '—'
                          : `${Math.round(masterVolume * 100)}%`}
                </span>
            </div>

            {#if masterError}
                <div class="volume-error">{masterError}</div>
            {/if}

            <div class="volume-footer">
                <button
                    class="volume-link"
                    onclick={() => {
                        closeVolumePanel();
                        openStatusPage('mixer');
                    }}
                >
                    <svg
                        viewBox="0 0 16 16"
                        width="14"
                        height="14"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.4"
                        stroke-linecap="round"
                        aria-hidden="true"
                    >
                        <path d="M4 2.5v11M8 2.5v11M12 2.5v11" />
                        <circle cx="4" cy="5.5" r="1.4" fill="currentColor" stroke="none" />
                        <circle cx="8" cy="10" r="1.4" fill="currentColor" stroke="none" />
                        <circle cx="12" cy="6.5" r="1.4" fill="currentColor" stroke="none" />
                    </svg>
                    <span>Misturador de volume</span>
                    <svg
                        class="volume-chevron"
                        viewBox="0 0 16 16"
                        width="12"
                        height="12"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path d="M6.5 4.5L10 8l-3.5 3.5" />
                    </svg>
                </button>

                <button
                    class="volume-link"
                    onclick={() => {
                        closeVolumePanel();
                        openStatusPage('sound');
                    }}
                >
                    <svg
                        viewBox="0 0 16 16"
                        width="14"
                        height="14"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.4"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <circle cx="8" cy="8" r="2.2" />
                        <path
                            d="M8 1.8v1.5M8 12.7v1.5M1.8 8h1.5M12.7 8h1.5M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M12.4 3.6l-1.1 1.1M4.7 11.3l-1.1 1.1"
                        />
                    </svg>
                    <span>Configurações de som</span>
                    <svg
                        class="volume-chevron"
                        viewBox="0 0 16 16"
                        width="12"
                        height="12"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path d="M6.5 4.5L10 8l-3.5 3.5" />
                    </svg>
                </button>
            </div>
        </div>
    {/if}

    {#if wifiPanelOpen}
        <div
            class="wifi-backdrop"
            onclick={closeWifiPanel}
            oncontextmenu={event => {
                event.preventDefault();
                closeWifiPanel();
            }}
        ></div>

        <div
            class="wifi-panel"
            bind:this={wifiPanelEl}
            style={`left:${wifiPanelLeft}px; bottom:${menuBottom}px`}
            onpointerenter={clearHide}
            onpointerleave={() => scheduleHide()}
        >
            <div class="wifi-header">
                <div class="wifi-header-copy">
                    <h3>Rede</h3>
                    <span>
                        {wifiStatus?.adapter
                            ? (wifiStatus.interfaceName ?? 'Wi-Fi')
                            : 'Este computador'}
                    </span>
                </div>
                <div class="wifi-header-actions">
                    <button
                        class="icon-button"
                        class:spinning={wifiLoading}
                        onclick={refreshWifi}
                        title="Atualizar"
                    >
                        <svg
                            viewBox="0 0 16 16"
                            width="14"
                            height="14"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <path d="M13.2 8a5.2 5.2 0 1 1-1.6-3.7" />
                            <path d="M13.5 2.8v3.4h-3.4" />
                        </svg>
                    </button>
                    <button
                        class="icon-button"
                        onclick={closeWifiPanel}
                        title="Fechar"
                    >
                        <svg
                            viewBox="0 0 16 16"
                            width="13"
                            height="13"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.6"
                            stroke-linecap="round"
                            aria-hidden="true"
                        >
                            <path d="M4 4l8 8M12 4l-8 8" />
                        </svg>
                    </button>
                </div>
            </div>

            {#if !wifiStatus}
                <div class="wifi-loading">
                    <span class="spinner"></span>
                    <span>Procurando redes…</span>
                </div>
            {:else if !wifiStatus.adapter}
                <div class="wifi-empty">
                    <span class="wifi-empty-icon">
                        <svg
                            viewBox="0 0 24 24"
                            width="30"
                            height="30"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.4"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <path d="M8 3.5v4" />
                            <path d="M16 3.5v4" />
                            <path d="M5.5 7.5h13v5a6.5 6.5 0 0 1-13 0z" />
                            <path d="M12 19v2" />
                        </svg>
                    </span>
                    <strong>Sem adaptador Wi-Fi</strong>
                    <p>
                        {wifiStatus.message ??
                            'Este computador não tem hardware de Wi-Fi.'}
                    </p>
                </div>

                {#if wifiStatus.wired}
                    <div class="wired-card">
                        <div class="wired-title">
                            <span class="online-dot"></span>
                            <strong>Ethernet</strong>
                            <span>Conectado</span>
                        </div>
                        <div class="wired-name">{wifiStatus.wired.name}</div>
                        <div class="wired-grid">
                            <div>
                                <span>Endereço IP</span>
                                <strong>{wifiStatus.wired.ipv4}</strong>
                            </div>
                            <div>
                                <span>Gateway</span>
                                <strong>{wifiStatus.wired.gateway}</strong>
                            </div>
                        </div>
                    </div>
                {/if}

                <button class="wide-button" onclick={openNetworkSettings}>
                    Abrir conexões de rede
                </button>
            {:else}
                <div class="wifi-switch-row">
                    <span class="wifi-badge" class:is-on={wifiStatus.enabled}>
                        <svg
                            viewBox="0 0 16 16"
                            width="20"
                            height="20"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.45"
                            stroke-linecap="round"
                            aria-hidden="true"
                        >
                            <path d="M2.2 5.6a8.4 8.4 0 0 1 11.6 0" />
                            <path d="M4.6 8.2a5.1 5.1 0 0 1 6.8 0" />
                            <path d="M6.8 10.8a2.3 2.3 0 0 1 2.4 0" />
                            <circle cx="8" cy="13" r="0.9" fill="currentColor" stroke="none" />
                        </svg>
                    </span>
                    <div class="wifi-switch-copy">
                        <strong>Wi-Fi</strong>
                        <span>
                            {wifiStatus.enabled
                                ? wifiStatus.connected
                                    ? `Conectado a ${wifiStatus.ssid}`
                                    : 'Procurando redes'
                                : 'Desligado'}
                        </span>
                    </div>
                    <button
                        class="switch"
                        class:is-on={wifiStatus.enabled}
                        disabled={wifiBusy === 'toggle'}
                        onclick={toggleWifi}
                        aria-label="Ligar ou desligar o Wi-Fi"
                    ></button>
                </div>

                {#if wifiStatus.message}
                    <div class="wifi-hint">{wifiStatus.message}</div>
                {/if}

                {#if !wifiStatus.enabled}
                    <div class="wifi-off">
                        <span class="wifi-off-icon">
                            <svg
                                viewBox="0 0 24 24"
                                width="30"
                                height="30"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.5"
                                stroke-linecap="round"
                                aria-hidden="true"
                            >
                                <path d="M3.4 8.4a13 13 0 0 1 17.2 0" />
                                <path d="M7 12.3a8 8 0 0 1 10 0" />
                                <path d="M10.3 16.2a3.4 3.4 0 0 1 3.4 0" />
                                <circle cx="12" cy="19.4" r="1.3" fill="currentColor" stroke="none" />
                                <path d="M3 3l18 18" />
                            </svg>
                        </span>
                        <strong>Wi-Fi desligado</strong>
                        <p>Ligue o Wi-Fi para ver as redes disponíveis.</p>
                    </div>
                {:else}
                    {#if wifiStatus.connected}
                        <div class="wifi-current">
                            <div class="wifi-current-main">
                                {@render signalBars(
                                    signalLevel(wifiStatus.signal ?? 0),
                                )}
                                <div class="wifi-current-copy">
                                    <strong>{wifiStatus.ssid ?? 'Rede conectada'}</strong>
                                    <span>
                                        {wifiStatus.auth ?? 'Wi-Fi'} · {wifiStatus.signal ??
                                            0}% de sinal
                                    </span>
                                </div>
                                <span class="connected-pill">Conectado</span>
                            </div>
                            <div class="wifi-current-actions">
                                <button
                                    class="ghost-button"
                                    onclick={disconnectWifi}
                                    disabled={wifiBusy === 'disconnect'}
                                >
                                    {wifiBusy === 'disconnect'
                                        ? 'Desconectando…'
                                        : 'Desconectar'}
                                </button>
                                {#if currentWifi?.hasProfile}
                                    <button
                                        class="ghost-button"
                                        class:danger={wifiForgetFor ===
                                            currentWifi.ssid}
                                        onclick={() => armForget(currentWifi)}
                                        disabled={!!wifiBusy}
                                    >
                                        {wifiForgetFor === currentWifi.ssid
                                            ? 'Confirmar?'
                                            : 'Esquecer'}
                                    </button>
                                {/if}
                            </div>
                        </div>
                    {/if}

                    <div class="wifi-list-head">
                        <span>Redes disponíveis</span>
                        {#if wifiLoading}
                            <span class="wifi-scan">Atualizando…</span>
                        {/if}
                    </div>

                    <div class="wifi-list">
                        {#if wifiLoading && wifiStatus.networks.length === 0}
                            <div class="wifi-list-empty">
                                <span class="spinner"></span>
                                <span>Procurando redes…</span>
                            </div>
                        {:else if wifiStatus.networks.length === 0}
                            <div class="wifi-list-empty">
                                Nenhuma rede encontrada.
                            </div>
                        {:else}
                            {#each wifiStatus.networks as network (network.ssid)}
                                <button
                                    class="wifi-row"
                                    class:is-connected={network.connected}
                                    class:is-busy={!!wifiBusy}
                                    onclick={() => connectNetwork(network)}
                                >
                                    {@render signalBars(
                                        signalLevel(network.signal),
                                    )}
                                    <span class="wifi-row-copy">
                                        <strong>{network.ssid}</strong>
                                        <span>
                                            {network.auth}{network.hasProfile
                                                ? ' · Salva'
                                                : ''}
                                        </span>
                                    </span>
                                    {#if wifiBusy ===
                                    `connect:${network.ssid}`}
                                        <span class="row-spinner"></span>
                                    {:else if network.connected}
                                        <span class="check-dot" title="Conectado">
                                            <svg
                                                viewBox="0 0 16 16"
                                                width="11"
                                                height="11"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                                aria-hidden="true"
                                            >
                                                <path d="M3.2 8.3l2.9 2.9 6.7-6.6" />
                                            </svg>
                                        </span>
                                    {:else if network.secured}
                                        <span class="lock-icon" title="Rede protegida">
                                            <svg
                                                viewBox="0 0 16 16"
                                                width="12"
                                                height="12"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="1.4"
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                                aria-hidden="true"
                                            >
                                                <rect x="3.5" y="7" width="9" height="6.5" rx="1.8" />
                                                <path d="M5.5 7V5.3a2.5 2.5 0 0 1 5 0V7" />
                                            </svg>
                                        </span>
                                    {/if}
                                </button>

                                {#if wifiPasswordFor === network.ssid}
                                    <div class="wifi-password">
                                        <div class="password-field">
                                            <input
                                                type={wifiPasswordVisible
                                                    ? 'text'
                                                    : 'password'}
                                                bind:value={wifiPassword}
                                                placeholder={`Senha de ${network.ssid}`}
                                                autocomplete="off"
                                                spellcheck="false"
                                                onkeydown={event => {
                                                    if (event.key === 'Enter') {
                                                        void submitWifiPassword(
                                                            network,
                                                        );
                                                    }
                                                }}
                                            />
                                            <button
                                                class="eye-button"
                                                onclick={() =>
                                                    (wifiPasswordVisible =
                                                        !wifiPasswordVisible)}
                                                title={wifiPasswordVisible
                                                    ? 'Ocultar senha'
                                                    : 'Mostrar senha'}
                                            >
                                                <svg
                                                    viewBox="0 0 16 16"
                                                    width="14"
                                                    height="14"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="1.35"
                                                    stroke-linecap="round"
                                                    stroke-linejoin="round"
                                                    aria-hidden="true"
                                                >
                                                    <path d="M1.8 8s2.2-3.3 6.2-3.3S14.2 8 14.2 8s-2.2 3.3-6.2 3.3S1.8 8 1.8 8z" />
                                                    <circle cx="8" cy="8" r="1.7" />
                                                    {#if !wifiPasswordVisible}
                                                        <path d="M2.4 13.6L13.6 2.4" />
                                                    {/if}
                                                </svg>
                                            </button>
                                        </div>
                                        {#if wifiError}
                                            <div class="wifi-error">
                                                {wifiError}
                                            </div>
                                        {/if}
                                        <div class="password-actions">
                                            <button
                                                class="text-button"
                                                onclick={cancelWifiPassword}
                                            >
                                                Cancelar
                                            </button>
                                            <button
                                                class="primary-button"
                                                onclick={() =>
                                                    void submitWifiPassword(
                                                        network,
                                                    )}
                                                disabled={wifiBusy ===
                                                    `connect:${network.ssid}`}
                                            >
                                                {wifiBusy ===
                                                `connect:${network.ssid}`
                                                    ? 'Conectando…'
                                                    : 'Conectar'}
                                            </button>
                                        </div>
                                    </div>
                                {/if}
                            {/each}
                        {/if}
                    </div>

                    {#if wifiError && !wifiPasswordFor}
                        <div class="wifi-error">{wifiError}</div>
                    {/if}
                {/if}
            {/if}

            <div class="wifi-footer">
                <span>
                    {#if wifiStatus?.adapter}
                        {wifiStatus.connected
                            ? 'Wi-Fi conectado'
                            : wifiStatus.enabled
                              ? 'Wi-Fi ativado'
                              : 'Wi-Fi desligado'}
                    {:else if wifiStatus?.wired}
                        Ethernet ativo
                    {:else}
                        Rede
                    {/if}
                </span>
                <button class="text-button" onclick={openNetworkSettings}>
                    Configurações de rede
                </button>
            </div>
        </div>
    {/if}

    {#if mediaOpen && media && media.hasSession}
        {#if mediaPinned}
            <div
                class="media-backdrop"
                onclick={() => closeMediaPanel()}
                oncontextmenu={event => {
                    event.preventDefault();
                    closeMediaPanel();
                }}
            ></div>
        {/if}

        <div
            class="media-panel"
            bind:this={mediaPanelEl}
            style={`left:${mediaPanelLeft}px; bottom:${menuBottom}px`}
            onpointerenter={onMediaPanelEnter}
            onpointerleave={onMediaPanelLeave}
        >
            {#if mediaArt}
                <div
                    class="media-ambient"
                    style={`background-image:url(${mediaArt})`}
                ></div>
            {/if}

            <div class="media-top">
                <button
                    class="media-cover"
                    class:is-live={mediaPreviewOpen}
                    onclick={toggleMediaPreview}
                    disabled={!media?.hasSession}
                    title={mediaPreviewOpen
                        ? 'Fechar prévia ao vivo'
                        : 'Ver prévia ao vivo'}
                    aria-label={mediaPreviewOpen
                        ? 'Fechar prévia ao vivo'
                        : 'Ver prévia ao vivo'}
                >
                    {#if mediaArt}
                        <img src={mediaArt} alt="" draggable="false" />
                    {:else}
                        <svg
                            viewBox="0 0 24 24"
                            width="34"
                            height="34"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <path d="M9 18V6l10-2v12" />
                            <circle cx="6.5" cy="18" r="2.5" />
                            <circle cx="16.5" cy="16" r="2.5" />
                        </svg>
                    {/if}
                    {#if !mediaPreviewOpen}
                        <span class="media-cover-live" aria-hidden="true">
                            <svg
                                viewBox="0 0 16 16"
                                width="10"
                                height="10"
                                fill="currentColor"
                            >
                                <path d="M5.2 3.4v9.2L12.6 8z" />
                            </svg>
                        </span>
                    {/if}
                </button>

                <div class="media-info">
                    <span class="media-app">
                        {#if mediaSourceIcon}
                            <img src={mediaSourceIcon} alt="" draggable="false" />
                        {/if}
                        {media.app ?? 'Mídia'}
                    </span>
                    <strong>{media.title ?? 'Sem título'}</strong>
                    <span>{media.artist ?? 'Artista desconhecido'}</span>
                    {#if media.album}
                        <span class="media-album">{media.album}</span>
                    {/if}
                </div>

                <button
                    class="icon-button media-hide"
                    onclick={() => closeMediaPanel()}
                    title="Fechar"
                >
                    <svg
                        viewBox="0 0 16 16"
                        width="13"
                        height="13"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.6"
                        stroke-linecap="round"
                        aria-hidden="true"
                    >
                        <path d="M4 4l8 8M12 4l-8 8" />
                    </svg>
                </button>
            </div>

            {#if mediaPreviewOpen}
                <div class="media-live">
                    {#if mediaPreviewFrame}
                        <button
                            class="media-live-frame"
                            onclick={openMediaPreviewWindow}
                            title="Abrir no {media.app ?? 'aplicativo'}"
                        >
                            <img
                                src={mediaPreviewFrame}
                                alt="Prévia ao vivo"
                                draggable="false"
                            />
                            <span class="media-live-badge">
                                <span class="media-live-dot"></span>
                                AO VIVO
                            </span>
                        </button>
                    {:else if mediaPreviewResolving}
                        <div class="media-live-empty">
                            Procurando a janela do vídeo…
                        </div>
                    {:else if mediaPreviewMissing}
                        <div class="media-live-empty">
                            Não encontrei a janela do vídeo.
                        </div>
                    {:else}
                        <div class="media-live-empty">
                            Conectando à janela do vídeo…
                        </div>
                    {/if}
                </div>
            {/if}

            <div class="media-progress">
                <input
                    type="range"
                    min="0"
                    max={mediaDuration || 0}
                    value={Math.round(mediaPosition)}
                    disabled={!mediaDuration}
                    oninput={onMediaSeekInput}
                    onchange={onMediaSeekCommit}
                    style={`--progress:${mediaProgress}%`}
                    aria-label="Posição da música"
                />
                <div class="media-times">
                    <span>{formatTime(mediaPosition)}</span>
                    <span class="media-times-right">
                        <span>{formatTime(mediaDuration)}</span>
                        <button
                            class="media-sync"
                            class:is-busy={mediaSyncing}
                            onclick={() => void syncMediaTimeline(true)}
                            disabled={mediaSyncing}
                            title="Sincronizar tempo com o vídeo"
                            aria-label="Sincronizar tempo com o vídeo"
                        >
                            <svg
                                viewBox="0 0 16 16"
                                width="11"
                                height="11"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.5"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                aria-hidden="true"
                            >
                                <path d="M13.2 6.2A5.4 5.4 0 1 0 13 10" />
                                <path d="M13.4 2.8v3.6H9.8" />
                            </svg>
                        </button>
                    </span>
                </div>
            </div>

            <div class="media-volume" class:is-muted={mediaMuted}>
                <button
                    class="media-volume-button"
                    onclick={toggleMediaMute}
                    disabled={mediaVolume == null}
                    title={mediaVolume == null
                        ? 'Áudio indisponível'
                        : mediaMuted
                          ? 'Ativar som'
                          : 'Silenciar'}
                    aria-label={mediaMuted ? 'Ativar som' : 'Silenciar'}
                >
                    <svg
                        viewBox="0 0 24 24"
                        width="17"
                        height="17"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.7"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path
                            d="M4 9.5h3.2L12 5.6v12.8l-4.8-3.9H4z"
                            fill="currentColor"
                            stroke="none"
                        />
                        {#if mediaMuted}
                            <path d="M15.8 9.4l4.4 5.2M20.2 9.4l-4.4 5.2" />
                        {:else}
                            <path d="M15.2 9.3a4.1 4.1 0 0 1 0 5.4" />
                            <path d="M17.8 6.9a7.7 7.7 0 0 1 0 10.2" />
                        {/if}
                    </svg>
                </button>

                <input
                    type="range"
                    min="0"
                    max="100"
                    step="1"
                    value={Math.round((mediaVolume ?? 0) * 100)}
                    disabled={mediaVolume == null}
                    oninput={onMediaVolumeInput}
                    onchange={onMediaVolumeCommit}
                    style={`--volume:${(mediaVolume ?? 0) * 100}%`}
                    aria-label="Volume do aplicativo"
                />

                <span class="media-volume-value">
                    {mediaVolume == null ? '—' : Math.round(mediaVolume * 100)}
                </span>
            </div>

            <div class="media-controls">
                <button
                    class="media-control"
                    onclick={() => mediaAction('previous')}
                    disabled={!media.canPrevious || !!mediaBusy}
                    title="Faixa anterior"
                >
                    <svg
                        viewBox="0 0 24 24"
                        width="20"
                        height="20"
                        fill="currentColor"
                        aria-hidden="true"
                    >
                        <path d="M6 5a1 1 0 0 1 2 0v14a1 1 0 1 1-2 0z" />
                        <path d="M19 5.5v13a1 1 0 0 1-1.5.9l-9-6.5a1 1 0 0 1 0-1.6l9-6.5a1 1 0 0 1 1.5.7z" />
                    </svg>
                </button>

                <button
                    class="media-control media-skip"
                    onclick={() => void seekRelative(-10000)}
                    disabled={!mediaDuration}
                    title="Voltar 10 segundos"
                >
                    -10
                </button>

                <button
                    class="media-control primary"
                    onclick={() => mediaAction('playPause')}
                    disabled={!media.canPlayPause || !!mediaBusy}
                    title={media.playing ? 'Pausar' : 'Reproduzir'}
                >
                    {#if media.playing}
                        <svg
                            viewBox="0 0 24 24"
                            width="22"
                            height="22"
                            fill="currentColor"
                            aria-hidden="true"
                        >
                            <rect x="6" y="4.5" width="4.2" height="15" rx="1.6" />
                            <rect x="13.8" y="4.5" width="4.2" height="15" rx="1.6" />
                        </svg>
                    {:else}
                        <svg
                            viewBox="0 0 24 24"
                            width="22"
                            height="22"
                            fill="currentColor"
                            aria-hidden="true"
                        >
                            <path d="M8 4.6a1 1 0 0 1 1.5-.9l11 7.4a1 1 0 0 1 0 1.8l-11 7.4A1 1 0 0 1 8 19.4z" />
                        </svg>
                    {/if}
                </button>

                <button
                    class="media-control media-skip"
                    onclick={() => void seekRelative(10000)}
                    disabled={!mediaDuration}
                    title="Avançar 10 segundos"
                >
                    +10
                </button>

                <button
                    class="media-control"
                    onclick={() => mediaAction('next')}
                    disabled={!media.canNext || !!mediaBusy}
                    title="Próxima faixa"
                >
                    <svg
                        viewBox="0 0 24 24"
                        width="20"
                        height="20"
                        fill="currentColor"
                        aria-hidden="true"
                    >
                        <path d="M16 5a1 1 0 0 1 2 0v14a1 1 0 1 1-2 0z" />
                        <path d="M5 5.5v13a1 1 0 0 0 1.5.9l9-6.5a1 1 0 0 0 0-1.6l-9-6.5A1 1 0 0 0 5 5.5z" />
                    </svg>
                </button>
            </div>
        </div>
    {/if}

    {#if bgPanelOpen}
        <div
            class="bg-backdrop"
            onclick={closeBackgroundPanel}
            oncontextmenu={event => {
                event.preventDefault();
                closeBackgroundPanel();
            }}
        ></div>

        <div
            class="bg-panel"
            bind:this={bgPanelEl}
            style={`left:${bgPanelLeft}px; bottom:${menuBottom}px`}
            onpointerenter={clearHide}
            onpointerleave={() => scheduleHide()}
        >
            <div class="bg-header">
                <div class="bg-header-copy">
                    <h3>Segundo plano</h3>
                    <span>
                        {bgApps.length}
                        {bgApps.length === 1 ? 'programa' : 'programas'}
                        minimizados ou sem janela
                    </span>
                </div>
                <div class="bg-header-actions">
                    <button
                        class="icon-button"
                        class:spinning={bgLoading}
                        onclick={() => void loadBackgroundApps()}
                        title="Atualizar"
                    >
                        <svg
                            viewBox="0 0 16 16"
                            width="14"
                            height="14"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <path d="M13.2 8a5.2 5.2 0 1 1-1.6-3.7" />
                            <path d="M13.5 2.8v3.4h-3.4" />
                        </svg>
                    </button>
                    <button
                        class="icon-button"
                        onclick={closeBackgroundPanel}
                        title="Fechar"
                    >
                        <svg
                            viewBox="0 0 16 16"
                            width="13"
                            height="13"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.6"
                            stroke-linecap="round"
                            aria-hidden="true"
                        >
                            <path d="M4 4l8 8M12 4l-8 8" />
                        </svg>
                    </button>
                </div>
            </div>

            {#if bgLoading && bgApps.length === 0}
                <div class="bg-loading">
                    <span class="spinner"></span>
                    <span>Procurando programas…</span>
                </div>
            {:else if bgApps.length === 0}
                <div class="bg-empty">
                    <span class="bg-empty-icon">
                        <svg
                            viewBox="0 0 24 24"
                            width="30"
                            height="30"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.4"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <path d="M12 3.5l8 4-8 4-8-4z" />
                            <path d="M4 11.5l8 4 8-4" />
                            <path d="M4 15.5l8 4 8-4" />
                        </svg>
                    </span>
                    <strong>Nada em segundo plano</strong>
                    <p>
                        Apps minimizados ou escondidos na bandeja aparecem
                        aqui.
                    </p>
                </div>
            {:else}
                <div class="bg-list">
                    {#each bgApps as app (app.key)}
                        <div
                            class="bg-row"
                            class:is-busy={bgBusy === app.key ||
                                bgBusy === `kill:${app.key}`}
                        >
                            <span class="bg-app-icon">
                                {#if bgIcons[app.key]}
                                    <img
                                        src={bgIcons[app.key]}
                                        alt=""
                                        draggable="false"
                                    />
                                {:else}
                                    <span class="letter"
                                        >{initial(app.name)}</span
                                    >
                                {/if}
                            </span>

                            <div class="bg-app-copy">
                                <strong>{app.name}</strong>
                                <span>
                                    {backgroundStateLabel(app.state)}
                                    {#if app.processes > 1}
                                        · {app.processes} processos
                                    {/if}
                                    {#if app.windows > 1}
                                        · {app.windows} janelas
                                    {/if}
                                    · {formatMemory(app.memory)}
                                </span>
                                {#if app.title}
                                    <span class="bg-app-title">{app.title}</span>
                                {/if}
                            </div>

                            <div class="bg-actions">
                                <button
                                    class="bg-open"
                                    onclick={() => openBackgroundApp(app)}
                                    disabled={!!bgBusy}
                                    title="Abrir"
                                >
                                    <svg
                                        viewBox="0 0 16 16"
                                        width="11"
                                        height="11"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="1.6"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        aria-hidden="true"
                                    >
                                        <path d="M5 11l6-6" />
                                        <path d="M6 5h5v5" />
                                    </svg>
                                    Abrir
                                </button>
                                <button
                                    class="bg-kill"
                                    class:confirm={bgConfirmPid === app.pid}
                                    onclick={() =>
                                        armBackgroundTerminate(app)}
                                    disabled={!!bgBusy}
                                    title="Encerrar"
                                >
                                    {#if bgConfirmPid === app.pid}
                                        Confirmar?
                                    {:else}
                                        <svg
                                            viewBox="0 0 16 16"
                                            width="12"
                                            height="12"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="1.55"
                                            stroke-linecap="round"
                                            aria-hidden="true"
                                        >
                                            <path d="M8 2.5v5.5" />
                                            <path d="M5.1 4.7a4.4 4.4 0 1 0 5.8 0" />
                                        </svg>
                                    {/if}
                                </button>
                            </div>
                        </div>
                    {/each}
                </div>
            {/if}

            {#if bgError}
                <div class="bg-error">{bgError}</div>
            {/if}

            <div class="bg-footer">
                <span>{bgApps.length} em segundo plano</span>
                <button class="text-button" onclick={openTaskManager}>
                    Gerenciador de tarefas
                </button>
            </div>
        </div>
    {/if}

    {#if menu}
        <div
            class="menu-backdrop"
            onclick={closeMenu}
            oncontextmenu={event => {
                event.preventDefault();
                closeMenu();
            }}
        ></div>

        <div
            class="dock-menu"
            bind:this={menuEl}
            style={`left:${menuLeft}px; bottom:${menuBottom}px`}
            onpointerenter={clearHide}
            onpointerleave={() => scheduleHide()}
        >
            {#if menu.kind === 'app'}
                {@const app = menu.app}
                <div class="menu-title">{app.name}</div>
                <button
                    class="menu-item"
                    onclick={() => {
                        closeMenu();
                        void activateApp(app);
                    }}
                >
                    Abrir
                </button>
                <button
                    class="menu-item"
                    onclick={() => {
                        closeMenu();
                        void activateApp(app, true);
                    }}
                >
                    Nova janela
                </button>
                <button class="menu-item" onclick={() => togglePin(app)}>
                    {menu.pinned ? 'Desafixar da dock' : 'Fixar na dock'}
                </button>

                {#if windowsFor(app).length > 0}
                    <div class="menu-separator"></div>
                    <div class="menu-label">Janelas</div>
                    {#each windowsFor(app).slice(0, 5) as window}
                        <button
                            class="menu-item window-item"
                            onclick={() => focusWindow(window.hwnd)}
                        >
                            {window.title}
                        </button>
                    {/each}
                    <button
                        class="menu-item destructive"
                        onclick={() => closeAppWindows(app)}
                    >
                        Fechar janelas
                    </button>
                {/if}
            {:else if menu.kind === 'dynamic'}
                {@const dynamicWindows = menu.windows}
                <div class="menu-title">{menu.name}</div>
                <div class="menu-label">Janelas</div>
                {#each dynamicWindows.slice(0, 8) as window}
                    <button
                        class="menu-item window-item"
                        onclick={() => focusWindow(window.hwnd)}
                    >
                        {window.title}
                    </button>
                {/each}
                <div class="menu-separator"></div>
                <button
                    class="menu-item destructive"
                    onclick={() => closeDynamicWindows(dynamicWindows)}
                >
                    Fechar janelas
                </button>
            {:else if menu.kind === 'workspace'}
                <div class="menu-title">Áreas de trabalho</div>
                <button class="menu-item" onclick={openTaskView}>
                    Visão de tarefas
                </button>
                <button class="menu-item" onclick={newVirtualDesktop}>
                    Nova área de trabalho
                </button>
                <button
                    class="menu-item"
                    onclick={() => switchVirtualDesktop('left')}
                >
                    Área anterior
                </button>
                <button
                    class="menu-item"
                    onclick={() => switchVirtualDesktop('right')}
                >
                    Próxima área
                </button>
                <div class="menu-separator"></div>
                <button
                    class="menu-item"
                    onclick={() => {
                        closeMenu();
                        openLauncher();
                    }}
                >
                    Pesquisar aplicativos
                </button>
                <div class="menu-separator"></div>
                <button class="menu-item destructive" onclick={closeVirtualDesktop}>
                    Fechar área atual
                </button>
            {:else if menu.kind === 'folder'}
                <div class="menu-title">{menu.name}</div>
                <button
                    class="menu-item"
                    onclick={() => openMenuFolder()}
                >
                    Abrir
                </button>
                <button
                    class="menu-item"
                    onclick={() => openMenuFolder(true)}
                >
                    Abrir em nova janela
                </button>
                <div class="menu-separator"></div>
                <button
                    class="menu-item destructive"
                    onclick={() => removeMenuFolder()}
                >
                    Remover da dock
                </button>
            {:else if menu.kind === 'trash'}
                <div class="menu-title">Lixeira</div>
                <button class="menu-item" onclick={openTrash}>Abrir</button>
                <button class="menu-item" onclick={emptyTrash}>
                    Esvaziar lixeira
                </button>
            {:else}
                <div class="menu-title">Dhruva</div>
                <button class="menu-item" onclick={openSettings}>
                    Configurações
                </button>
                <button class="menu-item" onclick={addDockFolder}>
                    Adicionar pasta à dock…
                </button>
                {#if settings.hideTaskbar}
                    <button class="menu-item" onclick={restoreTaskbar}>
                        Restaurar barras de tarefas
                    </button>
                {/if}
                <div class="menu-separator"></div>
                <div class="menu-label">Atalho de emergência</div>
                <div class="menu-hint">Ctrl + Alt + Shift + D</div>
                <div class="menu-separator"></div>
                <button class="menu-item destructive" onclick={quit}>
                    Sair da Dhruva
                </button>
                <div class="menu-version">versão {version}</div>
            {/if}
        </div>
    {/if}

    <div
        class="reveal-strip"
        onpointerenter={onRevealStripEnter}
        onpointerleave={onRevealStripLeave}
    ></div>
</div>

<style>
    .dock-stage {
        position: fixed;
        inset: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: flex-end;
        padding-bottom: var(--margin, 6px);
        overflow: hidden;
        pointer-events: none;
        user-select: none;
        -webkit-user-select: none;
    }

    .dock-bar {
        position: relative;
        display: flex;
        width: fit-content;
        max-width: calc(100vw - 8px);
        height: var(--dock-h);
        align-items: center;
        gap: 10px;
        padding: 0 var(--pad-x, 12px);
        border-radius: var(--dock-radius, 14px);
        pointer-events: auto;
        transition: transform var(--anim-duration, 260ms)
            cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    .dock-stage.is-hidden .dock-bar {
        transform: translateY(calc(100% + var(--margin, 6px) + 6px));
    }

    .dock-bar.is-zooming .dock-item img {
        filter: drop-shadow(0 7px 12px rgba(0, 0, 0, 0.45));
    }

    .dock-stage.is-hidden .tooltip {
        opacity: 0;
    }

    .dock-bg {
        position: absolute;
        top: 0;
        right: 0;
        bottom: 0;
        left: 0;
        z-index: 0;
        overflow: hidden;
        border-radius: inherit;
        background:
            linear-gradient(
                180deg,
                var(--dock-bg-top),
                rgba(255, 255, 255, 0) 44%
            ),
            var(--dock-bg);
        border: 1px solid var(--dock-border);
        box-shadow:
            0 12px 34px rgba(0, 0, 0, 0.42),
            0 2px 8px rgba(0, 0, 0, 0.24),
            inset 0 1px 0 var(--dock-inner);
        backdrop-filter: blur(30px) saturate(150%);
        -webkit-backdrop-filter: blur(30px) saturate(150%);
        pointer-events: none;
    }

    .dock-bg::before {
        content: '';
        position: absolute;
        top: 0;
        left: 8%;
        right: 8%;
        height: 1px;
        background: linear-gradient(
            90deg,
            transparent,
            rgba(255, 255, 255, 0.34),
            transparent
        );
        opacity: 0.4;
    }

    .dock-bg::after {
        display: none;
    }

    .dock-left {
        position: relative;
        z-index: 2;
        display: flex;
        height: 100%;
        flex: 0 0 auto;
        align-items: center;
        gap: 12px;
        min-width: 0;
    }

    .launcher-btn {
        position: relative;
        flex: 0 0 auto;
        width: 38px;
        height: 30px;
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 1px solid transparent;
        border-radius: 8px;
        background: transparent;
        color: rgba(255, 255, 255, 0.88);
        cursor: default;
    }

    .launcher-btn:hover {
        background: var(--hover);
        border-color: var(--panel-border);
    }

    .workspace-btn {
        width: auto;
        min-width: 38px;
        padding: 0 6px;
        border-color: transparent;
        background: transparent;
        box-shadow: none;
    }

    .workspace-btn:hover {
        border-color: var(--panel-border);
        background: var(--hover);
    }

    .workspace-line {
        display: flex;
        align-items: center;
        gap: 4px;
        animation: none;
    }

    .workspace-line i {
        display: block;
        width: 4px;
        height: 4px;
        border-radius: 50%;
        background: rgba(255, 255, 255, 0.5);
        transition:
            width 220ms cubic-bezier(0.2, 0.9, 0.25, 1),
            height 220ms cubic-bezier(0.2, 0.9, 0.25, 1),
            border-radius 220ms ease,
            background 160ms ease;
    }

    .workspace-line i.is-current {
        width: 15px;
        height: 2.5px;
        border-radius: 2px;
        background: rgba(255, 255, 255, 0.92);
    }

    .launcher-btn:hover .workspace-line i {
        background: rgba(255, 255, 255, 0.95);
    }

    .workspace-line.pulse-left {
        animation: workspace-pulse-left calc(200ms * var(--anim-speed, 1))
            ease;
    }

    .workspace-line.pulse-right {
        animation: workspace-pulse-right calc(200ms * var(--anim-speed, 1))
            ease;
    }

    @keyframes workspace-pulse-left {
        35% {
            transform: translateX(-4px);
        }
    }

    @keyframes workspace-pulse-right {
        35% {
            transform: translateX(4px);
        }
    }

    .dock-row {
        position: relative;
        z-index: 2;
        display: flex;
        height: calc(var(--icon) + 6px);
        align-items: flex-end;
        gap: var(--gap);
        overflow: visible;
        flex: 0 1 auto;
    }

    .dock-right {
        position: relative;
        z-index: 4;
        display: flex;
        height: 100%;
        flex: 0 0 auto;
        align-items: center;
        gap: 2px;
        margin-left: 2px;
        padding-left: 20px;
    }

    .dock-divider {
        width: 1px;
        height: 20px;
        margin-right: 2px;
        background: var(--sep);
    }

    .clock-divider {
        height: 24px;
        margin: 0 5px 0 7px;
    }

    .status-btn {
        display: flex;
        width: 25px;
        height: 25px;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--text);
        opacity: 0.68;
        cursor: default;
    }

    .status-btn:hover {
        background: var(--hover);
        opacity: 1;
    }

    .status-btn.battery {
        width: auto;
        padding: 0 3px;
        gap: 4px;
        font-size: 10.5px;
        opacity: 0.8;
        font-variant-numeric: tabular-nums;
    }

    .battery-bolt {
        font-size: 9px;
        color: var(--text-dim);
    }

    .status-btn.network {
        position: relative;
    }

    .status-btn.network.is-active {
        background: var(--hover);
        opacity: 1;
    }

    .status-btn.network.is-connected {
        opacity: 0.95;
    }

    .status-btn.network.is-muted {
        opacity: 0.55;
    }

    .status-btn.sound {
        position: relative;
    }

    .status-btn.sound.is-active {
        background: var(--hover);
        opacity: 1;
    }

    .status-btn.sound.is-muted {
        opacity: 0.55;
    }

    .status-online-dot {
        position: absolute;
        right: 4px;
        bottom: 4px;
        width: 4px;
        height: 4px;
        border-radius: 50%;
        background: #34c759;
        box-shadow: 0 0 0 1.5px var(--dock-bg);
    }

    .wifi-backdrop {
        position: fixed;
        inset: 0;
        z-index: 30;
        pointer-events: auto;
    }

    .wifi-panel {
        position: fixed;
        z-index: 31;
        width: 384px;
        max-height: min(548px, calc(100vh - 84px));
        display: flex;
        flex-direction: column;
        padding: 15px 16px 12px;
        transform: translateX(-50%);
        transform-origin: bottom center;
        border-radius: 20px;
        background: var(--menu-bg);
        border: 1px solid var(--glass-border);
        box-shadow: var(--menu-shadow), inset 0 1px 0 var(--glass-inner);
        backdrop-filter: blur(36px) saturate(180%);
        -webkit-backdrop-filter: blur(36px) saturate(180%);
        color: var(--text);
        pointer-events: auto;
        animation: wifi-panel-in var(--anim-duration, 190ms)
            cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    @keyframes wifi-panel-in {
        from {
            opacity: 0;
            transform: translateX(-50%) translateY(7px) scale(0.985);
        }
    }

    .volume-panel {
        position: fixed;
        z-index: 31;
        display: flex;
        width: 340px;
        flex-direction: column;
        padding: 15px 16px 10px;
        transform: translateX(-50%);
        transform-origin: bottom center;
        border: 1px solid var(--glass-border);
        border-radius: 20px;
        background: var(--menu-bg);
        box-shadow: var(--menu-shadow), inset 0 1px 0 var(--glass-inner);
        backdrop-filter: blur(36px) saturate(180%);
        -webkit-backdrop-filter: blur(36px) saturate(180%);
        color: var(--text);
        pointer-events: auto;
        animation: volume-panel-in var(--anim-duration, 190ms)
            cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    @keyframes volume-panel-in {
        from {
            opacity: 0;
            transform: translateX(-50%) translateY(7px) scale(0.985);
        }
    }

    .volume-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 10px;
        padding: 2px 2px 8px;
    }

    .volume-header-copy h3 {
        margin: 0;
        font-size: 15px;
        font-weight: 650;
        letter-spacing: -0.01em;
    }

    .volume-header-copy span {
        display: block;
        margin-top: 2px;
        font-size: 10.5px;
        color: var(--text-dim);
    }

    .volume-header-actions {
        display: flex;
        gap: 3px;
    }

    .volume-control {
        display: flex;
        align-items: center;
        gap: 11px;
        padding: 4px 2px 13px;
    }

    .volume-mute {
        display: flex;
        width: 32px;
        height: 32px;
        flex: none;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: var(--panel);
        color: var(--text);
        cursor: default;
        transition:
            background var(--anim-duration, 140ms) ease,
            color var(--anim-duration, 140ms) ease;
    }

    .volume-mute:not(:disabled):hover {
        background: var(--hover);
    }

    .volume-mute:disabled {
        cursor: default;
        opacity: 0.45;
    }

    .volume-control.is-muted .volume-mute {
        color: var(--text-dim);
        background: transparent;
    }

    .volume-control input[type='range'] {
        width: 100%;
        height: 4px;
        flex: 1;
        margin: 0;
        appearance: none;
        -webkit-appearance: none;
        border-radius: 999px;
        background: linear-gradient(
            to right,
            var(--accent) var(--volume, 0%),
            var(--sep) var(--volume, 0%)
        );
        outline: none;
        cursor: default;
    }

    .volume-control input[type='range']::-webkit-slider-thumb {
        width: 12px;
        height: 12px;
        appearance: none;
        -webkit-appearance: none;
        border: 0;
        border-radius: 50%;
        background: #fff;
        box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
    }

    .volume-control input[type='range']:disabled {
        opacity: 0.4;
    }

    .volume-control.is-muted input[type='range'] {
        background: linear-gradient(
            to right,
            var(--text-dim) var(--volume, 0%),
            var(--sep) var(--volume, 0%)
        );
    }

    .volume-value {
        width: 34px;
        flex: none;
        color: var(--text-dim);
        font-size: 10px;
        font-variant-numeric: tabular-nums;
        text-align: right;
    }

    .volume-error {
        margin: -5px 2px 9px;
        color: var(--danger);
        font-size: 10.5px;
        line-height: 1.35;
    }

    .volume-footer {
        display: flex;
        flex-direction: column;
        gap: 1px;
        padding-top: 8px;
        border-top: 1px solid var(--sep);
    }

    .volume-link {
        display: flex;
        width: 100%;
        align-items: center;
        gap: 9px;
        padding: 7px 8px;
        border: 0;
        border-radius: 9px;
        background: transparent;
        color: var(--text);
        font: inherit;
        font-size: 11px;
        text-align: left;
        cursor: default;
    }

    .volume-link:hover {
        background: var(--hover);
    }

    .volume-link > svg:first-child {
        flex: none;
        color: var(--text-dim);
    }

    .volume-chevron {
        flex: none;
        margin-left: auto;
        color: var(--text-dim);
    }

    .wifi-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 10px;
        padding: 2px 2px 12px;
    }

    .wifi-header-copy h3 {
        margin: 0;
        font-size: 15px;
        font-weight: 650;
        letter-spacing: -0.01em;
    }

    .wifi-header-copy span {
        display: block;
        margin-top: 2px;
        font-size: 10.5px;
        color: var(--text-dim);
    }

    .wifi-header-actions {
        display: flex;
        gap: 3px;
    }

    .icon-button {
        display: flex;
        width: 26px;
        height: 26px;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: var(--text-dim);
        cursor: default;
    }

    .icon-button:hover {
        background: var(--hover);
        color: var(--text);
    }

    .icon-button.spinning svg {
        animation: wifi-spin 0.9s linear infinite;
    }

    .wifi-loading,
    .wifi-list-empty {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 9px;
        padding: 30px 10px;
        color: var(--text-dim);
        font-size: 11px;
    }

    .spinner,
    .row-spinner {
        display: inline-block;
        border: 2px solid var(--sep);
        border-top-color: var(--accent);
        border-radius: 50%;
        animation: wifi-spin 0.8s linear infinite;
        flex: none;
    }

    .spinner {
        width: 15px;
        height: 15px;
    }

    .row-spinner {
        width: 14px;
        height: 14px;
    }

    @keyframes wifi-spin {
        to {
            transform: rotate(360deg);
        }
    }

    .wifi-switch-row {
        display: flex;
        align-items: center;
        gap: 11px;
        padding: 8px 2px 12px;
    }

    .wifi-badge {
        display: flex;
        width: 36px;
        height: 36px;
        flex: none;
        align-items: center;
        justify-content: center;
        border: 1px solid var(--panel-border);
        border-radius: 11px;
        background: var(--panel);
        color: var(--text-dim);
        transition: background 180ms ease, color 180ms ease;
    }

    .wifi-badge.is-on {
        border-color: transparent;
        background: linear-gradient(160deg, #3ea1ff, #1b6fe0);
        color: #fff;
        box-shadow: 0 5px 14px rgba(38, 125, 255, 0.28);
    }

    .wifi-switch-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 2px;
    }

    .wifi-switch-copy strong {
        font-size: 13px;
        font-weight: 600;
    }

    .wifi-switch-copy span {
        overflow: hidden;
        color: var(--text-dim);
        font-size: 10.5px;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .switch {
        position: relative;
        width: 38px;
        height: 22px;
        flex: none;
        padding: 0;
        border: 1px solid var(--panel-border);
        border-radius: 999px;
        background: var(--panel);
        cursor: default;
        transition: background 160ms ease, border-color 160ms ease;
    }

    .switch::after {
        content: '';
        position: absolute;
        top: 2px;
        left: 2px;
        width: 16px;
        height: 16px;
        border-radius: 50%;
        background: #fff;
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
        transition: transform 180ms cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    .switch.is-on {
        border-color: transparent;
        background: var(--accent);
    }

    .switch.is-on::after {
        transform: translateX(16px);
    }

    .switch:disabled {
        opacity: 0.55;
    }

    .wifi-hint {
        padding: 0 2px 9px;
        color: #f0b45b;
        font-size: 10.5px;
        line-height: 1.35;
    }

    .wifi-current {
        margin: 0 0 12px;
        padding: 11px 12px;
        border: 1px solid var(--panel-border);
        border-radius: 14px;
        background: var(--panel);
    }

    .wifi-current-main {
        display: flex;
        align-items: center;
        gap: 10px;
    }

    .wifi-current-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 2px;
    }

    .wifi-current-copy strong {
        overflow: hidden;
        font-size: 12.5px;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .wifi-current-copy span {
        color: var(--text-dim);
        font-size: 10px;
    }

    .connected-pill {
        flex: none;
        padding: 3px 7px;
        border: 1px solid rgba(52, 199, 89, 0.22);
        border-radius: 999px;
        background: rgba(52, 199, 89, 0.12);
        color: #45d478;
        font-size: 9.5px;
        font-weight: 600;
        white-space: nowrap;
    }

    .wifi-current-actions {
        display: flex;
        gap: 6px;
        margin-top: 10px;
    }

    .ghost-button {
        flex: 1;
        padding: 6px 8px;
        border: 1px solid var(--panel-border);
        border-radius: 9px;
        background: transparent;
        color: var(--text);
        font-size: 10.5px;
        cursor: default;
    }

    .ghost-button:hover {
        background: var(--hover);
    }

    .ghost-button.danger {
        border-color: rgba(255, 107, 99, 0.28);
        color: var(--danger);
    }

    .ghost-button:disabled {
        opacity: 0.45;
    }

    .wifi-list-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0 2px 6px;
        color: var(--text-dim);
        font-size: 10px;
        letter-spacing: 0.04em;
        text-transform: uppercase;
    }

    .wifi-scan {
        font-size: 9.5px;
        letter-spacing: 0;
        text-transform: none;
    }

    .wifi-list {
        min-height: 70px;
        flex: 1 1 auto;
        margin: 0 -6px;
        padding: 0 6px 2px;
        overflow-y: auto;
        overscroll-behavior: contain;
        max-height: 252px;
    }

    .wifi-row {
        display: flex;
        width: 100%;
        align-items: center;
        gap: 10px;
        padding: 8px 9px;
        border: 0;
        border-radius: 10px;
        background: transparent;
        color: inherit;
        text-align: left;
        cursor: default;
        transition: background 120ms ease;
    }

    .wifi-row:hover {
        background: var(--hover);
    }

    .wifi-row.is-connected {
        background: var(--accent-soft);
    }

    .wifi-row.is-busy {
        opacity: 0.72;
    }

    .wifi-row-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 1px;
    }

    .wifi-row-copy strong {
        overflow: hidden;
        font-size: 12.5px;
        font-weight: 550;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .wifi-row-copy span {
        overflow: hidden;
        color: var(--text-dim);
        font-size: 10px;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .signal-bars {
        display: inline-flex;
        width: 17px;
        height: 14px;
        flex: none;
        align-items: flex-end;
        gap: 2px;
        color: var(--text);
    }

    .signal-bars i {
        width: 3px;
        border-radius: 2px;
        background: var(--sep);
        transition: background 150ms ease;
    }

    .signal-bars i:nth-child(1) {
        height: 4px;
    }

    .signal-bars i:nth-child(2) {
        height: 7px;
    }

    .signal-bars i:nth-child(3) {
        height: 10px;
    }

    .signal-bars i:nth-child(4) {
        height: 13px;
    }

    .signal-bars[data-level='1'] i:nth-child(1),
    .signal-bars[data-level='2'] i:nth-child(-n + 2),
    .signal-bars[data-level='3'] i:nth-child(-n + 3),
    .signal-bars[data-level='4'] i:nth-child(-n + 4) {
        background: currentColor;
    }

    .check-dot {
        display: flex;
        width: 18px;
        height: 18px;
        flex: none;
        align-items: center;
        justify-content: center;
        border-radius: 50%;
        background: var(--accent-soft);
        color: var(--accent);
    }

    .lock-icon {
        display: flex;
        width: 18px;
        height: 18px;
        flex: none;
        align-items: center;
        justify-content: center;
        color: var(--text-dim);
    }

    .wifi-password {
        display: flex;
        flex-direction: column;
        gap: 8px;
        margin: 2px 4px 8px;
        padding: 9px;
        border: 1px solid var(--panel-border);
        border-radius: 12px;
        background: var(--panel);
    }

    .password-field {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 0 8px;
        border: 1px solid var(--panel-border);
        border-radius: 9px;
        background: var(--panel);
    }

    .password-field input {
        min-width: 0;
        flex: 1;
        padding: 8px 0;
        border: 0;
        outline: 0;
        background: transparent;
        color: var(--text);
        font-size: 12px;
    }

    .password-field input::placeholder {
        color: var(--text-dim);
    }

    .password-field:focus-within {
        border-color: var(--accent);
    }

    .eye-button {
        display: flex;
        width: 24px;
        height: 24px;
        flex: none;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--text-dim);
        cursor: default;
    }

    .eye-button:hover {
        color: var(--text);
    }

    .password-actions {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: 8px;
    }

    .primary-button {
        padding: 6px 14px;
        border: 0;
        border-radius: 8px;
        background: var(--accent);
        color: #fff;
        font-size: 11px;
        font-weight: 600;
        cursor: default;
    }

    .primary-button:hover {
        filter: brightness(1.08);
    }

    .primary-button:disabled {
        opacity: 0.55;
    }

    .text-button {
        padding: 4px 6px;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--text-dim);
        font-size: 10.5px;
        cursor: default;
    }

    .text-button:hover {
        background: var(--hover);
        color: var(--text);
    }

    .wifi-error {
        padding: 1px 2px;
        color: var(--danger);
        font-size: 10.5px;
        line-height: 1.35;
    }

    .wifi-empty,
    .wifi-off {
        display: flex;
        flex-direction: column;
        align-items: center;
        padding: 16px 14px 8px;
        text-align: center;
    }

    .wifi-empty-icon,
    .wifi-off-icon {
        display: flex;
        width: 54px;
        height: 54px;
        align-items: center;
        justify-content: center;
        margin-bottom: 10px;
        border: 1px solid var(--panel-border);
        border-radius: 16px;
        background: var(--panel);
        color: var(--text-dim);
    }

    .wifi-empty strong,
    .wifi-off strong {
        font-size: 13px;
        font-weight: 600;
    }

    .wifi-empty p,
    .wifi-off p {
        max-width: 280px;
        margin: 5px 0 0;
        color: var(--text-dim);
        font-size: 10.5px;
        line-height: 1.4;
    }

    .wired-card {
        margin-top: 4px;
        padding: 12px;
        border: 1px solid var(--panel-border);
        border-radius: 14px;
        background: var(--panel);
    }

    .wired-title {
        display: flex;
        align-items: center;
        gap: 7px;
    }

    .wired-title strong {
        font-size: 12.5px;
        font-weight: 600;
    }

    .wired-title span:last-child {
        margin-left: auto;
        color: #45d478;
        font-size: 9.5px;
        font-weight: 600;
    }

    .online-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: #34c759;
        box-shadow: 0 0 6px rgba(52, 199, 89, 0.55);
    }

    .wired-name {
        margin-top: 5px;
        color: var(--text-dim);
        font-size: 10px;
    }

    .wired-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 10px;
        margin-top: 10px;
    }

    .wired-grid div {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 2px;
    }

    .wired-grid span {
        color: var(--text-dim);
        font-size: 9.5px;
    }

    .wired-grid strong {
        overflow: hidden;
        font-size: 11.5px;
        font-variant-numeric: tabular-nums;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .wide-button {
        width: 100%;
        margin-top: 10px;
        padding: 8px;
        border: 1px solid var(--panel-border);
        border-radius: 10px;
        background: var(--panel);
        color: var(--text);
        font-size: 11px;
        cursor: default;
    }

    .wide-button:hover {
        background: var(--hover);
    }

    .wifi-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-top: auto;
        padding: 10px 2px 0;
        border-top: 1px solid var(--sep);
        color: var(--text-dim);
        font-size: 10px;
    }

    .media-dot {
        position: relative;
        z-index: 3;
        display: flex;
        width: 34px;
        height: 34px;
        flex: 0 0 auto;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: conic-gradient(
            from -90deg,
            var(--accent) calc(var(--progress, 0) * 1%),
            rgba(255, 255, 255, 0.14) 0
        );
        box-shadow:
            0 0 0 1px var(--media-pill-border),
            0 3px 10px rgba(0, 0, 0, 0.3);
        color: var(--text);
        cursor: default;
        transition:
            transform var(--anim-duration, 140ms) ease,
            box-shadow var(--anim-duration, 140ms) ease;
    }

    .media-dot:hover {
        transform: scale(1.06);
        box-shadow:
            0 0 0 1px rgba(147, 176, 255, 0.65),
            0 4px 14px rgba(0, 0, 0, 0.35);
    }

    .media-dot-inner {
        display: flex;
        width: 28px;
        height: 28px;
        align-items: center;
        justify-content: center;
        overflow: hidden;
        border-radius: 50%;
        background:
            linear-gradient(
                180deg,
                rgba(127, 156, 255, 0.14),
                rgba(127, 156, 255, 0.04)
            ),
            var(--media-pill-bg);
        color: rgba(255, 255, 255, 0.85);
        box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.12);
    }

    .media-dot-inner img {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .media-dot.is-playing .media-dot-inner {
        box-shadow:
            inset 0 1px 0 rgba(255, 255, 255, 0.12),
            0 0 0 1px rgba(127, 156, 255, 0.22);
    }

    .media-dot-badge {
        position: absolute;
        right: -2px;
        bottom: -2px;
        display: flex;
        width: 14px;
        height: 14px;
        align-items: center;
        justify-content: center;
        border-radius: 50%;
        background: var(--accent);
        color: #fff;
        box-shadow: 0 0 0 2px var(--dock-bg);
    }

    @keyframes media-spin {
        to {
            transform: rotate(360deg);
        }
    }

    .media-backdrop {
        position: fixed;
        inset: 0;
        z-index: 30;
        pointer-events: auto;
    }

    .media-panel {
        position: fixed;
        z-index: 31;
        display: flex;
        width: 360px;
        flex-direction: column;
        padding: 16px;
        overflow: hidden;
        transform: translateX(-50%);
        transform-origin: bottom center;
        border: 1px solid var(--glass-border);
        border-radius: 20px;
        background: var(--menu-bg);
        box-shadow: var(--menu-shadow), inset 0 1px 0 var(--glass-inner);
        backdrop-filter: blur(36px) saturate(180%);
        -webkit-backdrop-filter: blur(36px) saturate(180%);
        color: var(--text);
        pointer-events: auto;
        animation: media-panel-in var(--anim-duration, 200ms)
            cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    @keyframes media-panel-in {
        from {
            opacity: 0;
            transform: translateX(-50%) translateY(8px) scale(0.985);
        }
    }

    .media-ambient {
        position: absolute;
        inset: -40px;
        z-index: 0;
        background-position: center;
        background-size: cover;
        filter: blur(38px) saturate(1.5);
        opacity: 0.32;
        pointer-events: none;
        mask-image: linear-gradient(to bottom, #000 10%, transparent 88%);
        -webkit-mask-image: linear-gradient(to bottom, #000 10%, transparent 88%);
    }

    .media-panel > :not(.media-ambient) {
        position: relative;
        z-index: 1;
    }

    .media-top {
        display: flex;
        align-items: flex-start;
        gap: 13px;
        padding-bottom: 14px;
    }

    .media-cover {
        position: relative;
        display: flex;
        width: 78px;
        height: 78px;
        flex: none;
        align-items: center;
        justify-content: center;
        overflow: hidden;
        padding: 0;
        border: 0;
        appearance: none;
        border-radius: 14px;
        background: linear-gradient(150deg, #4b5fd6, #232a5c);
        color: rgba(255, 255, 255, 0.85);
        box-shadow: 0 10px 24px rgba(0, 0, 0, 0.3);
        cursor: pointer;
        font: inherit;
        transition:
            transform 140ms ease,
            box-shadow 140ms ease;
    }

    .media-cover:disabled {
        cursor: default;
    }

    .media-cover:not(:disabled):hover {
        transform: translateY(-1px) scale(1.02);
    }

    .media-cover.is-live {
        box-shadow:
            0 0 0 2px var(--accent),
            0 10px 24px rgba(0, 0, 0, 0.3);
    }

    .media-cover-live {
        position: absolute;
        right: 4px;
        bottom: 4px;
        display: flex;
        width: 20px;
        height: 20px;
        align-items: center;
        justify-content: center;
        border-radius: 50%;
        background: rgba(0, 0, 0, 0.62);
        color: #fff;
        box-shadow: 0 1px 4px rgba(0, 0, 0, 0.45);
        pointer-events: none;
    }

    .media-cover img {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .media-live {
        margin-bottom: 12px;
    }

    .media-live-frame {
        position: relative;
        display: block;
        width: 100%;
        aspect-ratio: 16 / 9;
        overflow: hidden;
        padding: 0;
        border: 1px solid var(--glass-border);
        border-radius: 12px;
        background: #000;
        cursor: pointer;
        box-shadow: 0 8px 22px rgba(0, 0, 0, 0.28);
    }

    .media-live-frame img {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .media-live-badge {
        position: absolute;
        top: 7px;
        left: 7px;
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 3px 7px;
        border-radius: 999px;
        background: rgba(0, 0, 0, 0.62);
        color: #fff;
        font-size: 8px;
        font-weight: 700;
        letter-spacing: 0.08em;
        backdrop-filter: blur(4px);
    }

    .media-live-dot {
        width: 5px;
        height: 5px;
        border-radius: 50%;
        background: #ff4d4f;
        box-shadow: 0 0 6px #ff4d4f;
    }

    .media-live-empty {
        display: flex;
        height: 185px;
        align-items: center;
        justify-content: center;
        padding: 12px;
        border: 1px dashed var(--glass-border);
        border-radius: 12px;
        color: var(--text-dim);
        font-size: 11px;
        text-align: center;
    }

    .media-info {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 3px;
        padding-top: 2px;
    }

    .media-app {
        display: flex;
        align-items: center;
        gap: 4px;
        color: var(--accent);
        font-size: 9px;
        font-weight: 700;
        letter-spacing: 0.08em;
        text-transform: uppercase;
    }

    .media-app img {
        width: 12px;
        height: 12px;
        flex: none;
        object-fit: contain;
    }

    .media-info strong {
        overflow: hidden;
        font-size: 15px;
        font-weight: 650;
        letter-spacing: -0.01em;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .media-info > span:not(.media-app) {
        overflow: hidden;
        color: var(--text-dim);
        font-size: 11px;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .media-info .media-album {
        font-size: 10px;
        opacity: 0.75;
    }

    .media-hide {
        flex: none;
        margin-left: auto;
    }

    .media-progress {
        padding: 0 2px;
    }

    .media-progress input[type='range'] {
        width: 100%;
        height: 4px;
        margin: 0;
        appearance: none;
        -webkit-appearance: none;
        border-radius: 999px;
        background: linear-gradient(
            to right,
            var(--accent) var(--progress, 0%),
            var(--sep) var(--progress, 0%)
        );
        outline: none;
        cursor: default;
    }

    .media-progress input[type='range']::-webkit-slider-thumb {
        width: 11px;
        height: 11px;
        appearance: none;
        -webkit-appearance: none;
        border: 0;
        border-radius: 50%;
        background: #fff;
        box-shadow: 0 1px 4px rgba(0, 0, 0, 0.45);
    }

    .media-progress input[type='range']:disabled {
        opacity: 0.5;
    }

    .media-times {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-top: 5px;
        color: var(--text-dim);
        font-size: 9.5px;
        font-variant-numeric: tabular-nums;
    }

    .media-times-right {
        display: flex;
        align-items: center;
        gap: 5px;
    }

    .media-sync {
        display: flex;
        width: 16px;
        height: 16px;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: transparent;
        color: var(--text-dim);
        cursor: pointer;
    }

    .media-sync:hover {
        color: var(--text);
    }

    .media-sync:disabled {
        cursor: default;
        opacity: 0.5;
    }

    .media-sync.is-busy svg {
        animation: media-sync-spin 800ms linear infinite;
    }

    @keyframes media-sync-spin {
        to {
            transform: rotate(360deg);
        }
    }

    .media-volume {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 8px 2px 0;
    }

    .media-volume-button {
        display: flex;
        width: 24px;
        height: 24px;
        flex: none;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: transparent;
        color: var(--text-dim);
        cursor: pointer;
        transition:
            color var(--anim-duration, 140ms) ease,
            background var(--anim-duration, 140ms) ease;
    }

    .media-volume-button:not(:disabled):hover {
        color: var(--text);
        background: rgba(255, 255, 255, 0.08);
    }

    .media-volume-button:disabled {
        cursor: default;
        opacity: 0.45;
    }

    .media-volume input[type='range'] {
        width: 100%;
        height: 3px;
        flex: 1;
        margin: 0;
        appearance: none;
        -webkit-appearance: none;
        border-radius: 999px;
        background: linear-gradient(
            to right,
            var(--accent) var(--volume, 0%),
            var(--sep) var(--volume, 0%)
        );
        outline: none;
        cursor: default;
    }

    .media-volume input[type='range']::-webkit-slider-thumb {
        width: 9px;
        height: 9px;
        appearance: none;
        -webkit-appearance: none;
        border: 0;
        border-radius: 50%;
        background: #fff;
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.45);
    }

    .media-volume input[type='range']:disabled {
        opacity: 0.45;
    }

    .media-volume-value {
        width: 22px;
        flex: none;
        color: var(--text-dim);
        font-size: 9px;
        font-variant-numeric: tabular-nums;
        text-align: right;
    }

    .media-controls {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 18px;
        padding: 16px 0 4px;
    }

    .media-control {
        display: flex;
        width: 38px;
        height: 38px;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: transparent;
        color: var(--text);
        opacity: 0.8;
        cursor: default;
        transition: background 150ms ease, opacity 150ms ease;
    }

    .media-control:hover {
        background: var(--hover);
        opacity: 1;
    }

    .media-control.primary {
        width: 50px;
        height: 50px;
        background: var(--accent);
        color: #fff;
        opacity: 1;
        box-shadow: 0 8px 20px rgba(80, 110, 255, 0.32);
    }

    .media-control.primary:hover {
        filter: brightness(1.08);
    }

    .media-control:disabled {
        background: transparent;
        box-shadow: none;
        opacity: 0.3;
    }

    .media-control.media-skip {
        width: 32px;
        height: 32px;
        font-size: 9.5px;
        font-weight: 650;
        font-variant-numeric: tabular-nums;
        letter-spacing: -0.02em;
    }

    .status-btn.background.is-active {
        background: var(--hover);
        opacity: 1;
    }

    .bg-backdrop {
        position: fixed;
        inset: 0;
        z-index: 30;
        pointer-events: auto;
    }

    .bg-panel {
        position: fixed;
        z-index: 31;
        display: flex;
        width: 400px;
        max-height: min(520px, calc(100vh - 84px));
        flex-direction: column;
        padding: 15px 16px 12px;
        transform: translateX(-50%);
        transform-origin: bottom center;
        border: 1px solid var(--glass-border);
        border-radius: 20px;
        background: var(--menu-bg);
        box-shadow: var(--menu-shadow), inset 0 1px 0 var(--glass-inner);
        backdrop-filter: blur(36px) saturate(180%);
        -webkit-backdrop-filter: blur(36px) saturate(180%);
        color: var(--text);
        pointer-events: auto;
        animation: bg-panel-in var(--anim-duration, 190ms)
            cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    @keyframes bg-panel-in {
        from {
            opacity: 0;
            transform: translateX(-50%) translateY(7px) scale(0.985);
        }
    }

    .bg-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 10px;
        padding: 2px 2px 12px;
    }

    .bg-header-copy h3 {
        margin: 0;
        font-size: 15px;
        font-weight: 650;
        letter-spacing: -0.01em;
    }

    .bg-header-copy span {
        display: block;
        margin-top: 2px;
        color: var(--text-dim);
        font-size: 10.5px;
    }

    .bg-header-actions {
        display: flex;
        gap: 3px;
    }

    .bg-loading {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 9px;
        padding: 30px 10px;
        color: var(--text-dim);
        font-size: 11px;
    }

    .bg-empty {
        display: flex;
        flex-direction: column;
        align-items: center;
        padding: 18px 14px 10px;
        text-align: center;
    }

    .bg-empty-icon {
        display: flex;
        width: 54px;
        height: 54px;
        align-items: center;
        justify-content: center;
        margin-bottom: 10px;
        border: 1px solid var(--panel-border);
        border-radius: 16px;
        background: var(--panel);
        color: var(--text-dim);
    }

    .bg-empty strong {
        font-size: 13px;
        font-weight: 600;
    }

    .bg-empty p {
        max-width: 280px;
        margin: 5px 0 0;
        color: var(--text-dim);
        font-size: 10.5px;
        line-height: 1.4;
    }

    .bg-list {
        min-height: 60px;
        flex: 1 1 auto;
        max-height: 330px;
        margin: 0 -6px;
        padding: 0 6px 2px;
        overflow-y: auto;
        overscroll-behavior: contain;
    }

    .bg-row {
        display: flex;
        align-items: flex-start;
        gap: 10px;
        padding: 8px 9px;
        border: 1px solid transparent;
        border-radius: 12px;
        transition: background 120ms ease;
    }

    .bg-row:hover {
        background: var(--hover);
    }

    .bg-row.is-busy {
        opacity: 0.6;
    }

    .bg-app-icon {
        display: flex;
        width: 32px;
        height: 32px;
        flex: none;
        align-items: center;
        justify-content: center;
        overflow: hidden;
        border: 1px solid var(--panel-border);
        border-radius: 8px;
        background: var(--panel);
    }

    .bg-app-icon img {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: contain;
    }

    .bg-app-icon .letter {
        border-radius: 7px;
        font-size: 13px;
    }

    .bg-app-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 1px;
    }

    .bg-app-copy strong {
        overflow: hidden;
        font-size: 12.5px;
        font-weight: 600;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .bg-app-copy > span {
        overflow: hidden;
        color: var(--text-dim);
        font-size: 9.5px;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .bg-app-copy .bg-app-title {
        font-size: 9px;
        opacity: 0.7;
    }

    .bg-actions {
        display: flex;
        flex: none;
        align-items: center;
        gap: 4px;
    }

    .bg-open {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 5px 8px;
        border: 1px solid var(--panel-border);
        border-radius: 8px;
        background: transparent;
        color: var(--text);
        font-size: 10px;
        cursor: default;
    }

    .bg-open:hover {
        background: var(--hover);
    }

    .bg-open:disabled {
        opacity: 0.5;
    }

    .bg-kill {
        display: flex;
        width: 26px;
        height: 26px;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 7px;
        background: transparent;
        color: var(--text-dim);
        font-size: 9.5px;
        cursor: default;
    }

    .bg-kill:hover {
        background: rgba(255, 107, 99, 0.12);
        color: var(--danger);
    }

    .bg-kill.confirm {
        width: auto;
        padding: 0 7px;
        background: var(--danger);
        color: #fff;
    }

    .bg-kill:disabled {
        opacity: 0.45;
    }

    .bg-error {
        padding: 6px 2px;
        color: var(--danger);
        font-size: 10.5px;
        line-height: 1.35;
    }

    .bg-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-top: auto;
        padding: 10px 2px 0;
        border-top: 1px solid var(--sep);
        color: var(--text-dim);
        font-size: 10px;
    }

    .clock-block {
        display: flex;
        height: 100%;
        min-width: 66px;
        flex-direction: column;
        align-items: flex-end;
        justify-content: center;
        gap: 0;
        padding: 0 6px 0 4px;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: var(--text);
        text-align: right;
        line-height: 1.12;
        cursor: default;
        transition: background 150ms ease;
    }

    .clock-block:hover {
        background: var(--hover);
    }

    .clock-time {
        font-size: 12.5px;
        font-weight: 600;
        letter-spacing: 0.01em;
        color: var(--text);
        font-variant-numeric: tabular-nums;
    }

    .clock-date {
        font-size: 9px;
        letter-spacing: 0.02em;
        color: var(--text-dim);
        font-variant-numeric: tabular-nums;
    }

    .dock-item {
        position: relative;
        flex: 0 0 auto;
        width: var(--icon);
        height: calc(var(--icon) + 6px);
        margin: 0;
        padding: 0;
        border: 0;
        background: none;
        color: inherit;
        cursor: default;
        outline: none;
        -webkit-tap-highlight-color: transparent;
    }

    .dock-item[draggable='true'] {
        cursor: grab;
    }

    .dock-item.is-dragging {
        opacity: 0.35;
    }

    .dock-item.drop-before::after,
    .dock-item.drop-after::after {
        content: '';
        position: absolute;
        top: 4px;
        bottom: 4px;
        z-index: 6;
        width: 2px;
        border-radius: 2px;
        background: var(--accent);
        box-shadow: 0 0 8px var(--accent);
    }

    .dock-item.drop-before::after {
        left: -4px;
    }

    .dock-item.drop-after::after {
        right: -4px;
    }

    .icon-scale {
        position: absolute;
        left: 0;
        top: 0;
        width: var(--icon);
        height: var(--icon);
        transform-origin: 50% 100%;
        will-change: transform;
        display: block;
    }

    .bounce-wrap {
        display: block;
        width: 100%;
        height: 100%;
    }

    .bounce-wrap.is-bouncing {
        animation: dock-bounce calc(1000ms * var(--anim-speed, 1))
            cubic-bezier(0.22, 0.75, 0.3, 1);
    }

    .bounce-wrap.is-pressed {
        animation: dock-press calc(190ms * var(--anim-speed, 1)) ease-out;
    }

    .dock-item img {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: contain;
        filter: drop-shadow(0 3px 4px rgba(0, 0, 0, 0.25));
        -webkit-user-drag: none;
        user-select: none;
    }

    .letter {
        display: flex;
        width: 100%;
        height: 100%;
        align-items: center;
        justify-content: center;
        border-radius: 12px;
        background: linear-gradient(160deg, #6f7bd6, #3f4a9c);
        color: #fff;
        font-size: calc(var(--icon) * 0.42);
        font-weight: 600;
        box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35);
    }

    .folder-tile {
        display: flex;
        width: 100%;
        height: 100%;
        align-items: center;
        justify-content: center;
        border-radius: 11px;
        background: linear-gradient(160deg, #5a8dff, #2f58c8);
        color: #ffe6a3;
        box-shadow:
            inset 0 1px 0 rgba(255, 255, 255, 0.35),
            0 3px 8px rgba(0, 0, 0, 0.2);
    }

    .dots {
        position: absolute;
        left: 50%;
        bottom: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 3px;
        transform: translateX(-50%);
        pointer-events: none;
    }

    .dots .dot {
        display: block;
        width: 4px;
        height: 4px;
        border-radius: 50%;
        background: var(--dot);
        opacity: 0.58;
        box-shadow: 0 0 4px rgba(0, 0, 0, 0.18);
        transition:
            opacity var(--anim-duration, 160ms) ease,
            width var(--anim-duration, 160ms) ease,
            height var(--anim-duration, 160ms) ease,
            background var(--anim-duration, 160ms) ease,
            box-shadow var(--anim-duration, 160ms) ease;
    }

    .dots .dot.focused {
        width: 6px;
        height: 6px;
        opacity: 1;
        background: var(--accent);
        box-shadow: 0 0 6px var(--accent-soft);
    }

    .dock-item.trash::before {
        content: '';
        position: absolute;
        left: calc(var(--gap) / -2 - 0.5px);
        top: 12%;
        width: 1px;
        height: 76%;
        background: var(--sep);
    }

    .trash-badge {
        position: absolute;
        top: 2px;
        right: 3px;
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background: #ff5f57;
        box-shadow: 0 0 0 2px var(--dock-bg);
    }

    .tooltip {
        position: fixed;
        z-index: 20;
        transform: translateX(-50%) translateY(4px);
        padding: 5px 10px;
        border-radius: 8px;
        background: var(--tooltip-bg);
        border: 1px solid var(--glass-border);
        color: var(--text);
        font-size: 12px;
        line-height: 1.2;
        max-width: 240px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        box-shadow: 0 6px 18px rgba(0, 0, 0, 0.25);
        backdrop-filter: blur(18px) saturate(160%);
        -webkit-backdrop-filter: blur(18px) saturate(160%);
        pointer-events: none;
        opacity: 0;
        transition:
            opacity var(--anim-duration, 120ms) ease,
            transform var(--anim-duration, 120ms) ease;
    }

    .tooltip.is-visible {
        opacity: 1;
        transform: translateX(-50%) translateY(0);
    }

    .workspace-panel {
        position: fixed;
        z-index: 26;
        display: flex;
        width: 252px;
        flex-direction: column;
        gap: 5px;
        padding: 7px;
        transform: translateX(-50%);
        border: 1px solid var(--glass-border);
        border-radius: 15px;
        background: var(--menu-bg);
        box-shadow:
            var(--menu-shadow),
            inset 0 1px 0 var(--glass-inner);
        backdrop-filter: blur(36px) saturate(180%);
        -webkit-backdrop-filter: blur(36px) saturate(180%);
        color: var(--text);
        pointer-events: auto;
        animation: preview-in var(--anim-duration, 140ms)
            cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    .workspace-head {
        display: flex;
        align-items: center;
        gap: 7px;
        padding: 4px 6px 5px;
        color: var(--text-dim);
        font-size: 10.5px;
        font-weight: 650;
        letter-spacing: 0.03em;
        text-transform: uppercase;
    }

    .workspace-head-icon {
        display: flex;
        color: var(--accent);
    }

    .workspace-count {
        margin-left: auto;
        font-size: 10px;
        letter-spacing: 0;
        text-transform: none;
    }

    .workspace-list {
        display: flex;
        max-height: min(290px, 42vh);
        flex-direction: column;
        gap: 2px;
        overflow-y: auto;
        overscroll-behavior: contain;
    }

    .workspace-row {
        display: flex;
        width: 100%;
        align-items: center;
        gap: 8px;
        padding: 8px 9px;
        border: 1px solid transparent;
        border-radius: 10px;
        background: transparent;
        color: var(--text);
        text-align: left;
        cursor: default;
    }

    .workspace-row:hover {
        background: var(--hover);
    }

    .workspace-row.is-current {
        background: var(--accent-soft);
    }

    .workspace-row-icon {
        display: flex;
        flex: none;
        color: var(--text-dim);
    }

    .workspace-row.is-current .workspace-row-icon {
        color: var(--accent);
    }

    .workspace-row-name {
        flex: 1;
        overflow: hidden;
        font-size: 12px;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .workspace-row.is-current .workspace-row-name {
        font-weight: 600;
    }

    .workspace-current {
        flex: none;
        padding: 2px 6px;
        border-radius: 999px;
        background: var(--accent);
        color: #fff;
        font-size: 9px;
        font-weight: 700;
    }

    .workspace-footer {
        display: flex;
        gap: 3px;
        padding-top: 5px;
        border-top: 1px solid var(--sep);
    }

    .workspace-action {
        flex: 1;
        padding: 6px 4px;
        border: 0;
        border-radius: 7px;
        background: transparent;
        color: var(--text-dim);
        font-size: 10px;
        cursor: default;
    }

    .workspace-action:hover {
        background: var(--hover);
        color: var(--text);
    }

    .workspace-loading,
    .workspace-error {
        display: flex;
        align-items: center;
        gap: 7px;
        padding: 12px 9px;
        color: var(--text-dim);
        font-size: 10.5px;
    }

    .workspace-error {
        color: var(--danger);
    }

    .workspace-spinner {
        width: 13px;
        height: 13px;
        flex: none;
        border: 1.5px solid var(--sep);
        border-top-color: var(--accent);
        border-radius: 50%;
        animation: media-spin 700ms linear infinite;
    }

    .window-preview {
        position: fixed;
        z-index: 25;
        display: flex;
        max-width: min(720px, 94vw);
        flex-direction: column;
        gap: 8px;
        padding: 10px;
        transform: translateX(-50%);
        border: 1px solid var(--glass-border);
        border-radius: 16px;
        background: var(--menu-bg);
        box-shadow:
            var(--menu-shadow),
            inset 0 1px 0 var(--glass-inner);
        backdrop-filter: blur(36px) saturate(180%);
        -webkit-backdrop-filter: blur(36px) saturate(180%);
        color: var(--text);
        pointer-events: auto;
        animation: preview-in var(--anim-duration, 140ms)
            cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    @keyframes preview-in {
        from {
            opacity: 0;
            transform: translateX(-50%) translateY(7px) scale(0.99);
        }
    }

    .preview-head {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 0 2px;
    }

    .preview-head img {
        width: 18px;
        height: 18px;
        flex: none;
        object-fit: contain;
    }

    .preview-name {
        overflow: hidden;
        font-size: 12px;
        font-weight: 600;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .preview-count {
        flex: none;
        margin-left: auto;
        color: var(--text-dim);
        font-size: 10px;
    }

    .preview-viewport {
        position: relative;
        min-width: 0;
    }

    .preview-row {
        display: flex;
        gap: 8px;
        overflow-x: auto;
        overscroll-behavior-x: contain;
        padding-bottom: 2px;
        scrollbar-width: none;
        -ms-overflow-style: none;
        user-select: none;
    }

    .preview-row::-webkit-scrollbar {
        display: none;
    }

    .preview-row.is-dragging,
    .preview-row.is-dragging .preview-card {
        cursor: grabbing;
    }

    .preview-edge {
        position: absolute;
        top: 0;
        bottom: 2px;
        z-index: 1;
        width: 34px;
        opacity: 0;
        pointer-events: none;
        transition: opacity 140ms ease;
    }

    .preview-edge.is-left {
        left: 0;
        background: linear-gradient(to right, var(--menu-bg) 8%, transparent);
    }

    .preview-edge.is-right {
        right: 0;
        background: linear-gradient(to left, var(--menu-bg) 8%, transparent);
    }

    .preview-edge.is-visible {
        opacity: 1;
    }

    .preview-nav {
        position: absolute;
        top: 50%;
        z-index: 2;
        display: grid;
        width: 26px;
        height: 52px;
        place-items: center;
        padding: 0;
        border: 1px solid var(--glass-border);
        border-radius: 999px;
        background: var(--dock-bg);
        box-shadow:
            0 8px 20px rgba(0, 0, 0, 0.24),
            inset 0 1px 0 var(--glass-inner);
        color: var(--text);
        opacity: 0;
        pointer-events: none;
        transform: translateY(-50%) scale(0.94);
        transition:
            opacity 120ms ease,
            transform 120ms ease,
            border-color 120ms ease,
            color 120ms ease;
    }

    .preview-nav.is-visible {
        opacity: 1;
        pointer-events: auto;
        transform: translateY(-50%) scale(1);
    }

    .preview-nav:hover {
        border-color: var(--accent);
        color: var(--accent);
    }

    .preview-nav.is-left {
        left: -4px;
    }

    .preview-nav.is-right {
        right: -4px;
    }

    .preview-card {
        position: relative;
        display: flex;
        width: 184px;
        flex: 0 0 auto;
        flex-direction: column;
        gap: 5px;
        padding: 5px;
        border: 1px solid transparent;
        border-radius: 10px;
        background: var(--panel);
        color: var(--text);
        text-align: left;
        cursor: default;
        transition:
            background var(--anim-duration, 130ms) ease,
            border-color var(--anim-duration, 130ms) ease,
            box-shadow var(--anim-duration, 130ms) ease;
    }

    .preview-card:hover,
    .preview-card.is-hovered {
        border-color: var(--accent);
        background: var(--hover);
        box-shadow: 0 0 0 1px var(--accent-soft);
    }

    .preview-card:focus-visible {
        outline: 2px solid var(--accent);
        outline-offset: 1px;
    }

    .preview-card:only-child {
        width: 240px;
    }

    .preview-thumb {
        display: flex;
        width: 100%;
        height: 104px;
        align-items: center;
        justify-content: center;
        overflow: hidden;
        border-radius: 7px;
        background: rgba(0, 0, 0, 0.3);
    }

    .preview-card:only-child .preview-thumb {
        height: 132px;
    }

    .preview-thumb img {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: contain;
    }

    .preview-title {
        display: -webkit-box;
        overflow: hidden;
        -webkit-box-orient: vertical;
        -webkit-line-clamp: 2;
        color: var(--text);
        font-size: 10.5px;
        line-height: 1.25;
    }

    .preview-placeholder {
        color: var(--text-dim);
        font-size: 10px;
    }

    .preview-hover-hint {
        position: absolute;
        top: 10px;
        right: 10px;
        padding: 3px 7px;
        border-radius: 999px;
        background: var(--accent);
        color: #fff;
        font-size: 8.5px;
        font-weight: 700;
        letter-spacing: 0.01em;
        opacity: 0;
        transform: translateY(-3px);
        transition:
            opacity 120ms ease,
            transform 120ms ease;
        pointer-events: none;
    }

    .preview-hover-hint.is-visible {
        opacity: 1;
        transform: translateY(0);
    }

    .preview-spinner {
        width: 16px;
        height: 16px;
        border: 1.5px solid rgba(255, 255, 255, 0.18);
        border-top-color: var(--accent);
        border-radius: 50%;
        animation: media-spin 700ms linear infinite;
    }

    .menu-backdrop {
        position: fixed;
        inset: 0;
        z-index: 30;
        pointer-events: auto;
    }

    .dock-menu {
        position: fixed;
        z-index: 31;
        transform: translateX(-50%);
        min-width: 250px;
        max-width: 320px;
        max-height: calc(100vh - 78px);
        overflow-y: auto;
        overscroll-behavior: contain;
        padding: 6px;
        border-radius: 14px;
        background: var(--menu-bg);
        border: 1px solid var(--glass-border);
        box-shadow: var(--menu-shadow), inset 0 1px 0 var(--glass-inner);
        backdrop-filter: blur(32px) saturate(180%);
        -webkit-backdrop-filter: blur(32px) saturate(180%);
        color: var(--text);
        pointer-events: auto;
    }

    .menu-title {
        padding: 7px 10px 5px;
        font-size: 13px;
        font-weight: 600;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .menu-label {
        padding: 5px 10px 2px;
        font-size: 10px;
        letter-spacing: 0.06em;
        text-transform: uppercase;
        color: var(--text-dim);
    }

    .menu-hint {
        padding: 2px 10px 6px;
        font-size: 11px;
        color: var(--text-dim);
    }

    .menu-item {
        display: block;
        width: 100%;
        padding: 7px 10px;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: inherit;
        font-size: 13px;
        text-align: left;
        cursor: default;
    }

    .menu-item:hover {
        background: var(--hover);
    }

    .menu-item.window-item {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--text-dim);
    }

    .menu-item.destructive {
        color: #ff6b63;
    }

    .menu-separator {
        height: 1px;
        margin: 5px 8px;
        background: var(--sep);
    }

    .menu-version {
        padding: 4px 10px 3px;
        font-size: 10px;
        color: var(--text-dim);
        text-align: right;
    }

    .reveal-strip {
        position: fixed;
        left: 0;
        right: 0;
        bottom: 0;
        height: 4px;
        z-index: 40;
        pointer-events: auto;
    }

    @keyframes dock-bounce {
        0%,
        100% {
            transform: translateY(0);
        }
        12% {
            transform: translateY(-26px);
        }
        28% {
            transform: translateY(0);
        }
        44% {
            transform: translateY(-16px);
        }
        60% {
            transform: translateY(0);
        }
        76% {
            transform: translateY(-7px);
        }
        88% {
            transform: translateY(0);
        }
    }

    @keyframes dock-press {
        0% {
            transform: scale(1);
        }
        45% {
            transform: scale(0.86);
        }
        100% {
            transform: scale(1);
        }
    }
</style>
