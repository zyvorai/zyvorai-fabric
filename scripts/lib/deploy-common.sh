# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# shellcheck shell=bash
# zyvor-fabricd deploy library (self-contained under scripts/lib/).

_DEPLOY_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DEPLOY_UI_PROJECT="zyvor-fabricd"
DEPLOY_UI_ICON="🖥️"
DEPLOY_UI_ICON_UNINSTALL="🗑️"
DEPLOY_UI_ICON_MAGIC="✨"
DEPLOY_UI_PORT="9095"
DEPLOY_UI_SCHEME="https"
DEPLOY_UI_DASH_PATH="/"
DEPLOY_UI_HEALTH_PATH="/health"

# shellcheck source=deploy-ui.sh
source "$_DEPLOY_LIB_DIR/deploy-ui.sh"

fabric_build_metadata() {
    local repo_dir="$1"
    FABRIC_VERSION=$(git -C "$repo_dir" describe --tags --always --dirty 2>/dev/null || echo 'dev')
    FABRIC_COMMIT=$(git -C "$repo_dir" rev-parse --short HEAD 2>/dev/null || echo 'unknown')
    export FABRIC_VERSION FABRIC_COMMIT
}

fabric_deploy_state_file() { deploy_ui_deploy_state_file "$1"; }
fabric_save_deploy_last() {
    deploy_ui_save_deploy_last "$1" "$2" "$3" "$4" "${FABRIC_VERSION:-}" "${FABRIC_COMMIT:-}"
}
fabric_load_deploy_last() { deploy_ui_load_deploy_last "$1"; }

fabric_elapsed_fmt() {
    local s="$1"
    local m=$((s / 60)) r=$((s % 60))
    ((m > 0)) && printf '%dm ' "$m"
    printf '%ds' "$r"
}

fabric_print_success() {
    local host="$1" elapsed="$2" user="$3"
    deploy_ui_success "$host" "$elapsed" "./scripts/deploy remote ${user}@${host} --quick"
}

fabric_remote_dir_for_user() {
    local user="$1"
    if [[ -n "${DEPLOY_DIR:-}" || -n "${REMOTE_DIR:-}" ]]; then
        echo "${REMOTE_DIR:-${DEPLOY_DIR}}"
        return
    fi
    if [[ "$user" == "root" ]]; then
        echo "/root/zyvor-fabric"
    else
        echo "/home/${user}/zyvor-fabric"
    fi
}

fabric_sudo_prefix_for_user() {
    local user="$1"
    [[ "$user" == "root" ]] && echo "" || echo "sudo"
}
