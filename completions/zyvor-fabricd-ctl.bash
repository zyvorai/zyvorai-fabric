# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

# Bash completion for zyvor-fabricd-ctl
_zyvor_fabricd_ctl() {
    local cur prev commands
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    commands="deps build test install uninstall deploy reinstall start stop restart status logs backup health doctor password demo help"

    case "$prev" in
        backup)
            COMPREPLY=( $(compgen -W "now enable disable status logs" -- "$cur") )
            return
            ;;
        build)
            COMPREPLY=( $(compgen -W "--no-web" -- "$cur") )
            return
            ;;
        fabricctl|./fabricctl)
            COMPREPLY=( $(compgen -W "$commands" -- "$cur") )
            return
            ;;
    esac

    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "$commands" -- "$cur") )
    fi
}
complete -F _zyvor_fabricd_ctl zyvor-fabricd-ctl
complete -F _zyvor_fabricd_ctl ./zyvor-fabricd-ctl
