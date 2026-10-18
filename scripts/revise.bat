@set PYTHONIOENCODING=utf-8
@powershell -noprofile -c "cmd /c \"$(revise %* $(doskey /history)[-2])\"; [Console]::ResetColor();"
