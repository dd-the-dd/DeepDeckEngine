$ErrorActionPreference = 'Stop'

$scriptPath = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\audit-card-set.ps1'))
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile(
    $scriptPath,
    [ref]$tokens,
    [ref]$parseErrors
)

if ($parseErrors.Count -ne 0) {
    throw "audit-card-set.ps1 has PowerShell parse errors: $($parseErrors.Message -join '; ')"
}
$scriptSource = $ast.Extent.Text
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

$contractManifest = Get-Content -LiteralPath (Join-Path $repoRoot 'crates\deepdeck-oracle-engine-contracts\Cargo.toml') -Raw
$contractLib = Get-Content -LiteralPath (Join-Path $repoRoot 'crates\deepdeck-oracle-engine-contracts\src\lib.rs') -Raw
$oracleManifest = Get-Content -LiteralPath (Join-Path $repoRoot 'crates\deepdeck-oracle\Cargo.toml') -Raw
$oracleLib = Get-Content -LiteralPath (Join-Path $repoRoot 'crates\deepdeck-oracle\src\lib.rs') -Raw
$parserRunnerManifest = Get-Content -LiteralPath (Join-Path $repoRoot 'crates\deepdeck-parser-fixture-runner\Cargo.toml') -Raw -Encoding UTF8
$ciWorkflow = Get-Content -LiteralPath (Join-Path $repoRoot '.github\workflows\ci.yml') -Raw
Assert-True ($contractManifest -match 'deepdeck-engine-runtime\s*=\s*\{[^}]*default-features\s*=\s*false' -and
    $contractManifest -match 'deepdeck-oracle\s*=\s*\{' -and
    $contractManifest -notmatch '(?m)^regex\s*=') `
    'The contract crate must join the parser and parser-free runtime without compiling an embedded parser copy.'
Assert-True ($contractLib -match 'pub use deepdeck_oracle::\{card_catalog, oracle\}' -and
    $contractLib -match 'pub use deepdeck_engine_runtime::engine' -and
    $contractLib -match '#\[path\s*=\s*"../../../src/model.rs"\]\s*pub mod model;' -and
    $contractLib -notmatch '#\[path\s*=\s*"[^"]*src/(oracle|engine)') `
    'Contracts must join public parser/runtime re-exports and the shared model adapter without embedding a parser or runtime copy.'
Assert-True ($oracleManifest -notmatch '(?m)^deepdeck-engine-runtime\s*=' -and
    $oracleLib -notmatch 'pub use deepdeck_engine_runtime::engine') `
    'The standalone parser must remain independent of the engine, including private tests; cross-boundary proofs belong in contracts.'
Assert-True ($parserRunnerManifest -match '(?m)^local-audit\s*=\s*\[\]' -and
    $parserRunnerManifest -match '(?m)^audit-cli\s*=\s*\["local-audit", "dep:ureq"\]' -and
    $parserRunnerManifest -match 'required-features\s*=\s*\["local-audit"\]' -and
    $parserRunnerManifest -notmatch 'deepdeck-oracle/audit-cli') `
    'Local audit must omit networking while the existing audit-cli keeps remote access without changing Oracle features.'
Assert-True ($ciWorkflow -match 'cargo test --locked -p deepdeck-oracle --lib --no-default-features --features "ci-unit-tests-\$partition"' -and
    $ciWorkflow -match 'cargo test --locked -p deepdeck-oracle-engine-contracts --tests') `
    'CI must execute private Oracle partitions and contract integration tests after thinning the contract crate.'

Assert-True ($scriptSource -match 'NotePropertyName timings_seconds') `
    'Machine-readable audit reports must persist phase timings.'
Assert-True ($scriptSource -match '\$reportTimings\.overhead') `
    'Audit timing must account for wall-clock overhead outside named phases.'
Assert-True ($scriptSource -match 'NotePropertyName run_metrics') `
    'Machine-readable audit reports must persist build, proof, and warm-cache metrics.'
Assert-True ($scriptSource -match 'architecture_fingerprint=\$architectureFingerprint' -and
    $scriptSource -match "failureStage='architecture-drift'" -and
    $scriptSource -match '\$finalArchitectureFingerprint-ne\$architectureFingerprint') `
    'Queue audits must persist and recheck the architecture fingerprint after all proofs.'
Assert-True ($scriptSource -match "\$line-notmatch'\^RULES\\t'" -and
    $scriptSource -match '\[switch\] \$SummaryOnly') `
    'SummaryOnly must suppress RULES console output without skipping its validation.'
function Get-FunctionText([string]$Name) {
    $function = $ast.Find({
        param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -eq $Name
    }, $true)
    Assert-True ($null -ne $function) "Missing function $Name."
    return $function.Extent.Text
}

$fixtureProof = Get-FunctionText 'Invoke-FixtureProof'
$harnessProof = Get-FunctionText 'Invoke-Proof'
Assert-True ($fixtureProof -match 'if\(-not\$SummaryOnly\).*Write-Host' -and
    $harnessProof -match 'if\(-not\$SummaryOnly\).*Write-Host') `
    'SummaryOnly must suppress successful proof chatter while proofs remain parsed and validated.'

$proofProductionFiles = Get-FunctionText 'Get-ProofProductionFiles'
Assert-True ($proofProductionFiles -match "src\\oracle" -and
    $proofProductionFiles -match "crates\\deepdeck-oracle\\src") `
    'Contract harness proof fingerprints must invalidate when parser production sources change.'

$profileParameter = $ast.ParamBlock.Parameters | Where-Object {
    $_.Name.VariablePath.UserPath -eq 'ParserBuildProfile'
}
Assert-True ($null -ne $profileParameter) 'Missing ParserBuildProfile parameter.'
Assert-True ($profileParameter.DefaultValue.Extent.Text -eq "'release'") `
    'ParserBuildProfile must default to release.'
$validateSet = $profileParameter.Attributes | Where-Object {
    $_.TypeName.Name -eq 'ValidateSet'
}
Assert-True ($null -ne $validateSet) 'ParserBuildProfile must have ValidateSet.'
$allowedProfiles = @($validateSet.PositionalArguments | ForEach-Object {
    $_.SafeGetValue().ToString()
})
Assert-True ($allowedProfiles.Count -eq 2 -and
    $allowedProfiles -contains 'release' -and $allowedProfiles -contains 'debug') `
    'ParserBuildProfile must allow exactly release and debug.'
