#Requires -Version 5.1
# status.ps1 - Painel do fluxo: worktrees ativas, posicao da main do fork vs
# upstream, branches locais de tarefa e Pull Requests abertos. Somente leitura.
#
# Uso: tools\worktrees\status.ps1
#
# ASCII-only de proposito (PowerShell 5.1 le .ps1 UTF-8 sem BOM como ANSI).
[CmdletBinding()]
param()

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

Write-Host "== Worktrees =="
Exec { git -C $principal worktree list } | ForEach-Object { Write-Host "  $_" }

Write-Host ""
Write-Host "== Main do fork (origin/main) vs upstream =="
$null = Exec -PermitirFalha { git -C $principal fetch origin --quiet }
$null = Exec -PermitirFalha { git -C $principal fetch upstream --quiet }
$cont = Exec -PermitirFalha { git -C $principal rev-list --left-right --count 'upstream/main...origin/main' }
if ("$cont".Trim()) {
  $partes = "$cont".Trim() -split '\s+'
  Write-Host ("  origin/main esta {0} commit(s) a frente e {1} atras do upstream/main" -f $partes[1], $partes[0])
  Write-Host "  (upstream = projeto original; o port Windows vive na main do fork)"
} else {
  Write-Host "  (nao consegui calcular - alguma ref nao existe?)"
}

Write-Host ""
Write-Host "== Branches locais de tarefa =="
$branches = @(Exec -PermitirFalha { git -C $principal branch --list 'feat/*' 'fix/*' 'docs/*' 'refactor/*' 'chore/*' 'test/*' 'perf/*' } | Where-Object { "$_".Trim() })
if ($branches.Count -gt 0) {
  $branches | ForEach-Object { Write-Host "  $_" }
  Write-Host "  (branches locais da worktree - nao existem no GitHub; o push vai para a main do fork)"
} else {
  Write-Host "  (nenhuma)"
}

Write-Host ""
Write-Host "== Limpeza pendente =="
$pastaWt = Join-Path $principal '.worktrees'
$registradas = @()
$linhasWt = Exec -PermitirFalha { git -C $principal worktree list --porcelain }
foreach ($linha in @($linhasWt)) {
  if ("$linha" -like 'worktree *') {
    $caminho = "$linha".Substring(9).Trim() -replace '/', '\'
    if ($caminho -ine $principal) { $registradas += $caminho }
  }
}
$pendencias = @()
foreach ($caminho in $registradas) {
  if (Test-Path $caminho) {
    $sujo = @(& git -C $caminho status --porcelain 2>$null | Where-Object { "$_".Trim() })
    $null = & git -C $caminho merge-base --is-ancestor HEAD origin/main 2>$null
    if ($sujo.Count -eq 0 -and $LASTEXITCODE -eq 0) {
      $pendencias += "PODE LIMPAR : $(Split-Path $caminho -Leaf)  (limpa e publicada; o fechar.ps1 ja removeria sozinho)"
    }
  }
}
if (Test-Path $pastaWt) {
  foreach ($d in @(Get-ChildItem $pastaWt -Directory -Force -ErrorAction SilentlyContinue)) {
    $ehRegistrada = $false
    foreach ($c in $registradas) { if ($c -ieq $d.FullName) { $ehRegistrada = $true } }
    if (-not $ehRegistrada) {
      $pendencias += "FANTASMA    : $($d.Name)  (pasta sem registro no git; limpar-agendada.ps1 -Nome $($d.Name) resolve)"
    }
  }
}
if ($pendencias.Count -gt 0) { $pendencias | ForEach-Object { Write-Host "  $_" } } else { Write-Host "  (nada pendente - tudo limpo)" }

Write-Host ""
$originUrl = "$(Exec -PermitirFalha { git -C $principal remote get-url origin })".Trim()
$repo = Get-GithubRepo $originUrl
Write-Host "== Pull Requests abertos ($repo) =="
if ($repo -and (Get-Command gh -ErrorAction SilentlyContinue)) {
  & gh pr list --repo $repo --state open --limit 20
} elseif (-not $repo) {
  Write-Host "  (nao consegui identificar o repositorio do origin)"
} else {
  Write-Host "  (gh CLI nao encontrado no PATH - instale e rode 'gh auth login')"
}
