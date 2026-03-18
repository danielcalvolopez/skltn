#!/usr/bin/env bash
set -euo pipefail

REPO="danielcalvolopez/skltn"
INSTALL_DIR="$HOME/.skltn/bin"
TMPDIR_CLEANUP=""

cleanup() {
    if [ -n "$TMPDIR_CLEANUP" ]; then
        rm -rf "$TMPDIR_CLEANUP"
    fi
}
trap cleanup EXIT

main() {
    echo "Installing skltn..."

    local os arch target version url

    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin) os="apple-darwin" ;;
        Linux)  os="unknown-linux-gnu" ;;
        *)
            echo "Error: Unsupported OS: $os" >&2
            exit 1
            ;;
    esac

    case "$arch" in
        arm64|aarch64) arch="aarch64" ;;
        x86_64)        arch="x86_64" ;;
        *)
            echo "Error: Unsupported architecture: $arch" >&2
            exit 1
            ;;
    esac

    target="${arch}-${os}"

    # Get latest release tag
    version="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"

    if [ -z "$version" ]; then
        echo "Error: Could not determine latest release version" >&2
        exit 1
    fi

    echo "  Version: $version"
    echo "  Target:  $target"

    url="https://github.com/${REPO}/releases/download/${version}/skltn-${version}-${target}.tar.gz"

    # Download and extract
    TMPDIR_CLEANUP="$(mktemp -d)"

    echo "  Downloading $url..."
    curl -fsSL "$url" -o "$TMPDIR_CLEANUP/skltn.tar.gz"

    mkdir -p "$INSTALL_DIR"
    tar -xzf "$TMPDIR_CLEANUP/skltn.tar.gz" -C "$INSTALL_DIR"
    chmod +x "$INSTALL_DIR/skltn" "$INSTALL_DIR/skltn-mcp" "$INSTALL_DIR/skltn-obs"

    echo "  Installed to $INSTALL_DIR"

    # Add to PATH if not already present
    add_to_path

    echo ""
    echo "Installation complete!"
    echo ""
    echo "Usage:"
    echo "  cd ~/your-project"
    echo "  skltn start"
}

add_to_path() {
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*)
            return
            ;;
    esac

    local rc_file=""
    local shell_name
    shell_name="$(basename "${SHELL:-/bin/bash}")"

    case "$shell_name" in
        zsh)  rc_file="$HOME/.zshrc" ;;
        bash)
            if [ -f "$HOME/.bashrc" ]; then
                rc_file="$HOME/.bashrc"
            elif [ -f "$HOME/.bash_profile" ]; then
                rc_file="$HOME/.bash_profile"
            else
                rc_file="$HOME/.bashrc"
            fi
            ;;
        fish)
            # fish uses a different mechanism
            echo ""
            echo "Add to your fish config:"
            echo "  set -gx PATH \$HOME/.skltn/bin \$PATH"
            return
            ;;
        *)
            rc_file="$HOME/.profile"
            ;;
    esac

    local path_line='export PATH="$HOME/.skltn/bin:$PATH"'

    if [ -n "$rc_file" ]; then
        if ! grep -qF '.skltn/bin' "$rc_file" 2>/dev/null; then
            echo "" >> "$rc_file"
            echo "# skltn" >> "$rc_file"
            echo "$path_line" >> "$rc_file"
            echo "  Added $INSTALL_DIR to PATH in $rc_file"
            echo "  Run: source $rc_file (or open a new terminal)"
        fi
    fi
}

main "$@"
