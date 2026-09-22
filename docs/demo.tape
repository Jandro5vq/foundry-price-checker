# VHS demo tape for Foundry Price Checker.
# Requires: vhs, ttyd, ffmpeg.  Run from the repo root:  vhs docs/demo.tape
#
# IMPORTANT: use VHS v0.11.0. v0.12.0 has a regression that cancels the context
# before invoking ffmpeg, so it produces no output.
#
# Regenerate:  cargo build --release && vhs docs/demo.tape

Output docs/demo.gif

Set Shell bash
Set FontSize 16
Set FontFamily "DejaVu Sans Mono"
Set Width 1500
Set Height 900
Set Padding 20
Set Margin 12
Set MarginFill "#1b1f27"
Set BorderRadius 10
Set Theme "Catppuccin Mocha"
Set Framerate 30
Set TypingSpeed 45ms

Type "./target/release/foundry-price-tui"
Enter

# wait for prices + LMArena Elo + OpenRouter capabilities to load
Sleep 14s

# sort by Elo (7 presses of `s`), descending
Type "sssssss"
Sleep 400ms
Type "r"
Sleep 2s
Screenshot docs/screenshot.png

# search a family
Type "/gpt-4.1"
Sleep 300ms
Enter
Sleep 2500ms

# filter panel: region / currency / services
Type "f"
Sleep 2s
Escape
Sleep 400ms

# help dialog
Type "?"
Sleep 3s
Screenshot docs/help.png
Escape
Sleep 400ms

# quit
Type "q"
Sleep 1s
