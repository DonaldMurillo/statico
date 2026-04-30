#!/usr/bin/env bash
# statico installer — curl | sh
# Usage: curl -fsSL https://github.com/domvess/statico/raw/main/install/install.sh | sh
#        curl -fsSL https://github.com/domvess/statico/raw/main/install/install.sh | sh -s -- --uninstall
set -euo pipefail

readonly REPO="domvess/statico"
readonly INSTALL_DIR="${HOME}/.statico"
readonly BIN_DIR="${INSTALL_DIR}/bin"
readonly COMPLETIONS_DIR="${INSTALL_DIR}/completions"
readonly BINARY_NAME="statico"

# ── colours ──────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'
    BOLD='\033[1m'; RESET='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BOLD=''; RESET=''
fi

info()  { printf "${GREEN}info${RESET}: %s\n" "$*" >&2; }
warn()  { printf "${YELLOW}warn${RESET}: %s\n" "$*" >&2; }
error() { printf "${RED}error${RESET}: %s\n" "$*" >&2; }

# ── helpers ──────────────────────────────────────────────────────────
detect_os() {
    local uname_out
    uname_out="$(uname -s)"
    case "${uname_out}" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *)       error "Unsupported OS: ${uname_out}"; exit 1 ;;
    esac
}

detect_arch() {
    local uname_m
    uname_m="$(uname -m)"
    case "${uname_m}" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             error "Unsupported architecture: ${uname_m}"; exit 1 ;;
    esac
}

download_url() {
    local os="$1" arch="$2"
    echo "https://github.com/${REPO}/releases/latest/download/statico-${os}-${arch}.tar.gz"
}

# Idempotent: append only if the exact marker line is missing.
ensure_line_in_file() {
    local file="$1" line="$2" marker="$3"

    # Create file if it doesn't exist
    if [[ ! -f "${file}" ]]; then
        printf '\n%s\n%s\n' "${marker}" "${line}" >> "${file}"
        return
    fi

    # Already present? Skip.
    if grep -qF "${line}" "${file}" 2>/dev/null; then
        return
    fi

    # Append after a blank line for neatness
    printf '\n%s\n%s\n' "${marker}" "${line}" >> "${file}"
}

remove_line_from_file() {
    local file="$1" line="$2"
    if [[ -f "${file}" ]]; then
        # Remove the line (and the marker comment, if present)
        local tmp
        tmp="$(mktemp)"
        grep -vF "${line}" "${file}" > "${tmp}" 2>/dev/null || true
        grep -v '# statico installer' "${tmp}" > "${file}" 2>/dev/null || true
        rm -f "${tmp}"
        # Remove trailing blank lines (keep one newline)
        sed -i '' -e :a -e '/^\n*$/{$d;N;ba' -e '}' "${file}" 2>/dev/null || \
        sed -i -e :a -e '/^\n*$/{$d;N;ba' -e '}' "${file}" 2>/dev/null || true
    fi
}

# ── uninstall ────────────────────────────────────────────────────────
uninstall() {
    info "Uninstalling statico…"

    # Remove the install directory
    if [[ -d "${INSTALL_DIR}" ]]; then
        rm -rf "${INSTALL_DIR}"
        info "Removed ${INSTALL_DIR}"
    fi

    # Remove PATH, alias, and completion source lines from shell configs
    local shell_configs=("${HOME}/.bashrc" "${HOME}/.zshrc" "${HOME}/.profile")
    for rc in "${shell_configs[@]}"; do
        remove_line_from_file "${rc}" 'export PATH="${HOME}/.statico/bin:${PATH}"'
        remove_line_from_file "${rc}" "alias st='statico'"
        remove_line_from_file "${rc}" '[ -f "${HOME}/.statico/completions/statico.bash" ] && . "${HOME}/.statico/completions/statico.bash"'
    done

    printf "\n${GREEN}statico has been uninstalled.${RESET}\n"
    info "Restart your shell or run: exec \$SHELL"
    exit 0
}

