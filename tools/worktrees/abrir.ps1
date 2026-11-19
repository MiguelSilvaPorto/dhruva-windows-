#Requires -Version 5.1
# abrir.ps1 - Cria a worktree isolada de uma tarefa (fluxo AGENTS.md secao 2).
#
# Uso:
#   tools\worktrees\abrir.ps1 feat/minha-tarefa
#   tools\worktrees\abrir.ps1 fix/outro-bug -SemDeps
#   tools\worktrees\abrir.ps1 feat/tarefa-antiga -Reabrir   (branch que ja existe)
#
# Roda do repositorio principal (o script recusa rodar de dentro de uma worktree).
# A copia principal NUNCA sai da branch main.
#
# ASCII-only de proposito: o PowerShell 5.1 le arquivos .ps1 UTF-8 sem BOM como
# ANSI - acento em script vira mojibake. Nao "corrija" isso.
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true, Position = 0)][string]$Nome,
  [switch]$Reabrir,
  [switch]$SemDeps
)

$ErrorActionPreference = 'Stop'

# Ciclo de vida do watcher do Graphify (grafo local, fora do git).
. (Join-Path $PSScriptRoot 'graphify.ps1')

# Executa um comando externo sem morrer com stderr do PS 5.1 e checa o exit code.
function Exec {
  param(
    [Parameter(Mandatory = $true)][scriptblock]$Cmd,
    [string]$Erro = 'Comando externo falhou.',
    [switch]$PermitirFalha
  )
  $ErrorActionPreference = 'Continue'
  $saida = & $Cmd 2>&1 | ForEach-Object {
    if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.ToString() } else { "$_" }
  }
  if ($LASTEXITCODE -ne 0 -and -not $PermitirFalha) {
    throw "$Erro`n--- saida do comando ---`n$($saida | Out-String)"
  }
  return $saida
}

# -- localiza o repositorio principal -----------------------------------------
$atual = Exec -PermitirFalha { git rev-parse --show-toplevel }
$atual = "$atual".Trim()
if (-not $atual) { throw "Nao estou dentro de um repositorio git." }
$atual = $atual -replace '/', '\'
if ($atual -match '\\\.worktrees\\') {
  throw "Voce esta DENTRO de uma worktree ($atual). Trabalhe nela, ou rode este script fora dela."
}
$principal = $atual
$raizWorktrees = Join-Path $principal '.worktrees'

# -- normaliza e valida o nome da branch --------------------------------------
$branch = $Nome.Trim()
if ($branch -notmatch '/') { $branch = "feat/$branch" }
if ($branch -notmatch '^(feat|fix|docs|refactor|chore|test|perf)/[a-z0-9][a-z0-9._-]*$') {
  throw "Nome de branch invalido: '$branch'. Use minusculas, sem espaco/acento, no padrao feat/alguma-coisa. Prefixos: feat, fix, docs, refactor, chore, test, perf."
}
$slug = $branch -replace '/', '-'
$destino = Join-Path $raizWorktrees $slug

if (Test-Path $destino) { throw "Ja existe uma worktree em: $destino" }

# -- checagens da copia principal ---------------------------------------------
$branchPrincipal = "$(Exec { git -C $principal symbolic-ref --short HEAD })".Trim()
if ($branchPrincipal -ne 'main') {
  throw "A copia principal esta na branch '$branchPrincipal' - ela deve ficar sempre na main. Resolva isso antes de criar uma worktree."
}
$sujo = Exec { git -C $principal status --porcelain } | Where-Object {
  $_ -notmatch '^\?\? \.(worktrees|claude/worktrees)/'
}
if ($sujo) {
  throw "A copia principal tem alteracoes nao commitadas. Resolva antes (a worktree so nasce de uma main limpa):`n$($sujo | Out-String)"
}

# -- base: main do fork recem-baixada (origin/main) ----------------------------
# A tarefa nasce de cima do que a main do fork JA tem (inclui os proprios
# scripts e o conteudo de PRs abertos); sem depender das credenciais do
# upstream a cada tarefa. O fechar.ps1 sempre integra a main do fork antes de
# publicar.
Write-Host "Baixando a main do fork (origin)..."
$null = Exec { git -C $principal fetch origin main --quiet } "Falha ao baixar a main do fork (origin/main). Confira a conexao com o GitHub."
$base = 'origin/main'

New-Item -ItemType Directory -Force $raizWorktrees | Out-Null

