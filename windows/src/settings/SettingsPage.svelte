<script lang="ts">
    import { onDestroy } from 'svelte';
    import { api } from '../lib/ipc';
    import type { ResolvedTheme, Settings } from '../lib/types';

    let {
        initial,
        systemTheme,
        version,
    }: {
        initial: Settings;
        systemTheme: ResolvedTheme;
        version: string;
    } = $props();

    let value = $state<Settings>({ ...initial, pinned: [...initial.pinned] });
    let status = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
    let errorText = $state('');
    let saveTimer: ReturnType<typeof setTimeout> | undefined;
    let statusTimer: ReturnType<typeof setTimeout> | undefined;
    let lastInitialTaskbar = $state(initial.hideTaskbar);

    $effect(() => {
        // The emergency shortcut changes this setting behind our back.
        if (initial.hideTaskbar !== lastInitialTaskbar) {
            lastInitialTaskbar = initial.hideTaskbar;
            value = { ...value, hideTaskbar: initial.hideTaskbar };
        }
    });

    onDestroy(() => {
        clearTimeout(saveTimer);
        clearTimeout(statusTimer);
    });

    function scheduleSave() {
        clearTimeout(saveTimer);
        saveTimer = setTimeout(() => void save(), 260);
    }

    async function save() {
        clearTimeout(saveTimer);
        status = 'saving';
        try {
            const saved = await api.saveSettings(value);
            value = saved;
            status = 'saved';
            clearTimeout(statusTimer);
            statusTimer = setTimeout(() => {
                if (status === 'saved') status = 'idle';
            }, 1600);
        } catch (reason) {
            status = 'error';
            errorText = String(reason);
        }
    }

    function update(patch: Partial<Settings>) {
        value = { ...value, ...patch };
        scheduleSave();
    }

    const DOCK_TEMPLATES: Record<string, Partial<Settings>> = {
        macos: {
            iconSize: 44,
            iconSpacing: 8,
            dockMargin: 8,
            dockPaddingX: 12,
            dockPaddingY: 8,
            zoomFactor: 1.6,
            zoomRise: 16,
            dockRadius: 18,
            animations: true,
            animationSpeed: 1.0,
            zoomSmoothness: 0.26,
        },
        windows11: {
            iconSize: 32,
            iconSpacing: 4,
            dockMargin: 2,
            dockPaddingX: 10,
            dockPaddingY: 4,
            zoomFactor: 1.0,
            zoomRise: 0,
            dockRadius: 8,
            animations: true,
            animationSpeed: 0.8,
            zoomSmoothness: 0.3,
        },
        cosmic: {
            iconSize: 36,
            iconSpacing: 8,
            dockMargin: 6,
            dockPaddingX: 10,
            dockPaddingY: 6,
            zoomFactor: 1.25,
            zoomRise: 4,
            dockRadius: 14,
            animations: true,
            animationSpeed: 1.0,
            zoomSmoothness: 0.22,
        },
        minimal: {
            iconSize: 34,
            iconSpacing: 6,
            dockMargin: 6,
            dockPaddingX: 8,
            dockPaddingY: 4,
            zoomFactor: 1.0,
            zoomRise: 0,
            dockRadius: 10,
            animations: true,
            animationSpeed: 0.9,
            zoomSmoothness: 0.3,
        },
    };

    const LAUNCHER_TEMPLATES: Record<string, Partial<Settings>> = {
        spotlight: {
            launcherWidth: 640,
            launcherTop: 12,
            launcherRadius: 20,
            launcherBlur: 36,
        },
        cosmic: {
            launcherWidth: 720,
            launcherTop: 10,
            launcherRadius: 14,
            launcherBlur: 30,
        },
        compact: {
            launcherWidth: 520,
            launcherTop: 8,
            launcherRadius: 14,
            launcherBlur: 24,
        },
        minimal: {
            launcherWidth: 600,
            launcherTop: 12,
            launcherRadius: 10,
            launcherBlur: 12,
        },
    };

    function applyDockTemplate(template: string) {
        const preset = DOCK_TEMPLATES[template];
        if (!preset) return;
        update({ ...preset, dockTemplate: template });
    }

    function applyLauncherTemplate(template: string) {
        const preset = LAUNCHER_TEMPLATES[template];
        if (!preset) return;
        update({ ...preset, launcherTemplate: template });
    }

    function updateDock(patch: Partial<Settings>) {
        update({ ...patch, dockTemplate: 'custom' });
    }

    function markDockCustom() {
        value = { ...value, dockTemplate: 'custom' };
        scheduleSave();
    }

    function updateLauncher(patch: Partial<Settings>) {
        update({ ...patch, launcherTemplate: 'custom' });
    }

    async function toggleTaskbar() {
        const next = !value.hideTaskbar;
        value = { ...value, hideTaskbar: next };
        try {
            const saved = await api.setTaskbarHidden(next);
            value = saved;
            lastInitialTaskbar = saved.hideTaskbar;
            status = 'saved';
            clearTimeout(statusTimer);
            statusTimer = setTimeout(() => {
                if (status === 'saved') status = 'idle';
            }, 1600);
        } catch (reason) {
            value = { ...value, hideTaskbar: !next };
            status = 'error';
            errorText = String(reason);
        }
    }

    async function reset() {
        if (
            !confirm(
                'Redefinir aparência e comportamento da dock para os padrões?',
            )
        ) {
            return;
        }
        try {
            const saved = await api.resetSettings();
            value = saved;
            status = 'saved';
        } catch (reason) {
            status = 'error';
            errorText = String(reason);
        }
    }

    function close() {
        void api.closeSettings().catch(() => {});
    }
