#Requires -Version 5.1
# fechar.ps1 - Fecha a tarefa: commit do que faltar, integra a main do fork e
# publica os commits direto na MAIN do fork (origin). NAO cria branch no GitHub.
#
# Roda DE DENTRO da worktree (.worktrees\...).
#
# CONFLITO: se a main do fork tiver andado e der conflito ao integrar, o script
# PARA, lista os arquivos em conflito e deixa o merge EM ANDAMENTO nesta
# worktree - a IA (ou o dev) resolve, roda "git add" e executa este script de
# novo para concluir e publicar. Nada e publicado sem o conflito resolvido.
#
# IMPORTANTE: nao abre Pull Request. Isso e acao humana do desenvolvedor
# (contrato na secao 2 do AGENTS.md).
#
# Uso:
#   tools\worktrees\fechar.ps1 -Mensagem "feat(escopo): descricao"
#   tools\worktrees\fechar.ps1              (sem pendencia: so integra e publica)
#   tools\worktrees\fechar.ps1 -SemPush     (commita/integra, sem publicar)
#   tools\worktrees\fechar.ps1 -ManterWorktree   (nao agenda a limpeza automatica)
#   tools\worktrees\fechar.ps1 -ParadaHoras 24   (aviso de outras worktrees paradas; padrao 12h)
#
# LIMPEZA: depois de publicar, ele AGENDA em segundo plano a remocao desta
# worktree e da branch local - um processo separado espera a pasta liberar
# (terminal/editor aberto nela) e roda a limpeza. Log em
# .worktrees\limpeza-automatica.log. Assim nenhuma tarefa deixa worktree orfa.
#
# ASCII-only de proposito (PowerShell 5.1 le .ps1 UTF-8 sem BOM como ANSI).
[CmdletBinding()]
param(
  [string]$Mensagem,
  [switch]$SemPush,
  [switch]$ManterWorktree,
  [int]$ParadaHoras = 12
)

$ErrorActionPreference = 'Stop'

# Evita travar indefinidamente em fetch/push se o GitHub nao responder:
# aborta a transferencia HTTP depois de 30s abaixo de 1 KB/s.
$env:GIT_HTTP_LOW_SPEED_LIMIT = '1000'
$env:GIT_HTTP_LOW_SPEED_TIME = '30'

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

# -- confirma que estamos numa worktree de tarefa ------------------------------
$raiz = Exec -PermitirFalha { git rev-parse --show-toplevel }
$raiz = "$raiz".Trim()
if (-not $raiz) { throw "Nao estou dentro de um repositorio git." }
$raiz = $raiz -replace '/', '\'
if ($raiz -notmatch '\\\.worktrees\\') {
  throw "Este script roda DE DENTRO de uma worktree (.worktrees\...). Aqui estamos em: $raiz"
}
$branch = Exec { git -C $raiz symbolic-ref --short HEAD }
$branch = "$branch".Trim()
if ($branch -eq 'main') { throw "A branch atual e a main - isso nao deveria acontecer numa worktree." }

# -- watcher do graphify: encerra antes de commitar/limpar --------------------
Parar-GraphifyWatch -Raiz $raiz

# -- 0) conflito nao resolvido? (o script devolve o controle para resolver) ----
# (filtra avisos do git, ex. "LF will be replaced by CRLF" - aviso nao e conflito)
$naoResolvidos = @(Exec -PermitirFalha { git -C $raiz diff --name-only --diff-filter=U } |
  Where-Object { "$_".Trim() -and ("$_" -notmatch '^\s*warning:') -and ("$_" -notmatch 'not a valid attribute name') })
if ($naoResolvidos.Count -gt 0) {
  throw "Ainda ha conflitos NAO RESOLVIDOS nesta worktree:`n$($naoResolvidos | Out-String)`nResolva os arquivos, rode: git add <arquivos> (ou git add -A), e execute este script de novo para concluir e publicar."
}

