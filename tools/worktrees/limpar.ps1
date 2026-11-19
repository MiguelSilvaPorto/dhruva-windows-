#Requires -Version 5.1
# limpar.ps1 - Remove a worktree de uma tarefa e devolve o repo ao estado main limpa.
#
# Roda de qualquer pasta do repo (usa sempre a copia principal). Nao apaga nada
# que tenha alteracao nao commitada sem -Forcar.
#
# Uso:
#   tools\worktrees\limpar.ps1 -Nome feat/minha-tarefa
#   tools\worktrees\limpar.ps1 -Nome feat/minha-tarefa -ApagarBranch
#   tools\worktrees\limpar.ps1 -Nome feat/minha-tarefa -Forcar   (descarta pendencias)
#
# ASCII-only de proposito (PowerShell 5.1 le .ps1 UTF-8 sem BOM como ANSI).
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true, Position = 0)][string]$Nome,
  [switch]$ApagarBranch,
  [switch]$Forcar
)

$ErrorActionPreference = 'Stop'

# Ciclo de vida do watcher do Graphify (grafo local, fora do git).
. (Join-Path $PSScriptRoot 'graphify.ps1')

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

$atual = Exec -PermitirFalha { git rev-parse --show-toplevel }
$atual = "$atual".Trim()
if (-not $atual) { throw "Nao estou dentro de um repositorio git." }
$atual = $atual -replace '/', '\'
$principal = $atual -replace '\\\.worktrees\\.*$', ''

$branch = $Nome.Trim()
if ($branch -notmatch '/') { $branch = "feat/$branch" }
$slug = $branch -replace '/', '-'
$destino = Join-Path (Join-Path $principal '.worktrees') $slug

if ($atual -eq $destino) {
  throw "Voce esta com o terminal DENTRO da worktree que quer remover. Saia dela (cd $principal) e rode de novo."
}

if (-not (Test-Path $destino)) {
  Write-Host "Nao existe worktree em: $destino"
  $null = Exec -PermitirFalha { git -C $principal worktree prune }
  Write-Host "Worktrees atuais:"
  Exec { git -C $principal worktree list } | ForEach-Object { Write-Host "  $_" }
  exit 0
}

# -- watcher do graphify: encerra antes de remover a pasta --------------------
Parar-GraphifyWatch -Raiz $destino

# Remove junctions/reparse points de primeiro nivel antes do git worktree remove:
# no Windows, remocao recursiva pode seguir o link e apagar o CONTEUDO DO ALVO
# real (ex.: node_modules ou data/ da copia principal). Aqui removemos apenas o
# link - o alvo fica intacto.
Get-ChildItem $destino -Force | ForEach-Object {
  if ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
    if ($_.PSIsContainer) { [System.IO.Directory]::Delete($_.FullName, $false) }
    else { [System.IO.File]::Delete($_.FullName) }
    Write-Host "Junction removido (apenas o link; alvo intacto): $($_.Name)"
  }
}

$argsRemover = @('-C', $principal, 'worktree', 'remove', $destino)
if ($Forcar) { $argsRemover += '--force' }
$saida = Exec -PermitirFalha { & git @argsRemover }
if ($LASTEXITCODE -ne 0) {
  throw "git worktree remove falhou - provavelmente ha alteracoes nao commitadas ou algum terminal/processo aberto na pasta.`nDica: feche terminais nessa pasta; se as alteracoes podem ser descartadas, use -Forcar.`n--- saida ---`n$($saida | Out-String)"
}
Write-Host "Worktree removida: $destino"

if ($ApagarBranch) {
  $null = Exec { git -C $principal branch -D $branch } "Nao consegui apagar a branch local '$branch'."
  Write-Host "Branch local apagada: $branch (a do fork continua la, se foi publicada)."
} else {
  Write-Host "Branch local mantida: $branch (use -ApagarBranch para apagar)."
}

Write-Host "Worktrees atuais:"
Exec { git -C $principal worktree list } | ForEach-Object { Write-Host "  $_" }
