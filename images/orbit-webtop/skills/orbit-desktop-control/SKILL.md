---
name: orbit-desktop-control
description: Control the containerized XFCE desktop - capture the screen and drive keyboard/mouse with scrot and xdotool.
---

# Orbit Desktop Control

Control the containerized XFCE desktop from a shell using screenshots and input events.

## Screenshots

- The desktop display is `:1`. Set it when needed: `export DISPLAY=:1`.
- Capture the current screen: `scrot -o /tmp/screen.png`.
- Read `/tmp/screen.png` to inspect the captured desktop.
- The resolution is fixed at 1600x900.

## Mouse

- Move the pointer: `xdotool mousemove X Y`.
- Click: `xdotool click 1`.
- Button numbers are 1=left, 2=middle, and 3=right.
- Move and click: `xdotool mousemove X Y click 1`.

Coordinates are absolute positions within the 1600x900 desktop.

## Keyboard

- Type text: `xdotool type 'text'`.
- Press Enter: `xdotool key Return`.
- Copy: `xdotool key ctrl+c`.
- Use xdotool key names for special keys, such as `Escape`, `Tab`, `BackSpace`, `Delete`, `Up`, `Down`, `Left`, `Right`, and `F5`.

## Windows

- Find windows by title: `xdotool search --name <title>`.
- Activate a window by ID: `xdotool windowactivate <window-id>`.

## Operating Loop

1. Capture a screenshot.
2. Inspect it and decide the next action.
3. Act with xdotool.
4. Capture another screenshot to verify the result.

The browser is Firefox: `BROWSER=firefox`.