$strictPrefixParameter = $ast.ParamBlock.Parameters | Where-Object {
    $_.Name.VariablePath.UserPath -eq 'StopAtFirstNonNative'
}
Assert-True ($null -ne $strictPrefixParameter) `
    'Audit runner must expose strict first-non-native queue mode.'
$refreshProofsParameter = $ast.ParamBlock.Parameters | Where-Object {
    $_.Name.VariablePath.UserPath -eq 'RefreshProofs'
}
Assert-True ($null -ne $refreshProofsParameter) `
    'Audit runner must expose explicit proof refresh; cache files cannot certify themselves.'
$strictInputs = Get-FunctionText 'New-AuditInputs'
Assert-True ($strictInputs -match 'New-StrictQueueCards' -and
    $strictInputs -match '\$inputs\.Clear\(\)' -and
    $strictInputs -match "SetCode='priority'") `
    'Strict mode must replace set-grouped inputs with one globally ordered queue input.'
$scriptText = Get-Content -LiteralPath $scriptPath -Raw
Assert-True ($scriptText -match 'AUDIT_STOP_AT_FIRST_NON_NATIVE' -and
    $scriptText -match '\$byAuditKey' -and $scriptText -match "\^RULES\\t" -and
    $scriptText -match '\$ErrorActionPreference=''Continue'';\$output=@\(& \$parser \$input\.Path 2>&1\)' -and
    $scriptText -match 'Assert-ParserAuditOutput') `
    'Strict parser invocation must enable binary early-stop and map output by stable audit key.'
$parserOutputValidation = Get-FunctionText 'Assert-ParserAuditOutput'
Assert-True ($parserOutputValidation -match '\$strictStop=\$Strict-and\$exitValue-eq1' -and
    $parserOutputValidation -match 'unexpected exit code' -and
    $parserOutputValidation -match "@\('CARD','RECOGNIZER','IR','RULES'\)" -and
    $parserOutputValidation -match 'FIRST_NON_NATIVE') `
    'Parser output validation must accept only a proved strict exit 1 and reject incomplete marker sets.'
$contractExitValidation = Get-FunctionText 'Assert-ContractAuditExitCode'
Assert-True ($contractExitValidation -match '\$exitValue-ne0-and\$exitValue-ne1' -and
    $contractExitValidation -match 'unexpected exit code') `
    'Contract audit output must accept only semantic exit codes 0/1 and reject infrastructure exits.'
$contractResultValidation = Get-FunctionText 'Assert-ContractAuditResultCount'
Assert-True ($contractResultValidation -match '\$Actual-ne\$Expected') `
    'Contract audit output must validate complete result cardinality.'
Assert-True ($scriptText -match '\$ErrorActionPreference=''Continue''\s*\r?\n\s*\$output=@\(& \$contractAudit \$input\.Path 2>&1\)' -and
    $scriptText -match 'Assert-ContractAuditExitCode \$exit \$input\.SetCode') `
    'Contract audit invocation must tolerate normal stderr and validate its native exit code.'
$stableCargoBuild = Get-FunctionText 'Invoke-StableCargoBuild'
Assert-True ($stableCargoBuild -match '\$ErrorActionPreference=''Continue''' -and
    $stableCargoBuild -match '& cargo @CargoArguments 2>&1' -and
    $stableCargoBuild -match '\$exit=\$LASTEXITCODE') `
    'Stable Cargo builds must tolerate normal progress on stderr and validate the native exit code.'
$reportPublication = Get-FunctionText 'Publish-AuditReport'
Assert-True ($reportPublication -match '\$Path\.failed\.json' -and
    $reportPublication -match 'InfrastructureFailure') `
    'Infrastructure failures must preserve an existing last-good report and publish a sidecar.'
$proofLedger = Get-FunctionText 'Apply-StrictProofLedger'
Assert-True ($proofLedger -match 'Test-ProofCache' -and
    $proofLedger -match 'Invoke-ManifestProof' -and
    $proofLedger -match 'Write-JsonAtomically') `
    'Strict proof ledger must verify cache fingerprints and atomically publish only executed proofs.'
$fixtureRunner = Get-FunctionText 'Get-FixtureRunnerBinary'
Assert-True ($fixtureRunner -match 'cargo build --locked -p \$package --no-default-features' -and
    $fixtureRunner -match 'Test-BinaryCacheMetadata') `
    'Fixture proof runners must be stable cached binaries built without parser features.'
$legacyHarnesses = Get-FunctionText 'Get-TestBinaries'
Assert-True ($legacyHarnesses -match 'oracle-compilation,ci-unit-tests-\$Partition') `
    'Legacy runtime harnesses must retain Oracle compilation until their tests become fixtures.'
$fixtureInputs = Get-FunctionText 'Get-FixtureRunnerInputs'
Assert-True ($fixtureInputs -match 'src\\model_runtime\.rs' -and
    $fixtureInputs -match 'deepdeck-contract-fixture-runner' -and
    $fixtureInputs -match 'deepdeck-runtime-fixture-runner') `
    'Fixture runner fingerprints must include engine-only production and runner sources.'

$fingerprint = Get-FunctionText 'Get-Fingerprint'
Assert-True ($fingerprint -match 'build=\$BuildIdentity') `
    'Fingerprint must include the parser build identity.'
Assert-True ($fingerprint -match 'CARGO_ENCODED_RUSTFLAGS') `
    'Fingerprint must include build-affecting Rust flags.'

$parserBinary = Get-FunctionText 'Get-ParserBinary'
Assert-True ($parserBinary -match 'features=local-audit' -and
    $parserBinary -match "'--features','local-audit'" -and
    $parserBinary -notmatch "'--features','audit-cli'") `
    'Parser audit build command and fingerprint must use the network-free local-audit graph.'
Assert-True ($parserBinary -match 'audit-parser-\$profile-\$targetKey\.json') `
    'Parser metadata must be isolated by profile and target.'
