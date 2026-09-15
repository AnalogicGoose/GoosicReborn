$env:GOOSIC_SERVICE_PATH = "C:\DEV\GoosicReborn\target\debug\goosic-service.exe"
Start-Process `
  -FilePath "C:\DEV\GoosicReborn\target\winui-audit\Goosic.Windows.exe" `
  -WorkingDirectory "C:\DEV\GoosicReborn\target\winui-audit"