# -- 1) commit do que faltar (ou conclusao de um merge em andamento) -----------
$null = Exec -PermitirFalha { git -C $raiz rev-parse --verify --quiet MERGE_HEAD }
$mergeEmAndamento = ($LASTEXITCODE -eq 0)
$mudancas = Exec { git -C $raiz status --porcelain }
if ($mudancas) {
  $msgCommit = $Mensagem
  if (-not $msgCommit -and $mergeEmAndamento) { $msgCommit = 'merge: integra a main do fork' }
  if (-not $msgCommit) {
    throw "Ha alteracoes sem commit. Rode de novo passando a mensagem: fechar.ps1 -Mensagem `"feat(escopo): descricao`""
  }
  if ($msgCommit -notmatch '^(feat|fix|docs|refactor|chore|test|perf)(\([a-z0-9._-]+\))?: .+' -and $msgCommit -notmatch '^merge: ') {
    Write-Warning "Mensagem fora do padrao Conventional Commits (vai assim mesmo): $msgCommit"
  }
  $null = Exec { git -C $raiz add -A } "git add falhou."
  $null = Exec { git -C $raiz commit -m $msgCommit } "git commit falhou."
  Write-Host "Commit criado: $msgCommit"
} else {
  Write-Host "Nada pendente para commitar."
}

# -- 2+3) integra a main do fork e publica (com uma retentativa) ---------------
$publicado = $false
for ($tentativa = 1; $tentativa -le 2 -and -not $publicado; $tentativa++) {
  $null = Exec { git -C $raiz fetch origin main --quiet } "Falha ao baixar origin/main. Confira a conexao com o GitHub."

  $null = Exec -PermitirFalha { git -C $raiz merge-base --is-ancestor origin/main HEAD }
  if ($LASTEXITCODE -ne 0) {
    Write-Host "A main do fork avancou - integrando (git merge origin/main)..."
    $null = Exec -PermitirFalha { git -C $raiz merge origin/main -m "merge: integra a main do fork" }
    if ($LASTEXITCODE -ne 0) {
      $conflitos = Exec -PermitirFalha { git -C $raiz diff --name-only --diff-filter=U }
      $conflitos = @($conflitos) | Where-Object { "$_".Trim() -and ("$_" -notmatch 'not a valid attribute name') -and ("$_" -notmatch '^\s*warning:') }
      throw "CONFLITO ao integrar a main do fork - o merge ficou EM ANDAMENTO nesta worktree.`nArquivos em conflito:`n$($conflitos | Out-String)`nResolva os arquivos, rode: git add <arquivos> (ou git add -A), e execute este script de novo para concluir e publicar. Nada foi publicado."
    }
    Write-Host "Main do fork integrada."
  }

  if ($SemPush) {
    Write-Host "Push pulado (-SemPush)."
    $publicado = $true
    break
  }

  $saidaPush = Exec -PermitirFalha { git -C $raiz push origin HEAD:main }
  if ($LASTEXITCODE -eq 0) {
    $publicado = $true
    Write-Host "Commits publicados na main do fork (origin/main)."
    $linhas = @($saidaPush | ForEach-Object { ("$_" -replace '://[^@\s]+@', '://***@') } | Where-Object { "$_".Trim() })
    if ($linhas) { $linhas | ForEach-Object { Write-Host "  $_" } }
  } elseif ($tentativa -lt 2) {
    Write-Host "Push rejeitado (a main do fork avancou de novo) - integrando e publicando outra vez..."
  } else {
    throw "git push para a main do fork falhou:`n$($saidaPush | Out-String)"
  }
}

# -- 4) resumo + proximo passo (o PR e HUMANO) ---------------------------------
$ultimos = Exec -PermitirFalha { git -C $raiz log --oneline -3 }
Write-Host ""
Write-Host "Ultimos commits da tarefa:"
if ($ultimos) { $ultimos | ForEach-Object { Write-Host "  $_" } } else { Write-Host "  (nenhum)" }

