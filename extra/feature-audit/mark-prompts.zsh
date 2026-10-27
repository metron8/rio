builtin autoload -Uz add-zsh-hook
_rio_audit_mark_prompt() { builtin printf '\e]133;A\a'; }
add-zsh-hook precmd _rio_audit_mark_prompt
