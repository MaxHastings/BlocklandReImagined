@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Trust-Host.ps1" %*
exit /b %ERRORLEVEL%
