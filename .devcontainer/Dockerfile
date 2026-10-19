FROM mcr.microsoft.com/devcontainers/base:ubuntu

# Create the directory and set permissions
RUN mkdir -p /home/vscode/.vscode-server/extensions \
             /home/vscode/.vscode-server/bin \
    && chown -R vscode:vscode /home/vscode/.vscode-server

# Install apt
RUN apt update

# Install Node.js if not present in your base image
RUN curl -fsSL https://deb.nodesource.com/setup_24.x | bash - && \
    apt-get install -y nodejs

# Install system package
RUN apt-get install -y build-essential

# Phase 0 detector spike: ffmpeg drives every decode, numpy/Pillow do the frame
# analysis. ffmpeg is a hard dependency of the product itself (see CLAUDE.md 3.1).
RUN apt-get install -y ffmpeg python3-numpy python3-pil

# The Tauri app (fpv-rs/crates/fpv-app): WebKitGTK and its build deps, the
# GStreamer plugins the web view needs to play the H.264 proxies, and Xvfb +
# xdotool to drive the window headless from here.
RUN apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev libsoup-3.0-dev libxdo-dev libssl-dev file pkg-config \
    gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav \
    xvfb xdotool imagemagick

USER vscode

# Rust toolchain for the Linux port (fpv-rs/, CLAUDE.md 9.9). rustup picks the
# host architecture itself, so the arm64 trap above does not apply here.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --component clippy,rustfmt
ENV PATH="/home/vscode/.cargo/bin:${PATH}"

# Install Claude Code
RUN curl -fsSL https://claude.ai/install.sh | bash

# Install OpenCode
RUN curl -fsSL https://opencode.ai/install | bash