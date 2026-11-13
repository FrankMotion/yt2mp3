# Release build without personal paths in the .exe.
# Rust stores source paths for error messages; this rewrites the user folder
# (C:\Users\<name>) to "~" and the project folder to ".". CARGO_ENCODED_RUSTFLAGS
# keeps paths with spaces intact. Works in Windows PowerShell 5.1 and pwsh 7.
$root = Split-Path $PSScriptRoot -Parent
$sep = [char]0x1f
$flags = @(
  "--remap-path-prefix=$root=."
  "--remap-path-prefix=$env:USERPROFILE=~"
)
if ($env:CARGO_HOME) { $flags += "--remap-path-prefix=$env:CARGO_HOME=cargo" }
$env:CARGO_ENCODED_RUSTFLAGS = $flags -join $sep
Push-Location $root
try {
  cargo build --release --locked
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  Copy-Item target\release\corvus.exe Corvus.exe -Force
  "Fertig: $root\Corvus.exe"
} finally {
  Pop-Location
}
