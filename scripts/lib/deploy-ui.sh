# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# shellcheck shell=bash
# Emoji-rich deploy UI (self-contained; configure via deploy-common.sh before sourcing).

DEPLOY_UI_PROJECT="${DEPLOY_UI_PROJECT:-zyvor-fabricd}"
DEPLOY_UI_ICON="${DEPLOY_UI_ICON:-🖥️}"
DEPLOY_UI_ICON_UNINSTALL="${DEPLOY_UI_ICON_UNINSTALL:-🗑️}"
DEPLOY_UI_ICON_MAGIC="${DEPLOY_UI_ICON_MAGIC:-✨}"
DEPLOY_UI_ICON_ROCKET="${DEPLOY_UI_ICON_ROCKET:-🚀}"
DEPLOY_UI_PORT="${DEPLOY_UI_PORT:-9095}"
DEPLOY_UI_SCHEME="${DEPLOY_UI_SCHEME:-http}"
DEPLOY_UI_DASH_PATH="${DEPLOY_UI_DASH_PATH:-/}"
DEPLOY_UI_HEALTH_PATH="${DEPLOY_UI_HEALTH_PATH:-/health}"

_deploy_ui_color_on() {
    [[ -t 1 ]] && [[ -z "${NO_COLOR:-}" ]] && command -v tput &>/dev/null && [[ "$(tput colors 2>/dev/null || echo 0)" -ge 8 ]]
}

if _deploy_ui_color_on; then
    _C_RESET=$'\033[0m' _C_BOLD=$'\033[1m' _C_DIM=$'\033[2m'
    _C_CYAN=$'\033[36m' _C_GREEN=$'\033[32m' _C_YELLOW=$'\033[33m'
    _C_RED=$'\033[31m' _C_MAGENTA=$'\033[35m' _C_BLUE=$'\033[34m'
    _C_WHITE=$'\033[97m' _C_BRIGHT_CYAN=$'\033[96m' _C_BRIGHT_GREEN=$'\033[92m'
    _C_BRIGHT_MAGENTA=$'\033[95m'
else
    _C_RESET='' _C_BOLD='' _C_DIM='' _C_CYAN='' _C_GREEN='' _C_YELLOW='' _C_RED='' _C_MAGENTA='' _C_BLUE=''
    _C_WHITE='' _C_BRIGHT_CYAN='' _C_BRIGHT_GREEN='' _C_BRIGHT_MAGENTA=''
fi

_deploy_ui_bar_width=58

_deploy_ui_repeat() {
    local ch="$1" n="$2" i out=""
    for ((i = 0; i < n; i++)); do out+="$ch"; done
    printf '%s' "$out"
}

deploy_ui_step_emoji() {
    local t
    t=$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')
    case "$t" in
        *preflight*|*ssh*|*connect*)       echo "🔌" ;;
        *build*|*compile*|*cargo*|*rust*)  echo "🔨" ;;
        *rsync*|*upload*|*push*)           echo "📤" ;;
        *sync*)                            echo "📤" ;;
        *uninstall*)                       echo "🗑️" ;;
        *deps*|*package*)                  echo "📦" ;;
        *install*)                         echo "📦" ;;
        *dashboard*|*web*|*npm*)           echo "🌐" ;;
        *systemd*|*service*|*daemon*|*reload*) echo "⚙️" ;;
        *libvirt*|*kvm*|*qemu*)  echo "🖥️" ;;
        *verify*|*smoke*|*test*|*health*|*e2e*|*post-flight*) echo "🩺" ;;
        *clean*|*nuke*)                    echo "🧹" ;;
        *detect*|*runtime*)                 echo "🔍" ;;
        *firewall*)                        echo "🔥" ;;
        *writable*|*chown*)               echo "🔐" ;;
        *snapshot*)                        echo "📸" ;;
        *)                                 echo "🔧" ;;
    esac
}

deploy_ui_info()  { printf '  %s✅%s %s\n' "$_C_BRIGHT_GREEN" "$_C_RESET" "$*"; }
deploy_ui_warn()  { printf '  %s⚠️ %s%s\n' "$_C_YELLOW" "$_C_RESET" "$*"; }
deploy_ui_error() { printf '  %s❌%s %s\n' "$_C_RED" "$_C_RESET" "$*"; exit 1; }
deploy_ui_note()  { printf '  %s💡%s %s\n' "$_C_BLUE" "$_C_RESET" "$*"; }
deploy_ui_info_b() { printf '  %sℹ️  %s%s\n' "$_C_BRIGHT_CYAN" "$*" "$_C_RESET"; }

deploy_ui_kv() {
    local icon="$1" label="$2" value="$3"
    printf '  %s%-16s%s %s%s%s\n' "$_C_DIM" "${icon} ${label}" "$_C_RESET" "$_C_WHITE" "$value" "$_C_RESET"
}

