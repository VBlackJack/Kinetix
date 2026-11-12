<#
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
#>

param(
    [switch]$Serve,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$WasmTarget = "wasm32-unknown-unknown"
$AppPackage = "kinetix-app"
$MiniquadPackage = "miniquad"
$ImportPrefix = "kinetix_"
$ServePort = 8080
$WasmSectionImport = 2
$BuildPlaceholder = "__KINETIX_BUILD__"
$BuildPlaceholderExpected = 2
$BuildIdLength = 12

function Get-Leb128 {
    param([byte[]]$Bytes, [ref]$Offset)
    [uint64]$result = 0
    $shift = 0
    while ($true) {
        $b = $Bytes[$Offset.Value]
        $Offset.Value++
        $result = $result -bor ([uint64]($b -band 0x7F) -shl $shift)
        if (($b -band 0x80) -eq 0) { break }
        $shift += 7
    }
    return [int]$result
}

function Get-WasmImport {
    # Returns "module.field" for every function/table/memory/global import of a wasm binary.
    param([Parameter(Mandatory)][string]$Path)

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 8 -or $bytes[0] -ne 0x00 -or $bytes[1] -ne 0x61 -or $bytes[2] -ne 0x73 -or $bytes[3] -ne 0x6D) {
        throw "Not a wasm binary: $Path"
    }
    $utf8 = [System.Text.Encoding]::UTF8
    $off = 8
    while ($off -lt $bytes.Length) {
        $sectionId = $bytes[$off]
        $off++
        $size = Get-Leb128 -Bytes $bytes -Offset ([ref]$off)
        $end = $off + $size
        if ($sectionId -eq $WasmSectionImport) {
            $count = Get-Leb128 -Bytes $bytes -Offset ([ref]$off)
            for ($i = 0; $i -lt $count; $i++) {
                $len = Get-Leb128 -Bytes $bytes -Offset ([ref]$off)
                $module = $utf8.GetString($bytes, $off, $len)
                $off += $len
                $len = Get-Leb128 -Bytes $bytes -Offset ([ref]$off)
                $field = $utf8.GetString($bytes, $off, $len)
                $off += $len
                $kind = $bytes[$off]
                $off++
                switch ($kind) {
                    0 { [void](Get-Leb128 -Bytes $bytes -Offset ([ref]$off)) }
                    1 {
                        $off++
                        $flags = $bytes[$off]; $off++
                        [void](Get-Leb128 -Bytes $bytes -Offset ([ref]$off))
                        if ($flags -band 1) { [void](Get-Leb128 -Bytes $bytes -Offset ([ref]$off)) }
                    }
                    2 {
                        $flags = $bytes[$off]; $off++
                        [void](Get-Leb128 -Bytes $bytes -Offset ([ref]$off))
                        if ($flags -band 1) { [void](Get-Leb128 -Bytes $bytes -Offset ([ref]$off)) }
                    }
                    3 { $off += 2 }
                    default { throw "Unsupported import kind $kind in $Path" }
                }
                "$module.$field"
            }
            return
        }
        $off = $end
    }
}

function Test-HtmlInlineJsSyntax {
    # Parses inline <script> blocks from web/index.html using Node when available.
    param([Parameter(Mandatory)][string]$HtmlPath)

    $node = Get-Command node -ErrorAction SilentlyContinue
    if (-not $node) {
        Write-Host "JS syntax guard: node not found in PATH, skipping inline check" -ForegroundColor Yellow
        return
    }
    $html = [System.IO.File]::ReadAllText($HtmlPath)
    $scripts = [regex]::Matches($html, '(?s)<script\b(?![^>]*\bsrc=)[^>]*>(.*?)</script>')
    $idx = 0
    foreach ($m in $scripts) {
        $idx++
        $tmp = [System.IO.Path]::ChangeExtension([System.IO.Path]::GetTempFileName(), ".js")
        try {
            [System.IO.File]::WriteAllText($tmp, $m.Groups[1].Value, [System.Text.UTF8Encoding]::new($false))
            & $node.Source --check $tmp
            if ($LASTEXITCODE -ne 0) {
                throw "JS syntax check failed on inline <script> #$idx in $HtmlPath (exit $LASTEXITCODE)"
            }
        } finally {
            Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
        }
    }
    Write-Host "JS syntax guard: $idx inline script(s) parsed cleanly"
}

