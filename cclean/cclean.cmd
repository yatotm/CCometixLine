@echo off
rem cclean Windows 启动器: 双击或在 cmd / PowerShell 中运行 .\cclean.cmd [参数]
setlocal
where py >nul 2>nul
if %errorlevel%==0 (
    py -3 "%~dp0cclean.py" %*
    goto :end
)
where python >nul 2>nul
if %errorlevel%==0 (
    python "%~dp0cclean.py" %*
    goto :end
)
echo 未找到 Python 3. 请先安装 https://www.python.org/downloads/ 并勾选 "Add python.exe to PATH".
:end
endlocal
