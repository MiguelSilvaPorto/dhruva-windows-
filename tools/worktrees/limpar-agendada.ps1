#Requires -Version 5.1
# limpar-agendada.ps1 - Limpeza "teimosa": espera a pasta da worktree liberar
# (terminal/editor aberto nela), detecta QUEM esta segurando e roda a limpeza
# sozinha, para nenhuma tarefa deixar worktree orfa em .worktrees/.
#
# Normalmente e chamada EM SEGUNDO PLANO pelo fechar.ps1, logo depois do push.
# Tambem serve para limpar manualmente uma worktree - inclusive uma pasta
# "fantasma" que ficou presa no disco depois de uma limpeza interrompida.
#
# DONOS DA PASTA (evacuacao CONSERVADORA): quando a remocao falha, o script le
# o diretorio atual (CWD) de cada processo (direto do PEB) e so encerra os
# descartaveis do Dhruva - com match ESTRITO na linha de comando:
#   - node*                 vite/tauri/cdp/screencast/graphify-mcp
#   - dhruva*               executavel de teste dentro da worktree
#   - cargo*/rustc*         build com src-tauri/dhruva na linha de comando
# E ainda: so encerra DEPOIS DE CONFIRMAR em duas varreduras seguidas (mesmo
# PID, ainda segurando a pasta); nunca encerra processo iniciado depois do
# inicio desta limpeza; nunca toca no proprio processo.
# TUDO O MAIS E APENAS LISTADO NO LOG - editor, terminal, sessao, navegador e
# qualquer processo do usuario. Feche o processo listado que a limpeza conclui.
# Use -Simular para ver o que seria encerrado SEM encerrar nada, e
# -NaoEncerrarDonos para desligar a evacuacao por completo.
#
# Uso:
#   tools\worktrees\limpar-agendada.ps1 -Nome feat/minha-tarefa
#   tools\worktrees\limpar-agendada.ps1 -Nome feat-minha-tarefa       (nome da pasta/slug)
#   tools\worktrees\limpar-agendada.ps1 -Nome feat/minha-tarefa -Tentativas 120 -IntervaloSegundos 5
#   tools\worktrees\limpar-agendada.ps1 -Nome feat/minha-tarefa -Simular
#   tools\worktrees\limpar-agendada.ps1 -Nome feat/minha-tarefa -NaoEncerrarDonos
#
# Log: .worktrees\limpeza-automatica.log (na copia principal)
#
# ASCII-only de proposito (PowerShell 5.1 le .ps1 UTF-8 sem BOM como ANSI).
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true, Position = 0)][string]$Nome,
  [string]$Principal,
  [int]$Tentativas = 60,
  [int]$IntervaloSegundos = 10,
  [switch]$NaoEncerrarDonos,
  [switch]$Simular
)

$ErrorActionPreference = 'Continue'
$script:InicioLimpeza = Get-Date
$script:LeitorCarregado = $false
$script:Avisados = @{}
$script:Candidatos = @{}
$script:AvisouSemDono = $false