if ($Reabrir) {
  # Reanexa uma branch que ja existe (localmente ou no fork) - ex.: resolver conflito do PR.
  $temLocal = Exec -PermitirFalha { git -C $principal rev-parse --verify --quiet "refs/heads/$branch" }
  if (-not "$temLocal".Trim()) {
    Write-Host "Branch '$branch' nao existe localmente; buscando do fork (origin)..."
    $null = Exec { git -C $principal fetch origin "${branch}:refs/heads/$branch" --quiet } "A branch '$branch' nao existe nem local nem no fork (origin)."
  }
  $null = Exec { git -C $principal worktree add $destino $branch } "git worktree add falhou para a branch '$branch'."
} else {
  $temLocal = Exec -PermitirFalha { git -C $principal rev-parse --verify --quiet "refs/heads/$branch" }
  if ("$temLocal".Trim()) { throw "A branch '$branch' ja existe localmente. Use -Reabrir para reanexa-la numa worktree." }
  $temRemoto = Exec { git -C $principal ls-remote --heads origin $branch }
  if ("$temRemoto".Trim()) { throw "A branch '$branch' ja existe no fork (origin). Escolha outro nome ou use -Reabrir." }
  $null = Exec { git -C $principal worktree add -b $branch $destino $base } "git worktree add falhou."
  # Evita push acidental para o upstream: o tracking so e definido no fechar.ps1 (origin).
  $null = Exec -PermitirFalha { git -C $principal branch --unset-upstream $branch }
}

# -- node_modules por junction (economiza um npm install por worktree) --------
$deps = 'nao ligado - rode npm install na worktree se precisar'
if (-not $SemDeps) {
  $nm = Join-Path $principal 'node_modules'
  if (Test-Path $nm) {
    try {
      New-Item -ItemType Junction -Path (Join-Path $destino 'node_modules') -Target $nm | Out-Null
      $deps = 'node_modules ligado a copia principal (junction). Se a tarefa mexer no package.json, remova o junction e rode npm install'
    } catch {
      $deps = 'falha ao criar o junction de node_modules - rode npm install na worktree'
    }
  }
}

# -- graphify: leva o grafo local para a worktree e inicia o watcher ----------
# graphify-out/ e ignorado pelo git; sem esta copia a worktree nasce sem grafo
# e as IAs nao conseguem consultar. A copia e um snapshot da copia principal;
# o watcher mantem so esta worktree atualizada ("graphify update ." tambem
# pode ser rodado na mao).
$grafo = 'nao copiado - a copia principal ainda nao tem graphify-out'
$origemGrafo = Join-Path $principal 'graphify-out'
if (Test-Path (Join-Path $origemGrafo 'graph.json')) {
  $destinoGrafo = Join-Path $destino 'graphify-out'
  try {
    New-Item -ItemType Directory -Force $destinoGrafo | Out-Null
    foreach ($arquivo in @(
        'graph.json', 'GRAPH_REPORT.md', 'manifest.json',
        '.graphify_labels.json', '.graphify_labels.json.sig',
        '.graphify_analysis.json', '.graphify_build.json', '.graphify_root'
      )) {
      $origemArquivo = Join-Path $origemGrafo $arquivo
      if (Test-Path $origemArquivo) {
        Copy-Item $origemArquivo (Join-Path $destinoGrafo $arquivo) -Force
      }
    }
    $origemCache = Join-Path $origemGrafo 'cache'
    if (Test-Path $origemCache) {
      Copy-Item $origemCache (Join-Path $destinoGrafo 'cache') -Recurse -Force
    }
    $grafo = Iniciar-GraphifyWatch -Raiz $destino
  } catch {
    $grafo = 'falha ao copiar graphify-out - rode graphify extract . --code-only na worktree'
  }
}

$commitBase = "$(Exec { git -C $principal rev-parse --short $base })".Trim()
Write-Host ""
Write-Host "OK - worktree pronta:"
Write-Host "  branch : $branch"
Write-Host "  pasta  : $destino"
Write-Host "  base   : $base (commit $commitBase)"
Write-Host "  deps   : $deps"
Write-Host "  grafo  : $grafo"
Write-Host ""
Write-Host "Trabalhe DENTRO dessa pasta (nunca na copia principal)."
Write-Host "Para fechar a tarefa, de dentro da worktree: tools\worktrees\fechar.ps1 -Mensagem `"...`""