if (-not $SemPush) {
  Write-Host ""
  Write-Host "=== PROXIMO PASSO (HUMANO): Pull Request (opcional) ==="
  Write-Host "  O push foi para a MAIN do fork - nenhuma branch nova foi criada no GitHub."
  Write-Host "  Pull Request e acao humana e nao faz parte deste fluxo (secao 2 do AGENTS.md)."
  Write-Host "  Para revisar uma branch antes de ir para a main, publique-a e use tools\worktrees\abrir-pr.ps1."
}
# -- 5) limpeza automatica: nao deixar worktree orfa ---------------------------
Write-Host ""
if ($publicado -and -not $SemPush -and -not $ManterWorktree) {
  $principal = $raiz -replace '\\\.worktrees\\.*$', ''
  $slug = $branch -replace '/', '-'
  $limpezaFonte = Join-Path $raiz 'tools\worktrees\limpar-agendada.ps1'
  if (Test-Path $limpezaFonte) {
    # copia para o TEMP: o processo destacado nao pode depender de um arquivo
    # que vive na pasta que sera removida
    $limpezaTemp = Join-Path $env:TEMP "dhruva-limpar-agendada-$slug.ps1"
    Copy-Item $limpezaFonte $limpezaTemp -Force
    $argsLimpeza = '-NoProfile -ExecutionPolicy Bypass -File "{0}" -Nome "{1}" -Principal "{2}" -Tentativas 30 -IntervaloSegundos 5' -f $limpezaTemp, $branch, $principal
    Start-Process powershell -ArgumentList $argsLimpeza -WorkingDirectory $principal -WindowStyle Hidden
    Write-Host "Limpeza agendada: esta worktree e a branch local serao removidas em segundo plano,"
    Write-Host "assim que a pasta for liberada - a limpeza tambem encerra MCPs orfaos que a estejam travando."
    Write-Host "Log: .worktrees\limpeza-automatica.log"
    Write-Host "  (para manter a worktree em vez de remover, rode com -ManterWorktree)"
  } else {
    Write-Host "Para limpar a worktree (do repositorio principal):"
    Write-Host "  tools\worktrees\limpar.ps1 -Nome $branch"
  }
} else {
  Write-Host "Para limpar a worktree (do repositorio principal):"
  Write-Host "  tools\worktrees\limpar.ps1 -Nome $branch"
}

# -- 6) varredura: worktrees paradas com pendencia (aviso para a proxima IA) ---
# Compara cada worktree com a main do fork (commits nao publicados + alteracoes
# sem commit) e mede quanto tempo esta parada; passando de $ParadaHoras, avisa
# quem passou: integrar/publicar (rodando o fechar dela) ou descartar.
$principal = $raiz -replace '\\\.worktrees\\.*$', ''
$limiteSegundos = $ParadaHoras * 3600
$agoraSegundos = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()

$registradas = @()
$caminhoWt = $null
$linhasWt = & git -C $principal worktree list --porcelain 2>$null
foreach ($linha in @($linhasWt)) {
  if ("$linha" -like 'worktree *') {
    $caminhoWt = "$linha".Substring(9).Trim() -replace '/', '\'
  } elseif ("$linha" -like 'branch *' -and $caminhoWt) {
    $b = "$linha".Substring(7).Trim() -replace '^refs/heads/', ''
    $registradas += [pscustomobject]@{ Caminho = $caminhoWt; Branch = $b }
    $caminhoWt = $null
  }
}

