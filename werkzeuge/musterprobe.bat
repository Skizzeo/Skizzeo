@echo off
rem Bildpruefung B7 (Paket 7): Muster aus dem Shader gegen die Rechnung.
rem Doppelklick genuegt. Die Bilder landen unter Dokumente\Skizzeo\musterprobe.
setlocal
chcp 65001 >nul
set "EXE=skizzeo.exe"
if exist "%~dp0skizzeo.exe" set "EXE=%~dp0skizzeo.exe"
if exist "%~dp0..\target\release\skizzeo.exe" set "EXE=%~dp0..\target\release\skizzeo.exe"
set "OUT=%USERPROFILE%\Documents\Skizzeo\musterprobe"
echo Musterprobe laeuft ...
start "" /wait "%EXE%" --musterprobe "%OUT%"
set "CODE=%ERRORLEVEL%"
if exist "%OUT%\ergebnis.txt" type "%OUT%\ergebnis.txt"
echo.
if "%CODE%"=="0" echo Ergebnis: alle Muster bestanden.
if "%CODE%"=="1" echo Ergebnis: NICHT bestanden - siehe Zeilen oben.
if "%CODE%"=="2" echo Ergebnis: kein OpenGL - keine Aussage ueber die Muster.
echo Bilder: %OUT%
pause
