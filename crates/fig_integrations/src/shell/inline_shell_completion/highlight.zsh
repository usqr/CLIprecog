
#--------------------------------------------------------------------#
# Highlighting                                                       #
#--------------------------------------------------------------------#

# If there was a highlight, remove it
_q_autosuggest_highlight_reset() {
	typeset -g _KIRO_AUTOSUGGEST_LAST_HIGHLIGHT

	if [[ -n "$_KIRO_AUTOSUGGEST_LAST_HIGHLIGHT" ]]; then
		region_highlight=("${(@)region_highlight:#$_KIRO_AUTOSUGGEST_LAST_HIGHLIGHT}")
		unset _KIRO_AUTOSUGGEST_LAST_HIGHLIGHT
	fi
}

# If there's a suggestion, highlight it
_q_autosuggest_highlight_apply() {
	typeset -g _KIRO_AUTOSUGGEST_LAST_HIGHLIGHT

	if (( $#POSTDISPLAY )); then
		typeset -g _KIRO_AUTOSUGGEST_LAST_HIGHLIGHT="$#BUFFER $(($#BUFFER + $#POSTDISPLAY)) $KIRO_AUTOSUGGEST_HIGHLIGHT_STYLE"
		region_highlight+=("$_KIRO_AUTOSUGGEST_LAST_HIGHLIGHT")
	else
		unset _KIRO_AUTOSUGGEST_LAST_HIGHLIGHT
	fi
}
