# Licenciamento / Licensing

Este repositório contém **duas obras distintas**. Elas não compartilham a mesma licença.

## 1. Extensão Dhruva para GNOME Shell (original)

Arquivos da extensão na raiz do repositório (`extension.js`, `prefs.js`, `src/`, `po/`, `schemas/`, `stylesheet.css`, `metadata.json`, `manage.sh` e afins).

- Projeto original: **Dhruva**, por **NarkAgni**
- GitHub: https://github.com/NarkAgni/dhruva
- Site oficial / instalação: https://extensions.gnome.org/extension/9495/dhruva/
- Licença: **GNU General Public License v3.0 ou posterior** (`GPL-3.0-or-later`), conforme o arquivo [`LICENSE`](LICENSE).
- Copyright (C) NarkAgni e contribuidores do projeto original.

Esses arquivos **permanecem em GPL-3.0-or-later**. O port para Windows não os relicencia.

## 2. Port Dhruva Windows (`windows/`)

Código original da dock nativa para Windows (Tauri 2 + Rust + Svelte 5), escrito para este projeto.

- Licença: **MIT OU Apache-2.0**, à sua escolha (`MIT OR Apache-2.0`).
- Textos: [`LICENSE-MIT`](LICENSE-MIT) e [`LICENSE-APACHE`](LICENSE-APACHE), com cópias em [`windows/`](windows/).
- Copyright (C) 2026 Miguel Silva Porto e contribuidores do Dhruva Windows.

A licença dupla vale para o código do port em `windows/`. Se você distribuir o repositório inteiro, os termos GPL-3.0-or-later continuam valendo para a extensão GNOME original, e os termos MIT/Apache-2.0 valem para o port Windows.

## Créditos

O port Windows é inspirado no **Dhruva** original para GNOME Shell, criado por **NarkAgni**:

- https://github.com/NarkAgni/dhruva
- https://extensions.gnome.org/extension/9495/dhruva/

Obrigado ao autor original por disponibilizar o Dhruva como software livre.
