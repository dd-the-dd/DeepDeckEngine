<#
.SYNOPSIS
Audits complete sets or an ordered multi-set migration queue with isolated cached binaries.

.DESCRIPTION
Supports the official played-card queue schema (`queue`, `global_rank`, `oracle_id`, `formats`),
legacy `cards` manifests, complete Scryfall sets, and local Scryfall arrays. Queue-only mode
resolves prints in Scryfall collection batches and audits only queued cards. An absent or
non-canonical queued card fails; unsupported cards outside a partial queue do not. Use
RequireFullSetCoverage with SetCode/CardsJson for a final complete regression.

Cargo uses TargetDirectory, CARGO_TARGET_DIR, or .tmp/targets/card-migration. Its actual target
path is read from cargo metadata. All migration agents should reuse the shared default target;
Cargo serializes writers and shared artifacts avoid duplicate engine builds. The parser audit
binary uses an optimized release build by default and is reused only while its source, toolchain,
profile, target, and executable hash match. Use ParserBuildProfile debug for a faster cold build
when optimized throughput is not required. A separate target is reserved for explicit cache-
isolation tests. A named mutex serializes parser cache publication on a shared target.
Structural parser coverage and batch executable-contract coverage are reported separately;
compile, audit, focused contract-test, and runtime-test times remain split.

.PARAMETER SetCode
One or more complete Scryfall set codes.
.PARAMETER CardsJson
Local Scryfall card array, split by set to avoid collector-number collisions.
.PARAMETER QueueFile
Official queue, cards manifest, or bare ordered array. Alias: AffectedManifest.
.PARAMETER RequireFullSetCoverage
Fails for any unsupported card in explicit SetCode/CardsJson input.
.PARAMETER AllowCompositeCanonical
Reports canonical composite IR without failing affected queue cards. Intended only for syntax
audits; composite IR is not primitive migration coverage.
.PARAMETER TargetDirectory
Cargo target directory. Migration agents should share `.tmp/targets/card-migration`; use a distinct
target only for an explicit cache-isolation test. Same-target parser cache access is serialized and
metadata updates are atomic.
.PARAMETER ParserBuildProfile
Parser audit binary profile: release (default, optimized warm audits) or debug (faster cold build).
.PARAMETER ReportJson
Optional machine-readable audit report. Valid semantic results are atomically replaced. If parser or
contract infrastructure fails and a prior report exists, it is preserved and the incomplete diagnostic
is written beside it as `<ReportJson>.failed.json` with complete=false and failure_stage.
.PARAMETER StopAtFirstNonNative
For an ordered queue, stop the workflow after parser classification and fail on the first card that
is not fully native. Mixed and legacy canonical cards are migration failures in this mode.
.PARAMETER RefreshProofs
In strict-prefix mode, execute only missing or stale manifest-declared contract/runtime fixtures or
legacy focused tests and atomically cache successful results. Stable fixture runners are compiled
once and reused; a failed or zero-match proof is never cached.
.PARAMETER ProofManifest
Tracked proof specification manifest. Defaults to docs/native-card-proof-manifest.json.
.PARAMETER PlanOnly
Validates architecture and queue order without network or Cargo.
.PARAMETER RefreshSetData
Refreshes cached complete-set data.
.PARAMETER SetCacheHours
Complete-set cache lifetime; default 24 hours.
.PARAMETER CacheDirectory
Data/fingerprint cache; default .tmp/card-migration-cache.
.PARAMETER RustMinStack
RUST_MIN_STACK; default 33554432.
.PARAMETER BudgetSeconds
Warm audit target; default 300 seconds.
.PARAMETER EnforceBudget
Fails a fully warm run exceeding BudgetSeconds.
.PARAMETER SummaryOnly
Validates full parser output but suppresses the large RULES lines on the console.
.PARAMETER Help
Prints help without side effects.

.EXAMPLE
.\scripts\audit-card-set.ps1 -QueueFile .\docs\played-card-migration-priority-2026-11-09.json

.EXAMPLE
.\scripts\audit-card-set.ps1 -SetCode MSH -RequireFullSetCoverage

.EXAMPLE
.\scripts\audit-card-set.ps1 -QueueFile .\docs\played-card-migration-priority-2026-11-09.json -PlanOnly
#>
[CmdletBinding()]
param(
    [string[]] $SetCode,
    [string] $CardsJson,
    [Alias('AffectedManifest')][string] $QueueFile,
    [switch] $RequireFullSetCoverage,
    [switch] $AllowCompositeCanonical,
    [string] $TargetDirectory,
    [ValidateSet('release', 'debug')][string] $ParserBuildProfile = 'release',
    [string] $ReportJson,
    [switch] $StopAtFirstNonNative,
    [switch] $RefreshProofs,
    [string] $ProofManifest,
    [switch] $PlanOnly,
    [switch] $RefreshSetData,
    [ValidateRange(1, 8760)][int] $SetCacheHours = 24,
    [string] $CacheDirectory,
    [ValidateRange(1, 2147483647)][int] $RustMinStack = 33554432,
    [ValidateRange(1, 86400)][int] $BudgetSeconds = 300,
    [switch] $EnforceBudget,
    [switch] $SummaryOnly,
    [switch] $Help
)

$ErrorActionPreference = 'Stop'
if ($Help) { Get-Help -Name $MyInvocation.MyCommand.Path -Detailed; exit 0 }
if (@($SetCode).Count -eq 0 -and [string]::IsNullOrWhiteSpace($CardsJson) -and
    [string]::IsNullOrWhiteSpace($QueueFile)) {
    throw 'Supply -SetCode, -CardsJson, or -QueueFile.'
}
if (@($SetCode).Count -gt 0 -and -not [string]::IsNullOrWhiteSpace($CardsJson)) {
    throw '-SetCode and -CardsJson are mutually exclusive.'
}
if ($RequireFullSetCoverage -and @($SetCode).Count -eq 0 -and
    [string]::IsNullOrWhiteSpace($CardsJson)) {
    throw '-RequireFullSetCoverage requires -SetCode or -CardsJson.'
}
if ($PlanOnly -and -not [string]::IsNullOrWhiteSpace($ReportJson)) {
    throw '-ReportJson cannot be combined with -PlanOnly because no audit results are produced.'
}
if ($StopAtFirstNonNative -and [string]::IsNullOrWhiteSpace($QueueFile)) {
    throw '-StopAtFirstNonNative requires an ordered -QueueFile.'
}
if ($RefreshProofs -and -not $StopAtFirstNonNative) {
    throw '-RefreshProofs currently requires -StopAtFirstNonNative.'
}

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
function Get-AbsolutePath([string] $Value, [string] $Default) {
    if ([string]::IsNullOrWhiteSpace($Value)) { $Value = $Default }
    if (-not [IO.Path]::IsPathRooted($Value)) { $Value = Join-Path $repoRoot $Value }
    return [IO.Path]::GetFullPath($Value)
}
$CacheDirectory = Get-AbsolutePath $CacheDirectory (Join-Path $repoRoot '.tmp\card-migration-cache')
$targetDefault = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else {
    Join-Path $repoRoot '.tmp\targets\card-migration'
}
$TargetDirectory = Get-AbsolutePath $TargetDirectory $targetDefault
$ReportJson = if([string]::IsNullOrWhiteSpace($ReportJson)){$null}else{Get-AbsolutePath $ReportJson $null}
$ProofManifest = Get-AbsolutePath $ProofManifest (Join-Path $repoRoot 'docs\native-card-proof-manifest.json')
$env:CARGO_TARGET_DIR = $TargetDirectory
$env:RUST_MIN_STACK = $RustMinStack.ToString([Globalization.CultureInfo]::InvariantCulture)
$architectureBaseline = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'card-migration-architecture-baseline.json') -Raw | ConvertFrom-Json
$env:AUDIT_FORBIDDEN_KINDS = @(
    'resolveTriggeredInstruction',
    'resolveSpellInstruction',
    @($architectureBaseline.legacyCompositeKinds)
) -join ','

$phaseSeconds = [ordered]@{
    architecture=0.0; input=0.0; 'cargo-metadata'=0.0; 'parser-build'=0.0;
    'parser-audit'=0.0; 'contract-audit-build'=0.0; 'contract-audit'=0.0;
    'contract-build'=0.0; 'contract-tests'=0.0;
    'runtime-build'=0.0; 'runtime-tests'=0.0
}
$binaryBuildCount = 0; $networkFetchCount = 0; $proofCount = 0
$fixtureRunnerBinaries = @{}
$auditedCardCount = 0; $unsupportedObserved = 0; $primitiveObserved = 0; $compositeObserved = 0
$nativeRecognizerObserved = 0; $mixedRecognizerObserved = 0; $legacyRecognizerObserved = 0
$unsupportedRecognizerObserved = 0; $nativeAbilityObserved = 0; $legacyAbilityObserved = 0; $unclaimedAbilityObserved = 0
$affectedCanonicalCount = 0; $affectedPrimitiveCount = 0; $affectedCompositeCount = 0
$contractExecutableObserved = 0; $contractUnexecutableObserved = 0
$affectedContractExecutableCount = 0; $affectedContractUnexecutableCount = 0
$affectedFailed = $false; $fullCoverageFailed = $false; $fatalError = $null
$failureStage=$null;$currentStage='initialization';$reportSubjects=@();$statuses=@{}
$parserAuditFingerprint=$null;$contractAuditFingerprint=$null
$architectureFingerprint=$null;$architectureReportPath=$null
$cargoTargetRoot = $null; $cargoPackageIds = @{}; $overallTimer = [Diagnostics.Stopwatch]::StartNew()