# -- localiza a copia principal ------------------------------------------------
if (-not $Principal) {
  $atual = & git rev-parse --show-toplevel 2>$null
  $atual = "$atual".Trim()
  if (-not $atual) { throw "Nao estou dentro de um repositorio git - rode da copia principal ou passe -Principal <caminho>." }
  $Principal = $atual
}
$Principal = ($Principal -replace '/', '\').TrimEnd('\')
# se apontaram para dentro de uma worktree, sobe para a copia principal
if ($Principal -match '\\\.worktrees\\') { $Principal = $Principal -replace '\\\.worktrees\\.*$', '' }

# -- resolve a pasta alvo: aceita a branch ("feat/x") OU o nome da pasta --------
$baseWt = Join-Path $Principal '.worktrees'
$nome = $Nome.Trim()
$destino = $null

if ($nome -match '^[A-Za-z0-9._-]+$') {
  # pode ser o nome exato da pasta (caso das pastas fantasma)
  $candidato = Join-Path $baseWt $nome
  if (Test-Path $candidato) { $destino = $candidato }
}
if (-not $destino) {
  $branch = $nome
  if ($branch -notmatch '/') { $branch = "feat/$branch" }
  $slug = $branch -replace '/', '-'
  if ($slug -notmatch '^[A-Za-z0-9._-]+$') { throw "Nome de worktree invalido: $Nome" }
  $destino = Join-Path $baseWt $slug
}
$destino = $destino.TrimEnd('\')
$script:Alvo = $nome

# -- watcher do graphify: encerra antes de tentar remover a pasta --------------
# O limpar-agendada pode rodar de uma copia no TEMP (agendada pelo fechar.ps1),
# entao o helper e carregado da copia principal, nao do $PSScriptRoot.
$helperGraphify = Join-Path (Join-Path $Principal 'tools\worktrees') 'graphify.ps1'
if (Test-Path $helperGraphify) { . $helperGraphify }
Parar-GraphifyWatch -Raiz $destino

$script:Log = Join-Path $baseWt 'limpeza-automatica.log'
$limpar = Join-Path $Principal 'tools\worktrees\limpar.ps1'

function Logar {
  param([string]$Texto)
  $linha = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')  $Texto"
  $utf8 = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::AppendAllText($script:Log, $linha + "`r`n", $utf8)
}

function PrimeiraLinha {
  param([string]$Texto)
  return ((("$Texto" -split "`n") | Where-Object { "$_".Trim() } | Select-Object -First 1) -join '').Trim()
}

# branch da worktree registrada nesta pasta (ou $null, se nao e worktree)
function BranchDaWorktree {
  $linhaAtual = $null
  foreach ($linha in @(& git -C $Principal worktree list --porcelain 2>$null)) {
    if ("$linha" -like 'worktree *') {
      $linhaAtual = "$linha".Substring(9).Trim() -replace '/', '\'
    } elseif ("$linha" -like 'branch *' -and $linhaAtual) {
      if ($linhaAtual -ieq $destino) { return ("$linha".Substring(7).Trim() -replace '^refs/heads/', '') }
      $linhaAtual = $null
    }
  }
  return $null
}

# apaga a branch local tentando as variantes do nome (feat-x -> feat/x etc.)
function ApagarBranchPorTentativa {
  param([string]$NomeBase)
  $variantes = @($NomeBase)
  if ($NomeBase -notmatch '/') {
    $variantes += "feat/$NomeBase"
    $pos = $NomeBase.IndexOf('-')
    if ($pos -gt 0) { $variantes += ($NomeBase.Substring(0, $pos) + '/' + $NomeBase.Substring($pos + 1)) }
  }
  foreach ($v in @($variantes | Select-Object -Unique)) {
    $null = & git -C $Principal branch -D $v 2>$null
  }
}

# -- le o CWD de cada processo (direto do PEB) e devolve quem esta na pasta -----
function DonosDaPasta {
  if (-not $script:LeitorCarregado) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class DhruvaCwdReader {
  [DllImport("kernel32.dll", SetLastError = true)]
  static extern IntPtr OpenProcess(int access, bool inherit, int pid);
  [DllImport("kernel32.dll")]
  static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")]
  static extern bool ReadProcessMemory(IntPtr h, IntPtr addr, byte[] buf, int size, out IntPtr lido);
  [DllImport("ntdll.dll")]
  static extern int NtQueryInformationProcess(IntPtr h, int cls, byte[] info, int len, out int ret);

  public static string GetCwd(int pid) {
    IntPtr h = OpenProcess(0x0410, false, pid);
    if (h == IntPtr.Zero) return null;
    try {
      byte[] pbi = new byte[48];
      int ret;
      if (NtQueryInformationProcess(h, 0, pbi, pbi.Length, out ret) != 0) return null;
      IntPtr peb = (IntPtr)BitConverter.ToInt64(pbi, 8);
      IntPtr lido;
      byte[] b8 = new byte[8];
      if (!ReadProcessMemory(h, (IntPtr)(peb.ToInt64() + 0x20), b8, 8, out lido)) return null;
      IntPtr pp = (IntPtr)BitConverter.ToInt64(b8, 0);
      if (!ReadProcessMemory(h, (IntPtr)(pp.ToInt64() + 0x38), b8, 2, out lido)) return null;
      int len = BitConverter.ToUInt16(b8, 0);
      if (len <= 0 || len > 4096) return null;
      if (!ReadProcessMemory(h, (IntPtr)(pp.ToInt64() + 0x40), b8, 8, out lido)) return null;
      IntPtr strBuf = (IntPtr)BitConverter.ToInt64(b8, 0);
      byte[] sb = new byte[len];
      if (!ReadProcessMemory(h, strBuf, sb, len, out lido)) return null;
      return Encoding.Unicode.GetString(sb);
    } catch { return null; } finally { CloseHandle(h); }
  }
}
'@
    $script:LeitorCarregado = $true
  }

  $prefixo = $destino + '\'
  $lista = @()
  foreach ($proc in (Get-Process -ErrorAction SilentlyContinue)) {
    if ($proc.Id -eq $PID) { continue }
    $cwd = $null
    try { $cwd = [DhruvaCwdReader]::GetCwd($proc.Id) } catch { }
    if (-not $cwd) { continue }
    $normal = $cwd.TrimEnd('\')
    if (($normal -ieq $destino) -or $normal.StartsWith($prefixo, [System.StringComparison]::OrdinalIgnoreCase)) {
      $idade = -1
      try { $idade = [int]((Get-Date) - $proc.StartTime).TotalMinutes } catch { }
      $lista += [pscustomobject]@{ Id = $proc.Id; Nome = $proc.ProcessName; IdadeMin = $idade; Iniciado = $proc.StartTime }
    }
  }
  return $lista
}

# -- evacuacao conservadora: so processos de teste do Dhruva, com confirmacao dupla --
function EncerrarDonos {
  param([array]$Donos)
  if (-not $Donos -or $Donos.Count -eq 0) {
    if (-not $script:AvisouSemDono) {
      $script:AvisouSemDono = $true
      Logar "SEM DONO  $script:Alvo : nao achei processo com o CWD dentro da pasta; se persistir, veja algum arquivo aberto por editor dentro dela."
    }
    return
  }

  # a linha de comando e o que da CERTEZA da classificacao (vite/dhruva/cargo)
  $cmdlines = @{}
  foreach ($c in (Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)) { $cmdlines[[int]$c.ProcessId] = "$($c.CommandLine)" }

  foreach ($dono in $Donos) {
    $chave = "$($dono.Id)-$($dono.Iniciado.Ticks)"
    $nomeProc = "$($dono.Nome)".ToLower()
    $cmd = "$($cmdlines[[int]$dono.Id])"
    $resumo = $cmd
    if ($resumo.Length -gt 120) { $resumo = $resumo.Substring(0, 120) + '...' }

    # classificacao ESTRITA: qualquer duvida = nao encerra
    $tipo = $null
    if ($nomeProc -eq 'node' -and $cmd -match '(vite|tauri|graphify-mcp|cdp\.mjs|screencast)') {
      $tipo = 'servidor/ferramenta Node do Dhruva'
    } elseif ($nomeProc -eq 'dhruva') {
      $tipo = 'Dhruva de teste (worktree)'
    } elseif ($nomeProc -match '^(cargo|rustc)$' -and $cmd -match '(src-tauri|dhruva)') {
      $tipo = 'build Rust do Dhruva'
    }

    if (-not $tipo) {
      # NUNCA encerra editor, terminal, sessao ou o agente do usuario.
      if (-not $script:Avisados.ContainsKey($chave)) {
        $script:Avisados[$chave] = $true
        Logar "MANUAL    $script:Alvo : PID $($dono.Id) ($($dono.Nome)) segura a pasta - nao encerro. Feche esse processo que a limpeza conclui."
      }
      continue
    }

    $nascidoAntes = $true
    try { $nascidoAntes = ($dono.Iniciado -lt $script:InicioLimpeza) } catch { }
    if (-not $nascidoAntes) {
      if (-not $script:Avisados.ContainsKey("$chave-novo")) {
        $script:Avisados["$chave-novo"] = $true
        Logar "MANUAL    $script:Alvo : PID $($dono.Id) ($($dono.Nome)) e mais novo que esta limpeza - nao encerro (algo abriu a pasta agora)."
      }
      continue
    }

    # primeira vez que ve: apenas candidata; encerra so na CONFIRMACAO
    if (-not $script:Candidatos.ContainsKey($chave)) {
      $script:Candidatos[$chave] = (Get-Date)
      Logar "CANDIDATO $script:Alvo : PID $($dono.Id) ($tipo) segura ha ~$($dono.IdadeMin) min - encerro so se continuar na proxima varredura. cmd: $resumo"
      continue
    }

    if ($Simular) {
      Logar "SIMULACAO $script:Alvo : encerraria PID $($dono.Id) ($tipo) - confirmado 2x, mas -Simular esta ligado. cmd: $resumo"
      continue
    }

    $saida = & taskkill /PID $dono.Id /T /F 2>&1
    if ($LASTEXITCODE -eq 0) {
      Logar "DESCARTE  $script:Alvo : encerrei PID $($dono.Id) ($tipo), confirmado 2x (~$($dono.IdadeMin) min segurando). cmd: $resumo"
      $script:Candidatos.Remove($chave)
    } else {
      Logar "ATENCAO   $script:Alvo : taskkill falhou para PID $($dono.Id) ($tipo): $(PrimeiraLinha ("$($saida -join ' ')"))"
    }
  }
}

if (-not (Test-Path $baseWt)) {
  New-Item -ItemType Directory -Path $baseWt -Force | Out-Null
}

Logar "INICIO  $script:Alvo : limpeza agendada (destino: $destino)"

for ($i = 1; $i -le $Tentativas; $i++) {
  if (-not (Test-Path $destino)) {
    Logar "OK  $script:Alvo : pasta ja nao existia"
    exit 0
  }

  $branchReal = BranchDaWorktree
  $erro = ''

  if ($branchReal) {
    # worktree de verdade: nunca descarta alteracao nao commitada sozinha
    $sujo = @(& git -C $destino status --porcelain 2>$null | Where-Object { "$_".Trim() })
    if ($sujo.Count -gt 0) {
      Logar "PENDENTE  $branchReal : worktree com alteracao nao commitada; nao removo sozinho. Se puder descartar: limpar.ps1 -Nome $branchReal -Forcar"
      exit 1
    }
    try {
      $null = & $limpar -Nome $branchReal -ApagarBranch 2>&1
      if (Test-Path $destino) { throw "limpar.ps1 terminou mas a pasta continua no disco." }
      Logar "OK  $branchReal : worktree removida e branch local apagada"
      exit 0
    } catch {
      $erro = PrimeiraLinha $_.Exception.Message
    }
  } else {
    # pasta "fantasma": o git nao a conhece mais (limpeza antiga interrompida).
    # So remove se nao houver conteudo real; junctions saem como link, nunca o alvo.
    try {
      $itens = @(Get-ChildItem $destino -Force -ErrorAction Stop)
      $temConteudoReal = $false
      foreach ($it in $itens) {
        if (-not ($it.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { $temConteudoReal = $true; break }
      }
      if ($temConteudoReal) {
        Logar "PENDENTE  $script:Alvo : pasta sem registro no git, mas com conteudo real - confira antes de apagar."
        exit 1
      }
      foreach ($it in $itens) {
        if ($it.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
          if ($it.PSIsContainer) { [System.IO.Directory]::Delete($it.FullName, $false) }
          else { [System.IO.File]::Delete($it.FullName) }
        }
      }
      Remove-Item $destino -Recurse -Force -ErrorAction Stop
      ApagarBranchPorTentativa $script:Alvo
      Logar "OK  $script:Alvo : pasta fantasma removida (nao era mais worktree registrada)"
      exit 0
    } catch {
      $erro = PrimeiraLinha $_.Exception.Message
    }
  }

  # falhou: registra (menos verboso) e, a partir da 2a tentativa, descobre quem
  # segura; candidatos so sao encerrados na varredura seguinte (confirmacao)
  if ($i -eq 1 -or ($i % 6) -eq 0) { Logar "tentativa $i/$Tentativas  $script:Alvo : $erro" }
  if (-not $NaoEncerrarDonos -and $i -ge 2 -and (($i -le 4) -or (($i % 5) -eq 0))) {
    try { EncerrarDonos -Donos (DonosDaPasta) } catch { }
  }

  if ($i -lt $Tentativas) { Start-Sleep -Seconds $IntervaloSegundos }
}

Logar "PENDENTE  $script:Alvo : nao consegui remover apos $Tentativas tentativa(s) - pasta em uso? Feche terminais/editores nela e rode de novo: tools\worktrees\limpar-agendada.ps1 -Nome $script:Alvo"
exit 1
