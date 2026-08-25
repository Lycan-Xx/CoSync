# Windows shell preference

Use Git Bash as the default shell and write Bash-compatible commands.

Prefer:

- `pwd`, `ls`, `find`, `grep`, `sed`, and `awk`
- Unix-style paths such as `/c/Users/...`
- Git Bash-compatible quoting, environment variables, and command chaining

Use PowerShell syntax only when:

1. Git Bash cannot perform the operation; or
2. A Windows-specific command or tool requires PowerShell.

When falling back to PowerShell, state that the command is a PowerShell fallback and use PowerShell syntax consistently for that command.
