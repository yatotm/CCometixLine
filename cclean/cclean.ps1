# cclean one-line launcher for Windows (PowerShell 5.1+ / 7).
#
#   irm https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.ps1 | iex
#
# With arguments (everything after the script block goes to cclean.py):
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.ps1))) run -y
#
# Downloads cclean.py from the same folder as this script into %TEMP% and runs it
# with Python 3. Running through `iex` keeps the console attached, so the
# interactive TUI works, and `irm | iex` needs no execution-policy change.
#
#   $env:CCLEAN_RAW   base URL of the cclean folder (default: master branch on GitHub)
#   $env:CCLEAN_PY    Python executable to use (default: py -3, python3, python)
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $CcleanArgs = @()
)

# Everything runs inside a script block so `return`, preferences and variables
# never leak into (or terminate) the caller's interactive session.
& {
    param([string[]] $CcleanArgs)

    $ErrorActionPreference = 'Stop'
    $base = if ($env:CCLEAN_RAW) { $env:CCLEAN_RAW } else { 'https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean' }

    function Find-Python {
        if ($env:CCLEAN_PY) { return @{ Exe = $env:CCLEAN_PY; Args = @() } }
        $candidates = @(
            @{ Exe = 'py';      Args = @('-3') },
            @{ Exe = 'python3'; Args = @() },
            @{ Exe = 'python';  Args = @() }
        )
        $ErrorActionPreference = 'Continue'
        foreach ($cand in $candidates) {
            if (-not (Get-Command $cand.Exe -ErrorAction SilentlyContinue)) { continue }
            $candArgs = @($cand.Args)
            try {
                $probe = & $cand.Exe @candArgs -c 'import sys; print(sys.version_info >= (3, 8))' 2>$null
                if ("$probe".Trim() -eq 'True') { return $cand }
            } catch { }
        }
        return $null
    }

    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    } catch { }

    $py = Find-Python
    if (-not $py) {
        Write-Host 'cclean: Python 3.8+ not found.' -ForegroundColor Red
        Write-Host 'Install it with:  winget install Python.Python.3.12'
        Write-Host 'or from https://www.python.org/downloads/ (tick "Add python.exe to PATH"), then reopen the terminal.'
        $global:LASTEXITCODE = 1
        return
    }

    $tmp = Join-Path $env:TEMP ('cclean-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    $script = Join-Path $tmp 'cclean.py'

    try {
        Invoke-WebRequest -UseBasicParsing -Uri "$base/cclean.py" -OutFile $script
        if (-not (Select-String -Path $script -Pattern 'cclean' -Quiet)) {
            throw "downloaded file does not look like cclean.py ($base/cclean.py)"
        }

        $pyArgs = @($py.Args)
        & $py.Exe @pyArgs $script @CcleanArgs
    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }
} $CcleanArgs
