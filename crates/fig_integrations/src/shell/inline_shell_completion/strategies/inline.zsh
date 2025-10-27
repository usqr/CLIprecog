
#--------------------------------------------------------------------#
# InlineShell Suggestion Strategy                                          #
#--------------------------------------------------------------------#
# Suggests the inline_shell_completion command.
#

_q_autosuggest_strategy_inline_shell_completion() {
	typeset -g suggestion="$(command -v kiro-cli >/dev/null 2>&1 && kiro-cli _ inline-shell-completion --buffer "${BUFFER}")"
}