function Test-KinetixImportGuard {
    # Fails when the wasm imports a kinetix_* function that index.html does not define.
    param([string]$WasmPath, [string]$HtmlPath)

    $html = Get-Content -LiteralPath $HtmlPath -Raw
    $names = @(Get-WasmImport -Path $WasmPath |
        Where-Object { $_.StartsWith("env.$ImportPrefix") } |
        ForEach-Object { $_.Substring(4) })
    $missing = @()
    foreach ($name in $names) {
        if (-not $html.Contains("importObject.env.$name")) { $missing += $name }
    }
    Write-Host "Import guard: wasm imports $($names.Count) $ImportPrefix* function(s): $($names -join ', ')"
    if ($missing.Count -gt 0) {
        throw "Import guard failed: web/index.html does not define: $($missing -join ', ')"
    }
}

function Get-MiniquadGlJs {
    # Locates miniquad's gl.js through cargo metadata so it matches Cargo.lock.
    $json = cargo metadata --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed (exit $LASTEXITCODE)" }
    $meta = $json | ConvertFrom-Json
    $pkg = @($meta.packages | Where-Object { $_.name -eq $MiniquadPackage }) | Select-Object -First 1
    if (-not $pkg) { throw "Package '$MiniquadPackage' not found in cargo metadata" }
    $glJs = Join-Path (Split-Path -Parent $pkg.manifest_path) "js\gl.js"
    if (-not (Test-Path -LiteralPath $glJs)) { throw "gl.js not found at $glJs" }
    return $glJs
}

if ($MyInvocation.InvocationName -eq ".") { return }

Set-Location (Join-Path $PSScriptRoot "..")

$WasmPath = "target/$WasmTarget/release/$AppPackage.wasm"

if ($SkipBuild) {
    Write-Host "[1/3] Skipping compilation (-SkipBuild)" -ForegroundColor Yellow
} else {
    Write-Host "[1/3] Compiling KINETIX to WebAssembly ($WasmTarget)..." -ForegroundColor Cyan
    cargo build --locked --release --target $WasmTarget -p $AppPackage
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
}

Write-Host "[2/3] Checking wasm imports and JS syntax in web/index.html..." -ForegroundColor Cyan
Test-KinetixImportGuard -WasmPath $WasmPath -HtmlPath "web\index.html"
Test-HtmlInlineJsSyntax -HtmlPath "web\index.html"

$DistDir = Join-Path $PWD "dist"
New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
Copy-Item (Get-MiniquadGlJs) -Destination (Join-Path $DistDir "gl.js") -Force
Copy-Item $WasmPath -Destination (Join-Path $DistDir "$AppPackage.wasm") -Force

# Cache busting: stamp the dist copy of index.html with a hash of the wasm (source stays untouched).
$BuildId = (Get-FileHash -LiteralPath (Join-Path $DistDir "$AppPackage.wasm") -Algorithm SHA256).Hash.Substring(0, $BuildIdLength)
$HtmlText = [System.IO.File]::ReadAllText((Join-Path $PWD "web\index.html"))
$PlaceholderCount = ([regex]::Matches($HtmlText, [regex]::Escape($BuildPlaceholder))).Count
if ($PlaceholderCount -ne $BuildPlaceholderExpected) {
    throw "Expected $BuildPlaceholderExpected '$BuildPlaceholder' placeholders in web/index.html, found $PlaceholderCount"
}
[System.IO.File]::WriteAllText((Join-Path $DistDir "index.html"), $HtmlText.Replace($BuildPlaceholder, $BuildId), [System.Text.UTF8Encoding]::new($false))
Write-Host "Cache-busting build id: $BuildId"

Write-Host "[3/3] Web bundle ready in dist/ (index.html + gl.js + $AppPackage.wasm)" -ForegroundColor Green

if ($Serve) {
    Write-Host "Serving on http://localhost:$ServePort ..." -ForegroundColor Yellow
    python -m http.server $ServePort --directory $DistDir
    if ($LASTEXITCODE -ne 0) { throw "python http.server failed (exit $LASTEXITCODE)" }
}