Assert-True ($parserBinary -match '\$cargoArguments.*--release') `
    'Release profile must pass --release to Cargo.'
Assert-True ($parserBinary -match '\$profile\\audit_parser_set\$suffix') `
    'Executable lookup must follow the selected Cargo profile.'
Assert-True ($parserBinary -match 'executable_hash') `
    'Parser metadata must record an executable hash.'
$cacheValidation = Get-FunctionText 'Test-BinaryCacheMetadata'
Assert-True ($cacheValidation -match 'Get-FileHash -LiteralPath \$cachedExe') `
    'A cached parser binary must be hash-verified before reuse.'
Assert-True ($cacheValidation -match 'schema_version-ne2') `
    'Old parser metadata schemas must be invalidated.'
Assert-True ($parserBinary -match '\[Threading\.Mutex\]::new' -and
    $parserBinary -match 'WaitOne') `
    'Same-target parser cache access must be serialized across processes.'
$cacheWrite = Get-FunctionText 'Write-JsonAtomically'
Assert-True ($cacheWrite -match '\[IO\.File\]::Replace') `
    'Existing metadata must be replaced atomically.'

$componentInputs = Get-FunctionText 'Get-ComponentInputs'
foreach ($provider in @($componentInputs, $fixtureInputs, $proofProductionFiles)) {
    Assert-True ($provider.Contains('crates\deepdeck-card-domain\Cargo.toml') -and
        $provider.Contains('crates\deepdeck-card-domain\src') -and
        $provider.Contains('data\oracle-vocabulary')) `
        'Every audit binary/proof inventory must include shared card-domain code and all subtype/alias data.'
}
Assert-True ($componentInputs -match "src\\lib\.rs") `
    'Parser fingerprint must include the crate entry point.'
Assert-True ($componentInputs -match "src\\model_runtime\.rs" -and
    $componentInputs -notmatch "src\\model\.rs") `
    'Parser-free runtime and contract fingerprints must use the runtime model boundary.'
Assert-True ($componentInputs -match "build\.rs") `
    'Parser fingerprint must include applicable build scripts.'

# Execute the cache functions with a fake Cargo command. This proves the
# transition miss -> hit and that changed/corrupt metadata is never accepted.
foreach ($name in @(
    'Get-Hash',
    'New-Directory',
    'Get-Property',
    'Get-QueueOracle',
    'Get-QueueSet',
    'Get-QueueCollector',
    'Get-QueueKey',
    'Get-QueueLabel',
    'Get-Set',
    'Get-CardSet',
    'Get-CardKeys',
    'Get-AuditIdentity',
    'Copy-CardWithAuditKey',
    'New-StrictQueueCards',
    'Get-CardRecord',
    'Convert-BlockedKinds',
    'Get-RecognizerNativePrefix',
    'Get-MigrationPrefix',
    'New-AuditReportRecord',
    'New-AuditReport',
    'Write-JsonAtomically',
    'Assert-ParserAuditOutput',
    'Assert-ContractAuditExitCode',
    'Assert-ContractAuditResultCount',
    'Test-IsInfrastructureFailure',
    'Publish-AuditReport',
    'Read-CacheMetadata',
    'Write-CacheMetadata',
    'Get-OracleDigest',
    'Get-SemanticIrDigest',
    'Resolve-ProofFile',
    'Get-ProofEntries',
    'Get-ProofSpecDigest',
    'Get-ProofProductionFiles',
    'Get-FixtureRunnerInputs',
    'Get-ProofSemanticsFingerprint',
    'Test-ProofCache',
    'Invoke-FixtureProof',
    'Invoke-ManifestProof',
    'Apply-StrictProofLedger',
    'Test-BinaryCacheMetadata',
    'Test-ParserCacheMetadata',
    'Invoke-StableCargoBuild',
    'Get-ContractAuditBinary',
    'Get-ParserBinary'
)) {
    Invoke-Expression (Get-FunctionText $name)
}

$script:repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$manifest = Get-Content -LiteralPath (Join-Path $script:repoRoot 'docs\native-card-proof-manifest.json') -Raw | ConvertFrom-Json
foreach ($spec in @($manifest.cards)) {
    Assert-True (-not [string]::IsNullOrWhiteSpace((Get-ProofSpecDigest $spec))) `
        "Proof spec digest must include existing test files for $($spec.name)."
    Assert-True ([string]$spec.expected_semantic_ir_digest -match '^[A-F0-9]{64}$') `
        "Proof spec must pin a 64-character semantic IR digest for $($spec.name)."
}
$stockUpSpec = @($manifest.cards | Where-Object { $_.name -eq 'Stock Up' })[0]
$llanowarSpec = @($manifest.cards | Where-Object { $_.name -eq 'Llanowar Elves' })[0]
$unpinnedStockUpSpec = $stockUpSpec.PSObject.Copy()
$unpinnedStockUpSpec.PSObject.Properties.Remove('expected_semantic_ir_digest')
Assert-True ((Get-ProofSpecDigest $stockUpSpec) -eq (Get-ProofSpecDigest $unpinnedStockUpSpec)) `
    'Adding the separately validated IR pin must not invalidate unchanged fixture proof caches.'
Assert-True ($stockUpSpec.contract.fixture_path -and $stockUpSpec.runtime.fixture_path) `
    'Stock Up contract and runtime proofs must use data-driven fixtures.'
Assert-True ($llanowarSpec.contract.fixture_path -and $llanowarSpec.runtime.fixture_path) `
    'Llanowar contract and runtime proofs must use equivalent data-driven fixtures.'
