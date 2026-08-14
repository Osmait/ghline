#!/bin/sh
# Install github-tui.
#
#   curl -fsSL https://raw.githubusercontent.com/Osmait/github-tui/main/install.sh | sh
#
# Downloads the release binary for this machine, checks it against the
# published SHA-256, and puts it in ~/.local/bin. Nothing is built, nothing
# needs root, and nothing outside the install directory is touched.
#
# Knobs, all optional:
#   GITHUB_TUI_VERSION      tag to install, e.g. v0.1.0 (default: latest)
#   GITHUB_TUI_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#
# Plain POSIX sh on purpose: macOS still ships bash 3.2, and this has to run
# before the user has installed anything at all.

set -eu

REPO="Osmait/github-tui"
BIN="github-tui"

INSTALL_DIR="${GITHUB_TUI_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || die "this needs \`$1\`, which is not installed"
}

# --- what are we running on ------------------------------------------------

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os_part="unknown-linux-gnu" ;;
        Darwin) os_part="apple-darwin" ;;
        *) die "no prebuilt binary for $os — build from source: https://github.com/$REPO" ;;
    esac

    case "$arch" in
        x86_64 | amd64)  arch_part="x86_64" ;;
        arm64 | aarch64) arch_part="aarch64" ;;
        *) die "no prebuilt binary for $arch — build from source: https://github.com/$REPO" ;;
    esac

    printf '%s-%s' "$arch_part" "$os_part"
}

# --- which release ---------------------------------------------------------

# The tag GitHub calls "latest", read off the redirect rather than the JSON
# API: one request, no parser, and no token needed.
latest_tag() {
    url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")"
    case "$url" in
        */releases/tag/*) printf '%s' "${url##*/tag/}" ;;
        # With no releases at all, GitHub redirects to the releases index
        # instead of a tag — which is a real answer, not a failure to parse.
        *) return 1 ;;
    esac
}

# --- go --------------------------------------------------------------------

main() {
    need curl
    need tar

    target="$(detect_target)"

    if [ -n "${GITHUB_TUI_VERSION:-}" ]; then
        tag="$GITHUB_TUI_VERSION"
        case "$tag" in v*) ;; *) tag="v$tag" ;; esac
    else
        tag="$(latest_tag || true)"
        [ -n "$tag" ] || die "no published release yet — see https://github.com/$REPO/releases"
    fi

    archive="$BIN-$target.tar.gz"
    base="https://github.com/$REPO/releases/download/$tag"

    say "Installing $BIN $tag ($target)"

    tmp="$(mktemp -d)"
    # Leave nothing behind, including on a failed download or a Ctrl-C.
    trap 'rm -rf "$tmp"' EXIT INT TERM

    curl -fsSL "$base/$archive" -o "$tmp/$archive" \
        || die "no build for $target in $tag — see https://github.com/$REPO/releases/tag/$tag"
    curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256" \
        || die "release $tag has no checksum for $archive; refusing to install unverified"

    verify "$tmp" "$archive"

    tar -xzf "$tmp/$archive" -C "$tmp"
    [ -f "$tmp/$BIN" ] || die "$archive did not contain $BIN"

    mkdir -p "$INSTALL_DIR"
    # Moved into place from the same directory, so a half-written binary can
    # never end up on PATH — and so replacing a copy that is currently running
    # works instead of failing with "text file busy".
    chmod +x "$tmp/$BIN"
    mv "$tmp/$BIN" "$INSTALL_DIR/$BIN.new"
    mv "$INSTALL_DIR/$BIN.new" "$INSTALL_DIR/$BIN"

    say "Installed to $INSTALL_DIR/$BIN"
    check_path
    check_gh
    say ""
    say "Run \`$BIN\` to start."
}

# Checksums are published as `<sha>  <name>`, the format both tools read.
verify() {
    dir="$1"
    name="$2"

    if command -v sha256sum >/dev/null 2>&1; then
        (cd "$dir" && sha256sum -c "$name.sha256" >/dev/null 2>&1) \
            || die "checksum mismatch for $name — refusing to install"
    elif command -v shasum >/dev/null 2>&1; then
        (cd "$dir" && shasum -a 256 -c "$name.sha256" >/dev/null 2>&1) \
            || die "checksum mismatch for $name — refusing to install"
    else
        die "neither sha256sum nor shasum found; refusing to install unverified"
    fi
}

# Say so rather than editing the user's shell config behind their back.
check_path() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) return 0 ;;
    esac

    say ""
    say "$INSTALL_DIR is not on your PATH. Add it with one of:"
    say ""
    say "  echo 'export PATH=\"\$PATH:$INSTALL_DIR\"' >> ~/.bashrc"
    say "  echo 'export PATH=\"\$PATH:$INSTALL_DIR\"' >> ~/.zshrc"
    say "  fish_add_path $INSTALL_DIR"
}

# github-tui reads GitHub through the `gh` CLI, so without it the app starts
# and then has nothing to show. Worth saying at install time rather than
# leaving the user to work it out from an empty pane. Not fatal: the demo mode
# runs without it.
check_gh() {
    if ! command -v gh >/dev/null 2>&1; then
        say ""
        say "note: \`gh\` is not installed. $BIN reads GitHub through it, and"
        say "      falls back to demo data without it. See https://cli.github.com"
    elif ! gh auth status >/dev/null 2>&1; then
        say ""
        say "note: \`gh\` is installed but not signed in. Run \`gh auth login\`."
    fi
}

main "$@"
