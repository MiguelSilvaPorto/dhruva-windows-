# graphify.ps1 - ciclo de vida do watcher do Graphify numa worktree.
#
# O grafo (graphify-out/) e local e fica fora do git. O abrir.ps1 copia o
# snapshot da copia principal para a worktree e inicia o watcher; fechar.ps1 e
# limpar/limpar-agendada encerram o watcher antes de apagar a pasta.
#
# ASCII-only de proposito (PowerShell 5.1 le .ps1 UTF-8 sem BOM como ANSI).

function Get-GraphifyExe {
  $cmd = Get-Command graphify -ErrorAction SilentlyContinue
  if ($cmd -and $cmd.Source) { return $cmd.Source }
  $candidato = Join-Path $env:USERPROFILE '.local\bin\graphify.exe'
  if (Test-Path $candidato) { return $candidato }
  return $null
}

function Get-PidDoArquivo {
  param([string]$Caminho)
  if (-not (Test-Path $Caminho)) { return 0 }
  $texto = (Get-Content $Caminho -Raw -ErrorAction SilentlyContinue)
  $id = 0
  [void][int]::TryParse((("$texto") -replace '\s', ''), [ref]$id)
  return $id
}

function Iniciar-GraphifyWatch {
  param([Parameter(Mandatory = $true)][string]$Raiz)

  $grafoDir = Join-Path $Raiz 'graphify-out'
  if (-not (Test-Path (Join-Path $grafoDir 'graph.json'))) {
    return 'nao iniciado - graphify-out ausente'
  }

  $exe = Get-GraphifyExe
  if (-not $exe) {
    return 'nao iniciado - graphify fora do PATH'
  }

  $pidFile = Join-Path $grafoDir '.graphify_watch.pid'
  $idAntigo = Get-PidDoArquivo $pidFile
  if ($idAntigo -gt 0 -and (Get-Process -Id $idAntigo -ErrorAction SilentlyContinue)) {
    return "ja ativo (PID $idAntigo)"
  }
  Remove-Item $pidFile -Force -ErrorAction SilentlyContinue

  $out = Join-Path $grafoDir 'watch.out.log'
  $err = Join-Path $grafoDir 'watch.err.log'
  try {
    $env:PYTHONUNBUFFERED = '1'
    $argumentos = 'watch "{0}" --debounce 5' -f $Raiz
    $p = Start-Process -FilePath $exe `
      -ArgumentList $argumentos `
      -WorkingDirectory $Raiz -WindowStyle Hidden -PassThru `
      -RedirectStandardOutput $out -RedirectStandardError $err
    Start-Sleep -Milliseconds 1500
    if ($p.HasExited) {
      $detalhe = (Get-Content $err -ErrorAction SilentlyContinue | Select-Object -Last 1)
      if (-not $detalhe) { $detalhe = 'processo encerrou na largada' }
      return "nao iniciado - $detalhe"
    }
    Set-Content -Path $pidFile -Value $p.Id -Encoding ASCII
    return "ativo (PID $($p.Id))"
  } catch {
    return "falha ao iniciar - $($_.Exception.Message)"
  }
}

function Parar-GraphifyWatch {
  param([Parameter(Mandatory = $true)][string]$Raiz)

  $pidFile = Join-Path (Join-Path $Raiz 'graphify-out') '.graphify_watch.pid'
  $idGrafo = Get-PidDoArquivo $pidFile
  if ($idGrafo -le 0) {
    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
    return
  }

  $proc = Get-Process -Id $idGrafo -ErrorAction SilentlyContinue
  if ($proc) {
    $nomeProc = "$($proc.ProcessName)".ToLowerInvariant()
    $ehGraphify = ($nomeProc -eq 'graphify')
    $ehPython = ($nomeProc -eq 'python' -or $nomeProc -eq 'pythonw')
    if ($ehPython) {
      # Confirma que o python e o watcher (e nao um PID reaproveitado).
      $wmi = Get-CimInstance Win32_Process -Filter "ProcessId=$idGrafo" -ErrorAction SilentlyContinue
      if (-not $wmi -or "$($wmi.CommandLine)" -notmatch 'graphify' -or "$($wmi.CommandLine)" -notmatch '\bwatch\b') {
        $ehPython = $false
      }
    }
    if ($ehGraphify -or $ehPython) {
      # /T encerra tambem o python.exe filho que o shim do graphify cria.
      $saida = & taskkill /PID $idGrafo /T /F 2>&1
      if ($LASTEXITCODE -eq 0) {
        Write-Host "Watcher do graphify encerrado (PID $idGrafo)."
      }
    }
  }
  Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
}
