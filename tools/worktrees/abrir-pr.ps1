#Requires -Version 5.1
# abrir-pr.ps1 - Abre um Pull Request no GitHub (branch da tarefa -> main do fork).
#
# ATENCAO: este script e do DESENVOLVEDOR (acao humana). A IA nao abre Pull
# Request: ela publica os commits na main do fork com o fechar.ps1 e entrega o
# comando pronto (contrato na secao 2 do AGENTS.md).
#
# Uso:
#   tools\worktrees\abrir-pr.ps1                         (branch atual -> main)
#   tools\worktrees\abrir-pr.ps1 -Branch feat/minha-tarefa
#   tools\worktrees\abrir-pr.ps1 -Titulo "..." -Descricao "..."
#   tools\worktrees\abrir-pr.ps1 -Rascunho               (abre como rascunho)
#   tools\worktrees\abrir-pr.ps1 -DryRun                 (so mostra, nao abre)
#
# A branch precisa estar publicada no origin. O fechar.ps1 publica direto na
# main do fork; para revisar uma branch antes, publique-a com:
#   git push -u origin feat/minha-tarefa
#
# ASCII-only de proposito (PowerShell 5.1 le .ps1 UTF-8 sem BOM como ANSI).
[CmdletBinding()]
param(
  [string]$Branch,
  [string]$Base = 'main',
  [string]$Repo,
  [string]$Titulo,
  [string]$Descricao,
  [switch]$Rascunho,
  [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

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

# Aceita https://github.com/dono/repo(.git) e git@github.com:dono/repo(.git).
function Get-GithubRepo {
  param([string]$Url)
  $u = "$Url".Trim() -replace '\.git/?$', '' -replace '/$', ''
  if ($u -match 'github\.com[:/]([^/]+)/([^/]+)$') {
    return "$($Matches[1])/$($Matches[2])"
  }
  return $null
}

$atual = Exec -PermitirFalha { git rev-parse --show-toplevel }
$atual = "$atual".Trim()
if (-not $atual) { throw "Nao estou dentro de um repositorio git." }
$principal = ($atual -replace '/', '\') -replace '\\\.worktrees\\.*$', ''

if (-not $Branch) {
  $Branch = "$(Exec { git -C $principal rev-parse --abbrev-ref HEAD })".Trim()
}
if (-not $Branch -or $Branch -eq 'HEAD') {
  throw "Nao consegui identificar a branch atual. Use -Branch <nome>."
}
if ($Branch -eq $Base) {
  throw "A branch atual e '$Base'. O PR precisa de uma branch de tarefa: crie a worktree com abrir.ps1 e publique com git push -u origin <branch>."
}

if (-not $Repo) {
  $origin = "$(Exec -PermitirFalha { git -C $principal remote get-url origin })".Trim()
  $Repo = Get-GithubRepo $origin
}
if (-not $Repo) {
  throw "Nao consegui identificar o repositorio do origin. Use -Repo dono/repo."
}

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
  throw "gh CLI nao encontrado no PATH. Instale o GitHub CLI e rode 'gh auth login'."
}

$noFork = Exec { git -C $principal ls-remote --heads origin $Branch }
if (-not "$noFork".Trim()) {
  throw "A branch '$Branch' nao existe no origin. Publique com: git push -u origin $Branch"
}

if (-not $Titulo) {
  $Titulo = Read-Host "Titulo do PR (Conventional Commits, ex: feat(escopo): resumo)"
}
if (-not $Descricao) {
  $Descricao = Read-Host "Descricao do PR (o que mudou e por que)"
}
if (-not "$Titulo".Trim()) { throw "Titulo obrigatorio." }

$parametros = @(
  'pr', 'create',
  '--repo', $Repo,
  '--head', $Branch,
  '--base', $Base,
  '--title', $Titulo,
  '--body', $Descricao
)
if ($Rascunho) { $parametros += '--draft' }

if ($DryRun) {
  Write-Host "DRY-RUN - nada foi criado. O comando que seria executado:"
  Write-Host ("  gh " + ($parametros -join ' '))
  exit 0
}

Write-Host "Abrindo Pull Request: $Repo ($Branch -> $Base)"
& gh @parametros
if ($LASTEXITCODE -ne 0) { throw "gh pr create falhou (confira a mensagem acima)." }
Write-Host ""
Write-Host "PR criado. Bom review!"
