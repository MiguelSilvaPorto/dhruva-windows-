<script lang="ts">
    import { onMount, tick } from 'svelte';
    import { builtinIcon } from '../lib/builtin-icons';
    import { api, onLauncherOpen, onSettingsChanged } from '../lib/ipc';
    import { applyTheme } from '../lib/theme';
    import type { AppEntry, ResolvedTheme, Settings, WindowInfo } from '../lib/types';

    type LauncherApp = AppEntry & {
        running?: boolean;
        hwnd?: number;
        windowTitle?: string;
        exe?: string;
    };

    let apps = $state<LauncherApp[]>([]);
    let baseApps: LauncherApp[] = [];
    let settings = $state<Settings | null>(null);
    let query = $state('');
    let selected = $state(0);
    let icons = $state<Record<string, string>>({});
    let loading = $state(true);
    let launching = $state(false);
    let error = $state<string | null>(null);
    let inputEl = $state<HTMLInputElement | null>(null);
    let systemTheme: ResolvedTheme = 'dark';
    const requested = new Set<string>();

    const normalize = (value: string) =>
        value
            .normalize('NFD')
            .replace(/[\u0300-\u036f]/g, '')
            .toLowerCase()
            .trim();

    const normalizePath = (value: string) =>
        value.replace(/\//g, '\\').trim().toLowerCase();

    const RUNNING_NAMES: Record<string, string> = {
        'anydesk.exe': 'AnyDesk',
        'claude.exe': 'Claude',
        'ms-teams.exe': 'Microsoft Teams',
        'teams.exe': 'Microsoft Teams',
        'taskmgr.exe': 'Gerenciador de Tarefas',
        'setup_wm.exe': 'Windows Media Player',
        'wmplayer.exe': 'Windows Media Player',
        'mremoteng.exe': 'mRemoteNG',
        'opencode.exe': 'OpenCode',
        'opencode-cli.exe': 'OpenCode CLI',
        'code.exe': 'Visual Studio Code',
        'chrome.exe': 'Google Chrome',
        'msedge.exe': 'Microsoft Edge',
        'firefox.exe': 'Firefox',
        'brave.exe': 'Brave',
        'spotify.exe': 'Spotify',
        'discord.exe': 'Discord',
        'slack.exe': 'Slack',
        'steam.exe': 'Steam',
        'thunderbird.exe': 'Thunderbird',
        'notepad.exe': 'Bloco de Notas',
        'explorer.exe': 'Explorer',
        'windowsterminal.exe': 'Terminal',
        'wt.exe': 'Terminal',
        'rustdesk.exe': 'RustDesk',
        'veracrypt.exe': 'VeraCrypt',
    };

    function runningName(exe: string, title: string): string {
        const base = normalizePath(exe).split('\\').pop() ?? '';
        const known = RUNNING_NAMES[base];
        if (known) return known;

        const shortTitle = title.trim();
        if (
            shortTitle &&
            shortTitle.length <= 30 &&
            !shortTitle.includes(' - ') &&
            !shortTitle.includes(' | ')
        ) {
            return shortTitle;
        }

        const stem = base.replace(/\.exe$/i, '');
        const words = stem.split(/[-_\s]+/).filter(Boolean);
        if (words.length === 0) return title.trim() || 'Aplicativo';
        return words
            .map(word => word.charAt(0).toUpperCase() + word.slice(1))
            .join(' ');
    }

    function mergeRunning(
        base: LauncherApp[],
        windows: WindowInfo[],
    ): LauncherApp[] {
        const catalogByExe = new Map<string, LauncherApp>();
        for (const app of base) {
            const target = normalizePath(app.target);
            const targetBase = target.split('\\').pop() ?? '';
            if (targetBase.endsWith('.exe')) {
                catalogByExe.set(targetBase, app);
            }
        }

        const byApp = new Map<string, WindowInfo[]>();
        const unmatched: WindowInfo[] = [];

        for (const window of windows) {
            const exe = normalizePath(window.exe);
            const baseName = exe.split('\\').pop() ?? '';
            if (!baseName || baseName === 'applicationframehost.exe') continue;

            let app = catalogByExe.get(baseName);
            if (!app) {
                const title = normalize(window.title);
                app = base.find(candidate => {
                    const name = normalize(candidate.name);
                    return (
                        name.length >= 4 &&
                        (title === name ||
                            title.startsWith(`${name} `) ||
                            title.startsWith(`${name} -`))
                    );
                });
            }

            if (app) {
                const group = byApp.get(app.id) ?? [];
                group.push(window);
                byApp.set(app.id, group);
            } else {
                unmatched.push(window);
            }
        }

        const merged = base.map(app => {
            const group = byApp.get(app.id) ?? [];
            if (group.length === 0) return app;
            const active =
                group.find(window => window.focused) ??
                group.find(window => !window.minimized) ??
                group[0];
            return {
                ...app,
                running: true,
                hwnd: active.hwnd,
                windowTitle: active.title,
                exe: active.exe,
            };
        });

        const dynamic = new Map<string, WindowInfo[]>();
        for (const window of unmatched) {
            const exe = normalizePath(window.exe);
            const group = dynamic.get(exe) ?? [];
            group.push(window);
            dynamic.set(exe, group);
        }

        for (const [exe, group] of dynamic) {
            const active =
                group.find(window => window.focused) ??
                group.find(window => !window.minimized) ??
                group[0];
            const baseName = exe.split('\\').pop() ?? '';
            const name = runningName(exe, active.title);
            if (!name || name.length < 2) continue;
            merged.push({
                id: `running:${exe}`,
                name,
                path: '',
                target: active.exe,
                args: '',
                aumid: null,
                iconSource: active.exe,
                builtin: false,
                running: true,
                hwnd: active.hwnd,
                windowTitle: active.title,
                exe: active.exe,
            });
        }

        return merged;
    }

    async function refreshRunning() {
        try {
            const windows = await api.listWindows();
            apps = mergeRunning(baseApps, windows);
        } catch {
            // Keep the local catalog if the window list is unavailable.
        }
    }

    const results = $derived.by(() => {
        const queryText = normalize(query);
        const tokens = queryText.split(/\s+/).filter(Boolean);

        const scored = apps
            .map(app => {
                const name = normalize(app.name);
                const target = normalize(app.target);
                const exe = normalize(app.exe ?? '');
                const windowTitle = normalize(app.windowTitle ?? '');
                let score = 4;

                if (!queryText) {
                    score = 0;
                } else if (name === queryText) {
                    score = 0;
                } else if (name.startsWith(queryText)) {
                    score = 1;
                } else if (name.includes(queryText)) {
                    score = 2;
                } else if (exe.includes(queryText)) {
                    score = 2.4;
                } else if (windowTitle.includes(queryText)) {
                    score = 2.6;
                } else if (target.includes(queryText)) {
                    score = 3;
                } else {
                    const initials = name
                        .split(/\s+/)
                        .filter(Boolean)
                        .map(word => word[0])
                        .join('');
                    const haystack = `${name} ${target} ${exe} ${windowTitle}`;
                    if (
                        queryText.length >= 2 &&
                        initials.startsWith(queryText)
                    ) {
                        score = 3.2;
                    } else if (
                        tokens.length > 1 &&
                        tokens.every(token => haystack.includes(token))
                    ) {
                        score = 3.5;
                    } else {
                        return null;
                    }
                }

                return { app, score };
            })
            .filter(
                (entry): entry is { app: LauncherApp; score: number } =>
                    entry !== null,
            );

        scored.sort(
            (a, b) =>
                a.score - b.score ||
                Number(b.app.running ?? false) -
                    Number(a.app.running ?? false) ||
                a.app.name.localeCompare(b.app.name, 'pt-BR'),
        );

        return scored.map(entry => entry.app);
    });

    const runningResults = $derived(
        results.filter(app => app.running).length,
    );

    const launcherStyle = $derived.by(() => {
        const width = settings?.launcherWidth ?? 640;
        const top = settings?.launcherTop ?? 12;
        const radius = settings?.launcherRadius ?? 20;
        const blur = settings?.launcherBlur ?? 36;
        const speed =
            settings?.animations === false ? 0 : (settings?.animationSpeed ?? 1);
        return `--launcher-width:${width}px; --launcher-top:${top}vh; --launcher-radius:${radius}px; --launcher-blur:${blur}px; --anim-duration:${Math.round(170 * speed)}ms;`;
    });

    $effect(() => {
        if (selected >= results.length) {
            selected = Math.max(0, results.length - 1);
        }
    });

    $effect(() => {
        for (const app of results) {
            if (icons[app.id] || requested.has(app.id)) continue;
            requested.add(app.id);

            if (app.id.startsWith('running:')) {
                api.getExeIcon(app.exe ?? app.target)
                    .then(data => {
                        if (data) icons = { ...icons, [app.id]: data };
                    })
                    .catch(() => {});
                continue;
            }

            const modern = builtinIcon(app);
            if (modern) {
                icons = { ...icons, [app.id]: modern };
                continue;
            }

            api.getIcon(app.id)
                .then(data => {
                    if (data) icons = { ...icons, [app.id]: data };
                })
                .catch(() => {});
        }
    });

    async function load() {
        try {
            const boot = await api.bootstrap();
            baseApps = boot.apps;
            apps = boot.apps;
            settings = boot.settings;
            systemTheme = boot.theme;
            applyTheme(boot.theme);
            if (boot.settings.theme === 'auto' && boot.theme) {
                systemTheme = boot.theme;
            }
            await refreshRunning();
        } catch (reason) {
            error = String(reason);
        } finally {
            loading = false;
        }
    }

    function resetAndFocus() {
        query = '';
        selected = 0;
        error = null;
        void refreshRunning();
        void tick().then(() => inputEl?.focus());
    }

    function close() {
        void api.closeLauncher().catch(() => {});
    }

    async function launch(app: LauncherApp | undefined) {
        if (!app || launching) return;
        launching = true;
        error = null;

        try {
            if (app.running && app.hwnd) {
                const focused = await api.focusWindow(app.hwnd);
                if (focused) {
                    await api.closeLauncher();
                    return;
                }
                if (app.id.startsWith('running:')) {
                    await api.openPath(app.target);
                    await api.closeLauncher();
                    return;
                }
            }

            await api.launchApp(app.id);
            await api.closeLauncher();
        } catch (reason) {
            error = String(reason);
        } finally {
            launching = false;
        }
    }

    function revealSelected() {
        void tick().then(() => {
            document
                .querySelector<HTMLElement>(`[data-result="${selected}"]`)
                ?.scrollIntoView({ block: 'nearest' });
        });
    }

    function onKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            event.preventDefault();
            close();
            return;
        }

        if (event.key === 'ArrowDown') {
            event.preventDefault();
            if (results.length > 0) {
                selected = Math.min(results.length - 1, selected + 1);
                revealSelected();
            }
            return;
        }

        if (event.key === 'ArrowUp') {
            event.preventDefault();
            if (results.length > 0) {
                selected = Math.max(0, selected - 1);
                revealSelected();
            }
            return;
        }

        if (event.key === 'Enter') {
            event.preventDefault();
            void launch(results[selected]);
            return;
        }

        if (event.key === 'Tab') {
            event.preventDefault();
            inputEl?.focus();
        }
    }

    function subtitle(app: LauncherApp): string {
        if (app.running) {
            return app.windowTitle?.trim() || 'Janela aberta';
        }
        if (app.builtin) return 'Aplicativo do sistema';
        const file = app.target.replace(/\//g, '\\').split('\\').pop();
        return file || app.path.split('\\').pop() || 'Aplicativo';
    }

    function initial(name: string): string {
        return (name.trim()[0] ?? '?').toUpperCase();
    }

    onMount(() => {
        let disposed = false;
        let unlistenLauncher: (() => void) | undefined;
        let unlistenSettings: (() => void) | undefined;

        onLauncherOpen(resetAndFocus).then(unlisten => {
            if (disposed) unlisten();
            else unlistenLauncher = unlisten;
        });

        onSettingsChanged(next => {
            settings = next;
        }).then(unlisten => {
            if (disposed) unlisten();
            else unlistenSettings = unlisten;
        });

        const onWindowFocus = () => resetAndFocus();
        window.addEventListener('focus', onWindowFocus);

        const scheme = window.matchMedia('(prefers-color-scheme: dark)');
        systemTheme = scheme.matches ? 'dark' : 'light';
        applyTheme(systemTheme);

        void load();
        void tick().then(() => inputEl?.focus());

        return () => {
            disposed = true;
            unlistenLauncher?.();
            unlistenSettings?.();
            window.removeEventListener('focus', onWindowFocus);
        };
    });
</script>

<svelte:window onkeydown={onKeydown} />

<div
    class="launcher-stage"
    role="presentation"
    style={launcherStyle}
    onclick={close}
    oncontextmenu={event => {
        event.preventDefault();
        close();
    }}
>
    <div
        class="launcher-panel"
        role="dialog"
        aria-label="Pesquisar aplicativos"
        onclick={event => event.stopPropagation()}
    >
        <div class="launcher-search">
            <svg
                viewBox="0 0 20 20"
                width="19"
                height="19"
                fill="none"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linecap="round"
                aria-hidden="true"
            >
                <circle cx="8.6" cy="8.6" r="5.4" />
                <path d="M12.7 12.7L17 17" />
            </svg>

            <input
                bind:this={inputEl}
                bind:value={query}
                oninput={() => (selected = 0)}
                placeholder="Pesquisar aplicativos ou janelas…"
                autocomplete="off"
                spellcheck="false"
                aria-label="Pesquisar aplicativos ou janelas"
            />

            {#if query}
                <button
                    class="launcher-clear"
                    title="Limpar pesquisa"
                    onclick={() => {
                        query = '';
                        selected = 0;
                        void tick().then(() => inputEl?.focus());
                    }}
                >
                    ×
                </button>
            {/if}

            <span class="launcher-esc">ESC</span>
        </div>

        <div class="launcher-results">
            {#if loading}
                <div class="launcher-empty">
                    <span class="launcher-spinner"></span>
                    Carregando aplicativos…
                </div>
            {:else if results.length === 0}
                <div class="launcher-empty">
                    <strong>Nenhum aplicativo encontrado</strong>
                    <span>Tente outro nome, parte do caminho ou título de janela.</span>
                </div>
            {:else}
                {#each results as app, index (app.id)}
                    <button
                        class="launcher-row"
                        class:is-selected={index === selected}
                        data-result={index}
                        onclick={() => launch(app)}
                        onpointermove={() => (selected = index)}
                    >
                        <span class="launcher-icon">
                            {#if icons[app.id]}
                                <img
                                    src={icons[app.id]}
                                    alt=""
                                    draggable="false"
                                />
                            {:else}
                                <span class="launcher-letter"
                                    >{initial(app.name)}</span
                                >
                            {/if}
                        </span>

                        <span class="launcher-copy">
                            <span class="launcher-name-row">
                                <strong>{app.name}</strong>
                                {#if app.running}
                                    <span class="launcher-running">
                                        Em execução
                                    </span>
                                {/if}
                            </span>
                            <span>{subtitle(app)}</span>
                        </span>

                        {#if index === selected}
                            <span class="launcher-enter">↵</span>
                        {/if}
                    </button>
                {/each}
            {/if}
        </div>

        <div class="launcher-footer">
            <span>
                {#if error}
                    <span class="launcher-error">{error}</span>
                {:else}
                    {results.length}
                    {results.length === 1 ? 'aplicativo' : 'aplicativos'}
                    {#if runningResults > 0}
                        · {runningResults} em execução
                    {/if}
                {/if}
            </span>
            <span class="launcher-hints">
                <kbd>↑</kbd><kbd>↓</kbd> navegar
                <kbd>Enter</kbd> abrir
                <kbd>Esc</kbd> fechar
            </span>
        </div>
    </div>
</div>

<style>
    .launcher-stage {
        position: fixed;
        inset: 0;
        display: flex;
        align-items: flex-start;
        justify-content: center;
        padding: var(--launcher-top, 12vh) 24px 24px;
        background: rgba(8, 8, 11, 0.42);
        cursor: default;
    }

    .launcher-panel {
        display: flex;
        width: min(var(--launcher-width, 640px), 92vw);
        max-height: 72vh;
        flex-direction: column;
        overflow: hidden;
        border: 1px solid var(--glass-border);
        border-radius: var(--launcher-radius, 20px);
        background: var(--menu-bg);
        box-shadow:
            var(--menu-shadow),
            inset 0 1px 0 var(--glass-inner);
        backdrop-filter: blur(var(--launcher-blur, 36px)) saturate(180%);
        -webkit-backdrop-filter: blur(var(--launcher-blur, 36px))
            saturate(180%);
        color: var(--text);
        animation: launcher-in var(--anim-duration, 170ms)
            cubic-bezier(0.2, 0.9, 0.25, 1);
    }

    @keyframes launcher-in {
        from {
            opacity: 0;
            transform: translateY(-8px) scale(0.985);
        }
    }

    .launcher-search {
        display: flex;
        min-height: 64px;
        align-items: center;
        gap: 11px;
        padding: 0 18px;
        border-bottom: 1px solid var(--sep);
        color: var(--text-dim);
    }

    .launcher-search input {
        min-width: 0;
        flex: 1;
        padding: 8px 0;
        border: 0;
        outline: 0;
        background: transparent;
        color: var(--text);
        font-size: 16.5px;
        letter-spacing: -0.01em;
    }

    .launcher-search input::placeholder {
        color: var(--text-dim);
    }

    .launcher-clear {
        display: flex;
        width: 20px;
        height: 20px;
        flex: none;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: var(--hover);
        color: var(--text-dim);
        font-size: 13px;
        line-height: 1;
        cursor: default;
    }

    .launcher-clear:hover {
        background: var(--panel-strong);
        color: var(--text);
    }

    .launcher-esc {
        flex: none;
        padding: 3px 7px;
        border: 1px solid var(--panel-border);
        border-radius: 6px;
        background: var(--panel);
        color: var(--text-dim);
        font-size: 9px;
        font-weight: 600;
        letter-spacing: 0.05em;
    }

    .launcher-results {
        min-height: 80px;
        flex: 1 1 auto;
        padding: 6px;
        overflow-y: auto;
        overscroll-behavior: contain;
    }

    .launcher-row {
        display: flex;
        width: 100%;
        align-items: center;
        gap: 11px;
        padding: 8px 10px;
        border: 0;
        border-radius: 10px;
        background: transparent;
        color: inherit;
        text-align: left;
        cursor: default;
        transition: background 100ms ease;
    }

    .launcher-row.is-selected {
        background: var(--accent-soft);
    }

    .launcher-icon {
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

    .launcher-icon img {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: contain;
    }

    .launcher-letter {
        display: flex;
        width: 100%;
        height: 100%;
        align-items: center;
        justify-content: center;
        border-radius: 7px;
        background: linear-gradient(160deg, #6f7bd6, #3f4a9c);
        color: #fff;
        font-size: 13px;
        font-weight: 600;
    }

    .launcher-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 1px;
    }

    .launcher-name-row {
        display: flex;
        min-width: 0;
        align-items: center;
        gap: 7px;
        overflow: visible;
        color: var(--text);
        font-size: inherit;
        white-space: normal;
    }

    .launcher-name-row strong {
        overflow: hidden;
        font-size: 13px;
        font-weight: 600;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .launcher-running {
        flex: none;
        padding: 2px 6px;
        border-radius: 999px;
        background: rgba(52, 199, 89, 0.16);
        color: #45d478;
        font-size: 8.5px;
        font-weight: 700;
        letter-spacing: 0;
        white-space: nowrap;
    }

    .launcher-copy span {
        overflow: hidden;
        color: var(--text-dim);
        font-size: 10px;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .launcher-enter {
        flex: none;
        color: var(--accent);
        font-size: 14px;
    }

    .launcher-empty {
        display: flex;
        min-height: 140px;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 4px;
        padding: 25px;
        color: var(--text-dim);
        font-size: 11px;
        text-align: center;
    }

    .launcher-empty strong {
        color: var(--text);
        font-size: 13px;
    }

    .launcher-spinner {
        width: 16px;
        height: 16px;
        margin-bottom: 5px;
        border: 2px solid var(--sep);
        border-top-color: var(--accent);
        border-radius: 50%;
        animation: launcher-spin 0.8s linear infinite;
    }

    @keyframes launcher-spin {
        to {
            transform: rotate(360deg);
        }
    }

    .launcher-footer {
        display: flex;
        min-height: 36px;
        align-items: center;
        justify-content: space-between;
        gap: 10px;
        padding: 6px 16px;
        border-top: 1px solid var(--sep);
        color: var(--text-dim);
        font-size: 9.5px;
    }

    .launcher-hints {
        display: flex;
        align-items: center;
        gap: 4px;
        white-space: nowrap;
    }

    .launcher-hints kbd {
        padding: 1px 5px;
        border: 1px solid var(--panel-border);
        border-radius: 4px;
        background: var(--panel);
        font-family: inherit;
        font-size: 9px;
    }

    .launcher-error {
        color: var(--danger);
    }
</style>
