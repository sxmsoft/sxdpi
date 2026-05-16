Add-Type -AssemblyName System.Drawing
$img = [System.Drawing.Image]::FromFile("C:\Users\enesy\.gemini\antigravity\brain\febbdc4a-a804-41a0-99de-a2925693124c\sxdpi_app_icon_1778962089391.png")
$img.Save("d:\Coding\SxDPI\src-tauri\icons\icon.png", [System.Drawing.Imaging.ImageFormat]::Png)
$img.Dispose()
Write-Host "PNG saved"
