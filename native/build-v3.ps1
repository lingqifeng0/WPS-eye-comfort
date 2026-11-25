param(
    [string]$Zig = 'zig',
    [ValidateSet('auto', 'msvc', 'zig')][string]$Compiler = 'auto'
)
$ErrorActionPreference = 'Stop'
$taskNativeRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$taskProjectRoot = Split-Path -Parent $taskNativeRoot
$taskOutput = Join-Path $taskNativeRoot 'v3'
New-Item -ItemType Directory -Force -Path $taskOutput | Out-Null

$vcvars = @(
    'C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat',
    'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat',
    'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvarsall.bat'
) | Where-Object { Test-Path $_ } | Select-Object -First 1

$useMsvc = ($Compiler -eq 'msvc') -or ($Compiler -eq 'auto' -and $vcvars)
if ($useMsvc) {
    if (-not $vcvars) { throw 'MSVC vcvarsall.bat not found.' }
    $objDir = Join-Path $env:TEMP 'wps-eye-v3-obj'
    New-Item -ItemType Directory -Force -Path $objDir | Out-Null
    $cmd = "`"$vcvars`" x86 && cl /nologo /LD /O2 /MT /EHs-c- /GR- " +
           "/Fe:`"$taskOutput\WpsNativeEyeV3.dll`" /Fo:`"$objDir\\`" " +
           "`"$taskNativeRoot\v3-src\agent.cpp`" " +
           "`"$taskNativeRoot\vendor\minhook\src\buffer.c`" " +
           "`"$taskNativeRoot\vendor\minhook\src\hook.c`" " +
           "`"$taskNativeRoot\vendor\minhook\src\trampoline.c`" " +
           "`"$taskNativeRoot\vendor\minhook\src\hde\hde32.c`" user32.lib gdi32.lib"
    cmd /c $cmd
    if ($LASTEXITCODE -ne 0) { throw 'Native component MSVC compilation failed.' }
    Remove-Item -ErrorAction SilentlyContinue "$taskOutput\WpsNativeEyeV3.exp","$taskOutput\WpsNativeEyeV3.lib"
} else {
    & $Zig cc -x c++ -target x86-windows-gnu -shared -O2 -fno-exceptions -fno-rtti `
        "$taskNativeRoot\v3-src\agent.cpp" `
        "$taskNativeRoot\vendor\minhook\src\buffer.c" `
        "$taskNativeRoot\vendor\minhook\src\hook.c" `
        "$taskNativeRoot\vendor\minhook\src\trampoline.c" `
        "$taskNativeRoot\vendor\minhook\src\hde\hde32.c" `
        -luser32 -lgdi32 -o "$taskOutput\WpsNativeEyeV3.dll"
    if ($LASTEXITCODE -ne 0) { throw 'Native component Zig compilation failed.' }
}

py -3.12 "$taskProjectRoot\build_native_manifest.py"
if ($LASTEXITCODE -ne 0) { throw 'Native manifest generation failed.' }