</script>

<div class="settings-shell">
    <header class="settings-header">
        <div class="logo" aria-hidden="true">
            <div class="bar">
                <span class="dot"></span>
                <span class="dot"></span>
                <span class="dot"></span>
            </div>
        </div>
        <div>
            <h1>Dhruva</h1>
            <p>Uma dock macOS para o Windows</p>
        </div>
    </header>

    <section class="settings-card">
        <h2>Aparência</h2>
        <p class="section-note">
            Escolha um modelo pronto ou ajuste cada detalhe. O vidro acompanha o
            tema do Windows.
        </p>

        <div class="template-block">
            <div class="template-heading">
                <strong>Modelo da dock</strong>
                <span>
                    {value.dockTemplate === 'custom'
                        ? 'Personalizado'
                        : value.dockTemplate === 'macos'
                          ? 'macOS'
                          : value.dockTemplate === 'windows11'
                            ? 'Windows 11'
                            : value.dockTemplate === 'cosmic'
                              ? 'COSMIC'
                              : 'Minimal'}
                </span>
            </div>
            <div class="template-grid">
                {#each [
                    { id: 'macos', name: 'macOS', desc: 'Flutuante, zoom suave' },
                    { id: 'windows11', name: 'Windows 11', desc: 'Compacta, sem zoom' },
                    { id: 'cosmic', name: 'COSMIC', desc: 'Pílula discreta' },
                    { id: 'minimal', name: 'Minimal', desc: 'Sóbria e direta' },
                ] as template (template.id)}
                    <button
                        class="template-card"
                        class:is-active={value.dockTemplate === template.id}
                        onclick={() => applyDockTemplate(template.id)}
                    >
                        <span class="template-preview dock-preview">
                            <i></i><i></i><i></i><i></i>
                        </span>
                        <span class="template-name">{template.name}</span>
                        <span class="template-desc">{template.desc}</span>
                    </button>
                {/each}
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Tema</strong>
                <span>Automático segue o modo claro/escuro do Windows.</span>
            </div>
            <div class="setting-control">
                <select
                    bind:value={value.theme}
                    onchange={() => scheduleSave()}
                >
                    <option value="auto">Automático</option>
                    <option value="light">Claro</option>
                    <option value="dark">Escuro</option>
                </select>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Tamanho dos ícones</strong>
                <span>Entre 28 e 80 pixels.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="28"
                    max="80"
                    step="2"
                    bind:value={value.iconSize}
                    oninput={markDockCustom}
                />
                <span class="setting-value">{value.iconSize}px</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Espaçamento entre ícones</strong>
                <span>Quanto maior, mais respirada fica a barra.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="0"
                    max="24"
                    step="1"
                    value={value.iconSpacing}
                    oninput={event =>
                        updateDock({
                            iconSpacing: Number(event.currentTarget.value),
                        })}
                />
                <span class="setting-value">{value.iconSpacing}px</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Margem da dock</strong>
                <span>Distância entre a dock e a borda da tela.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="0"
                    max="32"
                    step="1"
                    value={value.dockMargin}
                    oninput={event =>
                        updateDock({
                            dockMargin: Number(event.currentTarget.value),
                        })}
                />
                <span class="setting-value">{value.dockMargin}px</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Cantos da dock</strong>
                <span>De quadrada a totalmente arredondada.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="0"
                    max="28"
                    step="1"
                    value={value.dockRadius}
                    oninput={event =>
                        updateDock({
                            dockRadius: Number(event.currentTarget.value),
                        })}
                />
                <span class="setting-value">{value.dockRadius}px</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Zoom ao passar o mouse</strong>
                <span>O centro chega a {Math.round((1 + (value.zoomFactor - 1) * 2) * 100)}% do tamanho.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="1"
                    max="2.5"
                    step="0.1"
                    bind:value={value.zoomFactor}
                    oninput={markDockCustom}
                />
                <span class="setting-value">{value.zoomFactor.toFixed(1)}×</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Elevação no hover</strong>
                <span>Quanto os ícones sobem ao ampliar.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="0"
                    max="40"
                    step="2"
                    value={value.zoomRise}
                    oninput={event =>
                        updateDock({
                            zoomRise: Number(event.currentTarget.value),
                        })}
                />
                <span class="setting-value">{value.zoomRise}px</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Mostrar lixeira</strong>
                <span>Um separador e a Lixeira à direita da dock.</span>
            </div>
            <div class="setting-control">
                <button
                    class="switch"
                    class:is-on={value.showTrash}
                    aria-label="Mostrar lixeira"
                    onclick={() => update({ showTrash: !value.showTrash })}
                ></button>
            </div>
        </div>
    </section>

    <section class="settings-card">
        <h2>Movimento</h2>
        <p class="section-note">
            Controle a suavidade da dock, dos painéis e do launcher. Desligar as
            animações deixa tudo instantâneo.
        </p>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Animações</strong>
                <span>Transições, zoom e abertura dos painéis.</span>
            </div>
            <div class="setting-control">
                <button
                    class="switch"
                    class:is-on={value.animations}
                    aria-label="Ativar animações"
                    onclick={() => update({ animations: !value.animations })}
                ></button>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Duração das animações</strong>
                <span>Menor = mais rápido; maior = mais suave e lento.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="0.5"
                    max="2"
                    step="0.1"
                    bind:value={value.animationSpeed}
                    oninput={markDockCustom}
                />
                <span class="setting-value">{value.animationSpeed.toFixed(1)}×</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Suavidade do zoom</strong>
                <span>Quanto menor, mais macio o ícone segue o mouse.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="0.08"
                    max="0.45"
                    step="0.01"
                    bind:value={value.zoomSmoothness}
                    oninput={markDockCustom}
                />
                <span class="setting-value">
                    {value.zoomSmoothness.toFixed(2)}
                </span>
            </div>
        </div>
    </section>

    <section class="settings-card">
        <h2>Pesquisa</h2>
        <p class="section-note">
            Aparência do launcher aberto pela tecla Windows ou pelo botão de
            aplicativos.
        </p>

        <div class="template-block">
            <div class="template-heading">
                <strong>Estilo da barra de pesquisa</strong>
                <span>
                    {value.launcherTemplate === 'custom'
                        ? 'Personalizado'
                        : value.launcherTemplate === 'spotlight'
                          ? 'Spotlight'
                          : value.launcherTemplate === 'cosmic'
                            ? 'COSMIC'
                            : value.launcherTemplate === 'compact'
                              ? 'Compacta'
                              : 'Minimal'}
                </span>
            </div>
            <div class="template-grid">
                {#each [
                    { id: 'spotlight', name: 'Spotlight', desc: 'Centro, vidro forte' },
                    { id: 'cosmic', name: 'COSMIC', desc: 'Larga e baixa' },
                    { id: 'compact', name: 'Compacta', desc: 'Menor e direta' },
                    { id: 'minimal', name: 'Minimal', desc: 'Simples, pouco blur' },
                ] as template (template.id)}
                    <button
                        class="template-card"
                        class:is-active={value.launcherTemplate === template.id}
                        onclick={() => applyLauncherTemplate(template.id)}
                    >
                        <span class="template-preview search-preview">
                            <span class="search-preview-bar"></span>
                            <i></i><i></i><i></i>
                        </span>
                        <span class="template-name">{template.name}</span>
                        <span class="template-desc">{template.desc}</span>
                    </button>
                {/each}
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Largura da pesquisa</strong>
                <span>Entre 480 e 1000 pixels.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="480"
                    max="1000"
                    step="10"
                    value={value.launcherWidth}
                    oninput={event =>
                        updateLauncher({
                            launcherWidth: Number(event.currentTarget.value),
                        })}
                />
                <span class="setting-value">{value.launcherWidth}px</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Posição vertical</strong>
                <span>Distância do topo da tela.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="4"
                    max="30"
                    step="1"
                    value={value.launcherTop}
                    oninput={event =>
                        updateLauncher({
                            launcherTop: Number(event.currentTarget.value),
                        })}
                />
                <span class="setting-value">{value.launcherTop}vh</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Cantos da pesquisa</strong>
                <span>Arredondamento do painel.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="0"
                    max="32"
                    step="1"
                    value={value.launcherRadius}
                    oninput={event =>
                        updateLauncher({
                            launcherRadius: Number(event.currentTarget.value),
                        })}
                />
                <span class="setting-value">{value.launcherRadius}px</span>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Desfoque do vidro</strong>
                <span>Quanto o fundo atrás da pesquisa fica desfocado.</span>
            </div>
            <div class="setting-control">
                <input
                    type="range"
                    min="0"
                    max="60"
                    step="2"
                    value={value.launcherBlur}
                    oninput={event =>
                        updateLauncher({
                            launcherBlur: Number(event.currentTarget.value),
                        })}
                />
                <span class="setting-value">{value.launcherBlur}px</span>
            </div>
        </div>
    </section>

    <section class="settings-card">
        <h2>Comportamento</h2>
        <p class="section-note">
            O auto-hide esconde a dock depois de um tempo e ela volta quando o
            mouse toca a borda inferior. Com ele desligado, a dock reserva uma
            faixa na base da tela e as janelas maximizadas param acima dela.
        </p>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Esconder automaticamente</strong>
                <span>Some após {value.hideDelay} ms sem o mouse na dock.</span>
            </div>
            <div class="setting-control">
                <button
                    class="switch"
                    class:is-on={value.autoHide}
                    aria-label="Esconder automaticamente"
                    onclick={() => update({ autoHide: !value.autoHide })}
                ></button>
            </div>
        </div>

        <div class="setting-row">
            <div class="setting-copy">
                <strong>Ocultar as barras de tarefas</strong>
                <span>Oculta as barras dos monitores onde a dock está, sem mexer nos outros.</span>
            </div>
            <div class="setting-control">
                <button
                    class="switch"
                    class:is-on={value.hideTaskbar}
                    aria-label="Ocultar as barras de tarefas"
                    onclick={toggleTaskbar}
                ></button>
            </div>
        </div>

        {#if value.hideTaskbar}
            <div class="warning">
                <strong>Atalho de emergência:</strong> Ctrl + Alt + Shift + D
                restaura as barras de tarefas a qualquer momento. Elas também
                voltam sozinhas se a Dhruva for fechada.
            </div>
        {/if}
    </section>

    <section class="settings-card">
        <h2>Atalhos</h2>
        <div class="setting-row">
            <div class="setting-copy">
                <strong>Restaurar a barra do Windows</strong>
                <span>Funciona mesmo se a dock estiver travada.</span>
            </div>
            <div class="shortcut">
                <span class="key">Ctrl</span>
                <span class="key">Alt</span>
                <span class="key">Shift</span>
                <span class="key">D</span>
            </div>
        </div>
    </section>

    <section class="settings-card">
        <h2>Sobre</h2>
        <div class="about-grid">
            <span>Versão</span>
            <strong>{version}</strong>
            <span>Base</span>
            <strong>Dhruva Windows — inspirado no Dhruva de NarkAgni</strong>
            <span>Tema atual</span>
            <strong>{value.theme === 'auto' ? `Automático (${systemTheme === 'dark' ? 'escuro' : 'claro'})` : value.theme === 'dark' ? 'Escuro' : 'Claro'}</strong>
            <span>Licença do port</span>
            <strong>MIT OR Apache-2.0</strong>
            <span>Projeto original</span>
            <strong>Dhruva, de NarkAgni — GPL-3.0-or-later</strong>
            <span>Repositório original</span>
            <strong class="about-url">github.com/NarkAgni/dhruva</strong>
        </div>
    </section>

    <div class="actions">
        <div class="actions-left">
            <button class="button danger" onclick={reset}>
                Restaurar padrões
            </button>
        </div>
        <div class="actions-right">
            <div class="status" class:error={status === 'error'}>
                {#if status === 'saving'}
                    Salvando…
                {:else if status === 'saved'}
                    Salvo
                {:else if status === 'error'}
                    {errorText || 'Não foi possível salvar'}
                {/if}
            </div>
            <button class="button primary" onclick={close}>Fechar</button>
        </div>
    </div>
</div>