$paradas = @()
$prontas = @()
$ativas = 0
foreach ($wt in $registradas) {
  if ($wt.Caminho -ieq $raiz) { continue }
  # So pastas em .worktrees entram na varredura — a copia principal tambem e
  # uma worktree do git e nao pode aparecer como "pode remover".
  if ($wt.Caminho -notmatch '\\\.worktrees\\') { continue }
  if (-not (Test-Path $wt.Caminho)) { continue }

  $ahead = [int]0
  $aheadTxt = & git -C $wt.Caminho rev-list --count origin/main..HEAD 2>$null
  if ($LASTEXITCODE -eq 0) { [int]::TryParse("$aheadTxt".Trim(), [ref]$ahead) | Out-Null }

  $sujo = @(& git -C $wt.Caminho status --porcelain 2>$null | Where-Object { "$_".Trim() })

  $tsCommit = [long]0
  $ctTxt = & git -C $wt.Caminho log -1 --format=%ct 2>$null
  if ($LASTEXITCODE -eq 0) { [long]::TryParse("$ctTxt".Trim(), [ref]$tsCommit) | Out-Null }

  $tsSujo = 0
  foreach ($linhaSuja in $sujo) {
    $rel = "$linhaSuja"
    if ($rel.Length -gt 3) { $rel = $rel.Substring(3).Trim() }
    if ($rel -match ' -> ') { $rel = ($rel -split ' -> ')[-1] }
    $rel = $rel.Trim('"')
    try {
      $item = Get-Item (Join-Path $wt.Caminho $rel) -Force -ErrorAction Stop
      $ts = ([DateTimeOffset]$item.LastWriteTimeUtc).ToUnixTimeSeconds()
      if ($ts -gt $tsSujo) { $tsSujo = $ts }
    } catch { }
  }

  $ultima = [math]::Max($tsCommit, $tsSujo)
  $paradaSeg = 0
  if ($ultima -gt 0) { $paradaSeg = $agoraSegundos - $ultima }
  $paradaH = [int][math]::Floor($paradaSeg / 3600)

  if ($ahead -gt 0 -or $sujo.Count -gt 0) {
    if ($paradaSeg -ge $limiteSegundos) {
      $paradas += [pscustomobject]@{ Branch = $wt.Branch; Ahead = $ahead; Sujo = $sujo.Count; Horas = $paradaH }
    } else {
      $ativas++
    }
  } elseif ($paradaSeg -ge $limiteSegundos) {
    $prontas += [pscustomobject]@{ Branch = $wt.Branch; Horas = $paradaH }
  }
}

$fantasmas = @()
$pastaWt = Join-Path $principal '.worktrees'
foreach ($d in @(Get-ChildItem $pastaWt -Directory -Force -ErrorAction SilentlyContinue)) {
  $achou = $false
  foreach ($wt in $registradas) { if ($wt.Caminho -ieq $d.FullName) { $achou = $true } }
  if (-not $achou) { $fantasmas += $d.Name }
}

Write-Host ""
if ($paradas.Count -gt 0 -or $fantasmas.Count -gt 0 -or $prontas.Count -gt 0) {
  Write-Host "=== ATENCAO: limpeza pendente em outras worktrees (acao da IA - contrato secao 2) ==="
  foreach ($p in $paradas) {
    $dias = [math]::Round($p.Horas / 24, 1)
    $pedacos = @()
    if ($p.Ahead -gt 0) { $pedacos += "$($p.Ahead) commit(s) sem publicar" }
    if ($p.Sujo -gt 0) { $pedacos += "$($p.Sujo) alteracao(oes) sem commit" }
    Write-Host "  $($p.Branch)  (parada ha ~$($p.Horas)h / $dias dia(s))"
    Write-Host "      pendencias: $($pedacos -join ' + ')"
    Write-Host "      vale integrar?  rode o fechar.ps1 DE DENTRO dessa worktree (mensagem propria): publica na main e ela se limpa sozinha"
    Write-Host "      nao vale?       descarte: tools\worktrees\limpar.ps1 -Nome $($p.Branch) -Forcar"
  }
  foreach ($r in $prontas) {
    Write-Host "  $($r.Branch)  (ja publicada e limpa; parada ha ~$($r.Horas)h - so a pasta sobrou)"
    Write-Host "      remova sem risco: tools\worktrees\limpar.ps1 -Nome $($r.Branch) -ApagarBranch"
  }
  foreach ($f in $fantasmas) {
    Write-Host "  pasta fantasma: .worktrees\$f  (sem registro no git - limpeza antiga interrompida)"
    Write-Host "      remova com: tools\worktrees\limpar-agendada.ps1 -Nome $f"
  }
  Write-Host "  (worktrees ativas com pendencia dentro do prazo de $ParadaHoras h: $ativas - deixe com o responsavel)"
} else {
  Write-Host "(nenhuma outra worktree parada com pendencia)"
}
