@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-misaka-local-node-wsl.ps1" -Command web -Pause
