$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$output = Join-Path $root "source_bundle.txt"

$extensions = @(
    ".c",".h",".cpp",".hpp",".cc",
    ".rs",".go",".java",".kt",
    ".cs",".ts",".tsx",".js",".jsx",
    ".py",".swift",".m",".mm",
    ".sql",".sh",".bat",
    ".cmake",".xml",".json",
    ".yaml",".yml",".toml",".md"
)

$exclude = @(
    ".git",
    ".svn",
    "node_modules",
    "target",
    "build",
    "dist",
    "out",
    "bin",
    "obj",
    ".idea",
    ".vscode",
    "vendor"
)

$files = Get-ChildItem -Path $root -Recurse -File | Where-Object {
    $relative = $_.FullName.Substring($root.Length + 1)
    $parts = $relative.Split('\')

    ($extensions -contains $_.Extension.ToLower()) -and
    ($_.FullName -ne $output) -and
    (-not ($parts | Where-Object { $exclude -contains $_ })) -and
    ($_.Length -lt 5MB)
}

Set-Content $output "SOURCE BUNDLE" -Encoding UTF8
Add-Content $output "Generated: $(Get-Date)" -Encoding UTF8
Add-Content $output ""

Add-Content $output "========================================"
Add-Content $output "PROJECT FILE TREE"
Add-Content $output "========================================"
Add-Content $output ""

foreach ($file in $files) {
    $relative = $file.FullName.Substring($root.Length + 1)
    Add-Content $output $relative -Encoding UTF8
}

Add-Content $output ""
Add-Content $output "========================================"
Add-Content $output "SOURCE CONTENTS"
Add-Content $output "========================================"
Add-Content $output ""

foreach ($file in $files) {
    $relative = $file.FullName.Substring($root.Length + 1)

    Add-Content $output "[FILE] $relative" -Encoding UTF8
    Add-Content $output "==================================================" -Encoding UTF8

    Get-Content $file.FullName -Raw -Encoding UTF8 |
        Add-Content $output -Encoding UTF8

    Add-Content $output ""
    Add-Content $output ""
}

Write-Host "Completed: $output"