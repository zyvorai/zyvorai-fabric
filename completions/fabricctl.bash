# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

# Bash completion for fabricctl
_fabricctl() {
    local cur prev commands
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    commands="list create start stop restart delete clone snapshot backup status apply help"

    case "$prev" in
        -o|--output)
            COMPREPLY=( $(compgen -W "json yaml table" -- "$cur") )
            return
            ;;
        --image)
            COMPREPLY=( $(compgen -f -- "$cur") )
            return
            ;;
        fabricctl)
            COMPREPLY=( $(compgen -W "$commands" -- "$cur") )
            return
            ;;
    esac

    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "$commands" -- "$cur") )
    fi
}
complete -F _fabricctl fabricctl