# ── main install ─────────────────────────────────────────────────────
main() {
    if [[ "${1:-}" == "--uninstall" ]]; then
        uninstall
    fi

    local os arch url tmpdir

    os="$(detect_os)"
    arch="$(detect_arch)"
    url="$(download_url "${os}" "${arch}")"

    info "Detected: ${os}-${arch}"
    info "Downloading ${url}…"

    # Create dirs
    mkdir -p "${BIN_DIR}" "${COMPLETIONS_DIR}"

    # Download and extract
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "${tmpdir}"' EXIT

    if command -v curl &>/dev/null; then
        curl -fsSL "${url}" -o "${tmpdir}/statico.tar.gz"
    elif command -v wget &>/dev/null; then
        wget -qO "${tmpdir}/statico.tar.gz" "${url}"
    else
        error "Neither curl nor wget found. Please install one and retry."
        exit 1
    fi

    tar -xzf "${tmpdir}/statico.tar.gz" -C "${tmpdir}"

    # Find the binary inside the extracted archive (may be at root or in a subdirectory)
    local binary_path
    binary_path="$(find "${tmpdir}" -name "${BINARY_NAME}" -type f -print -quit 2>/dev/null || true)"
    if [[ -z "${binary_path}" ]]; then
        error "Could not find '${BINARY_NAME}' binary in the downloaded archive."
        exit 1
    fi

    # Install binary
    chmod +x "${binary_path}"
    mv -f "${binary_path}" "${BIN_DIR}/${BINARY_NAME}"
    info "Installed ${BINARY_NAME} to ${BIN_DIR}/${BINARY_NAME}"

    # ── shell integration ────────────────────────────────────────────
    local path_line='export PATH="${HOME}/.statico/bin:${PATH}"'
    local alias_line="alias st='statico'"

    local shell_configs=()
    for rc in ".bashrc" ".zshrc" ".profile"; do
        if [[ -f "${HOME}/${rc}" ]] || [[ "${rc}" == ".bashrc" ]]; then
            shell_configs+=("${HOME}/${rc}")
        fi
    done

    for rc in "${shell_configs[@]}"; do
        ensure_line_in_file "${rc}" "${path_line}"  "# statico installer — PATH"
        ensure_line_in_file "${rc}" "${alias_line}" "# statico installer — alias"
    done

    # ── completions ──────────────────────────────────────────────────
    if "${BIN_DIR}/${BINARY_NAME}" completions bash &>/dev/null > "${COMPLETIONS_DIR}/statico.bash"; then
        local completion_line='[ -f "${HOME}/.statico/completions/statico.bash" ] && . "${HOME}/.statico/completions/statico.bash"'
        for rc in "${shell_configs[@]}"; do
            # Only add bash completion to bash-like configs
            case "${rc}" in
                *.bashrc|*.profile)
                    ensure_line_in_file "${rc}" "${completion_line}" "# statico installer — completions"
                    ;;
            esac
        done
        info "Bash completions installed to ${COMPLETIONS_DIR}/statico.bash"
    else
        warn "Could not generate shell completions (non-fatal)."
    fi

    # ── zsh completions ──────────────────────────────────────────────
    if "${BIN_DIR}/${BINARY_NAME}" completions zsh &>/dev/null > "${COMPLETIONS_DIR}/_statico"; then
        info "Zsh completions installed to ${COMPLETIONS_DIR}/_statico"
        # Hint in .zshrc to add completions dir to fpath (idempotent)
        local fpath_line='fpath=("${HOME}/.statico/completions" ${fpath})'
        for rc in "${shell_configs[@]}"; do
            case "${rc}" in
                *.zshrc)
                    ensure_line_in_file "${rc}" "${fpath_line}" "# statico installer — zsh fpath"
                    ;;
            esac
        done
    fi

    # ── success ──────────────────────────────────────────────────────
    local version
    version="$("${BIN_DIR}/${BINARY_NAME}" --version 2>/dev/null || echo "unknown")"

    printf "\n"
    printf "${BOLD}${GREEN}✔ statico ${version} installed successfully!${RESET}\n"
    printf "\n"
    printf "  Binary:       ${BIN_DIR}/${BINARY_NAME}\n"
    printf "  Alias:        st → statico\n"
    printf "  Completions:  ${COMPLETIONS_DIR}/\n"
    printf "\n"
    printf "${YELLOW}Restart your shell or run:${RESET} exec \$SHELL\n"
    printf "\n"
    printf "  Or activate now:\n"
    printf "    export PATH=\"\${HOME}/.statico/bin:\${PATH}\"\n"
    printf "\n"
}

main "$@"
