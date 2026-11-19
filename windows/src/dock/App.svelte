<script lang="ts">
    import { onMount } from 'svelte';
    import Dock from './Dock.svelte';
    import {
        api,
        onRelayout,
        onSettingsChanged,
        onThemeChanged,
        onWindowsChanged,
    } from '../lib/ipc';
    import { applyTheme, resolveTheme } from '../lib/theme';
    import type { AppEntry, Bootstrap, ResolvedTheme, Settings, WindowInfo } from '../lib/types';

    let settings = $state<Settings | null>(null);
    let apps = $state<AppEntry[]>([]);
    let windows = $state<WindowInfo[]>([]);
    let systemTheme = $state<ResolvedTheme>('dark');
    let relayout = $state(0);
    let error = $state<string | null>(null);
    let bootVersion = $state('0.1.0');

    $effect(() => {
        if (!settings) return;
        applyTheme(resolveTheme(settings.theme, systemTheme));
    });

    async function refreshWindows() {
        try {
            windows = await api.listWindows();
        } catch {
            // The poller will catch up.
        }
    }

    onMount(() => {
        const disposers: Array<() => void> = [];
        let cancelled = false;

        (async () => {
            try {
                const boot: Bootstrap = await api.bootstrap();
                if (cancelled) return;
                settings = boot.settings;
                apps = boot.apps;
                systemTheme = boot.theme;
                bootVersion = boot.version;
                await refreshWindows();

                disposers.push(await onWindowsChanged(list => (windows = list)));
                disposers.push(
                    await onThemeChanged(theme => {
                        systemTheme = theme;
                    }),
                );
                disposers.push(
                    await onSettingsChanged(next => {
                        settings = next;
                    }),
                );
                disposers.push(await onRelayout(() => relayout++));
            } catch (reason) {
                error = String(reason);
            }
        })();

        return () => {
            cancelled = true;
            for (const dispose of disposers) dispose();
        };
    });
</script>

{#if error}
    <div class="fatal">
        <strong>Dhruva não conseguiu iniciar.</strong>
        <span>{error}</span>
    </div>
{/if}

{#if settings}
    <Dock {apps} {settings} {windows} {relayout} {refreshWindows} version={bootVersion} />
{/if}
