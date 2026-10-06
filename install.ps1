# ビルドした exe を ~/.herdr/bin に置く（herdr の prefix+o → kanban の arch/open-arch.ps1 はそこを開く）。
#   pwsh install.ps1           target/release の exe を置く
#   pwsh install.ps1 -Build    ビルドしてから置く（メモリが少ないので並列は 2）
# 開いている窓が使っている exe も、名前を変えて退かせば置き換えられる（Windows は使用中の exe の改名は許す）。
# 退かしたものは、次に置くときに（もう使われていなければ）消す。
param([switch]$Build)
$ErrorActionPreference = 'Stop'
if ($Build) {
    cargo build --release -j 2 --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml')
    if ($LASTEXITCODE) { throw 'ビルドに失敗した' }
}
$src = Join-Path $PSScriptRoot 'target\release\arch-view.exe'
if (-not (Test-Path $src)) { throw "exe が無い（先にビルドする）: $src" }
$bin = Join-Path $env:USERPROFILE '.herdr\bin'
New-Item -ItemType Directory -Force $bin | Out-Null
Get-ChildItem $bin -Filter 'arch-view.old-*.exe' | ForEach-Object { Remove-Item $_.FullName -ErrorAction SilentlyContinue }
$dst = Join-Path $bin 'arch-view.exe'
if (Test-Path $dst) { Rename-Item $dst ('arch-view.old-{0}.exe' -f (Get-Date -Format 'yyyyMMddHHmmss')) }
Copy-Item $src $dst
"置いた: $dst（開いている窓は古いまま。閉じて開き直すと新しくなる）"
