# Changelog — Dhruva Windows

## [Unreleased]

### Adicionado

- Templates de dock (macOS, Windows 11, COSMIC, Minimal) e de barra de pesquisa (Spotlight, COSMIC, Compacta, Minimal).
- Controles de animação: ligar/desligar, duração e suavidade do zoom.
- Controles de geometria da dock e da pesquisa (espaçamento, margem, cantos, largura, posição e desfoque).
- Licenciamento duplo **MIT OR Apache-2.0** para o port Windows.
- `AGENTS.md` com o fluxo de trabalho em worktrees, validação e convenções.
- Launcher com busca em janelas abertas, previews de mídia compactos e pastas na dock.
- Prévia ao vivo da mídia: clicar na capa abre um quadro ao vivo do vídeo; em navegadores, a aba correspondente é ativada e restaurada ao fechar.
- Controle de volume e mudo por aplicativo no painel de mídia, independente do volume do Windows.
- Controle de tempo preciso: sincroniza a posição com o player do navegador, permite arrastar a barra e pular 10 segundos.

### Corrigido

- A dock volta a reservar a área da tela ao ser exibida e depois do desbloqueio; janelas maximizadas não ficam mais por baixo dela.
- Passar o mouse pela prévia de uma janela não traz mais o aplicativo para frente; somente o clique ativa a janela.
- A prévia de mídia volta a encontrar a janela do vídeo mesmo quando o Windows entrega a sessão sem título; o painel usa o título/aplicativo que está exibindo como referência.
- A prévia de mídia usa uma miniatura DWM ao vivo (GPU) em vez de capturas de tela repetidas; se a miniatura não estiver disponível, mostra um quadro estático.
- A tecla Windows não abre mais o menu Iniciar nativo; o foco volta ao aplicativo anterior ao fechar a pesquisa.
- Win+Q e Win+S abrem a pesquisa do Dhruva em vez da pesquisa nativa do Windows.
- Cliques repetidos na tecla Windows não dessincronizam mais o launcher: as alternâncias são processadas em ordem.
- Atalhos com Windows (Win+Tab, Ctrl+Win+D etc.) continuam funcionando mesmo com a tecla física bloqueada.

### Créditos

- Inspirado no **Dhruva** original para GNOME Shell, por **NarkAgni**:
  - https://github.com/NarkAgni/dhruva
  - https://extensions.gnome.org/extension/9495/dhruva/
