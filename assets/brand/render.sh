#!/usr/bin/env bash
# Renders the PNGs from the SVGs. Needs resvg (heavy compile cargo install resvg --locked).
set -euo pipefail
cd "$(dirname "$0")"
resvg -w 180 -h 180 sigillr-app-icon.svg apple-touch-icon.png
resvg -w 1280 -h 640 sigillr-social-preview.svg sigillr-social-preview.png
