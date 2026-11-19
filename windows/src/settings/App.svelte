<script lang="ts">
    import { onMount } from 'svelte';
    import SettingsPage from './SettingsPage.svelte';
    import {
        api,
        onSettingsChanged,
        onThemeChanged,
    } from '../lib/ipc';
    import { applyTheme, resolveTheme } from '../lib/theme';
    import type { Bootstrap, ResolvedTheme, Settings } from '../lib/types';

    let settings = $state<Settings | null>(null);
    let systemTheme = $state<ResolvedTheme>('dark');
    let version = $state('0.1.0');
    let error = $state<string | null>(null);

    $effect(() => {
        if (!settings) return;
        applyTheme(resolveTheme(settings.theme, systemTheme));
    });

    onMount(() => {
        const disposers: Array<() => void> = [];
        let cancelled = false;

        (async () => {
            try {
                const boot: Bootstrap = await api.bootstrap();
                if (cancelled) return;
                settings = boot.settings;
                systemTheme = boot.theme;
                version = boot.version;

                disposers.push(
                    await onSettingsChanged(next => {
                        settings = next;
                    }),
                );
                disposers.push(
                    await onThemeChanged(theme => {
                        systemTheme = theme;
                    }),
                );
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
    <div class="settings-shell">
        <div class="settings-card">
            <h2>Não foi possível carregar as configurações</h2>
            <p class="section-note">{error}</p>
        </div>
    </div>
{:else if settings}
    <SettingsPage initial={settings} {systemTheme} {version} />
{:else}
    <div class="settings-shell">
        <div class="settings-card">
            <p class="section-note">Carregando Dhruva…</p>
        </div>
    </div>
{/if}
