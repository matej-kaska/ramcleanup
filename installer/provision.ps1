param([switch]$Remove, [string]$Payload)
$ErrorActionPreference = 'Stop'
try {
    $dir = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'RAM Cleanup'
    $exe = Join-Path $dir 'ramcleanup.exe'
    $helper = Join-Path $dir 'ramcleanup-helper.exe'
    $scheduler = New-Object -ComObject 'Schedule.Service'
    $scheduler.Connect()
    $folder = $scheduler.GetFolder('\')
    $name = 'RAMCleanup.Cleanup'
    if ($Remove) {
        $task = $null
        try { $task = $folder.GetTask($name) } catch {
            if ($_.Exception.HResult -ne -2147024894) { throw }
        }
        if ($task) { $task.Stop(0); $folder.DeleteTask($name, 0) }
        exit 0
    }
    if (-not $Payload -or -not (Test-Path -LiteralPath $Payload -PathType Leaf)) { throw 'Missing installation payload.' }
    $helperPayload = Join-Path (Split-Path $Payload -Parent) 'ramcleanup-helper.exe'
    if (-not (Test-Path -LiteralPath $helperPayload -PathType Leaf)) { throw 'Missing helper payload.' }
    # Reject junctions/symlinks before changing a privileged target directory.
    $cursor = $dir
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point is not allowed: $cursor" }
        }
        $parent = Split-Path $cursor -Parent
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)')
    Set-Acl -LiteralPath $dir -AclObject $acl
    # Stop an earlier worker before replacing the binary.
    $previous = $null
    try { $previous = $folder.GetTask($name) } catch {
        if ($_.Exception.HResult -ne -2147024894) { throw }
    }
    if ($previous) { $previous.Stop(0) }
    # Replace directory entries, never write through an existing file/hardlink.
    foreach ($binary in @(@{Source=$Payload;Destination=$exe}, @{Source=$helperPayload;Destination=$helper})) {
        $staged = Join-Path $dir ([Guid]::NewGuid().ToString('N') + '.tmp')
        $stream = [IO.File]::Open($staged, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $bytes = [IO.File]::ReadAllBytes($binary.Source); $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Dispose() }
        try { Move-Item -LiteralPath $staged -Destination $binary.Destination -Force } finally {
            if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
        }
    }
    foreach ($file in @('Uninstall.exe', 'LICENSE', 'THIRD-PARTY-NOTICES.txt', 'cleanup-last.txt')) {
        $oldFile = Join-Path $dir $file
        if (Test-Path -LiteralPath $oldFile) { Remove-Item -LiteralPath $oldFile -Force }
    }
    $definition = $scheduler.NewTask(0)
    $definition.RegistrationInfo.Description = 'On-demand RAM cleanup. No schedule or background service.'
    $definition.Principal.UserId = 'SYSTEM'
    $definition.Principal.LogonType = 5 # TASK_LOGON_SERVICE_ACCOUNT
    $definition.Settings.Enabled = $true
    $definition.Settings.AllowDemandStart = $true
    $definition.Settings.DisallowStartIfOnBatteries = $false
    $definition.Settings.StopIfGoingOnBatteries = $false
    $definition.Settings.ExecutionTimeLimit = 'PT5M'
    $definition.Settings.MultipleInstances = 2 # IgnoreNew
    $action = $definition.Actions.Create(0)
    $action.Path = $helper
    $action.Arguments = '--worker'
    $action.WorkingDirectory = $dir
    # Users can read/run the fixed action, but cannot change or delete it.
    $sddl = 'O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;GRGX;;;BU)'
    $null = $folder.RegisterTaskDefinition($name, $definition, 6, 'SYSTEM', $null, 5, $sddl)
    exit 0
} catch {
    Write-Error $_ -ErrorAction Continue
    exit 1
}
