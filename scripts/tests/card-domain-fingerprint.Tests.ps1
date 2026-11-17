param([string]$FingerprintModule = (Join-Path (Get-Location) 'scripts/runner-fingerprint.ps1'))
$ErrorActionPreference='Stop'
$scriptsRoot=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$repoRoot=Join-Path ([IO.Path]::GetTempPath()) ('deepdeck-domain-fingerprints-'+[Guid]::NewGuid().ToString('N'))
function Assert-True([bool]$Value,[string]$Message){if(-not $Value){throw $Message}}
function Write-Synthetic([string]$Relative,[string]$Text){
    $path=Join-Path $repoRoot $Relative
    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $path))
    [IO.File]::WriteAllText($path,$Text,[Text.UTF8Encoding]::new($false))
}
function Import-InputFunctions([string]$Name,[string[]]$Functions){
    $tokens=$null;$errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $scriptsRoot $Name),[ref]$tokens,[ref]$errors)
    Assert-True ($errors.Count -eq 0) "Provider must parse: $Name"
    $definitions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in $Functions},$false))
    Assert-True ($definitions.Count -eq $Functions.Count) "Provider function set changed: $Name"
    return ($definitions|ForEach-Object{$_.Extent.Text}) -join "`n"
}
. ([scriptblock]::Create((Import-InputFunctions 'run-parser-fixture.ps1' @('Get-ParserRunnerInputFiles','Get-ParserRunnerFingerprint'))))
. ([scriptblock]::Create((Import-InputFunctions 'run-runtime-fixture.ps1' @('Get-RuntimeRunnerInputFiles'))))
. ([scriptblock]::Create((Import-InputFunctions 'run-contract-fixture.ps1' @('Get-ContractRunnerInputFiles'))))
. ([scriptblock]::Create((Import-InputFunctions 'audit-card-set.ps1' @('Get-Hash','Get-Fingerprint','Get-ComponentInputs','Get-FixtureRunnerInputs','Get-ProofProductionFiles'))))
. ([scriptblock]::Create((Import-InputFunctions 'check-card-migration-architecture.ps1' @('Get-ArchitectureRustFiles','Get-ArchitectureFingerprint'))))
. $FingerprintModule
# Identity is deterministic here; this synthetic input test never invokes rustc or Cargo.
function Get-RustCompilerIdentity{return 'synthetic-compiler'}
function Get-DeepDeckRustCompilerIdentity{return 'synthetic-compiler'}
function rustc{$global:LASTEXITCODE=0;return 'synthetic-compiler'}
$cargoTargetRoot=Join-Path $repoRoot 'target'
$architectureCacheSchema='deepdeck-card-migration-architecture-cache/v1'
$RequireParserProof=$false;$RequireAllProofs=$false
$guardScriptPath=Join-Path $scriptsRoot 'check-card-migration-architecture.ps1'
$effectCensusScriptPath=Join-Path $repoRoot 'census.py'
$BaselinePath=Join-Path $repoRoot 'baseline.json';$manifestPath=$null;$effectCensusTestPaths=@()
function Flatten-Inputs($Paths){
    return @(foreach($path in $Paths){
        if(Test-Path -LiteralPath $path -PathType Container){Get-ChildItem -LiteralPath $path -File -Recurse}
        elseif(Test-Path -LiteralPath $path -PathType Leaf){Get-Item -LiteralPath $path}
        else{throw "Missing synthetic fingerprint dependency $path"}
    })
}
$roles=@('parser','runtime','contract','component-parser','component-runtime','component-contract','fixture-runtime','fixture-contract','proof-runtime','proof-contract','architecture')
function Read-Frame([string]$Role){
    switch($Role){
        parser{return Get-ParserRunnerFingerprint}
        runtime{return Get-DeepDeckRunnerFingerprint -Schema test -Profile runtime -RepoRoot $repoRoot -InputFiles @(Get-RuntimeRunnerInputFiles)}
        contract{return Get-DeepDeckRunnerFingerprint -Schema test -Profile contract -RepoRoot $repoRoot -InputFiles @(Get-ContractRunnerInputFiles)}
        architecture{return Get-ArchitectureFingerprint}
        default{
            $kind=$Role.Split('-')[1]
            $paths=switch($Role.Split('-')[0]){component{Get-ComponentInputs $kind} fixture{Get-FixtureRunnerInputs $kind} proof{Get-ProofProductionFiles $kind}}
            return Get-Fingerprint $paths test
        }
    }
}
try{
    foreach($name in @('Cargo.toml','Cargo.lock','rust-toolchain.toml','src/lib.rs','src/card_catalog.rs','src/oracle.rs','src/game_rules.rs','src/model.rs','src/model_runtime.rs','data/builtin-game-pieces.json','census.py','baseline.json')){Write-Synthetic $name 'synthetic'}
    foreach($crate in @('deepdeck-oracle','deepdeck-engine-runtime','deepdeck-card-domain','deepdeck-parser-fixture-runner','deepdeck-runtime-fixture-runner','deepdeck-contract-fixture-runner','deepdeck-oracle-engine-contracts')){
        Write-Synthetic "crates/$crate/Cargo.toml" 'synthetic manifest'
        Write-Synthetic "crates/$crate/src/lib.rs" 'synthetic source'
    }
    foreach($directory in @('src/oracle','src/engine','tests','crates/deepdeck-oracle/generated','crates/deepdeck-engine-runtime/generated','crates/deepdeck-oracle-engine-contracts/tests','crates/deepdeck-oracle-engine-contracts/examples','data/ability-catalog')){Write-Synthetic "$directory/synthetic.rs" 'synthetic'}
    Write-Synthetic 'crates/deepdeck-oracle/examples/audit_parser_set.rs' 'synthetic audit'
    Write-Synthetic 'src/engine/engine_tests.rs' 'test-only'
    Write-Synthetic 'crates/deepdeck-card-domain/src/domain_tests.rs' 'test-only'
    $vocabulary=@('creature-subtypes-cr-205.3m-2026-11-06.json','noncreature-subtypes-cr-205.3-2026-11-06.json','subtype-orthographic-aliases-2026-11-15.json')
    foreach($name in $vocabulary){Write-Synthetic "data/oracle-vocabulary/$name" '{"synthetic":true}'}
    $expected=@('crates/deepdeck-card-domain/Cargo.toml','crates/deepdeck-card-domain/src/lib.rs')+@($vocabulary|ForEach-Object{"data/oracle-vocabulary/$_"})
    foreach($role in $roles|Where-Object{$_ -ne 'architecture'}){
        $files=switch($role){parser{Get-ParserRunnerInputFiles} runtime{Get-RuntimeRunnerInputFiles} contract{Get-ContractRunnerInputFiles} default{
            $kind=$role.Split('-')[1];Flatten-Inputs $(switch($role.Split('-')[0]){component{Get-ComponentInputs $kind} fixture{Get-FixtureRunnerInputs $kind} proof{Get-ProofProductionFiles $kind}})
        }}
        foreach($relative in $expected){Assert-True (@($files|Where-Object{$_.FullName -eq (Join-Path $repoRoot $relative)}).Count -eq 1) "Required input absent/duplicated in ${role}: $relative"}
        if($role -like 'proof-*'){Assert-True (@($files|Where-Object{$_.Name -eq 'domain_tests.rs'}).Count -eq 0) 'Production proof inventory must omit dedicated domain tests.'}
    }
    $before=@{};foreach($role in $roles){$before[$role]=Read-Frame $role}
    foreach($relative in $expected){
        $path=Join-Path $repoRoot $relative;$bytes=[IO.File]::ReadAllBytes($path)
        try{
            Write-Synthetic $relative 'changed dependency'
            foreach($role in $roles){Assert-True ((Read-Frame $role) -cne $before[$role]) "Mutation did not invalidate ${role}: $relative"}
        }finally{[IO.File]::WriteAllBytes($path,$bytes)}
        foreach($role in $roles){Assert-True ((Read-Frame $role) -ceq $before[$role]) "Restore did not recover ${role}: $relative"}
    }
    $manifest=Join-Path $repoRoot 'crates/deepdeck-card-domain/Cargo.toml';$bytes=[IO.File]::ReadAllBytes($manifest)
    try{
        [IO.File]::Delete($manifest)
        foreach($role in $roles){$rejected=$false;try{Read-Frame $role|Out-Null}catch{$rejected=$true};Assert-True $rejected "Missing required manifest was ignored: $role"}
    }finally{[IO.File]::WriteAllBytes($manifest,$bytes)}
    Write-Host 'Shared domain fingerprint inventories:11 roles,5 dependency mutations/restores,11 missing-manifest rejections PASS; no compiler/Cargo.'
}finally{
    $absolute=[IO.Path]::GetFullPath($repoRoot);$temp=[IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if($absolute.StartsWith($temp,[StringComparison]::OrdinalIgnoreCase) -and ([IO.Path]::GetFileName($absolute)).StartsWith('deepdeck-domain-fingerprints-')){Remove-Item -LiteralPath $absolute -Recurse -Force}
}
