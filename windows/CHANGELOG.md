# Changelog — Dhruva Windows

## [Unreleased]

### Adicionado

- Templates de dock (macOS, Windows 11, COSMIC, Minimal) e de barra de pesquisa (Spotlight, COSMIC, Compacta, Minimal).
- Controles de animação: ligar/desligar, duração e suavidade do zoom.
- Controles de geometria da dock e da pesquisa (espaçamento, margem, cantos, largura, posição e desfoque).
- Licenciamento duplo **MIT OR Apache-2.0** para o port Windows.
- `AGENTS.md` com o fluxo de trabalho em worktrees, validação e convenções.
- Launcher com busca em janelas abertas, previews de mídia compactos e pastas na dock.

### Corrigido

- A tecla Windows não abre mais o menu Iniciar nativo; o foco volta ao aplicativo anterior ao fechar a pesquisa.
- Win+Q e Win+S abrem a pesquisa do Dhruva em vez da pesquisa nativa do Windows.
- Cliques repetidos na tecla Windows não dessincronizam mais o launcher: as alternâncias são processadas em ordem.
- Atalhos com Windows (Win+Tab, Ctrl+Win+D etc.) continuam funcionando mesmo com a tecla física bloqueada.

### Créditos

- Inspirado no **Dhruva** original para GNOME Shell, por **NarkAgni**:
  - https://github.com/NarkAgni/dhruva
  - https://extensions.gnome.org/extension/9495/dhruva/
