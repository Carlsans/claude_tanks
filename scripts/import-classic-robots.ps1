# Turns classic Robocode robots (robot .jar files, as in Robocode's robots
# folder or the RoboRumble) into Tank Royale bot directories that the
# Robocode tab and `royale` can start. Windows version of
# import-classic-robots.sh: uses the official robocode-api-bridge, compiled
# here with javac (no Gradle needed).
#
#   .\scripts\import-classic-robots.ps1 <folder with robot jars> [output, default bots\classic]
#
# Needs a JDK 11+ (java, javac, jar on PATH) and Windows 10+ (for tar).
# Downloads go to %LOCALAPPDATA%\claude_tanks\bridge and are reused.
param(
    [Parameter(Mandatory = $true)][string]$Robots,
    [string]$Out = "bots\classic"
)
$ErrorActionPreference = "Stop"

$BridgeCommit = "07108c333f48dad77237d453974aef0a414de07d"
$Maven = "https://repo1.maven.org/maven2"
$Cache = Join-Path $env:LOCALAPPDATA "claude_tanks\bridge"
New-Item -ItemType Directory -Force $Cache, (Join-Path $Out "lib") | Out-Null
$Out = (Resolve-Path $Out).Path
$Robots = (Resolve-Path $Robots).Path

function Fetch($url, $file) {
    $path = Join-Path $Cache $file
    if (Test-Path $path) { return }
    for ($try = 1; ; $try++) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $path
            return
        } catch {
            if ($try -ge 5) { throw }
            Start-Sleep -Seconds 2
        }
    }
}
Fetch "$Maven/dev/robocode/tankroyale/robocode-tankroyale-bot-api/1.4.0/robocode-tankroyale-bot-api-1.4.0.jar" "bot-api.jar"
Fetch "$Maven/org/apache/bcel/bcel/6.13.0/bcel-6.13.0.jar" "bcel.jar"
Fetch "$Maven/org/apache/commons/commons-lang3/3.20.0/commons-lang3-3.20.0.jar" "commons-lang3.jar"
Fetch "$Maven/commons-io/commons-io/2.22.0/commons-io-2.22.0.jar" "commons-io.jar"
# Robots written in Kotlin need its standard library, as classic Robocode ships it.
Fetch "$Maven/org/jetbrains/kotlin/kotlin-stdlib/1.9.24/kotlin-stdlib-1.9.24.jar" "kotlin-stdlib.jar"
Fetch "https://github.com/robocode-dev/robocode-api-bridge/archive/$BridgeCommit.tar.gz" "bridge.tar.gz"

Push-Location $Cache
try {
    if (-not (Test-Path "robocode-api.jar") -or -not (Test-Path "wrapper")) {
        Write-Host "Compiling the Robocode API bridge..."
        Remove-Item -Recurse -Force "src", "api", "wrapper" -ErrorAction SilentlyContinue
        New-Item -ItemType Directory "src", "api", "wrapper" | Out-Null
        tar xzf "bridge.tar.gz" -C "src" --strip-components 1
        # javac argument files take forward slashes.
        Get-ChildItem -Recurse "src\robocode-api\src\main\java" -Filter "*.java" |
            ForEach-Object { $_.FullName -replace '\\', '/' } |
            Set-Content -Encoding ASCII "api-sources.txt"
        javac --release 11 -nowarn -encoding UTF-8 -cp "bot-api.jar" -d "api" "@api-sources.txt"
        if ($LASTEXITCODE -ne 0) { throw "javac failed" }
        Push-Location "api"
        jar cf "..\robocode-api.jar" .
        Pop-Location
        $wrapperSources = Get-ChildItem "src\robots-wrapper\src\main\java\*.java" | ForEach-Object { $_.FullName }
        javac --release 11 -nowarn -encoding UTF-8 -cp "robocode-api.jar;bcel.jar" -d "wrapper" $wrapperSources
        if ($LASTEXITCODE -ne 0) { throw "javac failed" }
    }
} finally {
    Pop-Location
}

Copy-Item (Join-Path $Robots "*.jar") $Out
Copy-Item (Join-Path $Cache "robocode-api.jar"), (Join-Path $Cache "bot-api.jar"), (Join-Path $Cache "kotlin-stdlib.jar") (Join-Path $Out "lib")
$cp = ("wrapper", "robocode-api.jar", "bcel.jar", "commons-lang3.jar", "commons-io.jar" |
    ForEach-Object { Join-Path $Cache $_ }) -join ";"
Push-Location $Out
try {
    java -cp $cp Main $Out
} finally {
    Pop-Location
}
Write-Host "Done: point the Robocode tab's bot folder at $Out"
