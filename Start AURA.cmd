@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start-aura.ps1" %*
set "AURA_EXIT_CODE=%ERRORLEVEL%"
if not "%AURA_EXIT_CODE%"=="0" pause
exit /b %AURA_EXIT_CODE%
