---
apply_when: desktop-gui, web-ui
mode: inline
---
- **Window Chrome (Desktop):** Disable native OS titlebars; build an integrated frameless titlebar with drag region, double-click maximize, and custom min/max/close controls.
- **Scrollbars:** Ban raw OS scrollbars; use thin (4–6px) themed scrollbars with transparent tracks and zero layout shift.
- **Native App Feel:** Lock desktop shell to `100vw x 100vh` (`overflow: hidden`) with internal scroll panes; `user-select: none` on UI chrome; style all form controls; define hover/active/focus-visible/disabled states and snappy 120–180ms transitions.