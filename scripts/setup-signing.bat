@echo off
setlocal EnableExtensions
chcp 65001 >nul
title Чистовик: ключ подписи обновлений

echo.
echo  ============================================================
echo   Чистовик: создание ключа подписи обновлений (шаг 13)
echo  ============================================================
echo.
echo  Скрипт сам:
echo   1) создаст ключ и пароль к нему;
echo   2) положит их в GitHub Secrets репозитория kqsk/chistovik;
echo   3) скопирует публичный ключ в буфер обмена - его нужно отправить Claude.
echo.
pause

set "REPO=kqsk/chistovik"
set "DIR=%USERPROFILE%\.tauri"
set "KEY=%DIR%\chistovik.key"
set "PWFILE=%DIR%\chistovik-key-password.txt"

rem ---- 1. Node.js ----
where npx >nul 2>nul
if errorlevel 1 (
  echo.
  echo  [!] Не найден Node.js. Установите версию LTS с https://nodejs.org
  echo      ^(кнопка "Download", дальше "Далее" до конца^), потом запустите этот файл снова.
  start "" "https://nodejs.org/"
  goto :fail
)

if not exist "%DIR%" mkdir "%DIR%"

rem ---- 2. Ключ (существующий не трогаем) ----
if not exist "%KEY%" goto :newkey
echo.
echo  Ключ уже есть: %KEY%
echo  Новый НЕ создаю - старый остаётся, иначе прошлые версии не смогут обновиться.
if exist "%PWFILE%" goto :havekey
echo  [!] Нет файла с паролем: %PWFILE%
echo      Если ключ создавали вручную - добавьте секреты сами или удалите ключ и запустите снова.
goto :fail

:newkey
echo.
echo  Создаю пароль и ключ...
powershell -NoProfile -Command "$c='ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789'.ToCharArray(); $r=[Security.Cryptography.RandomNumberGenerator]::Create(); $b=New-Object byte[] 32; $r.GetBytes($b); -join ($b | ForEach-Object { $c[$_ %% $c.Length] }) | Set-Content -NoNewline -Encoding ascii '%PWFILE%'"
if not exist "%PWFILE%" goto :fail
set /p PW=<"%PWFILE%"
call npx --yes @tauri-apps/cli@2 signer generate -w "%KEY%" -p "%PW%" --ci >nul
if exist "%KEY%.pub" goto :created
echo  [!] Ключ не создался. Проверьте интернет и запустите файл снова.
del "%PWFILE%" >nul 2>nul
goto :fail

:created
echo  Готово: %KEY%

:havekey
set /p PW=<"%PWFILE%"

rem ---- 3. Секреты в GitHub ----
echo.
where gh >nul 2>nul
if errorlevel 1 goto :manual

gh auth status >nul 2>nul
if errorlevel 1 (
  echo  Нужно один раз войти в GitHub. Выберите: GitHub.com, HTTPS, Login with a web browser.
  gh auth login
  if errorlevel 1 goto :manual
)
echo  Добавляю секреты в GitHub...
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo %REPO% < "%KEY%"
if errorlevel 1 goto :manual
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo %REPO% --body "%PW%"
if errorlevel 1 goto :manual
echo  Секреты добавлены.
goto :pubkey

:manual
echo  Добавим секреты вручную через браузер (2 раза по одной вставке).
echo.
echo  --- Секрет 1 из 2 ---
echo  Сейчас откроется страница GitHub. Секретный ключ УЖЕ скопирован в буфер.
echo   * Name:   TAURI_SIGNING_PRIVATE_KEY
echo   * Secret: нажмите Ctrl+V
echo   * Кнопка "Add secret"
clip < "%KEY%"
start "" "https://github.com/%REPO%/settings/secrets/actions/new"
echo.
echo  Когда добавите - вернитесь сюда и нажмите любую клавишу.
pause >nul
echo.
echo  --- Секрет 2 из 2 ---
echo  Пароль УЖЕ скопирован в буфер. Снова нажмите "New repository secret":
echo   * Name:   TAURI_SIGNING_PRIVATE_KEY_PASSWORD
echo   * Secret: нажмите Ctrl+V
echo   * Кнопка "Add secret"
clip < "%PWFILE%"
start "" "https://github.com/%REPO%/settings/secrets/actions/new"
echo.
echo  Когда добавите - нажмите любую клавишу.
pause >nul

:pubkey
rem ---- 4. Публичный ключ для Claude ----
clip < "%KEY%.pub"
echo.
echo  ============================================================
echo   ГОТОВО.
echo.
echo   Публичный ключ скопирован в буфер обмена.
echo   Вставьте его (Ctrl+V) в чат с Claude - он сам впишет его в проект.
echo.
echo   ВАЖНО: сохраните копию папки
echo     %DIR%
echo   ^(ключ и пароль^) на флешку или в облако. Если её потерять,
echo   выпускать обновления станет невозможно. Никому её не отправляйте.
echo  ============================================================
echo.
pause
exit /b 0

:fail
echo.
echo  Ничего не испорчено. Исправьте причину выше и запустите файл снова.
pause
exit /b 1