$script:phaseSeconds=@{'contract-tests'=0.0;'runtime-tests'=0.0}
$script:proofCount=0
function Get-FixtureRunnerBinary { return 'fake-contract-fixture-runner' }
function fake-contract-fixture-runner {
    $global:LASTEXITCODE=0
    '{"schemaVersion":"deepdeck-contract-fixture-result/v1","fixtureId":"stock-up/top-cards-partition-is-executable","status":"passed"}'
    '{"schemaVersion":"deepdeck-contract-fixture-result/v1","type":"summary","status":"passed"}'
}
Invoke-FixtureProof $stockUpSpec.contract 'contract' 'Stock Up simulation'
Assert-True ($script:proofCount -eq 1) `
    'A matching successful fixture result must count as exactly one refreshed proof.'
$runtimeProduction = @(Get-ProofProductionFiles 'runtime')
Assert-True (@($runtimeProduction | Where-Object { $_ -match '[\\/]engine[\\/]tests([\\/]|\.rs$)' }).Count -eq 0) `
    'Runtime proof semantics fingerprint must exclude test modules; selected proof files are hashed separately.'
Assert-True (@($runtimeProduction | Where-Object { $_ -match '[\\/]deepdeck-card-domain[\\/]src[\\/].+_tests\.rs$' }).Count -eq 0) `
    'Shared-domain production proof inputs must exclude dedicated test modules.'
Assert-True (@($runtimeProduction | Where-Object { $_ -match '[\\/]engine[\\/]mod\.rs$' }).Count -eq 1) `
    'Runtime proof semantics fingerprint must include the engine production entry point.'
Assert-True ((Get-SemanticIrDigest '[ { "kind": "gainLife" } ]') -eq
    (Get-SemanticIrDigest '[{"kind":"gainLife"}]')) `
    'Semantic IR digest must ignore JSON whitespace.'

$digestFixtureRoot = Join-Path $script:repoRoot '.tmp\proof-digest-tests'
$contractDigestFixture = Join-Path $digestFixtureRoot "contract-$PID.json"
$runtimeDigestFixture = Join-Path $digestFixtureRoot "runtime-$PID.json"
$runtimeDigestFixture2 = Join-Path $digestFixtureRoot "runtime-2-$PID.json"
try {
    New-Directory $digestFixtureRoot
    [IO.File]::WriteAllText($contractDigestFixture,
        '{"schemaVersion":"deepdeck-contract-fixture/v1","id":"contract-digest","expectedExecutable":true,"rule":{"kind":"first"}}')
    [IO.File]::WriteAllText($runtimeDigestFixture,
        '{"schemaVersion":"deepdeck-runtime-fixture/v1","id":"runtime-digest","rule":{"kind":"first"}}')
    [IO.File]::WriteAllText($runtimeDigestFixture2,
        '{"schemaVersion":"deepdeck-runtime-fixture/v1","id":"runtime-digest-2","rule":{"kind":"second"}}')
    $contractRelative=$contractDigestFixture.Substring($script:repoRoot.Length+1)
    $runtimeRelative=$runtimeDigestFixture.Substring($script:repoRoot.Length+1)
    $runtimeRelative2=$runtimeDigestFixture2.Substring($script:repoRoot.Length+1)
    $digestSpec=[pscustomobject]@{
        contract=[pscustomobject]@{fixture_id='contract-digest';fixture_path=$contractRelative}
        runtime=@(
            [pscustomobject]@{fixture_id='runtime-digest';fixture_path=$runtimeRelative},
            [pscustomobject]@{fixture_id='runtime-digest-2';fixture_path=$runtimeRelative2}
        )
    }
    $firstFixtureDigest=Get-ProofSpecDigest $digestSpec
    [IO.File]::WriteAllText($contractDigestFixture,
        '{"schemaVersion":"deepdeck-contract-fixture/v1","id":"contract-digest","expectedExecutable":true,"rule":{"kind":"changed"}}')
    $secondFixtureDigest=Get-ProofSpecDigest $digestSpec
    Assert-True ($firstFixtureDigest -ne $secondFixtureDigest) `
        'Changing fixture IR must invalidate the proof specification digest.'
    [IO.File]::WriteAllText($runtimeDigestFixture2,
        '{"schemaVersion":"deepdeck-runtime-fixture/v1","id":"runtime-digest-2","rule":{"kind":"changed-second"}}')
    $thirdFixtureDigest=Get-ProofSpecDigest $digestSpec
    Assert-True ($secondFixtureDigest -ne $thirdFixtureDigest) `
        'Changing any fixture in a proof bundle must invalidate the proof specification digest.'
}
finally {
    foreach($path in @($contractDigestFixture,$runtimeDigestFixture,$runtimeDigestFixture2)){
        if(Test-Path -LiteralPath $path -PathType Leaf){Remove-Item -LiteralPath $path -Force}
    }
    if((Test-Path -LiteralPath $digestFixtureRoot -PathType Container)-and
        @(Get-ChildItem -LiteralPath $digestFixtureRoot -Force).Count-eq0){
        Remove-Item -LiteralPath $digestFixtureRoot -Force
    }
}

$expectedProof = [pscustomobject]@{
    schema_version='deepdeck-native-card-proof-cache/v1'
    oracle_id='oracle-1';oracle_digest='oracle-digest';semantic_ir_digest='ir-digest'
    proof_spec_digest='spec-digest';contract_semantics_fingerprint='contract-fingerprint'
    runtime_semantics_fingerprint='runtime-fingerprint'
}
$validProof = [pscustomobject]@{
    schema_version='deepdeck-native-card-proof-cache/v1'
    oracle_id='oracle-1';oracle_digest='oracle-digest';semantic_ir_digest='ir-digest'
    proof_spec_digest='spec-digest';contract_semantics_fingerprint='contract-fingerprint'
    runtime_semantics_fingerprint='runtime-fingerprint';contract_status='executable';runtime_status='passed'
}
Assert-True (Test-ProofCache $validProof $expectedProof) `
    'An exact successful proof cache record must be reusable.'
$validProof.semantic_ir_digest='changed-ir'
Assert-True (-not(Test-ProofCache $validProof $expectedProof)) `
    'Changing semantic IR must invalidate a cached runtime proof.'

$minimalQueueLabel = Get-QueueLabel ([pscustomobject]@{
    global_rank = 47
    name = 'Chaos Warp'
    oracle_id = 'oracle-chaos-warp'
})
Assert-True ($minimalQueueLabel -eq 'rank 47 Chaos Warp (oracle:oracle-chaos-warp)') `
    'Queue labels must accept the minimal rank/name/oracle_id schema without formats.'

