# AGENTS.md — Dhruva Windows

Contrato operacional para agentes e desenvolvedores que trabalham no **Dhruva Windows** (`windows/`). A extensão GNOME original na raiz do repositório não faz parte deste contrato de desenvolvimento e permanece em GPL-3.0-or-later.

## 1. Missão

Manter o port Windows estável, nativo e fiel à experiência da dock:

- Tauri 2 + Rust + Svelte 5.
- Interface em **português do Brasil**.
- Dados reais (Wi-Fi, mídia, janelas, áreas de trabalho); nunca mocks silenciosos.
- Preservar os créditos do projeto original **Dhruva**, de **NarkAgni**:
  - https://github.com/NarkAgni/dhruva
  - https://extensions.gnome.org/extension/9495/dhruva/

## 2. Worktrees (toda tarefa)

Nenhuma IA trabalha direto na cópia principal. A cópia principal fica na `main`, limpa.

```powershell
git fetch origin
git worktree add .worktrees/feat-minha-tarefa -b feat/minha-tarefa origin/main
```

- Trabalhe sempre dentro de `.worktrees/feat-minha-tarefa`.
- O `node_modules` pode ser ligado por junction ao `node_modules` da cópia principal (ou a `C:\dhruva-build\node_modules` nesta máquina).
- Ao concluir: commit, push para a `main` do seu fork (`origin`) e remova a worktree com `git worktree remove`.
- Abrir, aprovar ou mergear Pull Request é ação humana. A IA não faz isso.

## 3. Validação obrigatória

Execute a partir de `windows/`:

```powershell
npm run check
$env:CARGO_TARGET_DIR='C:\dhruva-build\target'
cargo check --manifest-path src-tauri\Cargo.toml
npx tauri build --debug --no-bundle
```

- O teste funcional usa o executável `C:\dhruva-build\target\debug\dhruva.exe`.
- O debug remoto (CDP) usa `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` e `tools/cdp.mjs`.
- Nunca deixe servidores, watchers ou processos de teste rodando sem encerrar no fim.

## 4. Convenções de código

- CSS: prefira as variáveis de `src/styles/global.css` (`--text`, `--glass`, `--accent`, `--dock-bg` etc.). Evite novos valores de cor soltos quando existir token.
- Componentes Svelte: estado reativo com `$state`/`$derived`; não use variáveis simples para estado que aparece na UI.
- Rust: comandos Tauri com nomes em `snake_case`; tipos serializados em `camelCase` com `#[serde(rename_all = "camelCase")]`.
- Janelas: mantenha as regiões nativas (`set_dock_region`) atualizadas para barras, painéis e popovers.
- Não introduza dependências pesadas sem necessidade; o app precisa iniciar rápido.

## 5. Git e changelog

- Commits em **Conventional Commits**: `feat(dock): ...`, `fix(media): ...`, `docs: ...`.
- Toda mudança visível ganha uma linha em `windows/CHANGELOG.md` (seção `[Unreleased]`).
- Nunca commite `.env`, tokens, credenciais ou artefatos de build.

## 6. Licenciamento

- O port em `windows/` é **MIT OR Apache-2.0** (veja `LICENSE-MIT` e `LICENSE-APACHE`).
- A extensão GNOME original na raiz é **GPL-3.0-or-later** e não é relicenciada.
- Leia [`../LICENSING.md`](../LICENSING.md) antes de redistribuir o repositório inteiro.

## 7. Ferramentas do projeto

- `windows/tools/cdp.mjs` — inspeção/screenshot/cliques via Chrome DevTools Protocol.
- `windows/tools/screencast.mjs` — gravação de quadros para bugs de animação.
- `windows/tools/screencast-sweep.mjs` — varredura do mouse para testar o efeito de zoom.
- `npm run check`, `cargo check` e `npx tauri build --debug --no-bundle` — validação padrão.

## 8. Créditos

O Dhruva Windows é inspirado no Dhruva original de NarkAgni. Mantenha o nome e os links do projeto original no README e nos créditos.