deploy_ui_hr() {
    printf '\n%b%s%b\n' "$_C_DIM" "$(_deploy_ui_repeat '─' "$_deploy_ui_bar_width")" "$_C_RESET"
}

deploy_ui_progress() {
    local step="$1" total="$2"
    local width=28 filled empty bar="" i
    (( total < 1 )) && total=1
    filled=$(( step * width / total ))
    empty=$(( width - filled ))
    for ((i = 0; i < filled; i++)); do bar+='█'; done
    for ((i = 0; i < empty; i++)); do bar+='░'; done
    printf '  %s%s %s%s %s/%s%s\n' \
        "$_C_BRIGHT_MAGENTA" "$bar" "$_C_RESET" "$_C_DIM" "$step" "$total" "$_C_RESET"
}

deploy_ui_phase() {
    local step="$1" total="$2" title="$3"
    local sub="${4:-}"
    local emoji
    emoji=$(deploy_ui_step_emoji "$title")
    printf '\n'
    printf '  %s╔%s╗%s\n' "$_C_CYAN" "$(_deploy_ui_repeat '═' "$_deploy_ui_bar_width")" "$_C_RESET"
    printf '  %s║%s  %s %b%s%b\n' "$_C_CYAN" "$_C_RESET" "$emoji" "$_C_BOLD" "$title" "$_C_RESET"
    if [[ -n "$sub" ]]; then
        printf '  %s║%s     %s%s%s\n' "$_C_CYAN" "$_C_RESET" "$_C_DIM" "$sub" "$_C_RESET"
    fi
    printf '  %s╚%s╝%s\n' "$_C_CYAN" "$(_deploy_ui_repeat '═' "$_deploy_ui_bar_width")" "$_C_RESET"
    deploy_ui_progress "$step" "$total"
}

deploy_ui_banner() {
    local title="$1" subtitle="${2:-}"
    local icon="${3:-$DEPLOY_UI_ICON}"
    local w=58
    printf '\n'
    printf '  %s%s %s %s%s\n' "$_C_BRIGHT_MAGENTA" "$(_deploy_ui_repeat '═' "$w")" "$DEPLOY_UI_ICON_ROCKET" "$(_deploy_ui_repeat '═' "$w")" "$_C_RESET"
    printf '  %s║%s %s %-52s %s║%s\n' "$_C_MAGENTA" "$_C_RESET" "$icon" "$title" "$_C_MAGENTA" "$_C_RESET"
    if [[ -n "$subtitle" ]]; then
        printf '  %s║%s   %s%-52s%s %s║%s\n' \
            "$_C_MAGENTA" "$_C_RESET" "$_C_DIM" "$subtitle" "$_C_RESET" "$_C_MAGENTA" "$_C_RESET"
    fi
    printf '  %s%s %s %s%s\n' "$_C_BRIGHT_MAGENTA" "$(_deploy_ui_repeat '═' "$w")" "$DEPLOY_UI_ICON_MAGIC" "$(_deploy_ui_repeat '═' "$w")" "$_C_RESET"
    printf '\n'
}

deploy_ui_uninstall_banner() {
    deploy_ui_banner "${DEPLOY_UI_PROJECT} Remote Uninstall" "" "$DEPLOY_UI_ICON_UNINSTALL"
}

deploy_ui_highlight() {
    printf '\n  %s━━ %s ━━%s\n' "$_C_BRIGHT_CYAN" "$*" "$_C_RESET"
}

deploy_ui_target_swap() {
    local user="$1" host="$2"
    printf '\n'
    deploy_ui_warn "You passed HOST then USER — using ${user}@${host}"
    deploy_ui_note "Tip: ./scripts/deploy-remote.sh ${user} ${host}   (USER HOST is canonical)"
    printf '\n'
}

_deploy_ui_spinner_pid=""
deploy_ui_spinner_start() {
    local msg="$1"
    [[ -t 1 ]] || return 0
    (
        local frames=('🌑' '🌒' '🌓' '🌔' '🌕' '🌖' '🌗' '🌘')
        local i=0
        while true; do
            printf '\r  %s %s %s···%s' "${frames[$i]}" "$msg" "$_C_DIM" "$_C_RESET"
            i=$(( (i + 1) % 8 ))
            sleep 0.12
        done
    ) &
    _deploy_ui_spinner_pid=$!
}

deploy_ui_spinner_stop() {
    if [[ -n "$_deploy_ui_spinner_pid" ]]; then
        kill "$_deploy_ui_spinner_pid" 2>/dev/null || true
        wait "$_deploy_ui_spinner_pid" 2>/dev/null || true
        _deploy_ui_spinner_pid=""
    fi
    printf '\r\033[K'
}

