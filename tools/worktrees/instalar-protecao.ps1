#Requires -Version 5.1
# instalar-protecao.ps1 - Instala o hook que bloqueia commit direto na branch main.
#
# Opcional, mas recomendado com varias IAs no mesmo repositorio. Roda de qualquer
# pasta do repo. O hook vale para TODAS as worktrees deste clone.
# Para desinstalar: apague .git\hooks\pre-commit.
#
# Uso: tools\worktrees\instalar-protecao.ps1
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

$atual = Exec -PermitirFalha { git rev-parse --show-toplevel }
$atual = "$atual".Trim()
if (-not $atual) { throw "Nao estou dentro de um repositorio git." }
$principal = ($atual -replace '/', '\') -replace '\\\.worktrees\\.*$', ''

$origem = Join-Path $principal 'tools\worktrees\hooks\pre-commit'
if (-not (Test-Path $origem)) { throw "Nao encontrei o hook em: $origem" }

# O diretorio de hooks e compartilhado por todas as worktrees (git-common-dir).
$gitCommon = "$(Exec { git -C $principal rev-parse --git-common-dir })".Trim()
if (-not [System.IO.Path]::IsPathRooted($gitCommon)) {
  $gitCommon = Join-Path $principal $gitCommon
}
$destino = Join-Path (Join-Path $gitCommon 'hooks') 'pre-commit'

if (Test-Path $destino) {
  Write-Host "Ja existe um hook em: $destino"
  Write-Host "Substituindo pelo hook de protecao da main..."
}
Copy-Item $origem $destino -Force
Write-Host "Hook instalado: $destino"
Write-Host ""
Write-Host "A partir de agora, commit direto na branch main e bloqueado (neste clone inteiro,"
Write-Host "inclusive dentro das worktrees). Para desinstalar: apague o arquivo acima."
