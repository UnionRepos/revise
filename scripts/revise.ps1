if ((Get-Command "revise").CommandType -eq "Function") {
	revise @args;
	[Console]::ResetColor()
	exit
}

"First time use of revise detected. "

if ((Get-Content $PROFILE -Raw -ErrorAction Ignore) -like "*revise*") {
} else {
	"  - Adding revise intialization to user `$PROFILE"
	$script = "`n`$env:PYTHONIOENCODING='utf-8' `niex `"`$(revise --alias)`"";
	Write-Output $script | Add-Content $PROFILE
}

"  - Adding revise() function to current session..."
$env:PYTHONIOENCODING='utf-8'
iex "$($(revise --alias).Replace("function revise", "function global:revise"))"

"  - Invoking revise()`n"
revise @args;
[Console]::ResetColor()