deploy_ui_dry_run() {
    local host="$1" user="$2" remote_dir="$3" quick="$4"
    deploy_ui_banner "${DEPLOY_UI_ICON_MAGIC} Dry run" "no changes will be made"
    deploy_ui_kv "🎯" "Target" "${user}@${host}"
    deploy_ui_kv "📁" "Remote dir" "$remote_dir"
    deploy_ui_kv "⚡" "Mode" "$([ "$quick" = true ] && echo 'quick ⚡' || echo 'full 📦')"
    printf '\n'
    deploy_ui_note "Would: rsync → chown → remote build (cargo/npm) → install → restart"
    printf '\n'
}

deploy_ui_checklist() {
    local label="$1" status="$2"
    local icon="✅" color="$_C_BRIGHT_GREEN"
    case "$status" in
        OK|ok|active|200) icon="✅" color="$_C_BRIGHT_GREEN" ;;
        WARN*|warn*|unknown) icon="⚠️" color="$_C_YELLOW" ;;
        *) icon="❌" color="$_C_RED" ;;
    esac
    printf '    %s %-16s %s%s%s\n' "$icon" "$label" "$color" "$status" "$_C_RESET"
}

deploy_ui_success() {
    local host="${1:-localhost}" secs="${2:-0}" extra_cmd="${3:-}"
    local base="${DEPLOY_UI_SCHEME}://${host}:${DEPLOY_UI_PORT}"
    printf '\n'
    printf '  %s╔══════════════════════════════════════════════════════════╗%s\n' "$_C_BRIGHT_GREEN" "$_C_RESET"
    printf '  %s║%s  %s %s deploy complete! %s %s  %s║%s\n' \
        "$_C_BRIGHT_GREEN" "$_C_RESET" "$DEPLOY_UI_ICON" "$DEPLOY_UI_PROJECT" "$DEPLOY_UI_ICON_MAGIC" "$DEPLOY_UI_ICON_ROCKET" \
        "$_C_BRIGHT_GREEN" "$_C_RESET"
    printf '  %s╠══════════════════════════════════════════════════════════╣%s\n' "$_C_BRIGHT_GREEN" "$_C_RESET"
    printf '  %s║%s 🌐 Dashboard    %s%-26s%s %s║%s\n' \
        "$_C_BRIGHT_GREEN" "$_C_RESET" "$_C_WHITE" "${base}${DEPLOY_UI_DASH_PATH}" "$_C_RESET" "$_C_BRIGHT_GREEN" "$_C_RESET"
    printf '  %s║%s 💚 Health       %s%-26s%s %s║%s\n' \
        "$_C_BRIGHT_GREEN" "$_C_RESET" "$_C_WHITE" "${base}${DEPLOY_UI_HEALTH_PATH}" "$_C_RESET" "$_C_BRIGHT_GREEN" "$_C_RESET"
    printf '  %s╚══════════════════════════════════════════════════════════╝%s\n' "$_C_BRIGHT_GREEN" "$_C_RESET"
    if [[ -n "$secs" ]] && [[ "$secs" -gt 0 ]] 2>/dev/null; then
        printf '  %s⏱️  Finished in %ss%s\n' "$_C_DIM" "$secs" "$_C_RESET"
    fi
    if [[ -n "$extra_cmd" ]]; then
        printf '  %s🔁 Quick redeploy:%s %s\n' "$_C_DIM" "$_C_RESET" "$extra_cmd"
    fi
    printf '\n'
}

deploy_ui_celebrate() {
    local msg="${1:-All systems go}"
    printf '  %s🎉 %s 🎊%s\n' "$_C_BRIGHT_MAGENTA" "$msg" "$_C_RESET"
}

deploy_ui_parse_target() {
    local _host_var="$1" _user_var="$2"
    local target
    eval "target=\${$_host_var}"
    if [[ "$target" == *@* ]]; then
        eval "${_user_var}=\${target%%@*}"
        eval "${_host_var}=\${target#*@}"
    fi
}

deploy_ui_deploy_state_file() {
    echo "$1/.deploy-last"
}

deploy_ui_save_deploy_last() {
    local repo_dir="$1" host="$2" user="$3" mode="$4" version="${5:-}" commit="${6:-}"
    local f
    f=$(deploy_ui_deploy_state_file "$repo_dir")
    cat >"$f" <<EOF
# Auto-generated by ${DEPLOY_UI_PROJECT} deploy (no passwords)
HOST=$host
USER=$user
MODE=$mode
UPDATED=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
VERSION=${version:-unknown}
COMMIT=${commit:-unknown}
EOF
    chmod 600 "$f" 2>/dev/null || true
}

deploy_ui_load_deploy_last() {
    local repo_dir="$1" f
    f=$(deploy_ui_deploy_state_file "$repo_dir")
    [[ -f "$f" ]] || return 1
    # shellcheck disable=SC1090
    source "$f"
    return 0
}
