@echo off
rem Resets Texcel to its default settings by deleting its settings file, for troubleshooting.
rem Work kept for recovery after a crash (the recovery folder) is left alone.
rem The UTF-8 code page lets the Japanese messages below show correctly.
chcp 65001 >nul
setlocal

set "DIR=%APPDATA%\Texcel"
set "FILE=%DIR%\settings.json"

rem A running Texcel writes its settings again when it quits, so it must be closed first.
tasklist /FI "IMAGENAME eq Texcel.exe" 2>nul | find /I "Texcel.exe" >nul
if not errorlevel 1 (
    echo Texcel is running. Quit Texcel, then run this again.
    echo Texcelが起動しています。Texcelを終了してから、もう一度実行してください。
    goto :end
)

if not exist "%FILE%" (
    echo There is no settings file; Texcel already uses its default settings.
    echo 設定ファイルはありません。Texcelはすでに初期設定で動きます。
    goto :end
)

echo This deletes the Texcel settings file / Texcelの設定ファイルを削除します:
echo   %FILE%
set "ANSWER="
set /p "ANSWER=Delete it? / 削除しますか？ [y/N]: "
if /I not "%ANSWER%"=="y" (
    echo Nothing was deleted. / 削除しませんでした。
    goto :end
)

rem Leftover temporary files of an interrupted settings save go too.
del /F /Q "%FILE%"
del /F /Q "%DIR%\.texcel-settings-*.tmp" 2>nul
echo Deleted. Texcel starts with its default settings next time.
echo 削除しました。次回の起動からTexcelは初期設定で動きます。

:end
pause