function New-Directory([string] $Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $Path -Force)
    }
}
function Get-Property($Object, [string[]] $Names) {
    if ($null -eq $Object) { return $null }
    foreach ($name in $Names) {
        $property = $Object.PSObject.Properties[$name]
        if ($null -ne $property -and $null -ne $property.Value) { return $property.Value }
    }
    return $null
}
function Get-Set([string] $Value) {
    if ([string]::IsNullOrWhiteSpace($Value)) { return $null }
    $value = [regex]::Replace($Value.ToLowerInvariant(), '[^a-z0-9]', '')
    if (-not $value) { throw 'Invalid empty set code.' }
    return $value
}
function Get-QueueOracle($Entry) { return [string](Get-Property $Entry @('oracleId','oracle_id')) }
function Get-QueueSet($Entry) { return Get-Set ([string](Get-Property $Entry @('setCode','set_code'))) }
function Get-QueueCollector($Entry) { return [string](Get-Property $Entry @('collectorNumber','collector_number')) }
function Get-QueueKey($Entry) {
    $oracle = Get-QueueOracle $Entry
    if ($oracle) { return "oracle:$($oracle.ToLowerInvariant())" }
    $set = Get-QueueSet $Entry; $collector = Get-QueueCollector $Entry
    if (-not $set -or -not $collector) { throw 'Queue entry needs oracle_id or setCode+collectorNumber.' }
    return "print:$set`:$($collector.ToLowerInvariant())"
}
function Get-QueueLabel($Entry) {
    $rank = Get-Property $Entry @('global_rank','globalRank')
    $name = [string](Get-Property $Entry @('name'))
    $formats = @((Get-Property $Entry @('formats')) | ForEach-Object {
        [string](Get-Property $_ @('format'))
    } | Where-Object { $_ })
    return "$(if($null-ne $rank){"rank $rank "})$name ($(Get-QueueKey $Entry))$(if($formats){" [$($formats -join ',')]"})"
}
function Read-Queue {
    if (-not $QueueFile) { return @() }
    $document = Get-Content -LiteralPath (Resolve-Path -LiteralPath $QueueFile) -Raw | ConvertFrom-Json
    $items = @(if ($null -ne $document.queue) { @($document.queue) }
        elseif ($null -ne $document.cards) { @($document.cards) }
        elseif ($document -is [array]) { @($document) }
        else { throw 'Queue JSON needs queue/cards or a bare array.' })
    $indexed = for ($i=0; $i-lt $items.Count; $i++) {
        $rank=Get-Property $items[$i] @('global_rank','globalRank')
        [pscustomobject]@{Item=$items[$i];Rank=if($null-ne $rank){[int]$rank}else{[int]::MaxValue};Index=$i}
    }
    $result=@($indexed|Sort-Object Rank,Index|ForEach-Object{$_.Item})
    $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach($item in $result){$key=Get-QueueKey $item;if(-not $seen.Add($key)){throw "Duplicate queue identity: $key"}}
    return $result
}
function Read-Cards([string] $Path) {
    $value=Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json
    if($value-isnot [array]){throw "Card JSON must be an array: $Path"};return @($value)
}
function Get-CardSet($Card,[string]$Fallback){$set=[string](Get-Property $Card @('set','set_code'));if(-not $set){$set=$Fallback};return Get-Set $set}
function Get-CardKeys($Card,[string]$FallbackSet){
    $keys=@();$oracle=[string](Get-Property $Card @('oracle_id','oracleId'))
    if($oracle){$keys+="oracle:$($oracle.ToLowerInvariant())"}
    $set=Get-CardSet $Card $FallbackSet;$collector=[string](Get-Property $Card @('collector_number','collectorNumber'))
    if($set-and $collector){$keys+="print:$set`:$($collector.ToLowerInvariant())"};return $keys
}
function Get-AuditIdentity($Card){
    $auditKey=[string](Get-Property $Card @('audit_key','auditKey'))
    if($auditKey){return $auditKey}
    return [string](Get-Property $Card @('collector_number','collectorNumber'))
}
function Copy-CardWithAuditKey($Card,[string]$AuditKey){
    $copy=[ordered]@{}
    foreach($property in $Card.PSObject.Properties){$copy[$property.Name]=$property.Value}
    $copy['audit_key']=$AuditKey
    return [pscustomobject]$copy
}
function New-StrictQueueCards($Queue,[hashtable]$Resolved){
    $cards=[Collections.ArrayList]::new()
    for($index=0;$index-lt$Queue.Count;$index++){
        $entry=$Queue[$index];$key=Get-QueueKey $entry
        if(-not$Resolved.ContainsKey($key)){throw "Resolved queue omitted $key."}
        $auditKey='queue-{0:D8}'-f($index+1)
        [void]$cards.Add((Copy-CardWithAuditKey $Resolved[$key] $auditKey))
    }
    return @($cards)
}
function Get-Hash([string]$Text){$sha=[Security.Cryptography.SHA256]::Create();try{return([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text)))).Replace('-','')}finally{$sha.Dispose()}}
function Write-Group([string]$Set,$Cards,[string]$Name){
    $directory=Join-Path $CacheDirectory 'audit-inputs';New-Directory $directory
    $text=(($Cards|ForEach-Object{$keys=(Get-CardKeys $_ $Set)-join'|';$auditKey=[string](Get-Property $_ @('audit_key','auditKey'));if($auditKey){"audit:$auditKey|$keys"}else{$keys}})-join"`n")
    $safe=[regex]::Replace($Name.ToLowerInvariant(),'[^a-z0-9_-]','-');$hash=(Get-Hash $text).Substring(0,16)
    $path=Join-Path $directory "$safe-$hash.json";ConvertTo-Json -InputObject @($Cards)-Depth 100|Set-Content -LiteralPath $path -Encoding UTF8;return $path
}
function Get-SetPath([string]$Code){
    $directory=Join-Path $CacheDirectory 'sets';New-Directory $directory;$path=Join-Path $directory "$Code.json"
    if(-not $RefreshSetData-and(Test-Path -LiteralPath $path)){$age=[DateTime]::UtcNow-(Get-Item $path).LastWriteTimeUtc;if($age.TotalHours-le $SetCacheHours){return $path}}
    Write-Host "Downloading complete set $Code..." -ForegroundColor Cyan
    $url="https://api.scryfall.com/cards/search?q=set%3A$Code&order=set&unique=cards";$cards=@()
    do{$page=Invoke-RestMethod -Uri $url -Headers @{'User-Agent'='DeepDeck parser audit/0.2';Accept='application/json'};$script:networkFetchCount++;$cards+=@($page.data);$url=if($page.has_more){[string]$page.next_page}else{$null};if($page.has_more-and-not $url){throw 'Scryfall omitted next_page.'}}while($url)
    ConvertTo-Json -InputObject @($cards)-Depth 100|Set-Content -LiteralPath $path -Encoding UTF8;return $path
}
function Get-QueueCachePath([string]$Key){$directory=Join-Path $CacheDirectory 'queue-cards';New-Directory $directory;$safe=[regex]::Replace($Key.ToLowerInvariant(),'[^a-z0-9_-]','-');return Join-Path $directory "$safe.json"}
function Resolve-Queue($Queue,[hashtable]$Known){
    $resolved=@{};$missing=@()
    foreach($entry in $Queue){$key=Get-QueueKey $entry;if($Known.ContainsKey($key)){$resolved[$key]=$Known[$key];continue};$path=Get-QueueCachePath $key;if(Test-Path $path){$resolved[$key]=Get-Content $path -Raw|ConvertFrom-Json}else{$missing+=[pscustomobject]@{Entry=$entry;Key=$key;Path=$path}}}
    for($offset=0;$offset-lt $missing.Count;$offset+=75){$last=[Math]::Min($offset+74,$missing.Count-1);$batch=@($missing[$offset..$last]);$ids=@($batch|ForEach-Object{$oracle=Get-QueueOracle $_.Entry;if($oracle){@{oracle_id=$oracle}}else{@{set=Get-QueueSet $_.Entry;collector_number=Get-QueueCollector $_.Entry}}})
        Write-Host "Resolving $($batch.Count) queue cards through Scryfall collection..." -ForegroundColor Cyan
        $response=Invoke-RestMethod -Method Post -Uri 'https://api.scryfall.com/cards/collection' -ContentType 'application/json' -Headers @{'User-Agent'='DeepDeck parser audit/0.2';Accept='application/json'} -Body(ConvertTo-Json -InputObject @{identifiers=$ids}-Depth 8);$script:networkFetchCount++
        $returned=@{};foreach($card in @($response.data)){foreach($key in(Get-CardKeys $card $null)){$returned[$key]=$card}}
        foreach($request in $batch){if(-not $returned.ContainsKey($request.Key)){throw "Scryfall could not resolve $($request.Key)."};$resolved[$request.Key]=$returned[$request.Key];ConvertTo-Json -InputObject $returned[$request.Key]-Depth 100|Set-Content $request.Path -Encoding UTF8}
    };return $resolved
}
function New-AuditInputs($Queue){
    $inputs=[Collections.ArrayList]::new();$known=@{};$sets=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    if($CardsJson){$path=(Resolve-Path $CardsJson).Path;$cards=Read-Cards $path;$groups=@($cards|Group-Object{Get-CardSet $_ 'local'});foreach($group in $groups){$set=[string]$group.Name;$groupPath=if($groups.Count-eq 1){$path}else{Write-Group $set @($group.Group) "local-$set"};[void]$inputs.Add([pscustomobject]@{SetCode=$set;Path=$groupPath;Cards=@($group.Group);Complete=$true});[void]$sets.Add($set);foreach($card in $group.Group){foreach($key in(Get-CardKeys $card $set)){$known[$key]=$card}}}}
    else{foreach($raw in @($SetCode|Where-Object{-not[string]::IsNullOrWhiteSpace([string]$_)})){$set=Get-Set $raw;if(-not $sets.Add($set)){continue};$path=Get-SetPath $set;$cards=Read-Cards $path;[void]$inputs.Add([pscustomobject]@{SetCode=$set;Path=$path;Cards=$cards;Complete=$true});foreach($card in $cards){foreach($key in(Get-CardKeys $card $set)){$known[$key]=$card}}}}
    if($Queue.Count){
        $resolved=Resolve-Queue $Queue $known
        if($StopAtFirstNonNative){
            $strictCards=@(New-StrictQueueCards $Queue $resolved)
            $inputs.Clear()
            $path=Write-Group 'priority' $strictCards 'queue-priority-strict'
            [void]$inputs.Add([pscustomobject]@{SetCode='priority';Path=$path;Cards=$strictCards;Complete=$false})
        }else{
            $supplemental=@();foreach($entry in $Queue){$key=Get-QueueKey $entry;if(-not $known.ContainsKey($key)){$supplemental+=$resolved[$key]}}
            foreach($group in @($supplemental|Group-Object{Get-CardSet $_ 'resolved'})){$set=[string]$group.Name;$path=Write-Group $set @($group.Group) "queue-$set";[void]$inputs.Add([pscustomobject]@{SetCode=$set;Path=$path;Cards=@($group.Group);Complete=$false})}
        }
    }
    return @($inputs)
}
function Initialize-Cargo{
    $timer=[Diagnostics.Stopwatch]::StartNew();try{New-Directory $TargetDirectory;$json=(& cargo metadata --locked --no-deps --format-version 1)-join"`n";if($LASTEXITCODE){throw 'cargo metadata failed.'};$metadata=$json|ConvertFrom-Json;$script:cargoTargetRoot=[string]$metadata.target_directory;if(-not $script:cargoTargetRoot){throw 'cargo metadata omitted target_directory.'};$script:cargoPackageIds=@{};foreach($package in @($metadata.packages)){$script:cargoPackageIds[[string]$package.name]=[string]$package.id}}finally{$timer.Stop();$phaseSeconds['cargo-metadata']+=$timer.Elapsed.TotalSeconds}
}
function Get-ComponentInputs([ValidateSet('parser','runtime','contract')]$Component){
    $common=@(
        'Cargo.toml','Cargo.lock','rust-toolchain.toml',
        'src\lib.rs','src\card_catalog.rs','src\oracle.rs','src\oracle',
        'crates\deepdeck-oracle\Cargo.toml','crates\deepdeck-oracle\src',
        'crates\deepdeck-oracle\generated',
        'crates\deepdeck-oracle\examples\audit_parser_set.rs',
        'crates\deepdeck-parser-fixture-runner\Cargo.toml',
        'crates\deepdeck-parser-fixture-runner\src',
        'data\builtin-game-pieces.json','data\ability-catalog','data\oracle-vocabulary'
    )|ForEach-Object{Join-Path $repoRoot $_}
    foreach($optional in @('.cargo\config','.cargo\config.toml','build.rs','crates\deepdeck-oracle\build.rs')){
        $path=Join-Path $repoRoot $optional
        if(Test-Path -LiteralPath $path){$common+=$path}
    }
    if($Component-eq'parser'){return $common}
    $runtime=$common+(@('src\engine','src\game_rules.rs','src\model_runtime.rs','crates\deepdeck-engine-runtime\Cargo.toml','crates\deepdeck-engine-runtime\src')|ForEach-Object{Join-Path $repoRoot $_});$runtimeTests=Join-Path $repoRoot 'crates\deepdeck-engine-runtime\tests';if(Test-Path $runtimeTests -PathType Container){$runtime+=$runtimeTests};if($Component-eq'runtime'){return $runtime};$contract=$runtime+(@('crates\deepdeck-oracle-engine-contracts\Cargo.toml','crates\deepdeck-oracle-engine-contracts\src','crates\deepdeck-oracle-engine-contracts\tests','crates\deepdeck-oracle-engine-contracts\examples')|ForEach-Object{Join-Path $repoRoot $_});return $contract
}
function Get-FixtureRunnerInputs([ValidateSet('contract','runtime')]$Kind){
    $paths=@('Cargo.toml','Cargo.lock','rust-toolchain.toml','src\card_catalog.rs','src\game_rules.rs','src\model_runtime.rs','crates\deepdeck-engine-runtime\Cargo.toml','crates\deepdeck-engine-runtime\src')|ForEach-Object{Join-Path $repoRoot $_}
    $engineRoot=Join-Path $repoRoot 'src\engine'
    $paths+=@(Get-ChildItem -LiteralPath $engineRoot -File -Recurse|Where-Object{
        $relative=$_.FullName.Substring($engineRoot.Length)-replace'^[\\/]+',''
        $relative-notmatch'(^|[\\/])tests([\\/]|\.rs$)'-and$relative-notmatch'(^|[\\/]).+_tests\.rs$'
    }|ForEach-Object{$_.FullName})
    $runner=if($Kind-eq'contract'){'crates\deepdeck-contract-fixture-runner'}else{'crates\deepdeck-runtime-fixture-runner'}
    $paths+=(Join-Path $repoRoot "$runner\Cargo.toml")
    $paths+=(Join-Path $repoRoot "$runner\src")
    return @($paths)
}
function Get-Fingerprint($Paths,[string]$BuildIdentity=''){$files=@();foreach($path in $Paths){if(Test-Path $path -PathType Container){$files+=Get-ChildItem $path -File -Recurse}elseif(Test-Path $path){$files+=Get-Item $path}else{throw "Missing fingerprint input $path"}};$rustc=(& rustc -Vv)-join"`n";if($LASTEXITCODE){throw 'rustc identity failed.'};$lines=@("rustc=$rustc","target=$cargoTargetRoot","build=$BuildIdentity","RUSTFLAGS=$env:RUSTFLAGS","CARGO_ENCODED_RUSTFLAGS=$env:CARGO_ENCODED_RUSTFLAGS","CARGO_BUILD_RUSTFLAGS=$env:CARGO_BUILD_RUSTFLAGS");foreach($file in($files|Sort-Object FullName -Unique)){$lines+="$($file.FullName)=$((Get-FileHash $file.FullName -Algorithm SHA256).Hash)"};return Get-Hash($lines-join"`n")}
function Read-CacheMetadata([string]$Path){if(-not(Test-Path -LiteralPath $Path -PathType Leaf)){return $null};try{return Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json -ErrorAction Stop}catch{Write-Warning "Ignoring invalid cache metadata $Path`: $($_.Exception.Message)";return $null}}
function Write-JsonAtomically($Value,[string]$Path,[int]$Depth=20){$directory=Split-Path -Parent $Path;New-Directory $directory;$nonce="$PID.$([Guid]::NewGuid().ToString('N'))";$temporary=Join-Path $directory ".$([IO.Path]::GetFileName($Path)).$nonce.tmp";$backup=Join-Path $directory ".$([IO.Path]::GetFileName($Path)).$nonce.bak";try{$Value|ConvertTo-Json -Depth $Depth|Set-Content -LiteralPath $temporary -Encoding UTF8;if(Test-Path -LiteralPath $Path -PathType Leaf){[IO.File]::Replace($temporary,$Path,$backup,$true)}else{try{[IO.File]::Move($temporary,$Path)}catch [IO.IOException]{if(Test-Path -LiteralPath $Path -PathType Leaf){[IO.File]::Replace($temporary,$Path,$backup,$true)}else{throw}}}}finally{foreach($artifact in @($temporary,$backup)){if(Test-Path -LiteralPath $artifact -PathType Leaf){Remove-Item -LiteralPath $artifact -Force}}}}
function Write-CacheMetadata($Metadata,[string]$Path){Write-JsonAtomically $Metadata $Path 5}
function Assert-ParserAuditOutput($Output,$ExitCode,[bool]$Strict,[int]$ExpectedCardCount,[string]$Label){
    if($null-eq$ExitCode){throw "Parser audit returned no exit code for $Label."}
    $exitValue=[long]$ExitCode
    $strictStop=$Strict-and$exitValue-eq1
    $coverageGap=(-not$Strict)-and$exitValue-eq1
    if($exitValue-ne0-and-not$strictStop-and-not$coverageGap){throw "Parser audit failed for $Label with unexpected exit code $exitValue."}

    $markers=@{}
    foreach($kind in @('CARD','RECOGNIZER','IR','RULES')){
        $markers[$kind]=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    }
    $cardOrder=[Collections.Generic.List[string]]::new()
    $firstNonNative=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $summaryCount=0
    $summaryTotal=$null
    foreach($raw in @($Output)){
        $line=$raw.ToString()
        if($line-match'^(CARD|RECOGNIZER|IR|RULES)\t([^\t]+)\t'){
            $kind=$Matches[1];$auditKey=$Matches[2]
            if(-not$markers[$kind].Add($auditKey)){throw "Parser audit emitted duplicate $kind output for $auditKey in $Label."}
            if($kind-eq'CARD'){$cardOrder.Add($auditKey)}
            continue
        }
        if($line-match'^FIRST_NON_NATIVE\t([^\t]+)\t'){
            if(-not$firstNonNative.Add($Matches[1])){throw "Parser audit emitted duplicate FIRST_NON_NATIVE output in $Label."}
            continue
        }
        if($line-match'^SUMMARY\t.*\ttotal=([0-9]+)\t'){
            $summaryCount++
            $summaryTotal=[int]$Matches[1]
        }
    }

    if($markers.CARD.Count-eq0){throw "Parser audit emitted no CARD output for $Label."}
    foreach($auditKey in $markers.CARD){
        foreach($kind in @('RECOGNIZER','IR','RULES')){
            if(-not$markers[$kind].Contains($auditKey)){throw "Parser audit omitted $kind output for $auditKey in $Label."}
        }
    }
    foreach($kind in @('RECOGNIZER','IR','RULES')){
        foreach($auditKey in $markers[$kind]){
            if(-not$markers.CARD.Contains($auditKey)){throw "Parser audit emitted $kind before or without CARD output for $auditKey in $Label."}
        }
    }

    if($strictStop){
        if($firstNonNative.Count-ne1){throw "Strict parser audit exited 1 without exactly one FIRST_NON_NATIVE output for $Label."}
        $firstKey=@($firstNonNative)[0]
        if(-not$markers.CARD.Contains($firstKey)){throw "FIRST_NON_NATIVE references unaudited key $firstKey in $Label."}
        if($cardOrder[$cardOrder.Count-1]-ne$firstKey){throw "FIRST_NON_NATIVE does not reference the final audited card in $Label."}
        if($markers.CARD.Count-gt$ExpectedCardCount){throw "Strict parser audit returned more CARD results than supplied for $Label."}
    }else{
        if($firstNonNative.Count-ne0){throw "Non-strict parser audit emitted FIRST_NON_NATIVE output for $Label."}
        if($markers.CARD.Count-ne$ExpectedCardCount){throw "Parser audit returned $($markers.CARD.Count)/$ExpectedCardCount CARD results for $Label."}
        if($summaryCount-ne1-or$summaryTotal-ne$ExpectedCardCount){throw "Parser audit omitted a unique consistent SUMMARY for $Label."}
    }
}
function Assert-ContractAuditExitCode($ExitCode,[string]$Label){
    if($null-eq$ExitCode){throw "Contract audit returned no exit code for $Label."}
    $exitValue=[long]$ExitCode
    if($exitValue-ne0-and$exitValue-ne1){
        throw "Contract audit failed for $Label with unexpected exit code $exitValue."
    }
}
function Assert-ContractAuditResultCount([int]$Actual,[int]$Expected,[string]$Label){
    if($Actual-ne$Expected){
        throw "Contract audit returned $Actual/$Expected card results for $Label."
    }
}
function Test-IsInfrastructureFailure([bool]$HasFatalError,[string]$FailureStage){
    return $HasFatalError-and$FailureStage-notlike'native-prefix:*'
}
function Publish-AuditReport($Report,[string]$Path,[bool]$InfrastructureFailure){
    $destination=$Path
    if($InfrastructureFailure-and(Test-Path -LiteralPath $Path -PathType Leaf)){
        $destination="$Path.failed.json"
    }
    Write-JsonAtomically $Report $destination 20
    if(-not$InfrastructureFailure){
        $staleFailure="$Path.failed.json"
        if(Test-Path -LiteralPath $staleFailure -PathType Leaf){
            Remove-Item -LiteralPath $staleFailure -Force
        }
    }
    return $destination
}
function Get-OracleDigest($Card){
    $faces=@((Get-Property $Card @('card_faces','cardFaces'))|ForEach-Object{
        [ordered]@{
            name=[string](Get-Property $_ @('name'))
            type_line=[string](Get-Property $_ @('type_line','typeLine'))
            mana_cost=[string](Get-Property $_ @('mana_cost','manaCost'))
            oracle_text=[string](Get-Property $_ @('oracle_text','oracleText'))
        }
    })
    $semantic=[ordered]@{
        name=[string](Get-Property $Card @('name'))
        type_line=[string](Get-Property $Card @('type_line','typeLine'))
        mana_cost=[string](Get-Property $Card @('mana_cost','manaCost'))
        oracle_text=[string](Get-Property $Card @('oracle_text','oracleText'))
        layout=[string](Get-Property $Card @('layout'))
        card_faces=$faces
    }
    return Get-Hash ($semantic|ConvertTo-Json -Depth 20 -Compress)
}
function Get-SemanticIrDigest([string]$RulesJson){
    if([string]::IsNullOrWhiteSpace($RulesJson)){return $null}
    try{$canonical=($RulesJson|ConvertFrom-Json -ErrorAction Stop)|ConvertTo-Json -Depth 100 -Compress}
    catch{throw "Parser emitted invalid RULES JSON: $($_.Exception.Message)"}
    return Get-Hash $canonical
}
function Read-ProofManifest{
    if(-not(Test-Path -LiteralPath $ProofManifest -PathType Leaf)){throw "Proof manifest missing: $ProofManifest"}
    $manifest=Get-Content -LiteralPath $ProofManifest -Raw|ConvertFrom-Json
    if([string]$manifest.schema_version-ne'deepdeck-native-card-proofs/v1'){throw "Unsupported proof manifest schema: $($manifest.schema_version)"}
    $byOracle=@{}
    foreach($card in @($manifest.cards)){
        $oracle=[string](Get-Property $card @('oracle_id','oracleId'))
        if([string]::IsNullOrWhiteSpace($oracle)){throw 'Every proof manifest card needs oracle_id.'}
        $key=$oracle.ToLowerInvariant();if($byOracle.ContainsKey($key)){throw "Duplicate proof manifest oracle_id: $oracle"};$byOracle[$key]=$card
    }
    return $byOracle
}
function Resolve-ProofFile([string]$RelativePath){
    if([string]::IsNullOrWhiteSpace($RelativePath)){throw 'Proof test file path cannot be empty.'}
    $path=[IO.Path]::GetFullPath((Join-Path $repoRoot $RelativePath))
    $rootPrefix=$repoRoot.TrimEnd([IO.Path]::DirectorySeparatorChar,[IO.Path]::AltDirectorySeparatorChar)+[IO.Path]::DirectorySeparatorChar
    if(-not$path.StartsWith($rootPrefix,[StringComparison]::OrdinalIgnoreCase)){throw "Proof test file escapes repository: $RelativePath"}
    if(-not(Test-Path -LiteralPath $path -PathType Leaf)){throw "Proof test file missing: $RelativePath"}
    return $path
}
function Get-ProofEntries($Value,[string]$Label){
    $entries=@($Value)
    if($entries.Count-eq0-or($entries.Count-eq1-and$null-eq$entries[0])){throw "Proof spec omitted $Label."}
    foreach($entry in $entries){if($null-eq$entry){throw "Proof spec $Label contains a null entry."}}
    return $entries
}
function Get-ProofSpecDigest($Spec){
    # The expected IR digest is checked directly against parser output and the
    # actual digest is already part of the proof-cache key. Excluding the pin
    # here avoids needlessly rerunning unchanged fixtures when the manifest is
    # upgraded from an unpinned entry.
    $digestSpec=[ordered]@{}
    foreach($property in $Spec.PSObject.Properties){
        if($property.Name -notin @('expected_semantic_ir_digest','expectedSemanticIrDigest')){
            $digestSpec[$property.Name]=$property.Value
        }
    }
    $lines=@(($digestSpec|ConvertTo-Json -Depth 20 -Compress))
    foreach($kind in @('contract','runtime')){
        $proofs=@(Get-ProofEntries (Get-Property $Spec @($kind)) $kind)
        foreach($proof in $proofs){
            $fixtureRelative=[string](Get-Property $proof @('fixture_path','fixturePath'))
            $fixtureId=[string](Get-Property $proof @('fixture_id','fixtureId'))
            if(-not[string]::IsNullOrWhiteSpace($fixtureRelative)){
                if([string]::IsNullOrWhiteSpace($fixtureId)){throw "Proof spec $kind fixture needs fixture_id."}
                $fixturePath=Resolve-ProofFile ($fixtureRelative-replace'\\','/')
                try{$fixture=Get-Content -LiteralPath $fixturePath -Raw|ConvertFrom-Json -ErrorAction Stop}catch{throw "Invalid $kind fixture JSON $fixtureRelative`: $($_.Exception.Message)"}
                if([string](Get-Property $fixture @('id'))-ne$fixtureId){throw "Proof spec $kind fixture_id does not match $fixtureRelative."}
                $expectedSchema=if($kind-eq'contract'){'deepdeck-contract-fixture/v1'}else{'deepdeck-runtime-fixture/v1'}
                if([string](Get-Property $fixture @('schemaVersion','schema_version'))-ne$expectedSchema){throw "Proof spec $kind fixture has an unsupported schema: $fixtureRelative"}
                $lines+="$fixturePath=$((Get-FileHash -LiteralPath $fixturePath -Algorithm SHA256).Hash)"
                continue
            }
            if(-not[string]::IsNullOrWhiteSpace($fixtureId)){throw "Proof spec $kind fixture_id needs fixture_path."}
            $filter=[string](Get-Property $proof @('filter'));$partition=Get-Property $proof @('partition')
            if([string]::IsNullOrWhiteSpace($filter)-or$null-eq$partition){throw "Proof spec $kind needs fixture_path+fixture_id or filter+partition."}
            foreach($relative in @((Get-Property $proof @('test_files','testFiles')))){
                $relativeText=([string]$relative)-replace'\\','/'
                $path=Resolve-ProofFile $relativeText;$lines+="$path=$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash)"
                if($relativeText.StartsWith('src/engine/tests/',[StringComparison]::OrdinalIgnoreCase)-and$relativeText.EndsWith('.rs',[StringComparison]::OrdinalIgnoreCase)){
                    $module=[IO.Path]::GetFileNameWithoutExtension($relativeText);$registryPath=Join-Path $repoRoot 'src\engine\tests.rs';$registry=[IO.File]::ReadAllText($registryPath)
                    $pathDeclaration="#[path = `"tests/$module.rs`"]";$moduleDeclaration="mod $module;"
                    if($registry.IndexOf($pathDeclaration,[StringComparison]::Ordinal)-lt0-or$registry.IndexOf($moduleDeclaration,[StringComparison]::Ordinal)-lt0){throw "Runtime proof module $module is not registered in src/engine/tests.rs."}
                    $lines+="runtime-test-module=$module"
                }
            }
        }
    }
    return Get-Hash ($lines-join"`n")
}
function Get-ProofProductionFiles([ValidateSet('contract','runtime')]$Kind){
    $paths=@('Cargo.toml','Cargo.lock','rust-toolchain.toml','src\game_rules.rs','src\model_runtime.rs','crates\deepdeck-engine-runtime\Cargo.toml','crates\deepdeck-engine-runtime\src')|ForEach-Object{Join-Path $repoRoot $_}
    $engineRoot=Join-Path $repoRoot 'src\engine'
    $paths+=@(Get-ChildItem -LiteralPath $engineRoot -File -Recurse|Where-Object{
        $relative=$_.FullName.Substring($engineRoot.Length)-replace'^[\\/]+',''
        $relative-notmatch'(^|[\\/])tests([\\/]|\.rs$)'-and$relative-notmatch'(^|[\\/]).+_tests\.rs$'
    }|ForEach-Object{$_.FullName})
    if($Kind-eq'contract'){
        $paths+=@('src\card_catalog.rs','src\oracle','crates\deepdeck-oracle\Cargo.toml','crates\deepdeck-oracle\src','crates\deepdeck-oracle-engine-contracts\Cargo.toml','crates\deepdeck-oracle-engine-contracts\src')|ForEach-Object{Join-Path $repoRoot $_}
    }
    return @($paths)
}
function Get-ProofSemanticsFingerprint($Proof,[ValidateSet('contract','runtime')]$Kind){
    $proofs=@(Get-ProofEntries $Proof $Kind)
    if($proofs.Count-eq1){
        $fixturePath=[string](Get-Property $proofs[0] @('fixture_path','fixturePath'))
        if(-not[string]::IsNullOrWhiteSpace($fixturePath)){
            return Get-Fingerprint (Get-FixtureRunnerInputs $Kind) "native-proof-$Kind-fixture/v1"
        }
        return Get-Fingerprint (Get-ProofProductionFiles $Kind) "native-proof-$Kind/v1"
    }
    $modes=@($proofs|ForEach-Object{
        $fixturePath=[string](Get-Property $_ @('fixture_path','fixturePath'))
        if([string]::IsNullOrWhiteSpace($fixturePath)){'harness'}else{'fixture'}
    }|Sort-Object -Unique)
    $parts=@()
    foreach($mode in $modes){
        $fingerprint=if($mode-eq'fixture'){
            Get-Fingerprint (Get-FixtureRunnerInputs $Kind) "native-proof-$Kind-fixture/v1"
        }else{
            Get-Fingerprint (Get-ProofProductionFiles $Kind) "native-proof-$Kind/v1"
        }
        $parts+="$mode=$fingerprint"
    }
    return Get-Hash ($parts-join"`n")
}
function Get-ProofCachePath([string]$OracleId){$directory=Join-Path $CacheDirectory 'proofs';New-Directory $directory;return Join-Path $directory "$($OracleId.ToLowerInvariant()).json"}
function Test-ProofCache($Cached,$Expected){
    if($null-eq$Cached-or[string]$Cached.schema_version-ne'deepdeck-native-card-proof-cache/v1'){return $false}
    foreach($property in @('oracle_id','oracle_digest','semantic_ir_digest','proof_spec_digest','contract_semantics_fingerprint','runtime_semantics_fingerprint')){
        if([string](Get-Property $Cached @($property))-ne[string](Get-Property $Expected @($property))){return $false}
    }
    return [string]$Cached.contract_status-eq'executable'-and[string]$Cached.runtime_status-eq'passed'
}
function Test-BinaryCacheMetadata($Cached,[string]$Kind,[string]$Profile,[string]$Target,[string]$Fingerprint){if($null-eq$Cached-or$Cached.schema_version-ne2-or[string]$Cached.artifact_kind-ne$Kind-or[string]$Cached.profile-ne$Profile-or[string]$Cached.target_directory-ne$Target-or[string]$Cached.fingerprint-ne$Fingerprint){return $false};$cachedExe=[string]$Cached.executable;$cachedHash=[string]$Cached.executable_hash;if(-not$cachedExe-or-not$cachedHash-or-not(Test-Path -LiteralPath $cachedExe -PathType Leaf)){return $false};try{return(Get-FileHash -LiteralPath $cachedExe -Algorithm SHA256).Hash-eq$cachedHash}catch{return $false}}
function Test-ParserCacheMetadata($Cached,[string]$Profile,[string]$Target,[string]$Fingerprint){return Test-BinaryCacheMetadata $Cached 'parser-audit' $Profile $Target $Fingerprint}
function Invoke-StableCargoBuild([ValidateSet('parser','contract')]$Component,[string]$BuildIdentity,[string[]]$CargoArguments,[string]$InitialFingerprint,[string]$Label){
    $before=$InitialFingerprint
    for($attempt=1;$attempt-le3;$attempt++){
        # Cargo reports ordinary progress on stderr. Keep the script-wide Stop
        # policy from turning `Compiling`/`Finished` into terminating errors
        # when the caller redirects all PowerShell streams for timing logs.
        $priorErrorActionPreference=$ErrorActionPreference
        try{
            $ErrorActionPreference='Continue'
            $output=@(& cargo @CargoArguments 2>&1)
            $exit=$LASTEXITCODE
        }finally{$ErrorActionPreference=$priorErrorActionPreference}
        $output|ForEach-Object{Write-Host $_}
        if($exit){throw "$Label build failed."}
        $after=Get-Fingerprint (Get-ComponentInputs $Component) $BuildIdentity
        if($after-eq$before){return $after}
        Write-Warning "$Label inputs changed during build attempt $attempt; rebuilding against the new snapshot."
        $before=$after
    }
    throw "$Label inputs remained unstable across 3 build attempts; no cache metadata was published."
}
function Get-ParserBinary{
    $timer=[Diagnostics.Stopwatch]::StartNew()
    $mutex=$null;$lockTaken=$false
    try{
        $directory=Join-Path $CacheDirectory 'binaries';New-Directory $directory
        $profile=$ParserBuildProfile.ToLowerInvariant()
        $targetKey=(Get-Hash("target=$($cargoTargetRoot.ToLowerInvariant())")).Substring(0,16)
        $metaPath=Join-Path $directory "audit-parser-$profile-$targetKey.json"
        $mutexName="DeepDeckParserCache_$((Get-Hash("metadata=$($metaPath.ToLowerInvariant())")).Substring(0,32))"
        $mutex=[Threading.Mutex]::new($false,$mutexName)
        try{$lockTaken=$mutex.WaitOne([TimeSpan]::FromMinutes(15))}catch [Threading.AbandonedMutexException]{$lockTaken=$true}
        if(-not$lockTaken){throw "Timed out waiting for parser cache lock: $metaPath"}

        $buildIdentity="parser-audit|profile=$profile|package=deepdeck-parser-fixture-runner|bin=audit_parser_set|features=audit-cli"
        $fingerprint=Get-Fingerprint (Get-ComponentInputs 'parser') $buildIdentity
        $cached=Read-CacheMetadata $metaPath
        if(Test-ParserCacheMetadata $cached $profile $cargoTargetRoot $fingerprint){$script:parserAuditFingerprint=$fingerprint;return [string]$cached.executable}

        $cargoArguments=@('build','--locked','-p','deepdeck-parser-fixture-runner','--bin','audit_parser_set','--features','audit-cli')
        if($profile-eq'release'){$cargoArguments+='--release'}
        $fingerprint=Invoke-StableCargoBuild 'parser' $buildIdentity $cargoArguments $fingerprint 'Parser'
        $suffix=if([Environment]::OSVersion.Platform-eq[PlatformID]::Win32NT){'.exe'}else{''}
        $exe=Join-Path $cargoTargetRoot "$profile\audit_parser_set$suffix"
        if(-not(Test-Path -LiteralPath $exe -PathType Leaf)){throw "Parser binary missing: $exe"}
        $metadata=[pscustomobject]@{
            schema_version=2;artifact_kind='parser-audit';profile=$profile;target_directory=$cargoTargetRoot
            fingerprint=$fingerprint;executable=$exe
            executable_hash=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        }
        Write-CacheMetadata $metadata $metaPath
        $verified=Read-CacheMetadata $metaPath
        if(-not(Test-ParserCacheMetadata $verified $profile $cargoTargetRoot $fingerprint)){
            throw "Parser cache metadata failed post-build verification: $metaPath"
        }
        $script:parserAuditFingerprint=$fingerprint;$script:binaryBuildCount++;return [string]$verified.executable
    }finally{
        if($lockTaken){$mutex.ReleaseMutex()}
        if($null-ne$mutex){$mutex.Dispose()}
        $timer.Stop();$phaseSeconds['parser-build']+=$timer.Elapsed.TotalSeconds
    }
}
function Get-ContractAuditBinary{
    $timer=[Diagnostics.Stopwatch]::StartNew()
    $mutex=$null;$lockTaken=$false
    try{
        $directory=Join-Path $CacheDirectory 'binaries';New-Directory $directory
        $profile='debug';$targetKey=(Get-Hash("target=$($cargoTargetRoot.ToLowerInvariant())")).Substring(0,16)
        $metaPath=Join-Path $directory "audit-contract-$profile-$targetKey.json"
        $mutexName="DeepDeckContractAuditCache_$((Get-Hash("metadata=$($metaPath.ToLowerInvariant())")).Substring(0,32))"
        $mutex=[Threading.Mutex]::new($false,$mutexName)
        try{$lockTaken=$mutex.WaitOne([TimeSpan]::FromMinutes(15))}catch [Threading.AbandonedMutexException]{$lockTaken=$true}
        if(-not$lockTaken){throw "Timed out waiting for contract audit cache lock: $metaPath"}
        $buildIdentity='contract-audit|profile=debug'
        $fingerprint=Get-Fingerprint (Get-ComponentInputs 'contract') $buildIdentity
        $cached=Read-CacheMetadata $metaPath
        if(Test-BinaryCacheMetadata $cached 'contract-audit' $profile $cargoTargetRoot $fingerprint){$script:contractAuditFingerprint=$fingerprint;return [string]$cached.executable}
        $cargoArguments=@('build','--locked','-p','deepdeck-oracle-engine-contracts','--example','audit_contract_set')
        $fingerprint=Invoke-StableCargoBuild 'contract' $buildIdentity $cargoArguments $fingerprint 'Contract audit'
        $suffix=if([Environment]::OSVersion.Platform-eq[PlatformID]::Win32NT){'.exe'}else{''}
        $exe=Join-Path $cargoTargetRoot "debug\examples\audit_contract_set$suffix"
        if(-not(Test-Path -LiteralPath $exe -PathType Leaf)){throw "Contract audit binary missing: $exe"}
        $metadata=[pscustomobject]@{schema_version=2;artifact_kind='contract-audit';profile=$profile;target_directory=$cargoTargetRoot;fingerprint=$fingerprint;executable=$exe;executable_hash=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash}
        Write-CacheMetadata $metadata $metaPath;$verified=Read-CacheMetadata $metaPath
        if(-not(Test-BinaryCacheMetadata $verified 'contract-audit' $profile $cargoTargetRoot $fingerprint)){throw "Contract audit cache metadata failed post-build verification: $metaPath"}
        $script:contractAuditFingerprint=$fingerprint;$script:binaryBuildCount++;return [string]$verified.executable
    }finally{if($lockTaken){$mutex.ReleaseMutex()};if($null-ne$mutex){$mutex.Dispose()};$timer.Stop();$phaseSeconds['contract-audit-build']+=$timer.Elapsed.TotalSeconds}
}
function Get-TestBinaries([ValidateSet('contract','runtime')]$Kind,[int]$Partition){
    $package=if($Kind-eq'contract'){'deepdeck-oracle-engine-contracts'}else{'deepdeck-engine-runtime'}
    if(-not$cargoPackageIds.ContainsKey($package)){throw "cargo metadata omitted package $package."}
    $packageId=$cargoPackageIds[$package];$timer=[Diagnostics.Stopwatch]::StartNew()
    try{
        $directory=Join-Path $CacheDirectory 'binaries';New-Directory $directory
        $metaPath=Join-Path $directory "$package-$Partition.json"
        $fingerprint=Get-Fingerprint (Get-ComponentInputs $Kind)
        if(Test-Path $metaPath){
            $cached=Get-Content $metaPath -Raw|ConvertFrom-Json
            $cachedExecutables=@($cached.executables)
            if($cached.fingerprint-eq$fingerprint-and$cachedExecutables.Count-gt0-and
                @($cachedExecutables|Where-Object{-not(Test-Path -LiteralPath ([string]$_) -PathType Leaf)}).Count-eq0){
                return $cachedExecutables
            }
        }
        # Cargo writes normal progress (`Compiling`, `Finished`) to stderr.
        # With the script-wide Stop policy PowerShell promotes those records to
        # terminating errors before we can inspect Cargo's real exit code.
        $priorErrorActionPreference=$ErrorActionPreference
        try{
            $ErrorActionPreference='Continue'
            $features=if($Kind-eq'runtime'){"oracle-compilation,ci-unit-tests-$Partition"}else{"ci-unit-tests-$Partition"}
            $lines=@(& cargo test --locked -p $package --no-default-features --features $features --no-run --message-format=json 2>&1)
            $exit=$LASTEXITCODE
        }finally{$ErrorActionPreference=$priorErrorActionPreference}
        $executables=@()
        foreach($raw in $lines){
            try{$message=$raw.ToString()|ConvertFrom-Json -ErrorAction Stop}catch{Write-Host $raw;continue}
            if($message.reason-eq'compiler-message'-and$message.message.rendered){Write-Host([string]$message.message.rendered)}
            $targetKinds=@($message.target.kind)
            if($message.reason-eq'compiler-artifact'-and[string]$message.package_id-eq$packageId-and
                $message.profile.test-and$message.executable-and
                ($targetKinds-contains'lib'-or$targetKinds-contains'test')){$executables+=[string]$message.executable}
        }
        if($exit){throw "$package build failed."}
        $executables=@($executables|Select-Object -Unique)
        if($executables.Count-eq0-or@($executables|Where-Object{-not(Test-Path -LiteralPath $_ -PathType Leaf)}).Count-gt0){
            throw "Cannot locate test binaries for package $package."
        }
        [pscustomobject]@{fingerprint=$fingerprint;executables=$executables}|ConvertTo-Json -Depth 4|Set-Content $metaPath -Encoding UTF8
        $script:binaryBuildCount++;return $executables
    }finally{$timer.Stop();$phaseSeconds["$Kind-build"]+=$timer.Elapsed.TotalSeconds}
}
function Invoke-Proof($Kind,[string[]]$Executables,$Filter,$Cards){
    $timer=[Diagnostics.Stopwatch]::StartNew()
    try{
        if(-not$SummaryOnly){Write-Host "[$Kind] $Cards -> $Filter ($($Executables.Count) harnesses)" -ForegroundColor Cyan}
        $totalPassed=0;$summaryCount=0
        foreach($executable in $Executables){
            if(-not$SummaryOnly){Write-Host "  harness: $([IO.Path]::GetFileName($executable))"}
            $output=@(& $executable $Filter --nocapture 2>&1);$exit=$LASTEXITCODE
            if(-not$SummaryOnly){$output|ForEach-Object{Write-Host $_}}
            $joined=$output-join"`n"
            $summaries=[regex]::Matches($joined,'test result: (?<status>ok|FAILED)\.\s+(?<passed>[0-9]+) passed;\s+(?<failed>[0-9]+) failed')
            if($summaries.Count-eq0){throw "Harness emitted no test summary: $executable"}
            foreach($summary in $summaries){
                $summaryCount++;$passed=[int]$summary.Groups['passed'].Value;$failed=[int]$summary.Groups['failed'].Value
                $totalPassed+=$passed
                if($failed-gt0-or$summary.Groups['status'].Value-eq'FAILED'){throw "$Kind proof failed in ${executable}: $Filter"}
            }
            if($exit){throw "$Kind harness exited with code ${exit}: $executable"}
        }
        if($summaryCount-eq0-or$totalPassed-eq0){throw "Proof filter matched no test in any harness: $Filter"}
        $script:proofCount++
    }finally{$timer.Stop();$phaseSeconds["$Kind-tests"]+=$timer.Elapsed.TotalSeconds}
}
function Add-Proof([Collections.IDictionary]$Proofs,$Kind,[int]$Partition,[string]$Filter,[string]$Label){if(-not$Filter){return};if($Partition-lt1-or$Partition-gt4){throw "Invalid $Kind partition."};$key="$Kind|$Partition|$Filter";if(-not$Proofs.Contains($key)){$Proofs[$key]=[pscustomobject]@{Kind=$Kind;Partition=$Partition;Filter=$Filter;Cards=@($Label)}}else{$Proofs[$key].Cards+=$Label}}
function Get-FixtureRunnerBinary([ValidateSet('contract','runtime')]$Kind){
    if($fixtureRunnerBinaries.ContainsKey($Kind)){return [string]$fixtureRunnerBinaries[$Kind]}
    $timer=[Diagnostics.Stopwatch]::StartNew();$mutex=$null;$lockTaken=$false
    try{
        $package=if($Kind-eq'contract'){'deepdeck-contract-fixture-runner'}else{'deepdeck-runtime-fixture-runner'}
        $directory=Join-Path $CacheDirectory 'binaries';New-Directory $directory
        $targetKey=(Get-Hash("target=$($cargoTargetRoot.ToLowerInvariant())")).Substring(0,16)
        $metaPath=Join-Path $directory "$package-debug-$targetKey.json"
        $mutexName="DeepDeckFixtureRunnerCache_$((Get-Hash("metadata=$($metaPath.ToLowerInvariant())")).Substring(0,32))"
        $mutex=[Threading.Mutex]::new($false,$mutexName)
        try{$lockTaken=$mutex.WaitOne([TimeSpan]::FromMinutes(15))}catch [Threading.AbandonedMutexException]{$lockTaken=$true}
        if(-not$lockTaken){throw "Timed out waiting for fixture runner cache lock: $metaPath"}
        $buildIdentity="$package|profile=debug|default-features=false"
        $inputs=@(Get-FixtureRunnerInputs $Kind);$fingerprint=Get-Fingerprint $inputs $buildIdentity
        $cached=Read-CacheMetadata $metaPath
        if(Test-BinaryCacheMetadata $cached "$Kind-fixture-runner" 'debug' $cargoTargetRoot $fingerprint){$script:fixtureRunnerBinaries[$Kind]=[string]$cached.executable;return [string]$cached.executable}
        for($attempt=1;$attempt-le3;$attempt++){
            $priorErrorActionPreference=$ErrorActionPreference
            try{$ErrorActionPreference='Continue';& cargo build --locked -p $package --no-default-features;$exit=$LASTEXITCODE}
            finally{$ErrorActionPreference=$priorErrorActionPreference}
            if($exit){throw "$package build failed."}
            $after=Get-Fingerprint (Get-FixtureRunnerInputs $Kind) $buildIdentity
            if($after-eq$fingerprint){break}
            if($attempt-eq3){throw "$package inputs remained unstable across 3 build attempts."}
            Write-Warning "$package inputs changed during build attempt $attempt; rebuilding.";$fingerprint=$after
        }
        $suffix=if([Environment]::OSVersion.Platform-eq[PlatformID]::Win32NT){'.exe'}else{''}
        $exe=Join-Path $cargoTargetRoot "debug\$package$suffix"
        if(-not(Test-Path -LiteralPath $exe -PathType Leaf)){throw "$package binary missing: $exe"}
        $metadata=[pscustomobject]@{schema_version=2;artifact_kind="$Kind-fixture-runner";profile='debug';target_directory=$cargoTargetRoot;fingerprint=$fingerprint;executable=$exe;executable_hash=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash}
        Write-CacheMetadata $metadata $metaPath;$verified=Read-CacheMetadata $metaPath
        if(-not(Test-BinaryCacheMetadata $verified "$Kind-fixture-runner" 'debug' $cargoTargetRoot $fingerprint)){throw "$package cache metadata failed post-build verification: $metaPath"}
        $script:binaryBuildCount++;$script:fixtureRunnerBinaries[$Kind]=[string]$verified.executable;return [string]$verified.executable
    }finally{if($lockTaken){$mutex.ReleaseMutex()};if($null-ne$mutex){$mutex.Dispose()};$timer.Stop();$phaseSeconds["$Kind-build"]+=$timer.Elapsed.TotalSeconds}
}
function Invoke-FixtureProof($Proof,[ValidateSet('contract','runtime')]$Kind,[string]$Label){
    $fixtureRelative=[string](Get-Property $Proof @('fixture_path','fixturePath'));$fixtureId=[string](Get-Property $Proof @('fixture_id','fixtureId'))
    if([string]::IsNullOrWhiteSpace($fixtureRelative)-or[string]::IsNullOrWhiteSpace($fixtureId)){throw "$Label $Kind fixture proof needs fixture_path and fixture_id."}
    $fixturePath=Resolve-ProofFile ($fixtureRelative-replace'\\','/');$runner=Get-FixtureRunnerBinary $Kind
    $timer=[Diagnostics.Stopwatch]::StartNew()
    try{
        if(-not$SummaryOnly){Write-Host "[$Kind fixture] $Label -> $fixtureId" -ForegroundColor Cyan}
        $output=@(& $runner $fixturePath 2>&1);$exit=$LASTEXITCODE;$results=@()
        foreach($raw in $output){
            if(-not$SummaryOnly){Write-Host $raw}
            try{$event=$raw.ToString()|ConvertFrom-Json -ErrorAction Stop}catch{throw "$Kind fixture runner emitted non-JSON output: $raw"}
            if([string](Get-Property $event @('fixtureId','fixture_id'))-eq$fixtureId){$results+=$event}
        }
        if($exit){throw "$Kind fixture runner exited with code $exit for $fixtureId."}
        if($results.Count-ne1){throw "$Kind fixture runner emitted $($results.Count) matching results for $fixtureId; expected exactly one."}
        $result=$results[0];$expectedSchema=if($Kind-eq'contract'){'deepdeck-contract-fixture-result/v1'}else{'deepdeck-runtime-fixture-result/v1'}
        if([string](Get-Property $result @('schemaVersion','schema_version'))-ne$expectedSchema){throw "$Kind fixture runner returned an unsupported result schema for $fixtureId."}
        if([string](Get-Property $result @('status'))-ne'passed'){throw "$Kind fixture proof failed for $fixtureId`: $([string](Get-Property $result @('error')))"}
        $script:proofCount++
    }finally{$timer.Stop();$phaseSeconds["$Kind-tests"]+=$timer.Elapsed.TotalSeconds}
}
function Invoke-ManifestProof($Spec,[string]$Kind,[string]$Label){
    $proofs=@(Get-ProofEntries (Get-Property $Spec @($Kind)) $Kind)
    foreach($proof in $proofs){
        $fixturePath=[string](Get-Property $proof @('fixture_path','fixturePath'))
        if(-not[string]::IsNullOrWhiteSpace($fixturePath)){Invoke-FixtureProof $proof $Kind $Label;continue}
        $partition=[int](Get-Property $proof @('partition'));$filter=[string](Get-Property $proof @('filter'))
        if($partition-lt1-or$partition-gt4){throw "$Label has invalid $Kind partition $partition."}
        if([string]::IsNullOrWhiteSpace($filter)){throw "$Label has empty $Kind proof filter."}
        $binaries=@(Get-TestBinaries $Kind $partition)
        Invoke-Proof $Kind $binaries $filter $Label
    }
}
function Apply-StrictProofLedger($Queue,$Inputs,[hashtable]$Statuses){
    $manifest=Read-ProofManifest;$resolved=@{}
    foreach($input in @($Inputs)){foreach($card in @($input.Cards)){foreach($key in(Get-CardKeys $card $input.SetCode)){$resolved[$key]=$card}}}
    $semanticsFingerprints=@{}
    foreach($entry in @($Queue)){
        $key=Get-QueueKey $entry;$record=Get-CardRecord $Statuses $entry (Get-QueueSet $entry)
        if($null-eq$record-or$record.Status-ne'canonical'-or$record.RecognizerStatus-ne'native'-or$record.IrClass-ne'primitive'){break}
        $oracle=[string](Get-Property $entry @('oracle_id','oracleId'))
        if([string]::IsNullOrWhiteSpace($oracle)){ $record.ProofStatus='missing';continue }
        $oracleKey="oracle:$($oracle.ToLowerInvariant())"
        if(-not$resolved.ContainsKey($oracleKey)){throw "Strict proof ledger cannot resolve card $oracle."}
        $record.OracleDigest=Get-OracleDigest $resolved[$oracleKey]
        if([string]::IsNullOrWhiteSpace([string]$record.SemanticIrDigest)){ $record.ProofStatus='missing';continue }
        $manifestKey=$oracle.ToLowerInvariant()
        if(-not$manifest.ContainsKey($manifestKey)){ $record.ProofStatus='missing';continue }
        $spec=$manifest[$manifestKey]
        $expectedSemanticIrDigest=[string](Get-Property $spec @('expected_semantic_ir_digest','expectedSemanticIrDigest'))
        if([string]::IsNullOrWhiteSpace($expectedSemanticIrDigest)){
            $record.ContractStatus='notChecked';$record.RuntimeStatus='notChecked'
            $record.ProofStatus='missing_ir_digest';$record.UnexecutableRuleCount=0
            continue
        }
        if(-not$expectedSemanticIrDigest.Equals([string]$record.SemanticIrDigest,[StringComparison]::OrdinalIgnoreCase)){
            $record.ContractStatus='notChecked';$record.RuntimeStatus='notChecked'
            $record.ProofStatus='semantic_ir_mismatch';$record.UnexecutableRuleCount=0
            continue
        }
        $specDigest=Get-ProofSpecDigest $spec
        $contractProof=Get-Property $spec @('contract');$runtimeProof=Get-Property $spec @('runtime')
        $contractModes=@((Get-ProofEntries $contractProof 'contract')|ForEach-Object{if([string]::IsNullOrWhiteSpace([string](Get-Property $_ @('fixture_path','fixturePath')))){'harness'}else{'fixture'}}|Sort-Object -Unique)
        $runtimeModes=@((Get-ProofEntries $runtimeProof 'runtime')|ForEach-Object{if([string]::IsNullOrWhiteSpace([string](Get-Property $_ @('fixture_path','fixturePath')))){'harness'}else{'fixture'}}|Sort-Object -Unique)
        $contractMode=$contractModes-join'+'
        $runtimeMode=$runtimeModes-join'+'
        $contractFingerprintKey="contract|$contractMode";$runtimeFingerprintKey="runtime|$runtimeMode"
        if(-not$semanticsFingerprints.ContainsKey($contractFingerprintKey)){$semanticsFingerprints[$contractFingerprintKey]=Get-ProofSemanticsFingerprint $contractProof 'contract'}
        if(-not$semanticsFingerprints.ContainsKey($runtimeFingerprintKey)){$semanticsFingerprints[$runtimeFingerprintKey]=Get-ProofSemanticsFingerprint $runtimeProof 'runtime'}
        $expected=[pscustomobject][ordered]@{
            schema_version='deepdeck-native-card-proof-cache/v1'
            oracle_id=$oracle.ToLowerInvariant()
            oracle_digest=$record.OracleDigest
            semantic_ir_digest=$record.SemanticIrDigest
            proof_spec_digest=$specDigest
            contract_semantics_fingerprint=$semanticsFingerprints[$contractFingerprintKey]
            runtime_semantics_fingerprint=$semanticsFingerprints[$runtimeFingerprintKey]
        }
        $cachePath=Get-ProofCachePath $oracle;$cached=Read-CacheMetadata $cachePath
        if(Test-ProofCache $cached $expected){
            $record.ContractStatus='executable';$record.RuntimeStatus='passed';$record.ProofStatus='cache_hit';$record.UnexecutableRuleCount=0
            continue
        }
        $record.ProofStatus=if($null-eq$cached){'missing'}else{'stale'}
        if(-not$RefreshProofs){continue}
        $label="$(Get-QueueLabel $entry)"
        Invoke-ManifestProof $spec 'contract' $label
        Invoke-ManifestProof $spec 'runtime' $label
        $verified=[pscustomobject][ordered]@{
            schema_version=$expected.schema_version
            oracle_id=$expected.oracle_id
            oracle_digest=$expected.oracle_digest
            semantic_ir_digest=$expected.semantic_ir_digest
            proof_spec_digest=$expected.proof_spec_digest
            contract_semantics_fingerprint=$expected.contract_semantics_fingerprint
            runtime_semantics_fingerprint=$expected.runtime_semantics_fingerprint
            contract_status='executable'
            runtime_status='passed'
            verified_at_utc=[DateTime]::UtcNow.ToString('o',[Globalization.CultureInfo]::InvariantCulture)
        }
        Write-JsonAtomically $verified $cachePath 10
        $readBack=Read-CacheMetadata $cachePath
        if(-not(Test-ProofCache $readBack $expected)){throw "Proof cache failed post-write verification: $cachePath"}
        $record.ContractStatus='executable';$record.RuntimeStatus='passed';$record.ProofStatus='verified';$record.UnexecutableRuleCount=0
    }
}
function Get-CardRecord([hashtable]$Statuses,$Card,[string]$FallbackSet){
    $set=Get-CardSet $Card $FallbackSet;$collector=[string](Get-Property $Card @('collector_number','collectorNumber'))
    if($set-and$collector){$printKey="print:$set`:$($collector.ToLowerInvariant())";if($Statuses.ContainsKey($printKey)){return $Statuses[$printKey]}}
    foreach($key in(Get-CardKeys $Card $FallbackSet)){if($Statuses.ContainsKey($key)){return $Statuses[$key]}}
    return $null
}
function Convert-BlockedKinds([string]$Value){if([string]::IsNullOrWhiteSpace($Value)){return @()};return @($Value.Split(',')|ForEach-Object{$_.Trim()}|Where-Object{$_}|Sort-Object -Unique)}
function Get-RecognizerNativePrefix($Cards){
    $length=0;$first=$null
    foreach($card in @($Cards)){
        if($card.parser_status-eq'canonical'-and$card.recognizer_status-eq'native'){$length++;continue}
        $reason=if($card.parser_status-ne'canonical'){"parser:$($card.parser_status)"}else{"recognizer:$($card.recognizer_status)"}
        $first=[pscustomobject][ordered]@{position=$card.position;oracle_id=$card.oracle_id;global_rank=$card.global_rank;name=$card.name;recognizer_status=$card.recognizer_status;reason=$reason}
        break
    }
    return [pscustomobject][ordered]@{length=$length;first_non_native=$first}
}
function Get-MigrationPrefix($Cards){
    $length=0;$first=$null
    foreach($card in @($Cards)){
        if($card.migration_status-eq'native-primitive-executable'){$length++;continue}
        $reason=if($card.parser_status-ne'canonical'){"parser:$($card.parser_status)"}elseif($card.recognizer_status-ne'native'){"recognizer:$($card.recognizer_status)"}elseif($card.ir_status-ne'primitive'){"ir:$($card.ir_status)"}elseif($card.contract_status-ne'executable'){"contract:$($card.contract_status)"}else{"runtime:$($card.runtime_status)"}
        $first=[pscustomobject][ordered]@{position=$card.position;oracle_id=$card.oracle_id;global_rank=$card.global_rank;name=$card.name;migration_status=$card.migration_status;reason=$reason}
        break
    }
    return [pscustomobject][ordered]@{length=$length;first_unmigrated=$first}
}
function New-AuditReportRecord($Subject,[hashtable]$Statuses){
    $entry=$Subject.Entry;$fallbackSet=[string]$Subject.FallbackSet;$record=Get-CardRecord $Statuses $entry $fallbackSet
    $parserStatus=if($null-eq$record){'not_checked'}else{[string]$record.Status}
    $recognizerStatus=if($null-eq$record-or-not$record.RecognizerStatus-or$record.RecognizerStatus-eq'notChecked'){'not_checked'}else{[string]$record.RecognizerStatus}
    $irStatus=if($null-eq$record-or-not$record.IrClass-or$record.IrClass-eq'unknown'){'not_checked'}else{[string]$record.IrClass}
    $contractStatus=if($irStatus-ne'primitive'){'not_applicable'}elseif($null-eq$record-or$record.ContractStatus-eq'notChecked'){'not_checked'}else{[string]$record.ContractStatus}
    $proofStatus=if($null-eq$record-or-not$record.ProofStatus){'not_applicable'}else{[string]$record.ProofStatus}
    $runtimeStatus=if($irStatus-ne'primitive'-or$proofStatus-eq'not_applicable'){'not_applicable'}elseif($null-eq$record-or-not$record.RuntimeStatus-or$record.RuntimeStatus-eq'notChecked'){'not_checked'}else{[string]$record.RuntimeStatus}
    $runtimeSatisfied=$runtimeStatus-eq'passed'-or$runtimeStatus-eq'not_applicable'
    $gapReason=if($parserStatus-ne'canonical'){"parser:$parserStatus"}elseif($recognizerStatus-ne'native'){"recognizer:$recognizerStatus"}elseif($irStatus-ne'primitive'){"ir:$irStatus"}elseif($contractStatus-ne'executable'){"contract:$contractStatus"}elseif(-not$runtimeSatisfied){"runtime:$runtimeStatus"}else{$null}
    $migrationStatus=if($parserStatus-ne'canonical'){'unsupported'}elseif($recognizerStatus-eq'mixed'){'partial-native'}elseif($recognizerStatus-eq'legacy'){'legacy'}elseif($recognizerStatus-ne'native'){'not_checked'}elseif($irStatus-ne'primitive'-or$contractStatus-ne'executable'-or-not$runtimeSatisfied){'native-semantic-gap'}else{'native-primitive-executable'}
    $rank=Get-Property $entry @('global_rank','globalRank');$set=Get-CardSet $entry $fallbackSet
    return [pscustomobject][ordered]@{
        position=[int]$Subject.Position
        oracle_id=[string](Get-Property $entry @('oracle_id','oracleId'))
        global_rank=if($null-eq$rank){$null}else{[int]$rank}
        name=[string](Get-Property $entry @('name'))
        set_code=$set
        collector_number=[string](Get-Property $entry @('collector_number','collectorNumber'))
        parser_status=$parserStatus
        recognizer_status=$recognizerStatus
        native_ability_count=if($null-eq$record){0}else{[int]$record.NativeAbilityCount}
        legacy_ability_count=if($null-eq$record){0}else{[int]$record.LegacyAbilityCount}
        unclaimed_ability_count=if($null-eq$record){0}else{[int]$record.UnclaimedAbilityCount}
        migration_status=$migrationStatus
        ir_status=$irStatus
        blocked_kinds=@(if($irStatus-ne'composite'-or$null-eq$record){@()}else{Convert-BlockedKinds ([string]$record.BlockedKinds)})
        contract_status=$contractStatus
        runtime_status=$runtimeStatus
        proof_status=$proofStatus
        oracle_digest=if($null-eq$record){$null}else{[string]$record.OracleDigest}
        semantic_ir_digest=if($null-eq$record){$null}else{[string]$record.SemanticIrDigest}
        unexecutable_rule_count=if($null-eq$record){0}else{[int]$record.UnexecutableRuleCount}
        gap=$null-ne$gapReason
        gap_reason=$gapReason
    }
}
function New-AuditReport($Subjects,[hashtable]$Statuses,[bool]$ExecutionComplete,[string]$FailureStage,$Source){
    $cards=@($Subjects|ForEach-Object{New-AuditReportRecord $_ $Statuses})
    $recognizerPrefix=Get-RecognizerNativePrefix $cards
    $migrationPrefix=Get-MigrationPrefix $cards
    $strictPrefix=[bool](Get-Property $Source @('stop_at_first_non_native'))
    $missing=@($cards|Where-Object{$_.parser_status-eq'not_checked'-or$_.recognizer_status-eq'not_checked'-or$_.ir_status-eq'not_checked'-or(-not$strictPrefix-and$_.ir_status-eq'primitive'-and$_.contract_status-eq'not_checked')})
    $complete=$ExecutionComplete-and$missing.Count-eq0
    $stage=if($complete){$null}elseif($FailureStage){$FailureStage}else{'report-validation'}
    $first=if($strictPrefix-and$null-ne$recognizerPrefix.first_non_native){
        $position=[int]$recognizerPrefix.first_non_native.position
        $cards|Where-Object{$_.position-eq$position}|Select-Object -First 1
    }else{$cards|Where-Object{$_.gap}|Select-Object -First 1}
    $firstGap=if($null-eq$first){$null}else{[pscustomobject][ordered]@{position=$first.position;oracle_id=$first.oracle_id;global_rank=$first.global_rank;name=$first.name;reason=$first.gap_reason}}
    return [pscustomobject][ordered]@{
        schema_version='deepdeck-card-audit/v3'
        generated_at_utc=[DateTime]::UtcNow.ToString('o',[Globalization.CultureInfo]::InvariantCulture)
        complete=$complete
        failure_stage=$stage
        source=$Source
        summary=[pscustomobject][ordered]@{
            total=$cards.Count
            canonical=@($cards|Where-Object{$_.parser_status-eq'canonical'}).Count
            full_native_recognizer=@($cards|Where-Object{$_.parser_status-eq'canonical'-and$_.recognizer_status-eq'native'}).Count
            recognizer_native_prefix_length=[int]$recognizerPrefix.length
            migration_prefix_length=[int]$migrationPrefix.length
            native_recognizer=@($cards|Where-Object{$_.recognizer_status-eq'native'}).Count
            mixed_recognizer=@($cards|Where-Object{$_.recognizer_status-eq'mixed'}).Count
            legacy_recognizer=@($cards|Where-Object{$_.recognizer_status-eq'legacy'}).Count
            unsupported_recognizer=@($cards|Where-Object{$_.recognizer_status-eq'unsupported'}).Count
            native_primitive_executable=@($cards|Where-Object{$_.migration_status-eq'native-primitive-executable'}).Count
            native_abilities=($cards|Measure-Object -Property native_ability_count -Sum).Sum
            legacy_abilities=($cards|Measure-Object -Property legacy_ability_count -Sum).Sum
            unclaimed_abilities=($cards|Measure-Object -Property unclaimed_ability_count -Sum).Sum
            primitive=@($cards|Where-Object{$_.ir_status-eq'primitive'}).Count
            composite=@($cards|Where-Object{$_.ir_status-eq'composite'}).Count
            executable=@($cards|Where-Object{$_.contract_status-eq'executable'}).Count
            runtime_passed=@($cards|Where-Object{$_.runtime_status-eq'passed'}).Count
            proof_cache_hits=@($cards|Where-Object{$_.proof_status-eq'cache_hit'}).Count
            proofs_verified=@($cards|Where-Object{$_.proof_status-eq'verified'}).Count
            gaps=@($cards|Where-Object{$_.gap}).Count
        }
        first_non_native=$recognizerPrefix.first_non_native
        first_unmigrated=$migrationPrefix.first_unmigrated
        first_gap=$firstGap
        cards=$cards
    }
}

New-Directory $CacheDirectory;New-Directory $TargetDirectory;$queue=@(Read-Queue)
if($QueueFile){
    $architectureReportPath=Join-Path $CacheDirectory "architecture-audit-$PID.json"
    $timer=[Diagnostics.Stopwatch]::StartNew()
    try{
        & (Join-Path $PSScriptRoot 'check-card-migration-architecture.ps1') -AffectedManifest $QueueFile -Enforce -SummaryOnly:$SummaryOnly -CacheDirectory (Join-Path $CacheDirectory 'architecture') -ReportJson $architectureReportPath
        if($LASTEXITCODE){throw 'Architecture guard failed.'}
        $architectureFingerprint=[string](Get-Content -LiteralPath $architectureReportPath -Raw|ConvertFrom-Json).fingerprint
        if([string]::IsNullOrWhiteSpace($architectureFingerprint)){throw 'Architecture guard omitted its fingerprint.'}
    }finally{$timer.Stop();$phaseSeconds.architecture+=$timer.Elapsed.TotalSeconds}
}
if($PlanOnly){Write-Host "Plan: $($queue.Count) queued cards; target=$TargetDirectory" -ForegroundColor Cyan;$queue|Select-Object -First 20|ForEach-Object{Write-Host "QUEUE $(Get-QueueLabel $_)"};if($queue.Count-gt20){Write-Host "... $($queue.Count-20) more"};exit 0}

Push-Location $repoRoot
try{
    Write-Host "Repository: $repoRoot`nCache: $CacheDirectory`nCargo target: $TargetDirectory`nParser profile: $ParserBuildProfile"
    $currentStage='input'
    $timer=[Diagnostics.Stopwatch]::StartNew();try{$inputs=@(New-AuditInputs $queue)}finally{$timer.Stop();$phaseSeconds.input+=$timer.Elapsed.TotalSeconds};if(-not$inputs.Count){throw 'No audit inputs.'}
    $position=0
    if($queue.Count){foreach($entry in $queue){$position++;$reportSubjects+=[pscustomobject]@{Position=$position;Entry=$entry;FallbackSet=(Get-QueueSet $entry)}}}
    else{foreach($input in $inputs){foreach($card in $input.Cards){$position++;$reportSubjects+=[pscustomobject]@{Position=$position;Entry=$card;FallbackSet=$input.SetCode}}}}
    $currentStage='cargo-metadata';Initialize-Cargo
    $currentStage='parser-build';$statuses=@{};$parser=Get-ParserBinary
    foreach($input in $inputs){$currentStage="parser-audit:$($input.SetCode)";Write-Host "Parser audit $($input.SetCode): $($input.Cards.Count) cards complete=$($input.Complete)" -ForegroundColor Cyan;$timer=[Diagnostics.Stopwatch]::StartNew();$strictEnvExisted=Test-Path Env:AUDIT_STOP_AT_FIRST_NON_NATIVE;$strictEnvPrevious=$env:AUDIT_STOP_AT_FIRST_NON_NATIVE;$priorErrorActionPreference=$ErrorActionPreference;try{if($StopAtFirstNonNative){$env:AUDIT_STOP_AT_FIRST_NON_NATIVE='1'}else{Remove-Item Env:AUDIT_STOP_AT_FIRST_NON_NATIVE -ErrorAction SilentlyContinue};$ErrorActionPreference='Continue';$output=@(& $parser $input.Path 2>&1);$exit=$LASTEXITCODE}finally{$ErrorActionPreference=$priorErrorActionPreference;if($strictEnvExisted){$env:AUDIT_STOP_AT_FIRST_NON_NATIVE=$strictEnvPrevious}else{Remove-Item Env:AUDIT_STOP_AT_FIRST_NON_NATIVE -ErrorAction SilentlyContinue};$timer.Stop();$phaseSeconds['parser-audit']+=$timer.Elapsed.TotalSeconds};Assert-ParserAuditOutput $output $exit ([bool]$StopAtFirstNonNative) $input.Cards.Count $input.SetCode;$byAuditKey=@{};foreach($card in $input.Cards){$auditKey=Get-AuditIdentity $card;if(-not$auditKey){throw "Card has no audit identity in $($input.SetCode)."};if($byAuditKey.ContainsKey($auditKey)){throw "Audit identity collision $($input.SetCode)/$auditKey"};$byAuditKey[$auditKey]=$card}
        foreach($raw in $output){
            $line=$raw.ToString();if(-not$SummaryOnly-or$line-notmatch'^RULES\t'){Write-Host $line}
            if($line-match'^CARD\t([^\t]+)\t(canonical|unsupported)\t([^\t]+)\t([0-9]+)us$'){
                $auditKey=$Matches[1];$status=$Matches[2];$name=$Matches[3]
                if(-not$byAuditKey.ContainsKey($auditKey)){throw "Unknown audit identity output $auditKey"}
                $card=$byAuditKey[$auditKey];$auditedCardCount++
                if($status-eq'unsupported'){$unsupportedObserved++;if($RequireFullSetCoverage-and$input.Complete){$fullCoverageFailed=$true}}
                foreach($key in(Get-CardKeys $card $input.SetCode)){
                    $record=[pscustomobject]@{Status=$status;Name=$name;RecognizerStatus='notChecked';NativeAbilityCount=0;LegacyAbilityCount=0;UnclaimedAbilityCount=0;IrClass='unknown';BlockedKinds='';ContractStatus='notChecked';RuntimeStatus='notChecked';ProofStatus='not_applicable';OracleDigest=$null;SemanticIrDigest=$null;UnexecutableRuleCount=0}
                    if(-not$statuses.ContainsKey($key)-or$status-eq'unsupported'){$statuses[$key]=$record}
                }
                continue
            }
            if($line-match'^RECOGNIZER\t([^\t]+)\t(native|mixed|legacy|unsupported)\t([0-9]+)\t([0-9]+)\t([0-9]+)\t([^\t]+)$'){
                $auditKey=$Matches[1];$recognizerStatus=$Matches[2];$nativeAbilities=[int]$Matches[3];$legacyAbilities=[int]$Matches[4];$unclaimedAbilities=[int]$Matches[5]
                if(-not$byAuditKey.ContainsKey($auditKey)){throw "Unknown recognizer audit identity output $auditKey"}
                $card=$byAuditKey[$auditKey]
                switch($recognizerStatus){'native'{$nativeRecognizerObserved++};'mixed'{$mixedRecognizerObserved++};'legacy'{$legacyRecognizerObserved++};'unsupported'{$unsupportedRecognizerObserved++}}
                $nativeAbilityObserved+=$nativeAbilities;$legacyAbilityObserved+=$legacyAbilities;$unclaimedAbilityObserved+=$unclaimedAbilities
                foreach($key in(Get-CardKeys $card $input.SetCode)){
                    if(-not$statuses.ContainsKey($key)){throw "RECOGNIZER output preceded CARD output for $auditKey"}
                    $statuses[$key].RecognizerStatus=$recognizerStatus;$statuses[$key].NativeAbilityCount=$nativeAbilities;$statuses[$key].LegacyAbilityCount=$legacyAbilities;$statuses[$key].UnclaimedAbilityCount=$unclaimedAbilities
                }
                continue
            }
            if($line-match'^IR\t([^\t]+)\t(primitive|composite|unsupported)\t([^\t]+)\t([^\t]*)$'){
                $auditKey=$Matches[1];$irClass=$Matches[2];$blockedKinds=$Matches[4]
                if(-not$byAuditKey.ContainsKey($auditKey)){throw "Unknown IR audit identity output $auditKey"}
                $card=$byAuditKey[$auditKey]
                if($irClass-eq'primitive'){$primitiveObserved++}
                elseif($irClass-eq'composite'){
                    $compositeObserved++
                    if($RequireFullSetCoverage-and$input.Complete-and-not$AllowCompositeCanonical){$fullCoverageFailed=$true}
                }
                foreach($key in(Get-CardKeys $card $input.SetCode)){
                    if(-not$statuses.ContainsKey($key)){throw "IR output preceded CARD output for $auditKey"}
                    $statuses[$key].IrClass=$irClass;$statuses[$key].BlockedKinds=$blockedKinds
                }
                continue
            }
            if($line-match'^RULES\t([^\t]+)\t(.+)$'){
                $auditKey=$Matches[1];$rulesJson=$Matches[2]
                if(-not$byAuditKey.ContainsKey($auditKey)){throw "Unknown RULES audit identity output $auditKey"}
                $card=$byAuditKey[$auditKey];$digest=Get-SemanticIrDigest $rulesJson
                foreach($key in(Get-CardKeys $card $input.SetCode)){
                    if(-not$statuses.ContainsKey($key)){throw "RULES output preceded CARD output for $auditKey"}
                    $statuses[$key].SemanticIrDigest=$digest
                }
            }
        }
    }
    if($StopAtFirstNonNative){
        $currentStage='proof-ledger'
        Apply-StrictProofLedger $queue $inputs $statuses
        $prefixCards=@($reportSubjects|ForEach-Object{New-AuditReportRecord $_ $statuses})
        $nativePrefix=Get-RecognizerNativePrefix $prefixCards
        if($null-ne$nativePrefix.first_non_native){
            $first=$nativePrefix.first_non_native
            $currentStage="native-prefix:$($first.position)"
            throw "First non-native card at queue position $($first.position): $($first.name) ($($first.reason)); native prefix length=$($nativePrefix.length)."
        }
    }
    if(-not$StopAtFirstNonNative){
    $contractInputs=@()
    foreach($input in $inputs){
        $primitiveCards=@($input.Cards|Where-Object{$record=Get-CardRecord $statuses $_ $input.SetCode;$null-ne$record-and$record.Status-eq'canonical'-and$record.IrClass-eq'primitive'})
        if($primitiveCards.Count){
            $contractInputs+=[pscustomobject]@{SetCode=$input.SetCode;Path=(Write-Group $input.SetCode $primitiveCards "contract-$($input.SetCode)");Cards=$primitiveCards;Complete=$input.Complete}
        }
    }
    if($contractInputs.Count){
        $currentStage='contract-audit-build'
        $contractAudit=Get-ContractAuditBinary
        foreach($input in $contractInputs){
            $currentStage="contract-audit:$($input.SetCode)"
            Write-Host "Contract audit $($input.SetCode): $($input.Cards.Count) structurally primitive cards" -ForegroundColor Cyan
            $timer=[Diagnostics.Stopwatch]::StartNew();$priorErrorActionPreference=$ErrorActionPreference
            try{
                $ErrorActionPreference='Continue'
                $output=@(& $contractAudit $input.Path 2>&1)
                $exit=$LASTEXITCODE
            }finally{
                $ErrorActionPreference=$priorErrorActionPreference
                $timer.Stop();$phaseSeconds['contract-audit']+=$timer.Elapsed.TotalSeconds
            }
            Assert-ContractAuditExitCode $exit $input.SetCode
            $byCollector=@{};foreach($card in $input.Cards){$collector=[string](Get-Property $card @('collector_number','collectorNumber'));if($byCollector.ContainsKey($collector)){throw "Contract collector collision $($input.SetCode)/$collector"};$byCollector[$collector]=$card}
            $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
            foreach($raw in $output){
                $line=$raw.ToString();Write-Host $line
                if($line-match'^CONTRACT\t([^\t]+)\t(executable|unexecutable|unsupported)\t([^\t]+)\t([0-9]+)$'){
                    $collector=$Matches[1];$contractStatus=$Matches[2];$failedRules=[int]$Matches[4]
                    if(-not$byCollector.ContainsKey($collector)){throw "Unknown contract collector output $collector"}
                    if(-not$seen.Add($collector)){throw "Duplicate contract collector output $collector"}
                    $card=$byCollector[$collector]
                    if($contractStatus-eq'executable'){$contractExecutableObserved++}else{$contractUnexecutableObserved++;if($RequireFullSetCoverage-and$input.Complete){$fullCoverageFailed=$true}}
                    foreach($key in(Get-CardKeys $card $input.SetCode)){
                        if(-not$statuses.ContainsKey($key)){throw "Contract output has no parser status for $collector"}
                        $statuses[$key].ContractStatus=$contractStatus;$statuses[$key].UnexecutableRuleCount=$failedRules
                    }
                }
            }
            Assert-ContractAuditResultCount $seen.Count $input.Cards.Count $input.SetCode
        }
    }
    $proofs=[ordered]@{}
    foreach($entry in $queue){
        $key=Get-QueueKey $entry;$label=Get-QueueLabel $entry
        if(-not$statuses.ContainsKey($key)){Write-Warning "$label absent";$affectedFailed=$true;continue}
        $record=$statuses[$key]
        if($record.Status-ne'canonical'){Write-Warning "$label unsupported";$affectedFailed=$true;continue}
        $affectedCanonicalCount++
        if($record.IrClass-eq'composite'){$affectedCompositeCount++;Write-Warning "$label canonical but composite IR: $($record.BlockedKinds)";if(-not$AllowCompositeCanonical){$affectedFailed=$true};continue}
        if($record.IrClass-ne'primitive'){Write-Warning "$label has no valid IR architecture result";$affectedFailed=$true;continue}
        $affectedPrimitiveCount++
        if($record.ContractStatus-ne'executable'){
            $affectedContractUnexecutableCount++;Write-Warning "$label structurally primitive but contract $($record.ContractStatus) ($($record.UnexecutableRuleCount) rejected rules)";$affectedFailed=$true;continue
        }
        $affectedContractExecutableCount++
        $contract=[string](Get-Property $entry @('contractTest','contract_test'));$runtime=[string](Get-Property $entry @('runtimeTest','runtime_test'));$cp=Get-Property $entry @('contractPartition','contract_partition');$rp=Get-Property $entry @('runtimePartition','runtime_partition');Add-Proof $proofs contract $(if($null-ne$cp){[int]$cp}else{3}) $contract $label;Add-Proof $proofs runtime $(if($null-ne$rp){[int]$rp}else{2}) $runtime $label
    }
    $currentStage='proof-tests';$binaries=@{};foreach($proof in $proofs.Values){$key="$($proof.Kind)|$($proof.Partition)";if(-not$binaries.ContainsKey($key)){$binaries[$key]=@(Get-TestBinaries $proof.Kind $proof.Partition)};Invoke-Proof $proof.Kind @($binaries[$key]) $proof.Filter ($proof.Cards-join', ')}
    }
    $currentStage='complete'
}catch{$fatalError=$_;$failureStage=$currentStage}finally{Pop-Location}

if($QueueFile-and-not[string]::IsNullOrWhiteSpace($architectureFingerprint)){
    try{
        & (Join-Path $PSScriptRoot 'check-card-migration-architecture.ps1') -AffectedManifest $QueueFile -Enforce -SummaryOnly -CacheDirectory (Join-Path $CacheDirectory 'architecture') -ReportJson $architectureReportPath
        if($LASTEXITCODE){throw 'Final architecture guard failed.'}
        $finalArchitectureFingerprint=[string](Get-Content -LiteralPath $architectureReportPath -Raw|ConvertFrom-Json).fingerprint
        if($finalArchitectureFingerprint-ne$architectureFingerprint){throw 'Architecture inputs changed during the audit.'}
    }catch{
        $fatalError=$_
        $failureStage='architecture-drift'
    }
}
$overallTimer.Stop()

if($ReportJson){
    try{
        $source=[pscustomobject][ordered]@{queue_file=$QueueFile;set_codes=@($SetCode);cards_json=$CardsJson;stop_at_first_non_native=[bool]$StopAtFirstNonNative;refresh_proofs=[bool]$RefreshProofs;proof_manifest=$ProofManifest;architecture_fingerprint=$architectureFingerprint;parser_fingerprint=$parserAuditFingerprint;contract_fingerprint=$contractAuditFingerprint}
        $report=New-AuditReport $reportSubjects $statuses ($null-eq$fatalError) $failureStage $source
        $reportTimings=[ordered]@{}
        $measuredPhaseTotal=0.0
        foreach($phase in $phaseSeconds.Keys){
            $reportTimings[$phase]=[Math]::Round([double]$phaseSeconds[$phase],3)
            $measuredPhaseTotal+=[double]$phaseSeconds[$phase]
        }
        $reportTimings.overhead=[Math]::Round([Math]::Max(0.0,$overallTimer.Elapsed.TotalSeconds-$measuredPhaseTotal),3)
        $reportTimings.total=[Math]::Round($overallTimer.Elapsed.TotalSeconds,3)
        $report|Add-Member -NotePropertyName timings_seconds -NotePropertyValue ([pscustomobject]$reportTimings)
        $report|Add-Member -NotePropertyName run_metrics -NotePropertyValue ([pscustomobject][ordered]@{
            binary_build_count=$binaryBuildCount
            network_fetch_count=$networkFetchCount
            proof_count=$proofCount
            warm=($binaryBuildCount-eq0-and$networkFetchCount-eq0)
            budget_seconds=$BudgetSeconds
            within_budget=($overallTimer.Elapsed.TotalSeconds-le$BudgetSeconds)
        })
        if(-not$report.complete-and$null-eq$fatalError){$fatalError=[Management.Automation.ErrorRecord]::new([InvalidOperationException]::new("Audit report is incomplete at $($report.failure_stage)."),'IncompleteAuditReport',[Management.Automation.ErrorCategory]::InvalidResult,$ReportJson);$failureStage=[string]$report.failure_stage}
        $infrastructureFailure=Test-IsInfrastructureFailure ($null-ne$fatalError) $failureStage
        $publishedReport=Publish-AuditReport $report $ReportJson $infrastructureFailure
        if($publishedReport-eq$ReportJson){
            Write-Host "Audit JSON report: $ReportJson complete=$($report.complete)" -ForegroundColor Cyan
        }else{
            Write-Warning "Infrastructure failure preserved last-good report $ReportJson; failure report: $publishedReport"
        }
    }catch{if($null-eq$fatalError){$fatalError=$_;$failureStage='report-write'}else{Write-Warning "Audit report write failed: $($_.Exception.Message)"}}
}

$warm=$binaryBuildCount-eq0-and$networkFetchCount-eq0;$within=$overallTimer.Elapsed.TotalSeconds-le$BudgetSeconds
$coverageCards=@($reportSubjects|ForEach-Object{New-AuditReportRecord $_ $statuses})
$recognizerPrefix=Get-RecognizerNativePrefix $coverageCards;$migrationPrefix=Get-MigrationPrefix $coverageCards
$fullNativeRecognizer=@($coverageCards|Where-Object{$_.parser_status-eq'canonical'-and$_.recognizer_status-eq'native'}).Count
$nativePrimitiveExecutable=@($coverageCards|Where-Object{$_.migration_status-eq'native-primitive-executable'}).Count
$measuredPhaseTotal=0.0;foreach($phase in $phaseSeconds.Keys){$measuredPhaseTotal+=[double]$phaseSeconds[$phase]}
$overheadSeconds=[Math]::Max(0.0,$overallTimer.Elapsed.TotalSeconds-$measuredPhaseTotal)
Write-Host 'Audit timing summary' -ForegroundColor Cyan;foreach($phase in $phaseSeconds.Keys){Write-Host('{0,-22} {1,9:N2} s'-f$phase,$phaseSeconds[$phase])};Write-Host('{0,-22} {1,9:N2} s'-f'overhead',$overheadSeconds);Write-Host('{0,-22} {1,9:N2} s'-f'total',$overallTimer.Elapsed.TotalSeconds)
Write-Host "cards=$auditedCardCount full-native-recognizer=$fullNativeRecognizer native-primitive-executable=$nativePrimitiveExecutable recognizer-native-prefix=$($recognizerPrefix.length) migration-prefix=$($migrationPrefix.length) recognizer-native=$nativeRecognizerObserved recognizer-mixed=$mixedRecognizerObserved recognizer-legacy=$legacyRecognizerObserved recognizer-unsupported=$unsupportedRecognizerObserved abilities-native=$nativeAbilityObserved abilities-legacy=$legacyAbilityObserved abilities-unclaimed=$unclaimedAbilityObserved primitive=$primitiveObserved composite=$compositeObserved unsupported=$unsupportedObserved contract-executable=$contractExecutableObserved contract-unexecutable=$contractUnexecutableObserved queued-canonical=$affectedCanonicalCount/$($queue.Count) queued-primitive=$affectedPrimitiveCount queued-composite=$affectedCompositeCount queued-contract-executable=$affectedContractExecutableCount queued-contract-unexecutable=$affectedContractUnexecutableCount proofs=$proofCount fetches=$networkFetchCount builds=$binaryBuildCount warm-budget=$(if($within){'PASS'}else{'MISS'})"
if($null-ne$recognizerPrefix.first_non_native){Write-Host "first-non-native=$($recognizerPrefix.first_non_native.position):$($recognizerPrefix.first_non_native.name):$($recognizerPrefix.first_non_native.reason)"}
if($null-ne$migrationPrefix.first_unmigrated){Write-Host "first-unmigrated=$($migrationPrefix.first_unmigrated.position):$($migrationPrefix.first_unmigrated.name):$($migrationPrefix.first_unmigrated.reason)"}
if($fatalError){Write-Error -ErrorAction Continue $fatalError.Exception.Message;exit 1};if($affectedFailed){Write-Error -ErrorAction Continue 'Affected queue contains absent, unsupported, composite, or contract-unexecutable cards.';exit 1};if($RequireFullSetCoverage-and$fullCoverageFailed){Write-Error -ErrorAction Continue 'Full-set coverage found unsupported, composite, or contract-unexecutable cards.';exit 1};if($EnforceBudget-and$warm-and-not$within){Write-Error -ErrorAction Continue 'Warm audit exceeded budget.';exit 1};exit 0
