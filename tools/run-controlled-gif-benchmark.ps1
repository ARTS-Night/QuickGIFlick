param(
    [int]$Seconds = 5,
    [string[]]$Scenario = @('static', 'cursor', 'small', 'typing', 'scroll', 'window-move', 'full')
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$screenDeltaRoot = Join-Path (Split-Path -Parent $root) 'ScreenDelta'
$qgif = Join-Path $root 'target\release\quickgiflick.exe'
$inspectGif = Join-Path $root 'target\release\examples\inspect_gif.exe'
$stimulus = Join-Path $screenDeltaRoot 'target\release\examples\controlled_stimulus.exe'
if (!(Test-Path $qgif) -or !(Test-Path $inspectGif) -or !(Test-Path $stimulus)) {
    throw 'Build both projects first: cargo build --release (QuickGIFlick) and cargo build --release --examples (ScreenDelta)'
}
$resultDir = Join-Path $root 'target\bench-results'
New-Item -ItemType Directory -Force -Path $resultDir | Out-Null
$rows = foreach ($name in $Scenario) {
    foreach ($mode in @('full', 'partial')) {
        $stimulusInfo = [Diagnostics.ProcessStartInfo]::new($stimulus, "$name $($Seconds + 2)")
        $stimulusInfo.WorkingDirectory = $screenDeltaRoot
        $stimulusInfo.UseShellExecute = $false
        $stimulusProcess = [Diagnostics.Process]::Start($stimulusInfo)
        Start-Sleep -Milliseconds 700
        $recorderInfo = [Diagnostics.ProcessStartInfo]::new($qgif)
        $recorderInfo.WorkingDirectory = $root
        $recorderInfo.UseShellExecute = $false
        $recorderInfo.RedirectStandardOutput = $true
        $recorderInfo.RedirectStandardError = $true
        $recorderInfo.Environment['QUICKGIFFLICK_BENCH'] = '1'
        $recorderInfo.Environment['QUICKGIFFLICK_SECONDS'] = "$Seconds"
        if ($mode -eq 'partial') { $recorderInfo.Environment['QUICKGIFFLICK_GIF_MODE'] = 'partial' }
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $recorder = [Diagnostics.Process]::Start($recorderInfo)
        $recorder.WaitForExit()
        $watch.Stop()
        $stdout = $recorder.StandardOutput.ReadToEnd().Trim()
        $stderr = $recorder.StandardError.ReadToEnd().Trim()
        $stimulusProcess.WaitForExit()
        $saved = ($stdout -split "`r?`n" | Where-Object { $_ -like 'Saved *' } | Select-Object -Last 1) -replace '^Saved ', ''
        $fileBytes = if ($saved -and (Test-Path $saved)) { (Get-Item $saved).Length } else { 0 }
        $decoded = if ($saved) { & $inspectGif $saved } else { '' }
        $stats = @{}
        [regex]::Matches($stderr, '(?<key>[a-z_]+)=(?<value>[0-9.]+)(?<unit>ms|B)?') | ForEach-Object {
            $stats[$_.Groups['key'].Value] = $_.Groups['value'].Value
        }
        [pscustomobject]@{
            scenario = $name
            mode = $mode
            seconds = $Seconds
            recorder_wall_ms = [math]::Round($watch.Elapsed.TotalMilliseconds, 3)
            output_bytes = $fileBytes
            decoded_frames = ([regex]::Match($decoded, 'frames=(\d+)')).Groups[1].Value
            decoded_centiseconds = ([regex]::Match($decoded, 'duration_centiseconds=(\d+)')).Groups[1].Value
            recording_updates = $stats['updates']
            resident_payload_bytes = $stats['resident_payload_bytes']
            spilled_payload_bytes = $stats['spilled_payload_bytes']
            store_write_ms = $stats['store_write_ms']
            encode_wall_ms = $stats['wall_ms']
            reconstruction_ms = $stats['reconstruction_ms']
            conversion_ms = $stats['conversion_ms']
            quantization_ms = $stats['quantization_ms']
            encoder_ms = $stats['encoder_ms']
            finalize_ms = $stats['finalize_ms']
            raw = $stderr
        }
    }
}
$path = Join-Path $resultDir ("QuickGIFlick_{0:yyyy-MM-dd_HH-mm-ss}.csv" -f (Get-Date))
$rows | Export-Csv -NoTypeInformation -Encoding utf8 -Path $path
Write-Output "Saved $path"
