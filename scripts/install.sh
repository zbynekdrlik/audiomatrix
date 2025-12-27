#!/bin/bash
#
# AudioMatrix Installer for Linux/macOS
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/zbynekdrlik/audiomatrix/main/scripts/install.sh | bash
#

set -euo pipefail

# Configuration
REPO="zbynekdrlik/audiomatrix"
INSTALL_DIR="${HOME}/.local/bin"
BIN_NAME="audiomatrix"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

print_header() {
    echo ""
    echo -e "${CYAN}=============================================${NC}"
    echo -e "${CYAN}       AudioMatrix Installer${NC}"
    echo -e "${CYAN}=============================================${NC}"
    echo ""
}

detect_platform() {
    local os=$(uname -s | tr '[:upper:]' '[:lower:]')
    local arch=$(uname -m)

    case "$os" in
        linux)
            case "$arch" in
                x86_64) echo "linux-x64" ;;
                *) echo "unsupported" ;;
            esac
            ;;
        darwin)
            case "$arch" in
                x86_64) echo "macos-x64" ;;
                arm64) echo "macos-arm64" ;;
                *) echo "unsupported" ;;
            esac
            ;;
        *)
            echo "unsupported"
            ;;
    esac
}

get_latest_version() {
    curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"v([^"]+)".*/\1/'
}

get_installed_version() {
    if command -v "$BIN_NAME" &> /dev/null; then
        "$BIN_NAME" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo ""
    else
        echo ""
    fi
}

download_and_install() {
    local version=$1
    local platform=$2

    local filename="audiomatrix-${version}-${platform}.tar.gz"
    local url="https://github.com/${REPO}/releases/download/v${version}/${filename}"
    local checksum_url="${url}.sha256"

    local tmp_dir=$(mktemp -d)
    trap "rm -rf $tmp_dir" EXIT

    echo -e "${YELLOW}Downloading AudioMatrix v${version}...${NC}"
    curl -sSL -o "${tmp_dir}/${filename}" "$url"

    # Verify checksum
    echo -e "${YELLOW}Verifying checksum...${NC}"
    local expected_hash=$(curl -sSL "$checksum_url" | awk '{print $1}')
    local actual_hash=$(sha256sum "${tmp_dir}/${filename}" 2>/dev/null || shasum -a 256 "${tmp_dir}/${filename}" | awk '{print $1}')

    if [ "$expected_hash" != "$actual_hash" ]; then
        echo -e "${RED}Checksum verification failed!${NC}"
        echo "Expected: $expected_hash"
        echo "Got: $actual_hash"
        exit 1
    fi
    echo -e "${GREEN}Checksum verified!${NC}"

    # Extract
    echo -e "${YELLOW}Extracting...${NC}"
    tar -xzf "${tmp_dir}/${filename}" -C "$tmp_dir"

    # Install
    echo -e "${YELLOW}Installing to ${INSTALL_DIR}...${NC}"
    mkdir -p "$INSTALL_DIR"
    cp "${tmp_dir}/audiomatrix" "${INSTALL_DIR}/${BIN_NAME}"
    chmod +x "${INSTALL_DIR}/${BIN_NAME}"
}

add_to_path() {
    local shell_rc=""

    if [ -n "${ZSH_VERSION:-}" ] || [ -f "${HOME}/.zshrc" ]; then
        shell_rc="${HOME}/.zshrc"
    elif [ -n "${BASH_VERSION:-}" ] || [ -f "${HOME}/.bashrc" ]; then
        shell_rc="${HOME}/.bashrc"
    fi

    if [ -n "$shell_rc" ] && ! grep -q "${INSTALL_DIR}" "$shell_rc" 2>/dev/null; then
        echo -e "${YELLOW}Adding ${INSTALL_DIR} to PATH in ${shell_rc}...${NC}"
        echo "" >> "$shell_rc"
        echo "# AudioMatrix" >> "$shell_rc"
        echo "export PATH=\"\$PATH:${INSTALL_DIR}\"" >> "$shell_rc"
        echo -e "${GREEN}Added to PATH${NC}"
        echo -e "${YELLOW}Run 'source ${shell_rc}' or restart your terminal${NC}"
    fi
}

# Main
main() {
    print_header

    # Detect platform
    local platform=$(detect_platform)
    if [ "$platform" = "unsupported" ]; then
        echo -e "${RED}Unsupported platform: $(uname -s) $(uname -m)${NC}"
        exit 1
    fi
    echo "Platform: $platform"

    # Get latest version
    local latest_version=$(get_latest_version)
    if [ -z "$latest_version" ]; then
        echo -e "${RED}Failed to fetch latest version${NC}"
        exit 1
    fi
    echo -e "${CYAN}Latest version: v${latest_version}${NC}"

    # Check installed version
    local installed_version=$(get_installed_version)
    if [ -n "$installed_version" ]; then
        echo -e "${CYAN}Installed version: v${installed_version}${NC}"

        if [ "$installed_version" = "$latest_version" ]; then
            echo ""
            echo -e "${GREEN}AudioMatrix is already up to date!${NC}"
            echo ""
            exit 0
        fi
        echo -e "${YELLOW}Updating from v${installed_version} to v${latest_version}...${NC}"
    else
        echo "No previous installation found"
    fi

    echo ""

    # Download and install
    download_and_install "$latest_version" "$platform"

    # Add to PATH if needed
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        add_to_path
        export PATH="$PATH:${INSTALL_DIR}"
    fi

    echo ""
    echo -e "${GREEN}=============================================${NC}"
    echo -e "${GREEN}  AudioMatrix v${latest_version} installed!${NC}"
    echo -e "${GREEN}=============================================${NC}"
    echo ""
    echo -e "${CYAN}Run 'audiomatrix --help' to get started.${NC}"
    echo ""
}

main "$@"