$strictQueue = @(
    [pscustomobject]@{oracle_id='oracle-a';global_rank=1;name='First'},
    [pscustomobject]@{oracle_id='oracle-b';global_rank=2;name='Second'}
)
$strictResolved = @{
    'oracle:oracle-a'=[pscustomobject]@{oracle_id='oracle-a';name='First';set='aaa';collector_number='1'}
    'oracle:oracle-b'=[pscustomobject]@{oracle_id='oracle-b';name='Second';set='bbb';collector_number='1'}
}
$strictCards = @(New-StrictQueueCards $strictQueue $strictResolved)
Assert-True ($strictCards.Count -eq 2 -and
    $strictCards[0].name -eq 'First' -and $strictCards[1].name -eq 'Second') `
    'Strict input cards must preserve global queue order across sets.'
Assert-True ((Get-AuditIdentity $strictCards[0]) -eq 'queue-00000001' -and
    (Get-AuditIdentity $strictCards[1]) -eq 'queue-00000002') `
    'Strict input cards must have unique stable identities despite collector-number collisions.'

$harnessRoot = Join-Path ([IO.Path]::GetTempPath()) `
    "deepdeck-audit-cache-$PID-$([Guid]::NewGuid().ToString('N'))"
$script:CacheDirectory = Join-Path $harnessRoot 'cache'
$script:cargoTargetRoot = Join-Path $harnessRoot 'target'
$script:ParserBuildProfile = 'debug'
$script:binaryBuildCount = 0
$script:phaseSeconds = @{'parser-build' = 0.0}
$script:harnessFingerprint = 'fingerprint-v1'
$script:cargoCalls = 0
$script:changeFingerprintDuringBuild = $false
$fakeExecutable = Join-Path $script:cargoTargetRoot 'debug\audit_parser_set.exe'

function Get-ComponentInputs { return @() }
function Get-Fingerprint { return $script:harnessFingerprint }
function cargo {
    $script:cargoCalls++
    if($script:changeFingerprintDuringBuild-and$script:cargoCalls-eq1){
        $script:harnessFingerprint='fingerprint-after-edit'
    }
    $global:LASTEXITCODE = 0
}

try {
    New-Directory (Split-Path -Parent $fakeExecutable)
    [IO.File]::WriteAllText($fakeExecutable, 'fake parser executable')

    $first = Get-ParserBinary
    Assert-True ($script:cargoCalls -eq 1 -and $script:binaryBuildCount -eq 1) `
        'Initial cache miss must invoke Cargo exactly once.'
    $second = Get-ParserBinary
    Assert-True ($second -eq $first -and $script:cargoCalls -eq 1) `
        'Valid metadata and executable hash must produce a cache hit.'

    $script:harnessFingerprint = 'fingerprint-v2'
    [void](Get-ParserBinary)
    Assert-True ($script:cargoCalls -eq 2) `
        'Changed source fingerprint must invalidate the parser cache.'

    $metadataPath = Get-ChildItem (Join-Path $script:CacheDirectory 'binaries') `
        -Filter 'audit-parser-debug-*.json' | Select-Object -ExpandProperty FullName
    [IO.File]::WriteAllText($metadataPath, '{invalid json')
    [void](Get-ParserBinary)
    Assert-True ($script:cargoCalls -eq 3) `
        'Corrupt metadata must be treated as a safe cache miss.'
    $recovered = Read-CacheMetadata $metadataPath
    Assert-True (Test-ParserCacheMetadata $recovered 'debug' `
        $script:cargoTargetRoot $script:harnessFingerprint) `
        'Cache miss recovery must atomically write verified metadata.'

    [IO.File]::WriteAllText($metadataPath, '{}')
    [void](Get-ParserBinary)
    Assert-True ($script:cargoCalls -eq 4) `
        'Incomplete metadata must be treated as a safe cache miss.'

    $script:CacheDirectory = Join-Path $harnessRoot 'drift-cache'
    $script:harnessFingerprint = 'fingerprint-before-edit'
    $script:cargoCalls = 0
    $script:changeFingerprintDuringBuild = $true
    [void](Get-ParserBinary)
    Assert-True ($script:cargoCalls -eq 2 -and
        $script:parserAuditFingerprint -eq 'fingerprint-after-edit') `
        'A fingerprint change during build must force a stable rebuild.'
    $script:changeFingerprintDuringBuild = $false

    $script:CacheDirectory = Join-Path $harnessRoot 'contract-cache'
    $script:cargoCalls = 0
    $script:harnessFingerprint = 'contract-fingerprint'
    $script:phaseSeconds['contract-audit-build'] = 0.0
    $fakeContractExecutable = Join-Path $script:cargoTargetRoot `
        'debug\examples\audit_contract_set.exe'
    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $fakeContractExecutable))
    [IO.File]::WriteAllText($fakeContractExecutable, 'fake contract executable')
    $contractFirst = Get-ContractAuditBinary
    $contractSecond = Get-ContractAuditBinary
    Assert-True ($contractSecond -eq $contractFirst -and $script:cargoCalls -eq 1 -and
        $script:contractAuditFingerprint -eq 'contract-fingerprint') `
        'Contract audit binary must use the same verified v2 cache contract.'

    $subjects = @(
        [pscustomobject]@{Position=1;FallbackSet='tst';Entry=[pscustomobject]@{oracle_id='oracle-1';global_rank=9;name='Primitive';set='tst';collector_number='1'}},
        [pscustomobject]@{Position=2;FallbackSet='tst';Entry=[pscustomobject]@{oracle_id='oracle-2';global_rank=12;name='Composite';set='tst';collector_number='2'}},
        [pscustomobject]@{Position=3;FallbackSet='tst';Entry=[pscustomobject]@{oracle_id='oracle-3';global_rank=14;name='Unsupported';set='tst';collector_number='3'}},
        [pscustomobject]@{Position=4;FallbackSet='tst';Entry=[pscustomobject]@{oracle_id='oracle-4';global_rank=15;name='Mixed';set='tst';collector_number='4'}}
    )
    $reportStatuses = @{
        'oracle:oracle-1'=[pscustomobject]@{Status='canonical';RecognizerStatus='native';NativeAbilityCount=1;LegacyAbilityCount=0;UnclaimedAbilityCount=0;IrClass='primitive';BlockedKinds='';ContractStatus='executable';UnexecutableRuleCount=0}
        'oracle:oracle-2'=[pscustomobject]@{Status='canonical';RecognizerStatus='native';NativeAbilityCount=2;LegacyAbilityCount=0;UnclaimedAbilityCount=0;IrClass='composite';BlockedKinds='legacyB,legacyA';ContractStatus='notChecked';UnexecutableRuleCount=0}
        'oracle:oracle-3'=[pscustomobject]@{Status='unsupported';RecognizerStatus='unsupported';NativeAbilityCount=0;LegacyAbilityCount=0;UnclaimedAbilityCount=1;IrClass='unsupported';BlockedKinds='';ContractStatus='notChecked';UnexecutableRuleCount=0}
        'oracle:oracle-4'=[pscustomobject]@{Status='canonical';RecognizerStatus='mixed';NativeAbilityCount=1;LegacyAbilityCount=1;UnclaimedAbilityCount=0;IrClass='primitive';BlockedKinds='';ContractStatus='executable';UnexecutableRuleCount=0}
    }
    $report = New-AuditReport $subjects $reportStatuses $true $null ([pscustomobject]@{queue_file='queue.json'})
    Assert-True ($report.schema_version -eq 'deepdeck-card-audit/v3') `
        'Proof-aware reports must use the v3 schema.'
    Assert-True ($report.complete -and $report.cards.Count -eq 4) `
        'Expected gaps must still produce a complete report.'
    Assert-True ($report.cards[0].oracle_id -eq 'oracle-1' -and
        $report.cards[1].oracle_id -eq 'oracle-2') `
        'Report cards must preserve supplied queue order.'
    Assert-True ($report.first_gap.oracle_id -eq 'oracle-2' -and
        $report.first_gap.reason -eq 'ir:composite') `
        'first_gap must identify the first non-primitive executable card.'
    Assert-True (($report.cards[1].blocked_kinds -join ',') -eq 'legacyA,legacyB' -and
        $report.cards[1].contract_status -eq 'not_applicable') `
        'Composite IR must expose sorted blocked kinds and no contract claim.'
    Assert-True ($report.summary.native_recognizer -eq 2 -and
        $report.summary.mixed_recognizer -eq 1 -and
        $report.summary.native_primitive_executable -eq 1) `
        'Only an all-native primitive executable card may enter the strict native counter.'
    Assert-True ($report.summary.full_native_recognizer -eq 2 -and
        $report.summary.recognizer_native_prefix_length -eq 2 -and
        $report.first_non_native.oracle_id -eq 'oracle-3' -and
        $report.first_non_native.reason -eq 'parser:unsupported') `
        'Native prefix must stop at the first unsupported, mixed, or legacy card in queue order.'
    Assert-True ($report.summary.migration_prefix_length -eq 1 -and
        $report.first_unmigrated.oracle_id -eq 'oracle-2' -and
        $report.first_unmigrated.reason -eq 'ir:composite') `
        'A native recognizer with an IR or contract gap must not enter the migration prefix.'
    Assert-True ($report.cards[3].migration_status -eq 'partial-native' -and
        $report.cards[3].gap_reason -eq 'recognizer:mixed') `
        'A mixed native/legacy card must remain explicit migration debt.'
    Assert-True ($report.cards[0].runtime_status -eq 'not_applicable' -and
        $report.cards[0].proof_status -eq 'not_applicable') `
        'Non-ledger full audits must preserve their existing contract-only coverage semantics.'

    $mixedFirst = Get-RecognizerNativePrefix @($report.cards[0],$report.cards[3],$report.cards[1])
    Assert-True ($mixedFirst.length -eq 1 -and
        $mixedFirst.first_non_native.reason -eq 'recognizer:mixed') `
        'A canonical mixed card must stop strict native-prefix evaluation.'

    $partial = New-AuditReport @($subjects[0]) @{} $false 'parser-build' $null
    Assert-True (-not $partial.complete -and $partial.failure_stage -eq 'parser-build' -and
        $partial.cards[0].parser_status -eq 'not_checked') `
        'Infrastructure failure must produce an explicitly incomplete report.'

    $strictStopStatuses = @{
        'oracle:oracle-1'=[pscustomobject]@{Status='canonical';RecognizerStatus='native';NativeAbilityCount=1;LegacyAbilityCount=0;UnclaimedAbilityCount=0;IrClass='primitive';BlockedKinds='';ContractStatus='notChecked';UnexecutableRuleCount=0}
        'oracle:oracle-2'=[pscustomobject]@{Status='canonical';RecognizerStatus='legacy';NativeAbilityCount=0;LegacyAbilityCount=1;UnclaimedAbilityCount=0;IrClass='primitive';BlockedKinds='';ContractStatus='notChecked';UnexecutableRuleCount=0}
    }
    $strictStopReport = New-AuditReport @($subjects[0..2]) $strictStopStatuses $false `
        'native-prefix:2' ([pscustomobject]@{stop_at_first_non_native=$true})
    Assert-True (-not $strictStopReport.complete -and
        $strictStopReport.summary.recognizer_native_prefix_length -eq 1 -and
        $strictStopReport.first_non_native.position -eq 2 -and
        $strictStopReport.first_non_native.reason -eq 'recognizer:legacy' -and
        $strictStopReport.first_gap.position -eq 2 -and
        $strictStopReport.cards[2].parser_status -eq 'not_checked') `
        'Expected strict early-stop reports must retain the prefix and first gap while later cards stay unchecked.'
    $strictAllNative = New-AuditReport @($subjects[0]) `
        @{'oracle:oracle-1'=$strictStopStatuses['oracle:oracle-1']} $true $null `
        ([pscustomobject]@{stop_at_first_non_native=$true})
    Assert-True ($strictAllNative.complete -and
        $strictAllNative.summary.recognizer_native_prefix_length -eq 1) `
        'An all-native strict recognizer audit must complete without running the contract phase.'

    $validStrictOutput = @(
        'checking 2 cards sequentially in Scryfall set order (parser only)',
        "CARD`tqueue-00000001`tcanonical`tFirst`t1us",
        "RECOGNIZER`tqueue-00000001`tnative`t1`t0`t0`tFirst",
        "IR`tqueue-00000001`tprimitive`tFirst`t-",
        "RULES`tqueue-00000001`t[]",
        "CARD`tqueue-00000002`tcanonical`tSecond`t1us",
        "RECOGNIZER`tqueue-00000002`tlegacy`t0`t1`t0`tSecond",
        "IR`tqueue-00000002`tprimitive`tSecond`t-",
        "RULES`tqueue-00000002`t[]",
        "FIRST_NON_NATIVE`tqueue-00000002`tlegacy`tSecond"
    )
    Assert-ParserAuditOutput $validStrictOutput 1 $true 2 'strict-test'
    $validCoverageGapOutput = @(
        "CARD`tqueue-00000001`tunsupported`tGap`t1us",
        "RECOGNIZER`tqueue-00000001`tunsupported`t0`t0`t1`tGap",
        "IR`tqueue-00000001`tunsupported`tGap`t-",
        "RULES`tqueue-00000001`t[]",
        "SUMMARY`tcanonical=0`tprimitive=0`tcomposite=0`tnative=0`tmixed=0`tlegacy=0`tunsupportedRecognizer=1`tnativeAbilities=0`tlegacyAbilities=0`tunclaimedAbilities=1`ttotal=1`telapsed=1us"
    )
    Assert-ParserAuditOutput $validCoverageGapOutput 1 $false 1 'coverage-gap-test'
    foreach($invalid in @(
        [pscustomobject]@{Output=$validStrictOutput;Exit=-1073741819;Strict=$true;Message='negative Windows exit'},
        [pscustomobject]@{Output=$validStrictOutput;Exit=1;Strict=$false;Message='nonstrict exit 1 without complete summary'},
        [pscustomobject]@{Output=@('checking only');Exit=0;Strict=$false;Message='zero CARD output'},
        [pscustomobject]@{Output=@(
            "CARD`tqueue-00000001`tcanonical`tFirst`t1us",
            "RECOGNIZER`tqueue-00000001`tnative`t1`t0`t0`tFirst",
            "IR`tqueue-00000001`tprimitive`tFirst`t-"
        );Exit=0;Strict=$false;Message='missing RULES output'}
    )){
        $rejected=$false
        try{Assert-ParserAuditOutput $invalid.Output $invalid.Exit $invalid.Strict 1 'invalid-test'}catch{$rejected=$true}
        Assert-True $rejected "Parser validation must reject $($invalid.Message)."
    }

    $reportPath = Join-Path $harnessRoot 'report.json'
    Write-JsonAtomically $partial $reportPath 20
    Write-JsonAtomically $report $reportPath 20
    $written = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
    Assert-True ($written.complete -and $written.cards.Count -eq 4) `
        'Atomic report replacement must leave valid final JSON.'
    Assert-True (@(Get-ChildItem $harnessRoot -Recurse -File | Where-Object {
        $_.Name -match '\.(tmp|bak)$'
    }).Count -eq 0) 'Atomic report writes must clean temporary artifacts.'
    Assert-True (Test-IsInfrastructureFailure $true 'parser-audit:priority') `
        'Parser infrastructure failures must be classified as non-authoritative.'
    Assert-True (-not(Test-IsInfrastructureFailure $true 'native-prefix:2')) `
        'A proved strict native-prefix stop is an authoritative semantic result.'
    $lastGood = Get-Content -LiteralPath $reportPath -Raw
    $failureDestination = Publish-AuditReport $partial $reportPath $true
    Assert-True ($failureDestination -eq "$reportPath.failed.json" -and
        (Get-Content -LiteralPath $reportPath -Raw) -eq $lastGood) `
        'Infrastructure failure publication must leave the primary report byte-for-byte unchanged.'
    $failureWritten = Get-Content -LiteralPath $failureDestination -Raw | ConvertFrom-Json
    Assert-True (-not$failureWritten.complete -and $failureWritten.failure_stage -eq 'parser-build') `
        'Infrastructure failure sidecar must retain the incomplete diagnostic report.'
    $successDestination = Publish-AuditReport $report $reportPath $false
    Assert-True ($successDestination -eq $reportPath -and
        -not(Test-Path -LiteralPath "$reportPath.failed.json" -PathType Leaf)) `
        'A later authoritative report must remove its stale infrastructure-failure sidecar.'

    Assert-ContractAuditExitCode 0 'contract-success'
    Assert-ContractAuditExitCode 1 'contract-semantic-failure'
    Assert-ContractAuditResultCount 2 2 'contract-complete'
    foreach($invalidExit in @(-1073741819,2,3)){
        $rejected=$false
        try{Assert-ContractAuditExitCode $invalidExit 'contract-invalid'}catch{$rejected=$true}
        Assert-True $rejected "Contract audit must reject unexpected exit code $invalidExit."
    }
    $rejectedCardinality=$false
    try{Assert-ContractAuditResultCount 1 2 'contract-incomplete'}catch{$rejectedCardinality=$true}
    Assert-True $rejectedCardinality 'Contract audit must reject incomplete result cardinality.'

    # Simulate the strict ledger without Cargo. Cache hits must not execute a
    # proof, stale records must stay untrusted, and refresh may publish only
    # after both manifest proofs return successfully.
    $ledgerQueue = @([pscustomobject]@{oracle_id='oracle-ledger';global_rank=1;name='Ledger Card'})
    $ledgerCard = [pscustomobject]@{
        oracle_id='oracle-ledger';name='Ledger Card';set='tst';collector_number='1'
        oracle_text='Test oracle.';type_line='Instant';mana_cost='{1}'
    }
    $ledgerInputs = @([pscustomobject]@{SetCode='tst';Cards=@($ledgerCard)})
    $ledgerRecord = [pscustomobject]@{
        Status='canonical';RecognizerStatus='native';IrClass='primitive';SemanticIrDigest='ir-digest'
        ContractStatus='notChecked';RuntimeStatus='notChecked';ProofStatus='not_applicable'
        OracleDigest=$null;UnexecutableRuleCount=0
    }
    $ledgerStatuses = @{'oracle:oracle-ledger'=$ledgerRecord}
    $script:cachedProof = [pscustomobject]@{
        schema_version='deepdeck-native-card-proof-cache/v1';oracle_id='oracle-ledger'
        oracle_digest='oracle-digest';semantic_ir_digest='ir-digest';proof_spec_digest='spec-digest'
        contract_semantics_fingerprint='proof-fingerprint';runtime_semantics_fingerprint='proof-fingerprint'
        contract_status='executable';runtime_status='passed'
    }
    $script:proofInvocations=0;$script:proofWrites=0;$script:proofReads=0
    $script:ledgerExpectedDigest='ir-digest'
    function Read-ProofManifest {
        return @{'oracle-ledger'=[pscustomobject]@{
            expected_semantic_ir_digest=$script:ledgerExpectedDigest
            contract=@{};runtime=@{}
        }}
    }
    function Get-OracleDigest { return 'oracle-digest' }
    function Get-ProofSpecDigest { return 'spec-digest' }
    function Get-ProofProductionFiles { return @() }
    function Get-Fingerprint { return 'proof-fingerprint' }
    function Get-ProofCachePath { return (Join-Path $harnessRoot 'proof.json') }
    function Read-CacheMetadata { $script:proofReads++;return $script:cachedProof }
    function Invoke-ManifestProof { $script:proofInvocations++ }
    function Write-JsonAtomically($Value) { $script:proofWrites++;$script:cachedProof=$Value }

    $script:RefreshProofs=$false
    $script:ledgerExpectedDigest=$null
    Apply-StrictProofLedger $ledgerQueue $ledgerInputs $ledgerStatuses
    Assert-True ($ledgerRecord.ProofStatus -eq 'missing_ir_digest' -and
        $ledgerRecord.ContractStatus -eq 'notChecked' -and
        $ledgerRecord.RuntimeStatus -eq 'notChecked' -and
        $script:proofReads -eq 0 -and $script:proofInvocations -eq 0 -and
        $script:proofWrites -eq 0) `
        'A missing expected IR digest must remain uncertified without reading or publishing proof results.'

    $script:ledgerExpectedDigest='different-ir-digest'
    Apply-StrictProofLedger $ledgerQueue $ledgerInputs $ledgerStatuses
    Assert-True ($ledgerRecord.ProofStatus -eq 'semantic_ir_mismatch' -and
        $ledgerRecord.ContractStatus -eq 'notChecked' -and
        $ledgerRecord.RuntimeStatus -eq 'notChecked' -and
        $script:proofReads -eq 0 -and $script:proofInvocations -eq 0 -and
        $script:proofWrites -eq 0) `
        'An IR digest mismatch must remain uncertified without executing or publishing proofs.'

    $script:ledgerExpectedDigest='ir-digest'
    Apply-StrictProofLedger $ledgerQueue $ledgerInputs $ledgerStatuses
    Assert-True ($ledgerRecord.ProofStatus -eq 'cache_hit' -and
        $ledgerRecord.ContractStatus -eq 'executable' -and
        $ledgerRecord.RuntimeStatus -eq 'passed' -and $script:proofReads -eq 1 -and
        $script:proofInvocations -eq 0) `
        'An exact ledger cache hit must certify the card without executing Cargo proofs.'

    $ledgerRecord.ContractStatus='notChecked';$ledgerRecord.RuntimeStatus='notChecked'
    $script:cachedProof.semantic_ir_digest='stale-ir'
    Apply-StrictProofLedger $ledgerQueue $ledgerInputs $ledgerStatuses
    Assert-True ($ledgerRecord.ProofStatus -eq 'stale' -and
        $ledgerRecord.ContractStatus -eq 'notChecked' -and $script:proofInvocations -eq 0) `
        'A stale proof must never be trusted when refresh is absent.'

    $script:RefreshProofs=$true
    Apply-StrictProofLedger $ledgerQueue $ledgerInputs $ledgerStatuses
    Assert-True ($ledgerRecord.ProofStatus -eq 'verified' -and
        $script:proofInvocations -eq 2 -and $script:proofWrites -eq 1 -and
        $ledgerRecord.ContractStatus -eq 'executable' -and $ledgerRecord.RuntimeStatus -eq 'passed') `
        'Refresh must execute contract and runtime before atomically publishing a verified proof.'

    $proofReadsBeforeMismatch=$script:proofReads
    $proofInvocationsBeforeMismatch=$script:proofInvocations
    $proofWritesBeforeMismatch=$script:proofWrites
    $script:ledgerExpectedDigest='changed-after-verification'
    Apply-StrictProofLedger $ledgerQueue $ledgerInputs $ledgerStatuses
    Assert-True ($ledgerRecord.ProofStatus -eq 'semantic_ir_mismatch' -and
        $ledgerRecord.ContractStatus -eq 'notChecked' -and
        $ledgerRecord.RuntimeStatus -eq 'notChecked' -and
        $script:proofReads -eq $proofReadsBeforeMismatch -and
        $script:proofInvocations -eq $proofInvocationsBeforeMismatch -and
        $script:proofWrites -eq $proofWritesBeforeMismatch) `
        'A later IR mismatch must revoke in-memory certification without consulting or mutating the proof cache.'
}
finally {
    if (Test-Path -LiteralPath $harnessRoot) {
        Remove-Item -LiteralPath $harnessRoot -Recurse -Force
    }
}

Write-Host 'audit-card-set AST/cache contract tests passed.'
